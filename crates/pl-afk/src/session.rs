//! One account's live session: an actor task that owns the client process.
//!
//! All state changes happen on the actor task, in the order commands, client output and timers
//! arrive; the manager and the UI read a snapshot behind a short lock and receive [`AfkEvent`]s.
//!
//! Launch: re-read the latest account settings and login commands → write the MCC config (the
//! login script only exists until the game is joined) → spawn `"<ini>" BasicIO-NoColor` with
//! `MCC_LAUNCHER_BRIDGE=1`; a released client of 4.1.0 or later (or of a version that does not
//! read as one) also gets a fresh `MCC_BRIDGE_NONCE` per process, and then only lines with that
//! nonce are events: a server's line that starts with the bare bridge prefix (a chat or kick text)
//! stays text and cannot fake a join or a disconnect.
//! Stop: `/quit`, then kill after the grace period. When the client exits on its own the
//! [`restart`](crate::restart) policy decides between a backed-off restart and an error state.
//!
//! Console looks and dialog cards (AFK-UX): while Studio's switches say so ([`ConsoleFeatures`])
//! a client 4.2.4 or later with a nonce also gets `MCC_CONSOLE_KINDS=1`. Every console write is
//! then a `line` event: its text goes the way a plain line goes (classified, masked, cut), with its
//! look when the account's rules leave the line as it came ([`crate::looks`]); what the server
//! keeps on screen (the action bar, titles, boss bars) is no console line, as before. The server's
//! open dialog is kept for the console's card ([`crate::dialog`]), and a press of the card goes to
//! the client as `/dialog act`, answered by its `dialogres`.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use jiff::Timestamp;
use parking_lot::{Mutex, RwLock};
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use tokio::io::{AsyncBufReadExt, AsyncRead, AsyncWrite, AsyncWriteExt, BufReader};
use tokio::sync::{mpsc, oneshot, watch};
use tokio::time::Instant;

use crate::bridge::{
    BridgeEvent, ClientLine, DisconnectReason, client_gives_looks, client_takes_nonce, new_nonce,
    parse_line_with_nonce,
};
use crate::client::ClientStep;
use crate::config::{
    ConfigFiles, INI_FILE_NAME, LOGIN_SCRIPT_FILE_NAME, build_config_in, has_login_commands,
    write_config, write_login_script,
};
use crate::console::{
    ConsoleBuffer, ConsoleLine, ConsoleLineKind, ConsoleOrigin, LineFilter, STDERR_PREFIX,
    cap_line, cut_units,
};
use crate::dialog::{self, ShownDialog};
use crate::error::{AfkError, Result};
use crate::housekeeping::{ConsoleLog, rotate_account_logs};
use crate::looks::{self, LineLooks};
use crate::model::{AfkAccount, AutoRelogOptions};
use crate::pacing::{ConnectPacer, connect_destination};
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
/// How long a press of the dialog card waits for the client's answer.
pub(crate) const PRESS_ANSWER: Duration = Duration::from_secs(4);

/// Studio's console switches (7/24 Bulut's offer says them): a client that gives them runs with
/// the console kinds while either is on; both apply from its next launch.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ConsoleFeatures {
    /// Lines in the game's colours, with their hovers and clicks (CONSOLE-COLOR).
    pub looks: bool,
    /// A server's dialog as a card the player may press (DIALOG-CARD).
    pub dialogs: bool,
}

/// The server's dialog as the console's card shows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogView {
    /// The client's revision of it: a press names it.
    pub rev: u32,
    /// Its JSON as checked here (the view checks it again).
    pub json: String,
    /// When it opened.
    pub at: Timestamp,
}

/// The client's answer to a press of the card.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DialogOutcome {
    pub ok: bool,
    /// Why not, in the client's word (`stale`: another dialog now; anything else: it did not
    /// work); the view says it without detail.
    pub why: Option<String>,
}

impl DialogOutcome {
    pub(crate) fn refused(why: &str) -> Self {
        Self {
            ok: false,
            why: Some(why.to_owned()),
        }
    }
}
/// How long to wait for the output pipes to drain after the process has exited (the default of
/// `ManagerConfig::reader_drain`).
pub(crate) const READER_DRAIN: Duration = Duration::from_secs(2);

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
    /// The server's dialog of the account opened, changed or closed (the console's card reads it
    /// again).
    Dialog,
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
    /// Connection slots per server, shared by every account (see [`crate::pacing`]).
    pub(crate) pacer: ConnectPacer,
    /// Zero: no pacing.
    pub(crate) connect_spacing: Duration,
    /// See `ManagerConfig::reader_drain`.
    pub(crate) reader_drain: Duration,
    /// Studio's console switches as last known (each launch reads them).
    pub(crate) console_features: RwLock<ConsoleFeatures>,
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
    /// The server's open dialog as the card shows it, and when it opened.
    dialog: Option<(ShownDialog, Timestamp)>,
}

enum Command {
    Start,
    Stop,
    Send(String),
    /// A press of the dialog card (its JSON as the view built it) and where the answer goes.
    DialogPress(String, oneshot::Sender<DialogOutcome>),
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
            dialog: None,
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
            paced: None,
            kill_at: None,
            failure: None,
            filter: LineFilter::default(),
            login_script: None,
            account_dir: None,
            account: None,
            log: None,
            features: ConsoleFeatures::default(),
            dialog_cap: false,
            press: None,
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

    /// The server's open dialog as the console's card shows it.
    pub(crate) fn dialog(&self) -> Option<DialogView> {
        self.shared
            .lock()
            .dialog
            .as_ref()
            .map(|(dialog, at)| DialogView {
                rev: dialog.rev,
                json: dialog.json.clone(),
                at: *at,
            })
    }

    /// Presses the dialog card; the client's answer comes on the returned channel (dropped
    /// unanswered when the client ends first).
    pub(crate) fn press_dialog(&self, press: String) -> Result<oneshot::Receiver<DialogOutcome>> {
        if !self.is_running() {
            return Err(AfkError::NotRunning);
        }
        let (tx, rx) = oneshot::channel();
        self.commands
            .send(Command::DialogPress(press, tx))
            .map_err(|_| AfkError::NotRunning)?;
        Ok(rx)
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
    /// This process's bridge nonce (clients 4.1.0 and later): only its event lines count.
    nonce: Option<String>,
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
    /// A launch waiting at `restart_at` for its connection slot (see [`crate::pacing`]): whether
    /// it is a restart.
    paced: Option<bool>,
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
    /// What the running client was launched with: none of it without the console kinds.
    features: ConsoleFeatures,
    /// The running client reports dialog cards (`caps` of its `connecting`).
    dialog_cap: bool,
    /// A press of the dialog card waiting for the client's `dialogres`.
    press: Option<oneshot::Sender<DialogOutcome>>,
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
                    Some(Command::DialogPress(press, reply)) => self.on_dialog_press(&press, reply),
                    Some(Command::Shutdown) | None => {
                        self.on_shutdown();
                        break;
                    }
                },
                Some(event) = io.recv() => self.on_io(event),
                () = sleep_until(restart_at) => {
                    self.restart_at = None;
                    match self.paced.take() {
                        // The connection slot this launch waited for: it is ours, go.
                        Some(is_restart) => {
                            if !self.stop_requested && self.process.is_none() {
                                self.launch_now(is_restart).await;
                            }
                        }
                        None => self.on_restart_due().await,
                    }
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
        // The console log's file is closed before the actor ends: a removed account's folder is
        // deleted right after, and Windows keeps a folder with an open file in it.
        if let Some(log) = self.log.take() {
            log.close().await;
        }
    }

    // --- Commands ---------------------------------------------------------------------------

    async fn on_start(&mut self) {
        self.shared.lock().start_pending = false;
        // Running, or a launch already waits for its connection slot.
        if self.process.is_some() || self.paced.is_some() {
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
        self.paced = None;
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
        // No echo line of our own: the server answers a chat message or a command, and that answer
        // is the player's feedback (owner, 22 Oct). A sensitive line is still remembered, so the
        // client's own echo of it stays masked.
        self.filter.add_user_command(&line);

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
        self.paced = None;
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

    /// Launches now, or once this account's turn to connect to its server comes.
    async fn launch(&mut self, is_restart: bool) {
        if let Some(slot) = self.connect_slot() {
            self.wait_for_slot(slot, is_restart);
            return;
        }
        self.launch_now(is_restart).await;
    }

    async fn launch_now(&mut self, is_restart: bool) {
        match self.prepare().await {
            Ok(prepared) => self.spawn_client(prepared, is_restart),
            Err(reason) => self.launch_failed(reason, is_restart),
        }
    }

    /// This launch's connection slot when it is not now. The slot is taken either way: the
    /// accounts connecting to one server go one by one, in the order they got here.
    fn connect_slot(&self) -> Option<Instant> {
        let spacing = self.env.connect_spacing;
        if spacing.is_zero() {
            return None;
        }
        // A removed account fails in `prepare` without touching the server.
        let account = self.env.store.account(&self.id)?;
        let now = Instant::now();
        let slot =
            self.env
                .pacer
                .reserve(&connect_destination(account.effective_host()), spacing, now);
        (slot > now).then_some(slot)
    }

    /// Waits for `slot` on the restart timer, so a stop still lands at once. A reconnect shows
    /// the slot as its next attempt; a first start stays "starting".
    fn wait_for_slot(&mut self, slot: Instant, is_restart: bool) {
        let wait = slot.saturating_duration_since(Instant::now());
        tracing::debug!(account = %self.id, ?wait, "waiting for a connection slot");
        self.paced = Some(is_restart);
        self.restart_at = Some(slot);
        if is_restart {
            let (attempt, reason) = match &self.shared.lock().state {
                SessionState::Reconnecting {
                    attempt, reason, ..
                } => (*attempt, reason.clone()),
                _ => (self.restart_attempts, None),
            };
            let next_attempt_at = jiff::SignedDuration::try_from(wait)
                .ok()
                .and_then(|d| Timestamp::now().checked_add(d).ok());
            self.set_state(SessionState::Reconnecting {
                attempt,
                next_attempt_at,
                reason,
            });
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
        // Decided by the version this session runs, never by the first line it sees: an older
        // client ignores the variable and its bare event lines are the only ones it prints. A
        // development build has no version to go by and keeps the bare lines.
        let nonce = (!client.dev && client_takes_nonce(&client.version)).then(new_nonce);
        // The console kinds: only with a nonce (events tell from text by it alone) and a client
        // that gives looks and dialogs, while one of Studio's switches is on.
        let wanted = *self.env.console_features.read();
        let kinds = nonce.is_some()
            && client_gives_looks(&client.version)
            && (wanted.looks || wanted.dialogs);
        let spec = LaunchSpec {
            exe: client.path.clone(),
            ini: dir.join(INI_FILE_NAME),
            cwd: dir.clone(),
            nonce: nonce.clone(),
            console_kinds: kinds,
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
        self.features = if kinds {
            wanted
        } else {
            ConsoleFeatures::default()
        };
        self.dialog_cap = false;
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
        let drain = self.env.reader_drain;
        tokio::spawn(async move {
            let code = exit.wait(exit_kill).await;
            // Deliver all output (the final bridge events) before the exit.
            let drained = async {
                let _ = out_task.await;
                let _ = err_task.await;
            };
            let _ = tokio::time::timeout(drain, drained).await;
            let _ = io_tx.send(Io::Exited { generation, code });
        });

        self.process = Some(Running {
            generation,
            stdin: stdin_tx,
            kill,
            nonce,
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
            } => {
                // The line is the current process's (checked above): its nonce decides.
                let nonce = self.process.as_ref().and_then(|p| p.nonce.as_deref());
                match parse_line_with_nonce(&text, nonce) {
                    ClientLine::Event(event) => self.on_bridge(event),
                    ClientLine::Malformed => {}
                    ClientLine::Text(text) => {
                        self.append_console(text, ConsoleOrigin::Client, None);
                    }
                }
            }
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
            BridgeEvent::Connecting { caps, .. } => {
                self.dialog_cap = caps.iter().any(|cap| cap == "dialog");
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
            BridgeEvent::Line {
                src,
                text,
                who,
                guess,
                st,
                ev,
            } => self.on_line(&src, &text, who, guess.as_deref(), st.as_ref(), ev.as_ref()),
            BridgeEvent::Dialog { rev, dialog } => self.on_dialog(rev, dialog.as_ref()),
            BridgeEvent::DialogResult { ok, why, .. } => {
                if let Some(reply) = self.press.take() {
                    let _ = reply.send(if ok {
                        DialogOutcome { ok, why: None }
                    } else {
                        DialogOutcome::refused(why.as_deref().unwrap_or("failed"))
                    });
                }
            }
            BridgeEvent::Unknown(_) => {}
        }
    }

    /// A console write of a client with the console kinds. The console shows what it showed
    /// before them (the client's own lines, the server's chat and messages); what the server
    /// keeps on screen (the action bar, titles) never was a console line, nor is it in 7/24 Bulut.
    fn on_line(
        &mut self,
        src: &str,
        text: &str,
        who: Option<String>,
        guess: Option<&str>,
        st: Option<&Value>,
        ev: Option<&Value>,
    ) {
        if matches!(src, "overlay" | "title" | "subtitle") {
            return;
        }
        let look = if self.features.looks {
            looks::read(st, ev, text)
        } else {
            None
        };
        // Whose chat line it is, for the card of a command clicked in it ("… adlı oyuncunun
        // mesajından"): a player's line, or the client's own reading of a server's chat line.
        let chat = src == "player" || (src == "system" && matches!(guess, Some("chat" | "pm")));
        let who = who.filter(|name| {
            chat && crate::model::is_valid_nickname(name) && self.filter.mask(name) == *name
        });
        self.append_line(text, ConsoleOrigin::Client, None, look, who);
    }

    /// The server's dialog (or its close) from a client that reports dialog cards: kept for the
    /// card while Studio's switch was on at the launch and the account's rules leave it as it came.
    fn on_dialog(&mut self, rev: i64, raw: Option<&Value>) {
        if !self.features.dialogs || !self.dialog_cap {
            return;
        }
        let shown = raw.and_then(|raw| dialog::check(rev, raw, &|text| self.filter.mask(text)));
        let changed = {
            let mut shared = self.shared.lock();
            let same = shared.dialog.as_ref().map(|(open, _)| open) == shown.as_ref();
            if !same {
                shared.dialog = shown.map(|open| (open, Timestamp::now()));
            }
            !same
        };
        if changed {
            self.env.emit(&self.id, AfkEventKind::Dialog);
        }
    }

    /// A press of the dialog card, checked against the dialog it was made on and sent to the
    /// client; its `dialogres` answers it. A command the press may run is masked in the console
    /// from now on when it carries a password, as a typed one is.
    fn on_dialog_press(&mut self, press: &str, reply: oneshot::Sender<DialogOutcome>) {
        let refuse = |reply: oneshot::Sender<DialogOutcome>, why: &str| {
            let _ = reply.send(DialogOutcome::refused(why));
        };
        let Some(stdin) = self.process.as_ref().map(|p| p.stdin.clone()) else {
            return refuse(reply, "failed");
        };
        // One press at a time: the card waits for its answer.
        if self
            .press
            .as_ref()
            .is_some_and(|earlier| !earlier.is_closed())
        {
            return refuse(reply, "failed");
        }
        let shown = self
            .shared
            .lock()
            .dialog
            .as_ref()
            .map(|(open, _)| open.clone());
        let Some(shown) = shown.filter(|_| self.features.dialogs) else {
            return refuse(reply, "stale");
        };
        match dialog::press_line(press, &shown) {
            Err(why) => refuse(reply, why),
            Ok(line) => {
                if let Some(command) = &line.command {
                    self.filter.add_user_command(command);
                }
                self.press = Some(reply);
                let _ = stdin.send(line.line);
            }
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
        // A press still waiting ends unanswered; the dialog went with the client.
        self.press = None;
        self.dialog_cap = false;
        let had_dialog = {
            let mut shared = self.shared.lock();
            shared.running = false;
            shared.stats = SessionStats::default();
            shared.dialog.take().is_some()
        };
        if had_dialog {
            self.env.emit(&self.id, AfkEventKind::Dialog);
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
        self.append_line(raw, origin, kind_override, None, None);
    }

    /// One console line: classified, masked and cut as always; its look and sender only when the
    /// account's rules leave the line as it came (a normal line, nothing masked).
    fn append_line(
        &mut self,
        raw: &str,
        origin: ConsoleOrigin,
        kind_override: Option<ConsoleLineKind>,
        look: Option<LineLooks>,
        who: Option<String>,
    ) {
        let kind = kind_override.unwrap_or_else(|| self.filter.classify(raw));
        let masked = self.filter.mask(raw);
        let as_it_came = kind == ConsoleLineKind::Normal && masked == raw;
        let look = look.filter(|_| as_it_came).and_then(|look| {
            let look = looks::unmask(look, &|text| self.filter.mask(text));
            looks::fit(match cut_units(raw) {
                Some(kept) => looks::cut(look, kept),
                None => look,
            })
        });
        let line = ConsoleLine {
            at: Timestamp::now(),
            text: cap_line(&masked),
            kind,
            origin,
            looks: None,
            who: None,
        };
        // The log and the event carry the text alone: the view reads the look from the console.
        if let Some(log) = &self.log {
            log.write(&line);
        }
        let kept = ConsoleLine {
            looks: look,
            who: who.filter(|_| as_it_came),
            ..line.clone()
        };
        self.shared.lock().console.push(kept);
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
