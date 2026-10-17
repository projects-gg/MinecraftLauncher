//! [`PresenceManager`]: a background actor that keeps Discord showing the desired activity.
//!
//! Callers only record what they want (never blocking); the actor connects lazily when there
//! is something to show, retries every `reconnect_interval` while Discord is not running,
//! re-applies the activity after a reconnect, answers pings and keeps updates within
//! Discord's rate limit (at most `max_updates` per `update_window`; intermediate states are
//! coalesced into the latest one).

use std::collections::VecDeque;
use std::path::PathBuf;
use std::sync::{Mutex, PoisonError};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use tokio::sync::watch;
use tokio::task::JoinHandle;
use tokio::time::Instant;

use crate::activity::Activity;
use crate::connection::{DiscordIpc, DiscordUser, Incoming};
use crate::error::{Error, Result};
use crate::frame::Frame;
use crate::ipc::default_endpoints;

#[derive(Debug, Clone)]
pub struct PresenceConfig {
    /// Discord application id.
    pub client_id: String,
    pub enabled: bool,
    /// Delay between connection attempts while Discord is not running.
    pub reconnect_interval: Duration,
    /// Limit for connecting plus the handshake.
    pub connect_timeout: Duration,
    /// Rate limit: at most `max_updates` activity updates per `update_window`.
    pub max_updates: usize,
    pub update_window: Duration,
    /// IPC endpoints to try instead of the platform defaults.
    pub endpoints: Option<Vec<PathBuf>>,
}

impl PresenceConfig {
    pub fn new(client_id: impl Into<String>) -> Self {
        Self {
            client_id: client_id.into(),
            enabled: true,
            reconnect_interval: Duration::from_secs(15),
            connect_timeout: Duration::from_secs(5),
            max_updates: 5,
            update_window: Duration::from_secs(20),
            endpoints: None,
        }
    }
}

/// Connection state for settings screens.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PresenceStatus {
    /// Turned off in settings (or shut down).
    Disabled,
    /// Enabled, nothing to show, not connected.
    Idle,
    Connecting,
    Connected {
        user: Option<DiscordUser>,
    },
    /// Discord is not running or refused the connection; retrying.
    Unavailable,
}

#[derive(Debug, Clone, PartialEq)]
struct Desired {
    enabled: bool,
    client_id: String,
    activity: Option<Activity>,
    shutdown: bool,
}

/// Handle to the presence actor. Dropping it stops the actor and clears the presence.
pub struct PresenceManager {
    desired: watch::Sender<Desired>,
    status: watch::Receiver<PresenceStatus>,
    task: Mutex<Option<JoinHandle<()>>>,
}

impl std::fmt::Debug for PresenceManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("PresenceManager")
            .field("status", &*self.status.borrow())
            .finish_non_exhaustive()
    }
}

impl PresenceManager {
    /// Starts the actor on the current tokio runtime.
    pub fn start(config: PresenceConfig) -> Result<Self> {
        let handle = tokio::runtime::Handle::try_current().map_err(|_| Error::NoRuntime)?;
        Ok(Self::start_on(&handle, config))
    }

    /// Starts the actor on `handle`.
    pub fn start_on(handle: &tokio::runtime::Handle, config: PresenceConfig) -> Self {
        let (desired, desired_rx) = watch::channel(Desired {
            enabled: config.enabled,
            client_id: config.client_id,
            activity: None,
            shutdown: false,
        });
        let initial = if config.enabled {
            PresenceStatus::Idle
        } else {
            PresenceStatus::Disabled
        };
        let (status_tx, status) = watch::channel(initial);
        let actor = Actor {
            endpoints: config.endpoints.unwrap_or_else(default_endpoints),
            reconnect_interval: config.reconnect_interval,
            connect_timeout: config.connect_timeout,
            desired: desired_rx,
            status: status_tx,
            conn: None,
            shown: None,
            limiter: RateLimiter::new(config.max_updates, config.update_window),
            retry_at: None,
        };
        let task = handle.spawn(actor.run());
        Self {
            desired,
            status,
            task: Mutex::new(Some(task)),
        }
    }

    /// Shows `activity` (or nothing). Returns immediately.
    pub fn set(&self, activity: Option<Activity>) {
        self.desired.send_if_modified(|desired| {
            if desired.activity == activity {
                return false;
            }
            desired.activity = activity;
            true
        });
    }

    pub fn clear(&self) {
        self.set(None);
    }

    /// Turning presence off disconnects (Discord clears it); turning it on reconnects lazily.
    pub fn set_enabled(&self, enabled: bool) {
        self.desired.send_if_modified(|desired| {
            let changed = desired.enabled != enabled;
            desired.enabled = enabled;
            changed
        });
    }

    /// Switches the Discord application (e.g. from the Studio config); reconnects if needed.
    pub fn set_client_id(&self, client_id: impl Into<String>) {
        let client_id = client_id.into();
        self.desired.send_if_modified(|desired| {
            if desired.client_id == client_id {
                return false;
            }
            desired.client_id = client_id;
            true
        });
    }

    pub fn status(&self) -> PresenceStatus {
        self.status.borrow().clone()
    }

    /// Status updates, e.g. to forward them to the UI.
    pub fn subscribe(&self) -> watch::Receiver<PresenceStatus> {
        self.status.clone()
    }

    /// Clears the presence, closes the connection and waits (briefly) for the actor to stop.
    pub async fn shutdown(&self) {
        self.desired.send_modify(|desired| desired.shutdown = true);
        let task = self
            .task
            .lock()
            .unwrap_or_else(PoisonError::into_inner)
            .take();
        if let Some(task) = task {
            let _ = tokio::time::timeout(Duration::from_secs(3), task).await;
        }
    }
}

struct Connection {
    ipc: DiscordIpc,
    client_id: String,
}

enum Wake {
    Changed,
    Closed,
    Frame(Option<Result<Frame>>),
    Timer,
}

struct Actor {
    endpoints: Vec<PathBuf>,
    reconnect_interval: Duration,
    connect_timeout: Duration,
    desired: watch::Receiver<Desired>,
    status: watch::Sender<PresenceStatus>,
    conn: Option<Connection>,
    /// What Discord shows for this connection; `None` while unknown.
    shown: Option<Option<Activity>>,
    limiter: RateLimiter,
    retry_at: Option<Instant>,
}

impl Actor {
    async fn run(mut self) {
        loop {
            let want = self.desired.borrow_and_update().clone();
            if want.shutdown {
                break;
            }
            let deadline = self.step(&want).await;
            let wake = tokio::select! {
                changed = self.desired.changed() => {
                    if changed.is_ok() { Wake::Changed } else { Wake::Closed }
                }
                frame = next_frame(&mut self.conn) => Wake::Frame(frame),
                () = sleep_until(deadline) => Wake::Timer,
            };
            match wake {
                Wake::Closed => break,
                Wake::Changed | Wake::Timer => {}
                Wake::Frame(frame) => self.on_frame(frame).await,
            }
        }
        self.disconnect().await;
        self.set_status(PresenceStatus::Disabled);
    }

    /// Moves one step towards `want`; returns when to wake up again (rate limit or retry).
    async fn step(&mut self, want: &Desired) -> Option<Instant> {
        if !want.enabled {
            self.disconnect().await;
            self.retry_at = None;
            self.set_status(PresenceStatus::Disabled);
            return None;
        }
        if self
            .conn
            .as_ref()
            .is_some_and(|conn| conn.client_id != want.client_id)
        {
            self.disconnect().await;
            self.retry_at = None;
        }

        if self.conn.is_none() {
            if want.activity.is_none() || want.client_id.trim().is_empty() {
                self.set_status(PresenceStatus::Idle);
                return None;
            }
            if let Some(at) = self.retry_at
                && Instant::now() < at
            {
                self.set_status(PresenceStatus::Unavailable);
                return Some(at);
            }
            self.set_status(PresenceStatus::Connecting);
            match DiscordIpc::connect(&want.client_id, &self.endpoints, self.connect_timeout).await
            {
                Ok(ipc) => {
                    tracing::info!(endpoint = %ipc.endpoint().display(), "connected to Discord");
                    self.set_status(PresenceStatus::Connected {
                        user: ipc.user().cloned(),
                    });
                    self.conn = Some(Connection {
                        ipc,
                        client_id: want.client_id.clone(),
                    });
                    self.shown = Some(None);
                    self.retry_at = None;
                }
                Err(error) => {
                    tracing::debug!(%error, "Discord is not available");
                    self.set_status(PresenceStatus::Unavailable);
                    return self.schedule_retry();
                }
            }
        }

        if self.shown.as_ref() == Some(&want.activity) {
            return None;
        }
        let now = Instant::now();
        if let Some(at) = self.limiter.next_slot(now) {
            return Some(at);
        }
        let conn = self.conn.as_mut()?;
        match conn.ipc.set_activity(want.activity.as_ref()).await {
            Ok(_) => {
                self.limiter.record(now);
                self.shown = Some(want.activity.clone());
                None
            }
            Err(error) => {
                tracing::debug!(%error, "lost the Discord connection");
                self.conn = None;
                self.shown = None;
                self.set_status(PresenceStatus::Unavailable);
                self.schedule_retry()
            }
        }
    }

    async fn on_frame(&mut self, frame: Option<Result<Frame>>) {
        let Some(conn) = self.conn.as_mut() else {
            return;
        };
        let result = match frame {
            Some(Ok(frame)) => conn.ipc.process(frame).await,
            Some(Err(error)) => Err(error),
            None => Err(Error::Disconnected),
        };
        match result {
            Ok(Incoming::Error { code, message, .. }) => {
                tracing::warn!(code, %message, "Discord rejected a presence update");
            }
            Ok(_) => {}
            Err(error) => {
                tracing::debug!(%error, "the Discord connection closed");
                self.conn = None;
                self.shown = None;
                self.set_status(PresenceStatus::Unavailable);
                self.schedule_retry();
            }
        }
    }

    /// Clears what we show (best effort) and closes the connection.
    async fn disconnect(&mut self) {
        if let Some(mut conn) = self.conn.take() {
            if matches!(self.shown, Some(Some(_))) {
                let _ =
                    tokio::time::timeout(Duration::from_secs(1), conn.ipc.set_activity(None)).await;
            }
            let _ = tokio::time::timeout(Duration::from_secs(1), conn.ipc.close()).await;
        }
        self.shown = None;
    }

    fn schedule_retry(&mut self) -> Option<Instant> {
        let at = Instant::now() + self.reconnect_interval;
        self.retry_at = Some(at);
        Some(at)
    }

    fn set_status(&self, status: PresenceStatus) {
        self.status.send_if_modified(|current| {
            if *current == status {
                return false;
            }
            *current = status;
            true
        });
    }
}

async fn next_frame(conn: &mut Option<Connection>) -> Option<Result<Frame>> {
    match conn {
        Some(conn) => conn.ipc.next_frame().await,
        None => std::future::pending().await,
    }
}

async fn sleep_until(deadline: Option<Instant>) {
    match deadline {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

/// Sliding-window limiter: at most `max` sends per `window`.
struct RateLimiter {
    max: usize,
    window: Duration,
    sent: VecDeque<Instant>,
}

impl RateLimiter {
    fn new(max: usize, window: Duration) -> Self {
        Self {
            max: max.max(1),
            window,
            sent: VecDeque::new(),
        }
    }

    /// `None` when a send is allowed now, otherwise when the next one is.
    fn next_slot(&mut self, now: Instant) -> Option<Instant> {
        while self
            .sent
            .front()
            .is_some_and(|sent| now.duration_since(*sent) >= self.window)
        {
            self.sent.pop_front();
        }
        if self.sent.len() < self.max {
            None
        } else {
            self.sent.front().map(|first| *first + self.window)
        }
    }

    fn record(&mut self, now: Instant) {
        self.sent.push_back(now);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::activity::StatusDisplayType;
    use crate::frame::{Opcode, read_frame, write_frame};
    use crate::ipc::BoxedStream;
    use serde_json::{Value, json};
    use tokio::time::timeout;

    const WAIT: Duration = Duration::from_secs(5);

    /// A fake Discord client listening on a private endpoint.
    struct FakeDiscord {
        endpoint: PathBuf,
        #[cfg(windows)]
        next: Option<tokio::net::windows::named_pipe::NamedPipeServer>,
        #[cfg(unix)]
        listener: tokio::net::UnixListener,
        #[cfg(unix)]
        _dir: tempfile::TempDir,
    }

    impl FakeDiscord {
        #[cfg(windows)]
        fn new() -> Self {
            use tokio::net::windows::named_pipe::ServerOptions;
            let endpoint = PathBuf::from(format!(
                r"\\.\pipe\pl-discord-test-{}",
                uuid::Uuid::new_v4()
            ));
            let server = ServerOptions::new()
                .first_pipe_instance(true)
                .create(&endpoint)
                .unwrap();
            Self {
                endpoint,
                next: Some(server),
            }
        }

        #[cfg(unix)]
        fn new() -> Self {
            let dir = tempfile::tempdir().unwrap();
            let endpoint = dir.path().join("discord-ipc-0");
            let listener = tokio::net::UnixListener::bind(&endpoint).unwrap();
            Self {
                endpoint,
                listener,
                _dir: dir,
            }
        }

        #[cfg(windows)]
        async fn accept(&mut self) -> BoxedStream {
            use tokio::net::windows::named_pipe::ServerOptions;
            let server = self.next.take().unwrap();
            server.connect().await.unwrap();
            self.next = Some(ServerOptions::new().create(&self.endpoint).unwrap());
            Box::new(server)
        }

        #[cfg(unix)]
        async fn accept(&mut self) -> BoxedStream {
            let (stream, _) = self.listener.accept().await.unwrap();
            Box::new(stream)
        }
    }

    async fn handshake(stream: &mut BoxedStream, client_id: &str) {
        let frame = timeout(WAIT, read_frame(stream)).await.unwrap().unwrap();
        assert_eq!(frame.opcode, Opcode::Handshake);
        let hello = frame.payload_json().unwrap();
        assert_eq!(hello, json!({ "v": 1, "client_id": client_id }));
        let ready = json!({
            "cmd": "DISPATCH",
            "evt": "READY",
            "data": { "v": 1, "user": { "id": "7", "username": "ejder", "global_name": null, "avatar": null } },
            "nonce": null
        });
        write_frame(stream, &Frame::json(Opcode::Frame, &ready).unwrap())
            .await
            .unwrap();
    }

    /// Reads until the next `SET_ACTIVITY`, acknowledges it and returns its `args`.
    async fn expect_activity(stream: &mut BoxedStream) -> Value {
        loop {
            let frame = read_frame(stream).await.unwrap();
            if frame.opcode != Opcode::Frame {
                continue;
            }
            let message = frame.payload_json().unwrap();
            if message["cmd"] == "SET_ACTIVITY" {
                let ack = json!({
                    "cmd": "SET_ACTIVITY",
                    "data": message["args"]["activity"].clone(),
                    "evt": null,
                    "nonce": message["nonce"].clone()
                });
                write_frame(stream, &Frame::json(Opcode::Frame, &ack).unwrap())
                    .await
                    .unwrap();
                return message["args"].clone();
            }
        }
    }

    async fn wait_for(manager: &PresenceManager, expected: impl Fn(&PresenceStatus) -> bool) {
        let mut status = manager.subscribe();
        timeout(WAIT, async {
            loop {
                if expected(&status.borrow_and_update()) {
                    return;
                }
                status.changed().await.unwrap();
            }
        })
        .await
        .unwrap_or_else(|_| panic!("status stayed {:?}", manager.status()));
    }

    fn test_config(discord: &FakeDiscord, client_id: &str) -> PresenceConfig {
        PresenceConfig {
            endpoints: Some(vec![discord.endpoint.clone()]),
            reconnect_interval: Duration::from_millis(200),
            ..PresenceConfig::new(client_id)
        }
    }

    #[tokio::test]
    async fn publishes_reconnects_and_clears() {
        let mut discord = FakeDiscord::new();
        let manager = PresenceManager::start(test_config(&discord, "1234")).unwrap();
        assert_eq!(manager.status(), PresenceStatus::Idle);

        let activity = Activity::new()
            .details("Ana menüde")
            .state("Projects")
            .status_display(StatusDisplayType::Details);
        manager.set(Some(activity));

        let mut stream = timeout(WAIT, discord.accept()).await.unwrap();
        handshake(&mut stream, "1234").await;
        let args = timeout(WAIT, expect_activity(&mut stream)).await.unwrap();
        assert_eq!(args["pid"], json!(std::process::id()));
        assert_eq!(args["activity"]["details"], "Ana menüde");
        assert_eq!(args["activity"]["status_display_type"], 2);
        wait_for(&manager, |s| {
            matches!(s, PresenceStatus::Connected { user: Some(user) } if user.username == "ejder")
        })
        .await;

        // Pings are answered with the same payload.
        write_frame(
            &mut stream,
            &Frame::new(Opcode::Ping, b"{\"n\":1}".to_vec()),
        )
        .await
        .unwrap();
        let pong = timeout(WAIT, read_frame(&mut stream))
            .await
            .unwrap()
            .unwrap();
        assert_eq!(pong, Frame::new(Opcode::Pong, b"{\"n\":1}".to_vec()));

        // Discord quits and comes back: the activity is applied again.
        drop(stream);
        let mut stream = timeout(WAIT, discord.accept()).await.unwrap();
        handshake(&mut stream, "1234").await;
        let args = timeout(WAIT, expect_activity(&mut stream)).await.unwrap();
        assert_eq!(args["activity"]["details"], "Ana menüde");

        manager.clear();
        let args = timeout(WAIT, expect_activity(&mut stream)).await.unwrap();
        assert!(args.get("activity").is_none());

        // Disabling closes the connection.
        manager.set_enabled(false);
        wait_for(&manager, |s| *s == PresenceStatus::Disabled).await;
        let closed = timeout(WAIT, async {
            loop {
                if read_frame(&mut stream).await.is_err() {
                    return;
                }
            }
        })
        .await;
        assert!(closed.is_ok(), "the connection must be closed");
        manager.shutdown().await;
    }

    #[tokio::test]
    async fn updates_are_rate_limited_and_coalesced() {
        let mut discord = FakeDiscord::new();
        let window = Duration::from_millis(600);
        let config = PresenceConfig {
            max_updates: 2,
            update_window: window,
            ..test_config(&discord, "42")
        };
        let manager = PresenceManager::start(config).unwrap();
        manager.set(Some(Activity::new().details("güncelleme 0")));
        let mut stream = timeout(WAIT, discord.accept()).await.unwrap();
        handshake(&mut stream, "42").await;
        let first = timeout(WAIT, expect_activity(&mut stream)).await.unwrap();
        let mut seen = vec![(Instant::now(), first["activity"]["details"].clone())];

        let started = Instant::now();
        for i in 1..=6 {
            manager.set(Some(Activity::new().details(format!("güncelleme {i}"))));
            tokio::time::sleep(Duration::from_millis(30)).await;
        }
        while let Ok(args) =
            tokio::time::timeout_at(started + window * 2, expect_activity(&mut stream)).await
        {
            seen.push((Instant::now(), args["activity"]["details"].clone()));
        }

        assert_eq!(seen.last().unwrap().1, "güncelleme 6", "{seen:?}");
        assert!(
            seen.len() <= 3,
            "intermediate updates must be coalesced: {seen:?}"
        );
        for trio in seen.windows(3) {
            assert!(
                trio[2].0 - trio[0].0 >= window - Duration::from_millis(100),
                "more than two updates inside one window: {seen:?}"
            );
        }
        manager.shutdown().await;
    }

    #[tokio::test]
    async fn disabled_or_missing_discord() {
        let mut discord = FakeDiscord::new();
        let config = PresenceConfig {
            enabled: false,
            ..test_config(&discord, "42")
        };
        let manager = PresenceManager::start(config).unwrap();
        manager.set(Some(Activity::new().details("gizli")));
        assert!(
            timeout(Duration::from_millis(300), discord.accept())
                .await
                .is_err(),
            "a disabled manager must not connect"
        );
        assert_eq!(manager.status(), PresenceStatus::Disabled);
        manager.shutdown().await;

        let config = PresenceConfig {
            endpoints: Some(vec![std::env::temp_dir().join("pl-discord-nobody-here")]),
            ..PresenceConfig::new("42")
        };
        let manager = PresenceManager::start(config).unwrap();
        manager.set(Some(Activity::new().details("bekliyor")));
        wait_for(&manager, |s| *s == PresenceStatus::Unavailable).await;
        manager.shutdown().await;
        assert_eq!(manager.status(), PresenceStatus::Disabled);
    }

    #[test]
    fn limiter_window() {
        let start = Instant::now();
        let mut limiter = RateLimiter::new(2, Duration::from_secs(20));
        assert_eq!(limiter.next_slot(start), None);
        limiter.record(start);
        limiter.record(start + Duration::from_secs(1));
        assert_eq!(
            limiter.next_slot(start + Duration::from_secs(2)),
            Some(start + Duration::from_secs(20))
        );
        assert_eq!(limiter.next_slot(start + Duration::from_secs(20)), None);
    }

    #[test]
    fn start_requires_a_runtime() {
        assert!(matches!(
            PresenceManager::start(PresenceConfig::new("1")),
            Err(Error::NoRuntime)
        ));
    }

    /// Live check against the Discord desktop client: shows a sample presence for 10 s, then
    /// clears it. Passes without doing anything when Discord is not running.
    /// `cargo test -p pl-discord -- --ignored --nocapture live_presence`
    #[tokio::test]
    #[ignore = "needs a running Discord desktop client"]
    async fn live_presence() {
        const CLIENT_ID: &str = "1560400835589181552";
        let activity = Activity::new()
            .details("Projects Launcher v2")
            .details_url("https://projects.gg")
            .state("Discord RPC testi")
            .status_display(StatusDisplayType::Details)
            .started_now()
            .party("pl-discord-live-test", 1, 4)
            .button("Projects.gg", "https://projects.gg");

        // 1. Raw connection: Discord must acknowledge the activity, not reject it.
        let mut ipc = match DiscordIpc::connect(CLIENT_ID, &default_endpoints(), WAIT).await {
            Ok(ipc) => ipc,
            Err(error) => {
                println!("Discord is not available ({error}); skipping");
                return;
            }
        };
        println!(
            "connected via {} as {:?}",
            ipc.endpoint().display(),
            ipc.user()
        );
        let nonce = ipc.set_activity(Some(&activity)).await.unwrap();
        let answer = timeout(WAIT, async {
            loop {
                let frame = ipc.next_frame().await.unwrap().unwrap();
                match ipc.process(frame).await.unwrap() {
                    Incoming::Ack { nonce: Some(n), .. } if n == nonce => return Ok(()),
                    Incoming::Error { code, message, .. } => {
                        return Err(format!("{code}: {message}"));
                    }
                    _ => {}
                }
            }
        })
        .await
        .expect("Discord did not answer SET_ACTIVITY");
        assert_eq!(answer, Ok(()), "Discord rejected the activity");
        ipc.close().await;

        // 2. The manager shows it for 10 seconds, then clears it.
        let manager = PresenceManager::start(PresenceConfig::new(CLIENT_ID)).unwrap();
        manager.set(Some(activity));
        wait_for(&manager, |s| matches!(s, PresenceStatus::Connected { .. })).await;
        println!("presence visible: {:?}", manager.status());
        tokio::time::sleep(Duration::from_secs(10)).await;
        manager.clear();
        tokio::time::sleep(Duration::from_secs(1)).await;
        manager.shutdown().await;
        println!("presence cleared");
    }
}
