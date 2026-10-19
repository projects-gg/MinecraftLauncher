//! One account's live session: an actor task that owns the client process.
//!
//! All state changes happen on the actor task, in the order commands, client output and timers
//! arrive; the manager and the UI read a snapshot behind a short lock and receive [`AfkEvent`]s.
//!
//! Launch: re-read the latest account settings and login commands → write the MCC config (the
//! login script only exists until the game is joined) → spawn `"<ini>" BasicIO-NoColor` with
//! `MCC_LAUNCHER_BRIDGE=1`. Stop: `/quit`, then kill after the grace period. When the client exits
//! on its own the [`restart`](crate::restart) policy decides between a backed-off restart and an
//! error state.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use parking_lot::{Mutex, RwLock};
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, watch};
use tokio::time::Instant;

use crate::bridge::{BridgeEvent, ClientLine, DisconnectReason, parse_line};
use crate::client::ClientStep;
use crate::config::{
    ConfigFiles, INI_FILE_NAME, LOGIN_SCRIPT_FILE_NAME, build_config_in, has_login_commands,
    write_config, write_login_script,
};
use crate::console::{
    ConsoleBuffer, ConsoleLine, ConsoleLineKind, ConsoleOrigin, INPUT_PREFIX, LineFilter,
    STDERR_PREFIX,
};
use crate::error::{AfkError, Result};
use crate::housekeeping::{ConsoleLog, rotate_account_logs};
use crate::model::{AfkAccount, AutoRelogOptions};
use crate::paths::CONSOLE_LOG_FILE_NAME;
use crate::process::{ClientProcess, ClientSpawner, LaunchSpec};
use crate::restart::{
    ExitDecision, Failure, FailureReason, KickCause, RestartSchedule, decide_exit, decide_failure,
    kick_cause,
};
use crate::runtime::ClientExe;
use crate::store::AccountStore;
use crate::util::{eq_ignore_case, remove_file_if_exists};

/// Longest stdout/stderr line read (bytes); the rest of the line is dropped.
const MAX_LINE_BYTES: usize = 64 * 1024;
/// How long to wait for the output pipes to drain after the process has exited.
const READER_DRAIN: Duration = Duration::from_secs(2);

/// Session lifecycle.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum SessionState {
    Stopped,
    /// Preparing the first launch (settings, config, client).
    Starting,
    /// The client runs and is connecting.
    Connecting,
    /// In game since `since`.
    Connected {
        since: Timestamp,
    },
    /// Waiting for (or performing) automatic restart number `attempt`. `next_attempt_at` is set
    /// while waiting; `reason` is why the previous client stopped.
    Reconnecting {
        attempt: u32,
        #[serde(rename = "nextAttemptAt")]
        next_attempt_at: Option<Timestamp>,
        reason: Option<FailureReason>,
    },
    /// `/quit` sent, waiting for the client to exit (killed after the grace period).
    Stopping,
    Error {
        reason: FailureReason,
        /// The AutoRelog retry limit stopped further restarts.
        #[serde(rename = "retryLimitReached")]
        retry_limit_reached: bool,
    },
}

impl SessionState {
    /// Running, connecting or waiting to reconnect: the launcher must not exit in these states.
    pub fn is_active(&self) -> bool {
        !matches!(self, Self::Stopped | Self::Error { .. })
    }
}

/// The accounts at a glance: how many are in game and how many are on their way there (the
/// launcher's tray summary).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct AfkActivity {
    pub in_game: usize,
    /// Starting, connecting or waiting to reconnect.
    pub connecting: usize,
}

impl AfkActivity {
    pub fn of<'a>(states: impl IntoIterator<Item = &'a SessionState>) -> Self {
        let mut activity = Self::default();
        for state in states {
            match state {
                SessionState::Connected { .. } => activity.in_game += 1,
                SessionState::Starting
                | SessionState::Connecting
                | SessionState::Reconnecting { .. } => activity.connecting += 1,
                // Not running, or on its way out.
                SessionState::Stopped | SessionState::Stopping | SessionState::Error { .. } => {}
            }
        }
        activity
    }
}

/// Live numbers from the bridge; cleared when the client exits.
#[derive(Debug, Clone, Copy, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionStats {
    pub hp: Option<f64>,
    pub food: Option<i32>,
    pub players: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionSnapshot {
    pub account_id: String,
    pub state: SessionState,
    pub stats: SessionStats,
    /// The client process is alive.
    pub running: bool,
    pub client_version: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkEvent {
    pub account_id: String,
    pub kind: AfkEventKind,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", content = "data", rename_all = "camelCase")]
pub enum AfkEventKind {
    State(SessionState),
    /// A console line (already masked).
    Line(ConsoleLine),
    Stats(SessionStats),
    /// The account auto-responded to `user`; `sound` mirrors the account's notification setting.
    AutoRespond {
        user: Option<String>,
        sound: bool,
    },
    /// Bulk connect: seconds until this account connects; 0 clears the countdown.
    Countdown {
        secs: u32,
    },
    /// The account waits for the AFK client (download progress included), or its connect could
    /// not get it; `None` once that is over (see [`crate::client`]).
    Client(Option<ClientStep>),
}

/// Receives every event. Called from runtime threads; must not block.
pub type EventSink = Arc<dyn Fn(AfkEvent) + Send + Sync>;

/// Shared by all sessions of a manager.
pub(crate) struct SessionEnv {
    pub(crate) store: Arc<AccountStore>,
    pub(crate) resolve_client: Arc<dyn Fn() -> Option<ClientExe> + Send + Sync>,
    pub(crate) spawner: Arc<dyn ClientSpawner>,
    pub(crate) launcher_nickname: RwLock<String>,
    pub(crate) events: EventSink,
    pub(crate) schedule: RestartSchedule,
    pub(crate) stop_grace: Duration,
    pub(crate) log_cap: u64,
}

impl SessionEnv {
    pub(crate) fn emit(&self, account_id: &str, kind: AfkEventKind) {
        (self.events)(AfkEvent {
            account_id: account_id.to_owned(),
            kind,
        });
    }
}

#[derive(Debug, Clone, PartialEq)]
struct Status {
    state: SessionState,
    running: bool,
}

struct Shared {
    state: SessionState,
    stats: SessionStats,
    console: ConsoleBuffer,
    running: bool,
    client_version: Option<String>,
    /// A start was requested and the actor has not picked it up yet (keeps `is_active` true in
    /// between, so the launcher never exits right after a connect).
    start_pending: bool,
    /// Store revision of the settings the running client was launched with.
    launched_revision: Option<u64>,
}

enum Command {
    Start,
    Stop,
    Send(String),
    Shutdown,
}

#[derive(Clone, Copy)]
enum Stream {
    Out,
    Err,
}

enum Io {
    Line {
        generation: u64,
        stream: Stream,
        text: String,
    },
    Exited {
        generation: u64,
        code: Option<i32>,
    },
}

/// Handle to a session actor.
pub(crate) struct Session {
    account_id: String,
    commands: mpsc::UnboundedSender<Command>,
    shared: Arc<Mutex<Shared>>,
    status: watch::Receiver<Status>,
    task: Mutex<Option<tokio::task::JoinHandle<()>>>,
}

impl Session {
    /// Starts the actor (needs a tokio runtime).
    pub(crate) fn spawn(account_id: String, env: Arc<SessionEnv>) -> Arc<Self> {
        let shared = Arc::new(Mutex::new(Shared {
            state: SessionState::Stopped,
            stats: SessionStats::default(),
            console: ConsoleBuffer::default(),
            running: false,
            client_version: None,
            start_pending: false,
            launched_revision: None,
        }));
        let (command_tx, command_rx) = mpsc::unbounded_channel();
        let (io_tx, io_rx) = mpsc::unbounded_channel();
        let (status_tx, status_rx) = watch::channel(Status {
            state: SessionState::Stopped,
            running: false,
        });
        let actor = Actor {
            id: account_id.clone(),
            env,
            shared: shared.clone(),
            status: status_tx,
            io_tx,
            process: None,
            generation: 0,
            stop_requested: false,
            restart_attempts: 0,
            joined_at: None,
            last_cause: None,
            cause_repeats: 0,
            restart_at: None,
            kill_at: None,
            failure: None,
            filter: LineFilter::default(),
            login_script: None,
            account_dir: None,
            account: None,
            log: None,
        };
        let task = tokio::spawn(actor.run(command_rx, io_rx));
        Arc::new(Self {
            account_id,
            commands: command_tx,
            shared,
            status: status_rx,
            task: Mutex::new(Some(task)),
        })
    }

    pub(crate) fn start(&self) {
        self.shared.lock().start_pending = true;
        if self.commands.send(Command::Start).is_err() {
            self.shared.lock().start_pending = false;
        }
    }

    pub(crate) fn stop(&self) {
        let _ = self.commands.send(Command::Stop);
    }

    /// Sends a command or chat line to the client's stdin.
    pub(crate) fn send(&self, line: String) -> Result<()> {
        if !self.is_running() {
            return Err(AfkError::NotRunning);
        }
        self.commands
            .send(Command::Send(line))
            .map_err(|_| AfkError::NotRunning)
    }

    pub(crate) fn is_running(&self) -> bool {
        self.shared.lock().running
    }

    pub(crate) fn is_active(&self) -> bool {
        let shared = self.shared.lock();
        shared.running || shared.start_pending || shared.state.is_active()
    }

    pub(crate) fn client_version(&self) -> Option<String> {
        let shared = self.shared.lock();
        shared
            .running
            .then(|| shared.client_version.clone())
            .flatten()
    }

    /// Store revision of the running client's settings (`None` while no client runs).
    pub(crate) fn launched_revision(&self) -> Option<u64> {
        let shared = self.shared.lock();
        shared.running.then_some(shared.launched_revision).flatten()
    }

    pub(crate) fn snapshot(&self) -> SessionSnapshot {
        let shared = self.shared.lock();
        SessionSnapshot {
            account_id: self.account_id.clone(),
            state: shared.state.clone(),
            stats: shared.stats,
            running: shared.running,
            client_version: shared.client_version.clone(),
        }
    }

    pub(crate) fn console(&self) -> Vec<ConsoleLine> {
        self.shared.lock().console.snapshot()
    }

    /// Waits until no process runs and no restart is pending. False on timeout.
    pub(crate) async fn wait_idle(&self, timeout: Duration) -> bool {
        let mut status = self.status.clone();
        tokio::time::timeout(
            timeout,
            status.wait_for(|s| !s.running && !s.state.is_active()),
        )
        .await
        .is_ok_and(|r| r.is_ok())
    }

    /// Stops the actor (killing a still running client) and waits for it to finish.
    pub(crate) async fn shutdown(&self) {
        let _ = self.commands.send(Command::Shutdown);
        let task = self.task.lock().take();
        if let Some(task) = task {
            let _ = task.await;
        }
    }
}

struct Running {
    generation: u64,
    stdin: mpsc::UnboundedSender<String>,
    kill: CancellationToken,
}

/// Everything a launch needs, gathered before spawning.
struct Prepared {
    account: AfkAccount,
    commands: Vec<String>,
    files: ConfigFiles,
    client: ClientExe,
    dir: PathBuf,
    /// Store revision read before the settings.
    revision: u64,
}

struct Actor {
    id: String,
    env: Arc<SessionEnv>,
    shared: Arc<Mutex<Shared>>,
    status: watch::Sender<Status>,
    io_tx: mpsc::UnboundedSender<Io>,
    process: Option<Running>,
    generation: u64,
    stop_requested: bool,
    /// Automatic restarts since the last lasting connection (see
    /// [`RestartSchedule::stable_after`]): a join that ends in a kick seconds later does not
    /// reset them, so a client that keeps getting kicked backs off.
    restart_attempts: u32,
    /// When the running client joined the game.
    joined_at: Option<Instant>,
    /// The kick cause of the last failure, and how many failures in a row had it.
    last_cause: Option<KickCause>,
    cause_repeats: u32,
    restart_at: Option<Instant>,
    kill_at: Option<Instant>,
    /// What the running client last reported through the bridge.
    failure: Option<Failure>,
    filter: LineFilter,
    /// Login script content while it has commands (rewritten for in-client reconnects).
    login_script: Option<String>,
    account_dir: Option<PathBuf>,
    /// Settings of the running launch.
    account: Option<AfkAccount>,
    log: Option<ConsoleLog>,
}

async fn sleep_until(at: Option<Instant>) {
    match at {
        Some(at) => tokio::time::sleep_until(at).await,
        None => std::future::pending().await,
    }
}

impl Actor {
    async fn run(
        mut self,
        mut commands: mpsc::UnboundedReceiver<Command>,
        mut io: mpsc::UnboundedReceiver<Io>,
    ) {
        loop {
            let restart_at = self.restart_at;
            let kill_at = self.kill_at;
            tokio::select! {
                command = commands.recv() => match command {
                    Some(Command::Start) => self.on_start().await,
                    Some(Command::Stop) => self.on_stop(),
                    Some(Command::Send(line)) => self.on_send(line),
                    Some(Command::Shutdown) | None => {
                        self.on_shutdown();
                        break;
                    }
                },
                Some(event) = io.recv() => self.on_io(event),
                () = sleep_until(restart_at) => {
                    self.restart_at = None;
                    self.on_restart_due().await;
                }
                () = sleep_until(kill_at) => {
                    self.kill_at = None;
                    if let Some(running) = &self.process {
                        tracing::info!(account = %self.id, "AFK client ignored /quit, killing it");
                        running.kill.cancel();
                    }
                }
            }
        }
    }

    // --- Commands ---------------------------------------------------------------------------

    async fn on_start(&mut self) {
        self.shared.lock().start_pending = false;
        if self.process.is_some() {
            return;
        }
        self.stop_requested = false;
        self.restart_at = None;
        self.restart_attempts = 0;
        self.joined_at = None;
        self.last_cause = None;
        self.cause_repeats = 0;
        self.failure = None;
        self.set_state(SessionState::Starting);
        self.launch(false).await;
    }

    fn on_stop(&mut self) {
        self.restart_at = None;
        self.stop_requested = true;
        match &self.process {
            None => {
                self.remove_login_script();
                self.set_state(SessionState::Stopped);
            }
            Some(running) => {
                let _ = running.stdin.send("/quit".to_owned());
                if self.kill_at.is_none() {
                    self.kill_at = Some(Instant::now() + self.env.stop_grace);
                }
                self.set_state(SessionState::Stopping);
            }
        }
    }

    fn on_send(&mut self, line: String) {
        if line.is_empty() {
            return;
        }
        let Some(stdin) = self.process.as_ref().map(|p| p.stdin.clone()) else {
            return;
        };
        self.filter.add_user_command(&line);
        self.append_console(
            &format!("{INPUT_PREFIX}{line}"),
            ConsoleOrigin::User,
            Some(ConsoleLineKind::Normal),
        );

        let command = line.trim().split([' ', '\t']).next().unwrap_or_default();
        if ["/quit", "/exit"]
            .iter()
            .any(|c| eq_ignore_case(c, command))
        {
            // The user asked the client to exit: treat it as a stop, not as a crash to recover.
            self.on_stop();
            return;
        }
        if ["/reco", "/connect"]
            .iter()
            .any(|c| eq_ignore_case(c, command))
        {
            // The client reconnects in-process and runs its login tasks again.
            self.restore_login_script();
        }
        let _ = stdin.send(line);
    }

    fn on_shutdown(&mut self) {
        self.restart_at = None;
        self.stop_requested = true;
        if let Some(running) = self.process.take() {
            running.kill.cancel();
        }
        self.remove_login_script();
        {
            let mut shared = self.shared.lock();
            shared.running = false;
            shared.stats = SessionStats::default();
        }
        self.set_state(SessionState::Stopped);
    }

    // --- Launching -----------------------------------------------------------------------------

    async fn launch(&mut self, is_restart: bool) {
        match self.prepare().await {
            Ok(prepared) => self.spawn_client(prepared, is_restart),
            Err(reason) => self.launch_failed(reason, is_restart),
        }
    }

    async fn prepare(&mut self) -> std::result::Result<Prepared, FailureReason> {
        let store = self.env.store.clone();
        // Read before the settings: a save racing this launch then counts as not applied.
        let revision = store.revision(&self.id);
        let account = store
            .account(&self.id)
            .ok_or_else(|| FailureReason::LaunchFailed {
                message: "account no longer exists".into(),
            })?;

        let commands = {
            let store = store.clone();
            let id = self.id.clone();
            tokio::task::spawn_blocking(move || store.login_commands(&id)).await
        };
        let commands = match commands {
            Ok(Ok(commands)) => commands,
            Ok(Err(error)) => {
                tracing::warn!(account = %self.id, %error, "login commands unavailable");
                return Err(FailureReason::SecretsUnavailable);
            }
            Err(_) => return Err(FailureReason::SecretsUnavailable),
        };

        let client = (self.env.resolve_client)().ok_or(FailureReason::ClientMissing)?;
        let nickname = self.env.launcher_nickname.read().clone();
        // MCC's own messages in the console follow the launcher's language.
        let files = build_config_in(
            &account,
            &commands,
            &nickname,
            &store.versions(),
            pl_lang::Lang::current(),
        );
        let dir = store.paths().account_dir(&self.id);

        let written = {
            let dir = dir.clone();
            let files = files.clone();
            let cap = self.env.log_cap;
            tokio::task::spawn_blocking(move || {
                rotate_account_logs(&dir, cap);
                write_config(&dir, &files)
            })
            .await
        };
        match written {
            Ok(Ok(())) => {}
            Ok(Err(error)) => {
                return Err(FailureReason::LaunchFailed {
                    message: error.to_string(),
                });
            }
            Err(error) => {
                return Err(FailureReason::LaunchFailed {
                    message: error.to_string(),
                });
            }
        }

        Ok(Prepared {
            account,
            commands,
            files,
            client,
            dir,
            revision,
        })
    }

    fn spawn_client(&mut self, prepared: Prepared, is_restart: bool) {
        let Prepared {
            account,
            commands,
            files,
            client,
            dir,
            revision,
        } = prepared;
        self.account_dir = Some(dir.clone());
        let spec = LaunchSpec {
            exe: client.path.clone(),
            ini: dir.join(INI_FILE_NAME),
            cwd: dir.clone(),
        };

        let process = match self.env.spawner.spawn(&spec) {
            Ok(process) => process,
            Err(error) => {
                self.remove_login_script();
                self.launch_failed(
                    FailureReason::LaunchFailed {
                        message: error.to_string(),
                    },
                    is_restart,
                );
                return;
            }
        };

        self.generation += 1;
        let generation = self.generation;
        self.filter = LineFilter::new(&commands);
        self.login_script =
            has_login_commands(&account.login_commands, &commands).then_some(files.login_script);
        self.account = Some(account);
        if self.log.is_none() {
            self.log = Some(ConsoleLog::open(
                dir.join(CONSOLE_LOG_FILE_NAME),
                self.env.log_cap,
            ));
        }

        let ClientProcess {
            stdin,
            stdout,
            stderr,
            exit,
        } = process;
        let out_task = tokio::spawn(read_lines(
            stdout,
            generation,
            Stream::Out,
            self.io_tx.clone(),
        ));
        let err_task = tokio::spawn(read_lines(
            stderr,
            generation,
            Stream::Err,
            self.io_tx.clone(),
        ));
        let (stdin_tx, stdin_rx) = mpsc::unbounded_channel();
        tokio::spawn(write_lines(stdin, stdin_rx));

        let kill = CancellationToken::new();
        let io_tx = self.io_tx.clone();
        let exit_kill = kill.clone();
        tokio::spawn(async move {
            let code = exit.wait(exit_kill).await;
            // Deliver all output (the final bridge events) before the exit.
            let drained = async {
                let _ = out_task.await;
                let _ = err_task.await;
            };
            let _ = tokio::time::timeout(READER_DRAIN, drained).await;
            let _ = io_tx.send(Io::Exited { generation, code });
        });

        self.process = Some(Running {
            generation,
            stdin: stdin_tx,
            kill,
        });
        {
            let mut shared = self.shared.lock();
            shared.running = true;
            shared.client_version = Some(client.version);
            shared.launched_revision = Some(revision);
        }
        tracing::info!(account = %self.id, is_restart, "AFK client started");
        self.set_state(SessionState::Connecting);
    }

    fn launch_failed(&mut self, reason: FailureReason, is_restart: bool) {
        tracing::warn!(account = %self.id, ?reason, "AFK client could not be started");
        let relog = self.latest_relog();
        if is_restart && relog.enabled {
            // Legacy: a failed restart (e.g. a briefly locked file) keeps the loop going.
            let decision = decide_failure(&relog, None, reason, self.restart_attempts, 0);
            self.note_cause(None);
            self.apply_decision(decision);
        } else {
            self.set_state(SessionState::Error {
                reason,
                retry_limit_reached: false,
            });
        }
    }

    async fn on_restart_due(&mut self) {
        if self.stop_requested || self.process.is_some() {
            return;
        }
        let reason = match &self.shared.lock().state {
            SessionState::Reconnecting { reason, .. } => reason.clone(),
            _ => None,
        };
        self.set_state(SessionState::Reconnecting {
            attempt: self.restart_attempts,
            next_attempt_at: None,
            reason,
        });
        self.failure = None;
        self.launch(true).await;
    }

    // --- Client output -------------------------------------------------------------------------

    fn is_current(&self, generation: u64) -> bool {
        self.process
            .as_ref()
            .is_some_and(|p| p.generation == generation)
    }

    fn on_io(&mut self, event: Io) {
        match event {
            Io::Line { generation, .. } if !self.is_current(generation) => {}
            Io::Line {
                stream: Stream::Out,
                text,
                ..
            } => match parse_line(&text) {
                ClientLine::Event(event) => self.on_bridge(event),
                ClientLine::Malformed => {}
                ClientLine::Text(text) => self.append_console(text, ConsoleOrigin::Client, None),
            },
            Io::Line {
                stream: Stream::Err,
                text,
                ..
            } => self.append_console(
                &format!("{STDERR_PREFIX}{text}"),
                ConsoleOrigin::ClientError,
                None,
            ),
            Io::Exited { generation, code } => {
                if self.is_current(generation) {
                    self.on_exited(code);
                }
            }
        }
    }

    fn on_bridge(&mut self, event: BridgeEvent) {
        match event {
            BridgeEvent::Connecting { .. } => {
                let connecting = matches!(
                    self.shared.lock().state,
                    SessionState::Starting
                        | SessionState::Connecting
                        | SessionState::Reconnecting { .. }
                );
                if !self.stop_requested && connecting {
                    self.set_state(SessionState::Connecting);
                }
            }
            BridgeEvent::Joined { .. } => {
                if self.stop_requested {
                    return;
                }
                // The client read the login script before announcing the join (ScriptScheduler
                // runs before the bridge bot), so the password file can go now.
                self.remove_login_script();
                // The restart count starts over only once this connection lasts.
                self.joined_at = Some(Instant::now());
                self.failure = None;
                self.set_state(SessionState::Connected {
                    since: Timestamp::now(),
                });
            }
            BridgeEvent::Health { hp, food } => {
                let stats = {
                    let mut shared = self.shared.lock();
                    shared.stats.hp = hp;
                    shared.stats.food = food;
                    shared.stats
                };
                self.env.emit(&self.id, AfkEventKind::Stats(stats));
            }
            BridgeEvent::Players { count } => {
                let stats = {
                    let mut shared = self.shared.lock();
                    shared.stats.players = count;
                    shared.stats
                };
                self.env.emit(&self.id, AfkEventKind::Stats(stats));
            }
            BridgeEvent::AutoRespond { user } => {
                let sound = self
                    .account
                    .as_ref()
                    .is_some_and(|a| a.auto_respond.sound_enabled);
                self.env
                    .emit(&self.id, AfkEventKind::AutoRespond { user, sound });
            }
            BridgeEvent::Disconnected { reason, message } => {
                self.on_disruption(reason, message.as_deref(), false);
            }
            BridgeEvent::Error {
                reason,
                message,
                version_error,
            } => self.on_disruption(reason, message.as_deref(), version_error),
            BridgeEvent::Unknown(_) => {}
        }
    }

    /// `HandleDisruption`: remember what went wrong and show where it is heading; the process
    /// exits right after and [`Self::on_exited`] makes the final decision.
    fn on_disruption(
        &mut self,
        reason: Option<DisconnectReason>,
        message: Option<&str>,
        version_error: bool,
    ) {
        if self.stop_requested {
            return;
        }
        let masked = message.map(|m| self.filter.mask(m));
        let failure = Failure::merge(
            self.failure.take(),
            Failure::new(reason, masked.as_deref(), version_error),
        );
        self.forget_failures_after_a_lasting_connection();
        let relog = self.latest_relog();
        let decision = decide_failure(
            &relog,
            failure.reason.as_ref(),
            failure.to_reason(),
            self.restart_attempts,
            self.repeats_of(&failure),
        );
        self.failure = Some(failure);
        let state = match decision {
            ExitDecision::Restart { attempt, reason } => SessionState::Reconnecting {
                attempt,
                next_attempt_at: None,
                reason: Some(reason),
            },
            ExitDecision::GiveUp(reason) => SessionState::Error {
                reason,
                retry_limit_reached: true,
            },
            ExitDecision::Fail(reason) => SessionState::Error {
                reason,
                retry_limit_reached: false,
            },
            ExitDecision::Stopped => SessionState::Stopped,
        };
        self.set_state(state);
    }

    fn on_exited(&mut self, code: Option<i32>) {
        self.process = None;
        self.kill_at = None;
        self.remove_login_script();
        self.login_script = None;
        {
            let mut shared = self.shared.lock();
            shared.running = false;
            shared.stats = SessionStats::default();
        }
        self.env
            .emit(&self.id, AfkEventKind::Stats(SessionStats::default()));
        tracing::info!(account = %self.id, ?code, "AFK client exited");

        self.forget_failures_after_a_lasting_connection();
        self.joined_at = None;
        let relog = self.latest_relog();
        let failure = self.failure.take();
        let repeats = failure
            .as_ref()
            .map_or(0, |failure| self.repeats_of(failure));
        let decision = decide_exit(
            &relog,
            self.stop_requested,
            code,
            failure.as_ref(),
            self.restart_attempts,
            repeats,
        );
        if !self.stop_requested {
            self.note_cause(
                failure
                    .as_ref()
                    .and_then(|f| kick_cause(f.reason.as_ref(), f.message.as_deref())),
            );
        }
        self.apply_decision(decision);
    }

    /// In game for [`RestartSchedule::stable_after`]: the connection worked, so the failures
    /// before it no longer count.
    fn forget_failures_after_a_lasting_connection(&mut self) {
        if self
            .joined_at
            .is_some_and(|at| at.elapsed() >= self.env.schedule.stable_after)
        {
            self.restart_attempts = 0;
            self.last_cause = None;
            self.cause_repeats = 0;
        }
    }

    /// How many failures right before `failure` had its kick cause.
    fn repeats_of(&self, failure: &Failure) -> u32 {
        match (
            kick_cause(failure.reason.as_ref(), failure.message.as_deref()),
            self.last_cause,
        ) {
            (Some(cause), Some(last)) if cause == last => self.cause_repeats,
            _ => 0,
        }
    }

    /// Records the cause of a finished failure (`None`: it had none, which ends a streak).
    fn note_cause(&mut self, cause: Option<KickCause>) {
        if cause.is_some() && cause == self.last_cause {
            self.cause_repeats += 1;
        } else {
            self.last_cause = cause;
            self.cause_repeats = u32::from(cause.is_some());
        }
    }

    fn apply_decision(&mut self, decision: ExitDecision) {
        match decision {
            ExitDecision::Stopped => self.set_state(SessionState::Stopped),
            ExitDecision::Fail(reason) => self.set_state(SessionState::Error {
                reason,
                retry_limit_reached: false,
            }),
            ExitDecision::GiveUp(reason) => self.set_state(SessionState::Error {
                reason,
                retry_limit_reached: true,
            }),
            ExitDecision::Restart { attempt, reason } => {
                self.restart_attempts = attempt;
                // Any varying value spreads the reconnects; no cryptographic randomness needed.
                let spread = f64::from(
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |since| since.subsec_nanos() % 1000),
                ) / 999.0;
                // The account's delay as it is now (it may have been edited since the launch).
                let relog = self.latest_relog();
                let delay = self.env.schedule.spread_delay(&relog, attempt, spread);
                self.restart_at = Some(Instant::now() + delay);
                let next_attempt_at = jiff::SignedDuration::try_from(delay)
                    .ok()
                    .and_then(|d| Timestamp::now().checked_add(d).ok());
                self.set_state(SessionState::Reconnecting {
                    attempt,
                    next_attempt_at,
                    reason: Some(reason),
                });
            }
        }
    }

    // --- Helpers -------------------------------------------------------------------------------

    /// AutoRelog settings as they are *now* (the user may have changed them since the launch). A
    /// removed account never reconnects.
    fn latest_relog(&self) -> AutoRelogOptions {
        self.env
            .store
            .account(&self.id)
            .map(|a| a.auto_relog)
            .unwrap_or_else(|| AutoRelogOptions {
                enabled: false,
                ..AutoRelogOptions::default()
            })
    }

    fn append_console(
        &mut self,
        raw: &str,
        origin: ConsoleOrigin,
        kind_override: Option<ConsoleLineKind>,
    ) {
        let (text, kind) = self.filter.process(raw);
        let line = ConsoleLine {
            at: Timestamp::now(),
            text,
            kind: kind_override.unwrap_or(kind),
            origin,
        };
        if let Some(log) = &self.log {
            log.write(&line);
        }
        self.shared.lock().console.push(line.clone());
        self.env.emit(&self.id, AfkEventKind::Line(line));
    }

    /// Records the state (emitting it only when it changed) and always republishes the
    /// state/running pair waiters observe.
    fn set_state(&mut self, state: SessionState) {
        let (changed, status) = {
            let mut shared = self.shared.lock();
            let changed = shared.state != state;
            if changed {
                shared.state = state.clone();
            }
            (
                changed,
                Status {
                    state: shared.state.clone(),
                    running: shared.running,
                },
            )
        };
        self.status.send_if_modified(|current| {
            let modified = *current != status;
            if modified {
                *current = status;
            }
            modified
        });
        if changed {
            self.env.emit(&self.id, AfkEventKind::State(state));
        }
    }

    fn remove_login_script(&self) {
        let dir = self
            .account_dir
            .clone()
            .unwrap_or_else(|| self.env.store.paths().account_dir(&self.id));
        let path = dir.join(LOGIN_SCRIPT_FILE_NAME);
        if let Err(error) = remove_file_if_exists(&path) {
            tracing::warn!(account = %self.id, %error, "could not delete the login script");
        }
    }

    fn restore_login_script(&self) {
        if let (Some(dir), Some(script)) = (&self.account_dir, &self.login_script)
            && let Err(error) = write_login_script(dir, script)
        {
            tracing::warn!(account = %self.id, %error, "could not restore the login script");
        }
    }
}

impl Drop for Actor {
    fn drop(&mut self) {
        // Never leave the password file behind, whatever ended the actor.
        if let Some(running) = self.process.take() {
            running.kill.cancel();
        }
        self.remove_login_script();
    }
}

/// Forwards complete lines (lossy UTF-8, `\r\n` stripped, capped) until EOF.
async fn read_lines<R: AsyncRead + Unpin>(
    reader: R,
    generation: u64,
    stream: Stream,
    tx: mpsc::UnboundedSender<Io>,
) {
    let mut reader = BufReader::new(reader);
    let mut line = Vec::new();
    let mut first = true;
    // EOF and read errors both end the stream.
    while let Ok(true) = read_line_capped(&mut reader, &mut line).await {
        let mut text = String::from_utf8_lossy(&line).into_owned();
        if first {
            // A UTF-8 preamble would hide the bridge prefix of the first line.
            if let Some(stripped) = text.strip_prefix('\u{feff}') {
                text = stripped.to_owned();
            }
            first = false;
        }
        let event = Io::Line {
            generation,
            stream,
            text,
        };
        if tx.send(event).is_err() {
            break;
        }
    }
}

/// Reads one line into `line` without its terminator, keeping at most [`MAX_LINE_BYTES`].
/// Returns false at EOF with nothing read.
async fn read_line_capped<R: AsyncBufReadExt + Unpin>(
    reader: &mut R,
    line: &mut Vec<u8>,
) -> std::io::Result<bool> {
    line.clear();
    let mut read_any = false;
    loop {
        let buffer = reader.fill_buf().await?;
        if buffer.is_empty() {
            break;
        }
        read_any = true;
        let (chunk, used, done) = match buffer.iter().position(|&b| b == b'\n') {
            Some(pos) => (&buffer[..pos], pos + 1, true),
            None => (buffer, buffer.len(), false),
        };
        let room = MAX_LINE_BYTES.saturating_sub(line.len());
        line.extend_from_slice(&chunk[..chunk.len().min(room)]);
        reader.consume(used);
        if done {
            break;
        }
    }
    if line.last() == Some(&b'\r') {
        line.pop();
    }
    Ok(read_any)
}

/// Writes queued lines (UTF-8, `\n`) to the client's stdin until the queue closes.
async fn write_lines<W: AsyncWrite + Unpin>(
    mut writer: W,
    mut rx: mpsc::UnboundedReceiver<String>,
) {
    while let Some(line) = rx.recv().await {
        let mut bytes = line.into_bytes();
        bytes.push(b'\n');
        if writer.write_all(&bytes).await.is_err() || writer.flush().await.is_err() {
            break;
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn activity_counts_accounts_in_game_and_on_their_way() {
        let failed = SessionState::Error {
            reason: FailureReason::Refused {
                cause: KickCause::Banned,
                message: None,
            },
            retry_limit_reached: false,
        };
        let states = [
            SessionState::Connected {
                since: Timestamp::UNIX_EPOCH,
            },
            SessionState::Connected {
                since: Timestamp::UNIX_EPOCH,
            },
            SessionState::Starting,
            SessionState::Connecting,
            SessionState::Reconnecting {
                attempt: 2,
                next_attempt_at: None,
                reason: None,
            },
            // Going away or not running: not counted.
            SessionState::Stopping,
            SessionState::Stopped,
            failed,
        ];
        assert_eq!(
            AfkActivity::of(&states),
            AfkActivity {
                in_game: 2,
                connecting: 3,
            }
        );
        assert_eq!(AfkActivity::of(&[]), AfkActivity::default());
    }
}
