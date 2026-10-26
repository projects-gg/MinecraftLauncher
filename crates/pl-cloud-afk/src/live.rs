//! The live console socket (the contract's "Live console socket"): one socket per machine the
//! watched accounts are on, open only while a console is on screen. `watch` opens it (or adds the
//! account to the open one), `unwatch` of the last account closes it: nothing runs while no console
//! is watched. While open: `{"t":"ping"}` every 30-45 s (the Durable Object answers it without
//! waking), `keep` every 5 minutes (renews the console lease and checks the session), and a
//! reconnect when nothing came back within 20 s of a ping.
//!
//! Close codes: 4000 a newer socket of this player took over (not reopened by itself), 4001 the
//! session ended, 4004 the player's accounts there changed (reopened at once), 4008 too many
//! messages (a minute's pause); anything else reopens with backoff (2 s doubling to 5 minutes,
//! ±20 %).
//!
//! AFK+ (README "AFK+ live socket"): the AFK+ panel watches its account with `ai: true`, which
//! streams the account's AI state, feed and approvals as well (same lease). A console and the
//! panel watch the same account through one subscription: closing the panel while its console stays
//! on screen watches the account again without `ai`.
//!
//! The inventory (README "Inventory", CAFK-FEAT): a view on screen watches with `inv: true` and
//! stops it with `inv: false` (no new console snapshot); its moves go out as `invop` frames and
//! come back as `invres`, both on the same socket. The status strip (`bars`) comes with the console.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use futures::{SinkExt, StreamExt};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use tokio::sync::mpsc;
use tokio::time::{Instant, sleep_until, timeout};
use tokio_tungstenite::tungstenite::client::IntoClientRequest;
use tokio_tungstenite::tungstenite::http::HeaderValue;
use tokio_tungstenite::tungstenite::http::header::{AUTHORIZATION, RETRY_AFTER, USER_AGENT};
use tokio_tungstenite::tungstenite::protocol::frame::coding::CloseCode;
use tokio_tungstenite::tungstenite::protocol::{CloseFrame, WebSocketConfig};
use tokio_tungstenite::tungstenite::{self, Message};
use tokio_tungstenite::{Connector, connect_async_tls_with_config};
use url::Url;

use crate::client::Bearer;
use crate::feat::{
    CloudBarText, CloudBars, CloudBossBar, CloudInvError, CloudInvItem, CloudTitleBar, CloudWinOpen,
};
use crate::model::{CloudConsoleLine, CloudLiveState};
use crate::plus::{PlusAi, PlusApproval, PlusFeedItem};
use crate::tls;

const PING: &str = r#"{"t":"ping"}"#;
const KEEP: &str = r#"{"t":"keep"}"#;
const CONNECT_TIMEOUT: Duration = Duration::from_secs(10);
const PONG_TIMEOUT: Duration = Duration::from_secs(20);
const KEEP_EVERY: Duration = Duration::from_secs(5 * 60);
const BACKOFF_MIN: Duration = Duration::from_secs(2);
const BACKOFF_MAX: Duration = Duration::from_secs(5 * 60);
const TOO_MANY_PAUSE: Duration = Duration::from_secs(60);

/// How one machine's socket is doing.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CloudLiveStatus {
    Connecting,
    Live,
    /// Dropped; opening again after a pause.
    Reconnecting,
    /// A newer console of this player took over (another launcher or the site): not reopened by
    /// itself.
    Replaced,
    /// The friends session ended.
    Ended,
    /// The service refused this socket (no account of the player on that machine, an old
    /// launcher): not reopened by itself.
    Refused,
}

/// What the socket tells the launcher.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CloudLiveEvent {
    /// The socket of `host` is open; whether its machine is connected now.
    Hello { host: String, online: bool },
    /// A watched account's live state changed (also once right after `watch`).
    State { acct: String, live: CloudLiveState },
    /// Masked console lines of a watched account. `snapshot`: the recent lines right after
    /// `watch` (they replace what was shown); `skipped`: lines dropped in between.
    Lines {
        acct: String,
        lines: Vec<CloudConsoleLine>,
        skipped: u32,
        snapshot: bool,
    },
    /// The machine connected or went away.
    Machine { host: String, online: bool },
    /// The service said no to something: `not_found` (not the player's account there),
    /// `rate_limited`, `host_offline`, `bad_request`.
    Problem { acct: Option<String>, code: String },
    Status {
        host: String,
        status: CloudLiveStatus,
    },
    /// AFK+ (watched with `ai`): the account's AI state changed (also once right after `watch`).
    Ai { acct: String, ai: Option<PlusAi> },
    /// AFK+: feed items in batches at least 2 s apart; `snapshot`: the latest 20 right after
    /// `watch`.
    Feed {
        acct: String,
        items: Vec<PlusFeedItem>,
        snapshot: bool,
    },
    /// AFK+: an approval came, was answered, expired or closed.
    Approval {
        acct: String,
        approval: PlusApproval,
    },
    /// AFK+ (QoL Q1): the assistant left the owner a message: "Mesajlar" is read again.
    Messages { acct: String, count: u32, at: i64 },
    /// The inventory (watched with `inv`): whether it shows now (`state` `ok`, or `off` with
    /// `reason` `not_running`, `setting`, `restart`, `version` or `service`); `lock: "trade"`
    /// while an AFK+ trade holds it (the view stays live, moves wait).
    InvState {
        acct: String,
        state: String,
        reason: Option<String>,
        lock: Option<String>,
    },
    /// The player inventory, whole: the selected hotbar slot (0-8), the cursor's item and every
    /// non-empty slot; `snap`: the first frame after a watch.
    Inventory {
        acct: String,
        held: u32,
        cursor: Option<CloudInvItem>,
        items: Vec<CloudInvItem>,
        snap: bool,
    },
    /// The window the server opened (`open` and its own slots' `items`), or `closed`; `wseq`
    /// names this opening (every move names the one the player sees).
    Window {
        acct: String,
        wseq: i64,
        open: Option<CloudWinOpen>,
        items: Vec<CloudInvItem>,
        closed: bool,
    },
    /// The answer to a move this launcher sent (`id` its own): `ok` means the account's client
    /// sent it on; what the server did shows in the frames that follow.
    InvResult {
        id: String,
        ok: bool,
        error: Option<CloudInvError>,
    },
    /// A watched account's status strip: the action bar, a title and the boss bars (never in the
    /// log).
    Bars { acct: String, bars: CloudBars },
}

pub type LiveSink = Arc<dyn Fn(CloudLiveEvent) + Send + Sync>;

enum Command {
    /// Watch the account (again), with its AI and its inventory or without.
    Watch {
        acct: String,
        streams: Streams,
    },
    Unwatch(String),
    /// A move (`invop` frame, its `id`) for the account's machine.
    InvOp {
        id: String,
        frame: String,
    },
}

/// What a socket streams of a watched account besides its console: the AI (the AFK+ panel), the
/// inventory (a view on screen).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Streams {
    ai: bool,
    inv: bool,
}

/// Who watches an account: its console, the AFK+ panel, an inventory view, or several.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
struct Want {
    console: bool,
    ai: bool,
    inv: bool,
}

impl Want {
    fn any(self) -> bool {
        self.console || self.ai || self.inv
    }

    fn streams(self) -> Streams {
        Streams {
            ai: self.ai,
            inv: self.inv,
        }
    }
}

struct HostTask {
    generation: u64,
    commands: mpsc::UnboundedSender<Command>,
    watched: HashMap<String, Want>,
}

struct Hub {
    runtime: Handle,
    user_agent: String,
    sink: LiveSink,
    hosts: Mutex<HashMap<String, HostTask>>,
    next_generation: Mutex<u64>,
}

/// The live sockets. Cheap to clone.
#[derive(Clone)]
pub struct LiveHub {
    hub: Arc<Hub>,
}

impl std::fmt::Debug for LiveHub {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LiveHub").finish_non_exhaustive()
    }
}

impl LiveHub {
    pub fn new(runtime: Handle, user_agent: impl Into<String>, sink: LiveSink) -> Self {
        Self {
            hub: Arc::new(Hub {
                runtime,
                user_agent: user_agent.into(),
                sink,
                hosts: Mutex::new(HashMap::new()),
                next_generation: Mutex::new(0),
            }),
        }
    }

    /// Streams `acct`'s console and state from `host`'s socket at `url` (opened when it is not).
    pub fn watch(&self, url: Url, token: Bearer, host: &str, acct: &str) {
        self.start(url, token, host, acct, |want| want.console = true);
    }

    /// AFK+: streams `acct`'s AI state, feed and approvals too, while the AFK+ panel is on screen.
    pub fn watch_ai(&self, url: Url, token: Bearer, host: &str, acct: &str) {
        self.start(url, token, host, acct, |want| want.ai = true);
    }

    /// An inventory view is on screen: streams `acct`'s inventory too (`inv: true`).
    pub fn watch_inventory(&self, url: Url, token: Bearer, host: &str, acct: &str) {
        self.start(url, token, host, acct, |want| want.inv = true);
    }

    fn start(&self, url: Url, token: Bearer, host: &str, acct: &str, change: fn(&mut Want)) {
        let mut hosts = self.hub.hosts.lock();
        if let Some(task) = hosts.get_mut(host) {
            let mut want = task.watched.get(acct).copied().unwrap_or_default();
            change(&mut want);
            let command = Command::Watch {
                acct: acct.to_owned(),
                streams: want.streams(),
            };
            if task.commands.send(command).is_ok() {
                task.watched.insert(acct.to_owned(), want);
                return;
            }
        }
        let generation = {
            let mut next = self.hub.next_generation.lock();
            *next += 1;
            *next
        };
        let (commands, receiver) = mpsc::unbounded_channel();
        let mut want = Want::default();
        change(&mut want);
        let watched: HashMap<String, Streams> = [(acct.to_owned(), want.streams())].into();
        hosts.insert(
            host.to_owned(),
            HostTask {
                generation,
                commands,
                watched: [(acct.to_owned(), want)].into(),
            },
        );
        let hub = self.hub.clone();
        let host = host.to_owned();
        self.hub.runtime.spawn(async move {
            let endpoint = Endpoint {
                url,
                token,
                user_agent: hub.user_agent.clone(),
            };
            run(&endpoint, &host, watched, receiver, &hub.sink).await;
            let mut hosts = hub.hosts.lock();
            if hosts
                .get(&host)
                .is_some_and(|task| task.generation == generation)
            {
                hosts.remove(&host);
            }
        });
    }

    /// The console left the screen; the socket closes with its last watched account.
    pub fn unwatch(&self, acct: &str) {
        self.stop(acct, |want| want.console = false);
    }

    /// The AFK+ panel left the screen.
    pub fn unwatch_ai(&self, acct: &str) {
        self.stop(acct, |want| want.ai = false);
    }

    /// The inventory view left the screen (`inv: false`, or the socket closes with it).
    pub fn unwatch_inventory(&self, acct: &str) {
        self.stop(acct, |want| want.inv = false);
    }

    /// Sends a move (`op`, the app's `InvOp`, shape already checked) for `acct` with the app's own
    /// `id`; its answer comes back as [`CloudLiveEvent::InvResult`]. False when no socket streams
    /// that account's inventory (the caller answers `host_offline`).
    pub fn inv_op(&self, acct: &str, id: &str, op: &serde_json::Value) -> bool {
        let frame =
            serde_json::json!({ "t": "invop", "acct": acct, "id": id, "op": op }).to_string();
        let hosts = self.hub.hosts.lock();
        hosts
            .values()
            .find(|task| task.watched.get(acct).is_some_and(|want| want.inv))
            .is_some_and(|task| {
                task.commands
                    .send(Command::InvOp {
                        id: id.to_owned(),
                        frame,
                    })
                    .is_ok()
            })
    }

    fn stop(&self, acct: &str, change: fn(&mut Want)) {
        let mut hosts = self.hub.hosts.lock();
        let mut emptied = None;
        for (host, task) in hosts.iter_mut() {
            let Some(want) = task.watched.get_mut(acct) else {
                continue;
            };
            let before = *want;
            change(want);
            if *want == before {
                continue;
            }
            if want.any() {
                // Still watched, now without its AI or its inventory (or the other way round).
                let _ = task.commands.send(Command::Watch {
                    acct: acct.to_owned(),
                    streams: want.streams(),
                });
                continue;
            }
            task.watched.remove(acct);
            let _ = task.commands.send(Command::Unwatch(acct.to_owned()));
            if task.watched.is_empty() {
                emptied = Some(host.clone());
            }
        }
        if let Some(host) = emptied {
            // The task closes the socket and ends by itself; a new watch starts a new one.
            hosts.remove(&host);
        }
    }

    /// Closes every socket (signed out, the service gone).
    pub fn stop_all(&self) {
        // Dropping the senders ends the tasks: they close their sockets.
        self.hub.hosts.lock().clear();
    }

    /// Whether any console is watched.
    pub fn is_watching(&self) -> bool {
        !self.hub.hosts.lock().is_empty()
    }
}

struct Endpoint {
    url: Url,
    token: Bearer,
    user_agent: String,
}

type Socket =
    tokio_tungstenite::WebSocketStream<tokio_tungstenite::MaybeTlsStream<tokio::net::TcpStream>>;

/// How a connection (or an attempt) ended.
enum Outcome {
    /// Open again after at least this long (zero: with the usual backoff).
    Retry(Duration),
    /// Open again at once.
    Now,
    Stop(CloudLiveStatus),
    /// Nobody watches any more.
    Done,
}

impl Endpoint {
    async fn connect(&self) -> Result<Socket, Outcome> {
        let mut request = self
            .url
            .as_str()
            .into_client_request()
            .map_err(|_| Outcome::Stop(CloudLiveStatus::Refused))?;
        let headers = request.headers_mut();
        let bearer = HeaderValue::from_str(&format!("Bearer {}", self.token.expose()))
            .map_err(|_| Outcome::Stop(CloudLiveStatus::Ended))?;
        headers.insert(AUTHORIZATION, bearer);
        if let Ok(agent) = HeaderValue::from_str(&self.user_agent) {
            headers.insert(USER_AGENT, agent);
        }
        let connector = if self.url.scheme() == "wss" {
            match tls::client_config() {
                Ok(config) => Connector::Rustls(config),
                Err(error) => {
                    tracing::warn!(%error, "live console TLS setup failed");
                    return Err(Outcome::Retry(BACKOFF_MAX));
                }
            }
        } else {
            Connector::Plain
        };
        let config = WebSocketConfig::default()
            .max_message_size(Some(1 << 20))
            .max_frame_size(Some(1 << 20));
        let attempt = connect_async_tls_with_config(request, Some(config), true, Some(connector));
        match timeout(CONNECT_TIMEOUT, attempt).await {
            Ok(Ok((socket, _))) => Ok(socket),
            Ok(Err(tungstenite::Error::Http(response))) => {
                let wait = response
                    .headers()
                    .get(RETRY_AFTER)
                    .and_then(|value| value.to_str().ok())
                    .and_then(|value| value.trim().parse::<u64>().ok())
                    .map(Duration::from_secs);
                Err(handshake_outcome(response.status().as_u16(), wait))
            }
            Ok(Err(error)) => {
                tracing::debug!(%error, "live console connect failed");
                Err(Outcome::Retry(Duration::ZERO))
            }
            Err(_) => Err(Outcome::Retry(Duration::ZERO)),
        }
    }
}

fn handshake_outcome(status: u16, retry_after: Option<Duration>) -> Outcome {
    match status {
        401 => Outcome::Stop(CloudLiveStatus::Ended),
        // Not the player's machine, a refused origin, an old protocol: asking again will not help.
        403 | 404 | 426 => Outcome::Stop(CloudLiveStatus::Refused),
        429 | 503 => Outcome::Retry(
            retry_after
                .unwrap_or(TOO_MANY_PAUSE)
                .clamp(BACKOFF_MIN, Duration::from_secs(3_600)),
        ),
        500..=599 => Outcome::Retry(Duration::ZERO),
        _ => Outcome::Retry(BACKOFF_MAX),
    }
}

fn close_outcome(frame: Option<&CloseFrame>) -> Outcome {
    match frame.map(|frame| u16::from(frame.code)) {
        Some(4000) => Outcome::Stop(CloudLiveStatus::Replaced),
        Some(4001) => Outcome::Stop(CloudLiveStatus::Ended),
        Some(4004) => Outcome::Now,
        Some(4008) => Outcome::Retry(TOO_MANY_PAUSE),
        _ => Outcome::Retry(Duration::ZERO),
    }
}

/// ±20 % of `base`.
fn jitter(base: Duration) -> Duration {
    let factor = 0.8 + f64::from(rand::random::<u16>()) / f64::from(u16::MAX) * 0.4;
    base.mul_f64(factor)
}

/// 30-45 s.
fn ping_wait() -> Duration {
    Duration::from_secs(30) + Duration::from_millis(u64::from(rand::random::<u16>()) % 15_000)
}

#[derive(Deserialize)]
#[serde(tag = "t", rename_all = "lowercase")]
enum Incoming {
    Hello {
        #[serde(default)]
        host: HelloHost,
    },
    St {
        acct: String,
        #[serde(default)]
        live: CloudLiveState,
    },
    Con {
        acct: String,
        #[serde(default)]
        lines: Vec<CloudConsoleLine>,
        #[serde(default)]
        skipped: u32,
        #[serde(default)]
        snapshot: bool,
    },
    Host {
        #[serde(default)]
        online: bool,
    },
    Error {
        #[serde(default)]
        code: String,
        #[serde(default)]
        acct: Option<String>,
    },
    Aist {
        acct: String,
        #[serde(default)]
        ai: Option<PlusAi>,
    },
    Aifeed {
        acct: String,
        #[serde(default)]
        items: Vec<PlusFeedItem>,
        #[serde(default)]
        snapshot: bool,
    },
    Aiapproval {
        acct: String,
        approval: PlusApproval,
    },
    Aimsg {
        acct: String,
        #[serde(default)]
        count: u32,
        #[serde(default)]
        at: i64,
    },
    Invst {
        acct: String,
        #[serde(default)]
        state: String,
        #[serde(default)]
        reason: Option<String>,
        #[serde(default)]
        lock: Option<String>,
    },
    Inv {
        acct: String,
        #[serde(default)]
        held: u32,
        #[serde(default)]
        cursor: Option<CloudInvItem>,
        #[serde(default)]
        items: Vec<CloudInvItem>,
        #[serde(default)]
        snap: bool,
    },
    Win {
        acct: String,
        #[serde(default)]
        wseq: i64,
        #[serde(default)]
        open: Option<CloudWinOpen>,
        #[serde(default)]
        items: Vec<CloudInvItem>,
        #[serde(default)]
        closed: bool,
    },
    Invres {
        #[serde(default)]
        id: String,
        #[serde(default)]
        ok: bool,
        #[serde(default)]
        error: Option<CloudInvError>,
    },
    Bars {
        acct: String,
        #[serde(default)]
        actionbar: Option<CloudBarText>,
        #[serde(default)]
        title: Option<CloudTitleBar>,
        #[serde(default)]
        boss: Vec<CloudBossBar>,
    },
    Bye,
    Pong,
    #[serde(other)]
    Other,
}

impl Incoming {
    /// The inventory's and the strip's messages as events (the others are matched where they
    /// come).
    fn feature_event(self) -> Option<CloudLiveEvent> {
        Some(match self {
            Self::Invst {
                acct,
                state,
                reason,
                lock,
            } => CloudLiveEvent::InvState {
                acct,
                state,
                reason,
                lock,
            },
            Self::Inv {
                acct,
                held,
                cursor,
                items,
                snap,
            } => CloudLiveEvent::Inventory {
                acct,
                held,
                cursor,
                items,
                snap,
            },
            Self::Win {
                acct,
                wseq,
                open,
                items,
                closed,
            } => CloudLiveEvent::Window {
                acct,
                wseq,
                open,
                items,
                closed,
            },
            Self::Invres { id, ok, error } => CloudLiveEvent::InvResult {
                id,
                ok: ok && error.is_none(),
                error,
            },
            Self::Bars {
                acct,
                actionbar,
                title,
                boss,
            } => CloudLiveEvent::Bars {
                acct,
                bars: CloudBars {
                    actionbar,
                    title,
                    boss,
                },
            },
            _ => return None,
        })
    }
}

/// A move that could not go out (the machine's socket is away).
fn unsent(id: String) -> CloudLiveEvent {
    CloudLiveEvent::InvResult {
        id,
        ok: false,
        error: Some(CloudInvError::new("host_offline")),
    }
}

#[derive(Default, Deserialize)]
#[serde(default)]
struct HelloHost {
    online: bool,
}

async fn send_text(socket: &mut Socket, text: String) -> bool {
    socket.send(Message::text(text)).await.is_ok()
}

fn watch_frame(t: &str, acct: &str) -> String {
    serde_json::json!({ "t": t, "acct": acct }).to_string()
}

/// `watch`, with `ai: true` for the AFK+ panel.
#[cfg(test)]
fn watch_ai_frame(acct: &str, ai: bool) -> String {
    watch_streams_frame(acct, ai, None)
}

/// `watch` with `ai: true` for the AFK+ panel, and `inv` when the inventory stream starts
/// (`true`) or stops (`false`; left out, it stays as it was).
fn watch_streams_frame(acct: &str, ai: bool, inv: Option<bool>) -> String {
    let mut frame = serde_json::json!({ "t": "watch", "acct": acct });
    if ai {
        frame["ai"] = serde_json::Value::Bool(true);
    }
    if let Some(inv) = inv {
        frame["inv"] = serde_json::Value::Bool(inv);
    }
    frame.to_string()
}

/// One machine's socket until nobody watches or it must not reopen.
async fn run(
    endpoint: &Endpoint,
    host: &str,
    mut watched: HashMap<String, Streams>,
    mut commands: mpsc::UnboundedReceiver<Command>,
    sink: &LiveSink,
) {
    let status = |status: CloudLiveStatus| {
        sink(CloudLiveEvent::Status {
            host: host.to_owned(),
            status,
        });
    };
    let mut backoff = BACKOFF_MIN;
    let mut first = true;
    loop {
        status(if first {
            CloudLiveStatus::Connecting
        } else {
            CloudLiveStatus::Reconnecting
        });
        first = false;
        let outcome = match endpoint.connect().await {
            Ok(mut socket) => {
                let outcome = connected(
                    &mut socket,
                    host,
                    &mut watched,
                    &mut commands,
                    sink,
                    &mut backoff,
                )
                .await;
                let _ = socket.close(None).await;
                outcome
            }
            Err(outcome) => outcome,
        };
        let wait = match outcome {
            Outcome::Done => return,
            Outcome::Stop(why) => {
                status(why);
                return;
            }
            Outcome::Now => Duration::ZERO,
            Outcome::Retry(at_least) => {
                let wait = jitter(backoff).max(at_least);
                backoff = (backoff * 2).min(BACKOFF_MAX);
                wait
            }
        };
        if wait.is_zero() {
            continue;
        }
        status(CloudLiveStatus::Reconnecting);
        // Wait, but follow the watchers meanwhile: the last one leaving ends it.
        let until = Instant::now() + wait;
        loop {
            tokio::select! {
                () = sleep_until(until) => break,
                command = commands.recv() => match command {
                    None => return,
                    Some(Command::Watch { acct, streams }) => { watched.insert(acct, streams); }
                    Some(Command::Unwatch(acct)) => {
                        watched.remove(&acct);
                        if watched.is_empty() {
                            return;
                        }
                    }
                    // Never queued for later: a move made now must not land in a later state.
                    Some(Command::InvOp { id, .. }) => sink(unsent(id)),
                },
            }
        }
    }
}

async fn connected(
    socket: &mut Socket,
    host: &str,
    watched: &mut HashMap<String, Streams>,
    commands: &mut mpsc::UnboundedReceiver<Command>,
    sink: &LiveSink,
    backoff: &mut Duration,
) -> Outcome {
    // What came while the socket opened: the watchers are followed, and a move made meanwhile is
    // answered as not sent, never sent late against a state it was not made for.
    loop {
        match commands.try_recv() {
            Ok(Command::Watch { acct, streams }) => {
                watched.insert(acct, streams);
            }
            Ok(Command::Unwatch(acct)) => {
                watched.remove(&acct);
            }
            Ok(Command::InvOp { id, .. }) => sink(unsent(id)),
            Err(mpsc::error::TryRecvError::Empty) => break,
            Err(mpsc::error::TryRecvError::Disconnected) => return Outcome::Done,
        }
    }
    if watched.is_empty() {
        return Outcome::Done;
    }
    for (acct, streams) in watched.iter() {
        let frame = watch_streams_frame(acct, streams.ai, streams.inv.then_some(true));
        if !send_text(socket, frame).await {
            return Outcome::Retry(Duration::ZERO);
        }
    }
    let mut next_ping = Instant::now() + ping_wait();
    let mut next_keep = Instant::now() + KEEP_EVERY;
    let mut pong_deadline: Option<Instant> = None;
    loop {
        let deadline = pong_deadline.unwrap_or(next_ping + Duration::from_secs(3_600));
        tokio::select! {
            frame = socket.next() => match frame {
                Some(Ok(Message::Text(text))) => {
                    pong_deadline = None;
                    match serde_json::from_str::<Incoming>(text.as_str()) {
                        Ok(Incoming::Hello { host: hello }) => {
                            *backoff = BACKOFF_MIN;
                            sink(CloudLiveEvent::Status { host: host.to_owned(), status: CloudLiveStatus::Live });
                            sink(CloudLiveEvent::Hello { host: host.to_owned(), online: hello.online });
                        }
                        Ok(Incoming::St { acct, live }) => sink(CloudLiveEvent::State { acct, live }),
                        Ok(Incoming::Con { acct, lines, skipped, snapshot }) => {
                            sink(CloudLiveEvent::Lines { acct, lines, skipped, snapshot });
                        }
                        Ok(Incoming::Host { online }) => {
                            sink(CloudLiveEvent::Machine { host: host.to_owned(), online });
                        }
                        Ok(Incoming::Error { code, acct }) => sink(CloudLiveEvent::Problem { acct, code }),
                        Ok(Incoming::Aist { acct, ai }) => sink(CloudLiveEvent::Ai { acct, ai }),
                        Ok(Incoming::Aifeed { acct, items, snapshot }) => {
                            sink(CloudLiveEvent::Feed { acct, items, snapshot });
                        }
                        Ok(Incoming::Aiapproval { acct, approval }) => {
                            sink(CloudLiveEvent::Approval { acct, approval });
                        }
                        Ok(Incoming::Aimsg { acct, count, at }) => {
                            sink(CloudLiveEvent::Messages { acct, count, at });
                        }
                        Ok(Incoming::Bye | Incoming::Pong | Incoming::Other) => {}
                        Ok(message) => {
                            if let Some(event) = message.feature_event() {
                                sink(event);
                            }
                        }
                        Err(error) => tracing::debug!(%error, "unreadable live console message"),
                    }
                }
                Some(Ok(Message::Close(frame))) => return close_outcome(frame.as_ref()),
                Some(Ok(_)) => pong_deadline = None,
                Some(Err(error)) => {
                    tracing::debug!(%error, "live console connection error");
                    return Outcome::Retry(Duration::ZERO);
                }
                None => return Outcome::Retry(Duration::ZERO),
            },
            command = commands.recv() => match command {
                None => {
                    let _ = socket
                        .send(Message::Close(Some(CloseFrame {
                            code: CloseCode::Normal,
                            reason: "hidden".into(),
                        })))
                        .await;
                    return Outcome::Done;
                }
                Some(Command::Watch { acct, streams }) => {
                    let before = watched.insert(acct.clone(), streams);
                    let sent = match before {
                        Some(same) if same == streams => true,
                        // The AI's stream ends only with the subscription: watch again without it
                        // (the inventory's comes back with the new subscription).
                        Some(old) if old.ai && !streams.ai => {
                            let again =
                                watch_streams_frame(&acct, false, streams.inv.then_some(true));
                            send_text(socket, watch_frame("unwatch", &acct)).await
                                && send_text(socket, again).await
                        }
                        // The inventory's stream starts or stops on the same subscription.
                        Some(old) => {
                            let inv = (old.inv != streams.inv).then_some(streams.inv);
                            send_text(socket, watch_streams_frame(&acct, streams.ai, inv)).await
                        }
                        None => {
                            let frame =
                                watch_streams_frame(&acct, streams.ai, streams.inv.then_some(true));
                            send_text(socket, frame).await
                        }
                    };
                    if !sent {
                        return Outcome::Retry(Duration::ZERO);
                    }
                }
                Some(Command::InvOp { id, frame }) => {
                    if !send_text(socket, frame).await {
                        sink(unsent(id));
                        return Outcome::Retry(Duration::ZERO);
                    }
                }
                Some(Command::Unwatch(acct)) => {
                    if watched.remove(&acct).is_some() {
                        let _ = send_text(socket, watch_frame("unwatch", &acct)).await;
                    }
                    if watched.is_empty() {
                        let _ = socket
                            .send(Message::Close(Some(CloseFrame {
                                code: CloseCode::Normal,
                                reason: "hidden".into(),
                            })))
                            .await;
                        return Outcome::Done;
                    }
                }
            },
            () = sleep_until(next_ping) => {
                // Exactly these bytes: the Durable Object answers them without waking.
                if !send_text(socket, PING.to_owned()).await {
                    return Outcome::Retry(Duration::ZERO);
                }
                if pong_deadline.is_none() {
                    pong_deadline = Some(Instant::now() + PONG_TIMEOUT);
                }
                next_ping = Instant::now() + ping_wait();
            }
            () = sleep_until(next_keep) => {
                if !watched.is_empty() && !send_text(socket, KEEP.to_owned()).await {
                    return Outcome::Retry(Duration::ZERO);
                }
                next_keep = Instant::now() + KEEP_EVERY;
            }
            () = sleep_until(deadline), if pong_deadline.is_some() => {
                // Nothing came back: the line is dead even if the socket says otherwise.
                return Outcome::Retry(Duration::ZERO);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex as StdMutex;

    use tokio::net::TcpListener;
    use tokio_tungstenite::accept_hdr_async;
    use tokio_tungstenite::tungstenite::handshake::server::{ErrorResponse, Request, Response};

    use super::*;

    #[test]
    fn handshakes_and_closes_decide_what_comes_next() {
        assert!(matches!(
            handshake_outcome(401, None),
            Outcome::Stop(CloudLiveStatus::Ended)
        ));
        assert!(matches!(
            handshake_outcome(404, None),
            Outcome::Stop(CloudLiveStatus::Refused)
        ));
        assert!(matches!(
            handshake_outcome(429, Some(Duration::from_secs(30))),
            Outcome::Retry(wait) if wait == Duration::from_secs(30)
        ));
        assert!(matches!(handshake_outcome(502, None), Outcome::Retry(wait) if wait.is_zero()));
        let frame = |code: u16| CloseFrame {
            code: CloseCode::from(code),
            reason: "".into(),
        };
        assert!(matches!(
            close_outcome(Some(&frame(4000))),
            Outcome::Stop(CloudLiveStatus::Replaced)
        ));
        assert!(matches!(
            close_outcome(Some(&frame(4001))),
            Outcome::Stop(CloudLiveStatus::Ended)
        ));
        assert!(matches!(close_outcome(Some(&frame(4004))), Outcome::Now));
        assert!(
            matches!(close_outcome(Some(&frame(4008))), Outcome::Retry(wait) if wait == TOO_MANY_PAUSE)
        );
        assert!(matches!(close_outcome(None), Outcome::Retry(wait) if wait.is_zero()));
    }

    #[test]
    fn reads_the_service_messages() {
        let parse = |text: &str| serde_json::from_str::<Incoming>(text).unwrap();
        assert!(matches!(
            parse(r#"{"t":"hello","v":1,"host":{"id":"kz1","online":true},"accts":["a_1"],"leaseSeconds":900,"now":1}"#),
            Incoming::Hello { host } if host.online
        ));
        assert!(matches!(
            parse(
                r#"{"t":"con","acct":"a_1","lines":[{"at":1,"text":"/login ******","k":"s"}],"snapshot":true}"#
            ),
            Incoming::Con {
                snapshot: true,
                skipped: 0,
                ..
            }
        ));
        assert!(matches!(parse(r#"{"t":"pong"}"#), Incoming::Pong));
        assert!(matches!(
            parse(r#"{"t":"aist","acct":"a_1","ai":{"state":"on","pendingApprovals":2}}"#),
            Incoming::Aist { ai: Some(ai), .. } if ai.pending_approvals == 2
        ));
        assert!(matches!(
            parse(r#"{"t":"aifeed","acct":"a_1","items":[{"id":"01J","at":5,"kind":"pm_in","counterpart":"Kaan_07","text":"elytra var 45k"}],"snapshot":true}"#),
            Incoming::Aifeed { snapshot: true, items, .. } if items[0].kind == "pm_in"
        ));
        assert!(matches!(
            parse(r#"{"t":"aiapproval","acct":"a_1","approval":{"id":"01J","state":"expired"}}"#),
            Incoming::Aiapproval { approval, .. } if approval.state == "expired"
        ));
        assert!(matches!(
            parse(r#"{"t":"aimsg","acct":"a_1","count":2,"at":9}"#),
            Incoming::Aimsg {
                count: 2,
                at: 9,
                ..
            }
        ));
        assert!(matches!(parse(r#"{"t":"later","x":1}"#), Incoming::Other));
    }

    /// A one-socket stand-in for the service: checks the Bearer, answers `hello`, a snapshot for
    /// each `watch`, and reports what the client sent.
    #[allow(clippy::result_large_err)] // tungstenite's handshake callback returns its response
    async fn fake_service(sent: Arc<StdMutex<Vec<String>>>) -> Url {
        let listener = TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        tokio::spawn(async move {
            while let Ok((stream, _)) = listener.accept().await {
                let sent = sent.clone();
                tokio::spawn(async move {
                    let check = |request: &Request,
                                 response: Response|
                     -> Result<Response, ErrorResponse> {
                        let bearer = request
                            .headers()
                            .get(AUTHORIZATION)
                            .and_then(|v| v.to_str().ok());
                        assert_eq!(bearer, Some("Bearer pls_test"));
                        assert_eq!(request.uri().query(), Some("host=kz1"));
                        Ok(response)
                    };
                    let mut socket = accept_hdr_async(stream, check).await.unwrap();
                    socket
                        .send(Message::text(
                            r#"{"t":"hello","v":1,"host":{"id":"kz1","online":true},"accts":["a_1"],"leaseSeconds":900,"now":1}"#,
                        ))
                        .await
                        .unwrap();
                    while let Some(Ok(message)) = socket.next().await {
                        let Message::Text(text) = message else {
                            continue;
                        };
                        sent.lock().unwrap().push(text.to_string());
                        if text.contains(r#""t":"watch""#) {
                            socket
                                .send(Message::text(
                                    r#"{"t":"con","acct":"a_1","lines":[{"at":5,"text":"Hoş geldin","k":"n"}],"snapshot":true}"#,
                                ))
                                .await
                                .unwrap();
                        }
                    }
                });
            }
        });
        Url::parse(&format!("ws://{address}/api/v1/live?host=kz1")).unwrap()
    }

    #[tokio::test]
    async fn watches_while_a_console_is_open_and_closes_after() {
        let sent = Arc::new(StdMutex::new(Vec::new()));
        let url = fake_service(sent.clone()).await;
        let (events_tx, mut events) = mpsc::unbounded_channel();
        let sink: LiveSink = Arc::new(move |event| {
            let _ = events_tx.send(event);
        });
        let hub = LiveHub::new(Handle::current(), "test", sink);
        hub.watch(url, Bearer::new("pls_test"), "kz1", "a_1");
        assert!(hub.is_watching());
        let mut seen = Vec::new();
        while let Some(event) = tokio::time::timeout(Duration::from_secs(5), events.recv())
            .await
            .unwrap()
        {
            let done = matches!(event, CloudLiveEvent::Lines { .. });
            seen.push(event);
            if done {
                break;
            }
        }
        assert!(seen.contains(&CloudLiveEvent::Status {
            host: "kz1".into(),
            status: CloudLiveStatus::Live
        }));
        assert!(seen.contains(&CloudLiveEvent::Hello {
            host: "kz1".into(),
            online: true
        }));
        assert!(seen.iter().any(|event| matches!(
            event,
            CloudLiveEvent::Lines { acct, snapshot: true, lines, .. } if acct == "a_1" && lines[0].text == "Hoş geldin"
        )));
        hub.unwatch("a_1");
        assert!(!hub.is_watching());
        // The service heard the watch and the unwatch, and nothing else is scheduled.
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                if sent
                    .lock()
                    .unwrap()
                    .iter()
                    .any(|text| text.contains(r#""t":"unwatch""#))
                {
                    break;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap();
        let sent = sent.lock().unwrap().clone();
        assert_eq!(sent[0], watch_frame("watch", "a_1"));
    }

    /// Waits (at most 5 s) until the service heard `count` frames.
    async fn heard(sent: &Arc<StdMutex<Vec<String>>>, count: usize) -> Vec<String> {
        tokio::time::timeout(Duration::from_secs(5), async {
            loop {
                let frames = sent.lock().unwrap().clone();
                if frames.len() >= count {
                    return frames;
                }
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
        })
        .await
        .unwrap()
    }

    #[tokio::test]
    async fn the_afk_plus_panel_watches_with_ai_and_leaves_its_console_watched() {
        let sent = Arc::new(StdMutex::new(Vec::new()));
        let url = fake_service(sent.clone()).await;
        let sink: LiveSink = Arc::new(|_| {});
        let hub = LiveHub::new(Handle::current(), "test", sink);
        hub.watch(url.clone(), Bearer::new("pls_test"), "kz1", "a_1");
        let frames = heard(&sent, 1).await;
        assert_eq!(frames[0], watch_frame("watch", "a_1"));
        // The panel opens on the open socket: the same subscription gains the AI.
        hub.watch_ai(url, Bearer::new("pls_test"), "kz1", "a_1");
        let frames = heard(&sent, 2).await;
        assert_eq!(frames[1], watch_ai_frame("a_1", true));
        // The panel closes, the console stays: watched again without the AI.
        hub.unwatch_ai("a_1");
        let frames = heard(&sent, 4).await;
        assert_eq!(frames[2], watch_frame("unwatch", "a_1"));
        assert_eq!(frames[3], watch_frame("watch", "a_1"));
        assert!(hub.is_watching());
        hub.unwatch("a_1");
        assert!(!hub.is_watching());
        let frames = heard(&sent, 5).await;
        assert_eq!(frames[4], watch_frame("unwatch", "a_1"));
    }

    #[test]
    fn reads_the_inventory_and_the_strip() {
        let event = |text: &str| {
            serde_json::from_str::<Incoming>(text)
                .unwrap()
                .feature_event()
                .unwrap()
        };
        assert_eq!(
            event(r#"{"t":"invst","acct":"a_1","state":"ok","lock":"trade"}"#),
            CloudLiveEvent::InvState {
                acct: "a_1".into(),
                state: "ok".into(),
                reason: None,
                lock: Some("trade".into()),
            }
        );
        let CloudLiveEvent::Inventory {
            held,
            cursor,
            items,
            snap,
            ..
        } = event(
            r#"{"t":"inv","acct":"a_1","held":2,"cursor":null,"snap":true,
                "items":[{"slot":36,"id":"minecraft:bread","count":12,"label":{"tr":"Ekmek","en":"Bread"}}]}"#,
        )
        else {
            panic!("not an inventory");
        };
        assert_eq!((held, cursor, snap), (2, None, true));
        assert_eq!(items[0].label.tr, "Ekmek");
        assert_eq!(
            event(
                r#"{"t":"win","acct":"a_1","wseq":1792000000123,"open":{"type":"generic_9x3","title":"Kitler","size":27},"items":[]}"#
            ),
            CloudLiveEvent::Window {
                acct: "a_1".into(),
                wseq: 1_792_000_000_123,
                open: Some(CloudWinOpen {
                    kind: "generic_9x3".into(),
                    title: "Kitler".into(),
                    size: 27,
                }),
                items: vec![],
                closed: false,
            }
        );
        assert_eq!(
            event(r#"{"t":"invres","id":"m1","error":{"code":"rate_limited","retryAfter":2}}"#),
            CloudLiveEvent::InvResult {
                id: "m1".into(),
                ok: false,
                error: Some(CloudInvError {
                    code: "rate_limited".into(),
                    retry_after: Some(2),
                    ..CloudInvError::default()
                }),
            }
        );
        let CloudLiveEvent::Bars { bars, .. } = event(
            r#"{"t":"bars","acct":"a_1","actionbar":{"text":"Günlük ödüle 2 saat","untilMs":5},"title":null,
                "boss":[{"id":"b1","text":"Etkinlik","progress":0.4,"color":"pink"}]}"#,
        ) else {
            panic!("not a strip");
        };
        assert_eq!(bars.actionbar.unwrap().text, "Günlük ödüle 2 saat");
        assert_eq!(bars.boss[0].progress, 0.4);
        assert!(
            serde_json::from_str::<Incoming>(r#"{"t":"pong"}"#)
                .unwrap()
                .feature_event()
                .is_none()
        );
    }

    #[tokio::test]
    async fn the_inventory_streams_on_the_console_subscription_and_moves_go_out() {
        let sent = Arc::new(StdMutex::new(Vec::new()));
        let url = fake_service(sent.clone()).await;
        let (events_tx, mut events) = mpsc::unbounded_channel();
        let sink: LiveSink = Arc::new(move |event| {
            let _ = events_tx.send(event);
        });
        let hub = LiveHub::new(Handle::current(), "test", sink);
        let snapshot = serde_json::json!({ "action": "snapshot" });
        assert!(!hub.inv_op("a_1", "m0", &snapshot), "nothing streams it");
        hub.watch(url.clone(), Bearer::new("pls_test"), "kz1", "a_1");
        hub.watch_inventory(url, Bearer::new("pls_test"), "kz1", "a_1");
        // Both came while the socket opened: its first frame asks for both.
        let frames = heard(&sent, 1).await;
        assert_eq!(frames[0], watch_streams_frame("a_1", false, Some(true)));
        assert!(hub.inv_op("a_1", "m1", &snapshot));
        let frames = heard(&sent, 2).await;
        let op: serde_json::Value = serde_json::from_str(&frames[1]).unwrap();
        assert_eq!(
            op,
            serde_json::json!({"t": "invop", "acct": "a_1", "id": "m1", "op": {"action": "snapshot"}})
        );
        // The view leaves, the console stays: the stream stops on the same subscription.
        hub.unwatch_inventory("a_1");
        let frames = heard(&sent, 3).await;
        assert_eq!(frames[2], watch_streams_frame("a_1", false, Some(false)));
        assert!(!hub.inv_op("a_1", "m2", &snapshot));
        assert!(hub.is_watching());
        hub.unwatch("a_1");
        assert!(!hub.is_watching());
        let frames = heard(&sent, 4).await;
        assert_eq!(frames[3], watch_frame("unwatch", "a_1"));
        // Nothing answered a move here, and nothing failed.
        while let Ok(event) = events.try_recv() {
            assert!(!matches!(event, CloudLiveEvent::InvResult { .. }));
        }
    }

    #[test]
    fn pings_are_exactly_the_bytes_the_service_answers() {
        assert_eq!(PING, r#"{"t":"ping"}"#);
        let wait = ping_wait();
        assert!(wait >= Duration::from_secs(30) && wait < Duration::from_secs(45));
        let spread = jitter(Duration::from_secs(10));
        assert!(spread >= Duration::from_secs(8) && spread <= Duration::from_secs(12));
    }
}
