//! All AFK sessions of the launcher.
//!
//! [`AfkManager`] owns one session actor per account, prepares the client build, runs the
//! staggered bulk connects (also used for auto-start, which the legacy launcher started all at
//! once) and is the single entry point the app shell talks to. It is cheap to clone.
//!
//! The client build is made ready by one background preparation at a time (see
//! [`crate::client`]): every connect that needs it waits on that one, its card showing the step
//! ([`AfkManager::client_step`]), and leaves it when the account is stopped. The last waiter to
//! leave stops the preparation (and its download).

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use pl_net::{CancellationToken, DownloadOptions, DownloadProgress, Downloader, ProgressFn};
use serde::{Deserialize, Serialize};
use tokio::sync::watch;

use crate::client::{ClientProblem, ClientStep, ReleaseSource};
use crate::console::ConsoleLine;
use crate::error::{AfkError, Result};
use crate::form::AccountSettings;
use crate::housekeeping::{DEFAULT_LOG_CAP_BYTES, purge_login_scripts};
use crate::model::{AfkAccount, AfkSettings};
use crate::orphans;
use crate::pacing::{ConnectPacer, MIN_CONNECT_SPACING};
use crate::page::SettingsPage;
use crate::process::{ClientSpawner, ProcessSpawner};
use crate::restart::RestartSchedule;
use crate::runtime::{AfkClientRelease, AfkRuntime, ClientExe};
use crate::session::{
    AfkActivity, AfkEventKind, EventSink, READER_DRAIN, Session, SessionEnv, SessionSnapshot,
    SessionState, SessionStats,
};
use crate::store::AccountStore;
use crate::versions::VersionCatalog;

/// How often a client download reports its progress to the waiting accounts.
const PROGRESS_INTERVAL: Duration = Duration::from_millis(250);

/// Timing knobs (production defaults; tests shrink them).
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    /// Gap between accounts in a bulk connect (legacy: 15 s).
    pub stagger: Duration,
    /// Bounds and ramp of the automatic restart back-off; the delays themselves are each
    /// account's AutoRelog delay.
    pub restart: RestartSchedule,
    /// Time a client gets to leave after `/quit` before it is killed (legacy: 4 s).
    pub stop_grace: Duration,
    /// Size cap of the per-account log files.
    pub log_cap_bytes: u64,
    /// Least time between two connection attempts to one server, whichever accounts make them
    /// (see [`crate::pacing`]); zero turns the pacing off.
    pub connect_spacing: Duration,
    /// How long the last output of a client that exited may still take to be read before its
    /// exit is decided without it (its final bridge events tell why it left). Only a client that
    /// left its pipes to a process of its own makes this run out (2 s).
    pub reader_drain: Duration,
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            stagger: Duration::from_secs(15),
            restart: RestartSchedule::default(),
            stop_grace: Duration::from_secs(4),
            log_cap_bytes: DEFAULT_LOG_CAP_BYTES,
            connect_spacing: MIN_CONNECT_SPACING,
            reader_drain: READER_DRAIN,
        }
    }
}

/// Optional parts of a manager.
#[derive(Default)]
pub struct ManagerOptions {
    pub config: ManagerConfig,
    /// Needed to install client builds; without it only installed builds are used.
    pub downloader: Option<Downloader>,
    /// Also told the client download's progress (the waiting accounts get it as
    /// [`AfkEventKind::Client`] events either way).
    pub download_progress: Option<ProgressFn>,
    /// Refreshes the content index when a connect finds no client installed and no build known
    /// (see [`crate::client`]). Without it such a connect fails with
    /// [`ClientProblem::NotPublished`].
    pub release_source: Option<ReleaseSource>,
    /// Defaults to [`ProcessSpawner`].
    pub spawner: Option<Arc<dyn ClientSpawner>>,
    /// The player's main launcher nickname (written as the bot owner).
    pub launcher_nickname: String,
}

/// Result of a staggered bulk connect.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BulkOutcome {
    /// Accounts that were started.
    pub started: Vec<String>,
    /// Accounts skipped because they were already active.
    pub skipped: Vec<String>,
    /// Accounts that could not be started (for example no client build available).
    pub failed: Vec<String>,
    /// The run was cancelled before reaching every account.
    pub cancelled: bool,
}

/// Result of saving the settings page.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SavedSettings {
    pub account: AfkAccount,
    /// The running client was launched with older settings and keeps them until it reconnects
    /// (legacy hint, now only when something actually changed).
    pub reconnect_needed: bool,
}

struct Inner {
    store: Arc<AccountStore>,
    runtime: Arc<AfkRuntime>,
    env: Arc<SessionEnv>,
    config: ManagerConfig,
    downloader: Option<Downloader>,
    download_progress: Option<ProgressFn>,
    release_source: Option<ReleaseSource>,
    release: RwLock<Option<AfkClientRelease>>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    bulk: Mutex<Option<BulkRun>>,
    prep: Mutex<ClientPrep>,
}

/// A staggered bulk connect in progress.
struct BulkRun {
    token: CancellationToken,
    /// Accounts it has not reached yet (the one it is connecting included). An account the
    /// player stops leaves it.
    remaining: Vec<String>,
}

/// The AFK client being made ready for connects, and the connects it could not serve.
#[derive(Default)]
struct ClientPrep {
    /// The preparation in progress: one at a time, later connects join it.
    current: Option<Preparation>,
    /// Why the last connect of an account could not get the client.
    failed: HashMap<String, ClientProblem>,
    /// Numbers preparations and waiters, so a newer one is told apart from an older one.
    serial: u64,
}

struct Preparation {
    serial: u64,
    waiters: HashMap<String, Waiter>,
    /// Shown on the waiting accounts' cards. `None` until there is something to show: a client
    /// that needs neither a download nor an index refresh is ready before anyone could notice.
    step: Option<ClientStep>,
    /// Stops the preparation: the last waiter left.
    cancel: CancellationToken,
    outcome: watch::Receiver<Option<PrepOutcome>>,
}

struct Waiter {
    /// Start the account's session once the client is ready (a connect). A bulk run's accounts
    /// only wait; the run connects them in turn afterwards.
    start: bool,
    serial: u64,
    /// Fires when the account stops waiting (stopped, removed, its run cancelled).
    left: CancellationToken,
}

#[derive(Debug, Clone)]
enum PrepOutcome {
    Ready,
    Failed {
        problem: ClientProblem,
        details: String,
    },
    Cancelled,
}

impl PrepOutcome {
    fn from_error(error: AfkError) -> Self {
        if error.is_cancelled() {
            return Self::Cancelled;
        }
        Self::Failed {
            problem: ClientProblem::of(&error),
            details: error.to_string(),
        }
    }
}

/// What joining a preparation gave a caller.
struct Joined {
    outcome: watch::Receiver<Option<PrepOutcome>>,
    /// Waiters this call added (accounts already waiting keep their place), by serial.
    added: Vec<(String, u64)>,
    /// Fire when each of the caller's accounts stops waiting.
    left: Vec<CancellationToken>,
}

#[derive(Clone)]
pub struct AfkManager {
    inner: Arc<Inner>,
}

impl std::fmt::Debug for AfkManager {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AfkManager")
            .field("sessions", &self.inner.sessions.lock().len())
            .finish_non_exhaustive()
    }
}

/// Clears the bulk-run slot when a run ends, however it ends.
struct BulkGuard<'a>(&'a Mutex<Option<BulkRun>>);

impl Drop for BulkGuard<'_> {
    fn drop(&mut self) {
        *self.0.lock() = None;
    }
}

impl AfkManager {
    /// Creates the manager. No client of ours runs yet, so clients a crashed launcher left
    /// running are stopped here (only processes this launcher recorded starting, see `orphans`)
    /// and the login scripts a crash left behind are deleted.
    pub fn new(
        store: Arc<AccountStore>,
        runtime: Arc<AfkRuntime>,
        events: EventSink,
        options: ManagerOptions,
    ) -> Self {
        let accounts_dir = store.paths().accounts_dir();
        let stopped = orphans::stop_orphans(&accounts_dir, &mut orphans::OsProcesses::new());
        if !stopped.is_empty() {
            tracing::warn!(
                ?stopped,
                "stopped AFK clients a previous launcher left running"
            );
        }
        let purged = purge_login_scripts(&accounts_dir);
        if purged > 0 {
            tracing::info!(purged, "deleted stale AFK login scripts");
        }
        let resolver_runtime = runtime.clone();
        let env = Arc::new(SessionEnv {
            store: store.clone(),
            resolve_client: Arc::new(move || resolver_runtime.resolve()),
            spawner: options.spawner.unwrap_or_else(|| Arc::new(ProcessSpawner)),
            launcher_nickname: RwLock::new(options.launcher_nickname),
            events,
            schedule: options.config.restart,
            stop_grace: options.config.stop_grace,
            log_cap: options.config.log_cap_bytes,
            pacer: ConnectPacer::default(),
            connect_spacing: options.config.connect_spacing,
            reader_drain: options.config.reader_drain,
        });
        Self {
            inner: Arc::new(Inner {
                store,
                runtime,
                env,
                config: options.config,
                downloader: options.downloader,
                download_progress: options.download_progress,
                release_source: options.release_source,
                release: RwLock::new(None),
                sessions: Mutex::new(HashMap::new()),
                bulk: Mutex::new(None),
                prep: Mutex::new(ClientPrep::default()),
            }),
        }
    }

    pub fn store(&self) -> &Arc<AccountStore> {
        &self.inner.store
    }

    pub fn runtime(&self) -> &Arc<AfkRuntime> {
        &self.inner.runtime
    }

    /// Bot owner for configs written from now on.
    pub fn set_launcher_nickname(&self, nickname: impl Into<String>) {
        *self.inner.env.launcher_nickname.write() = nickname.into();
    }

    /// The client build the content index currently publishes (`None` when unknown/offline).
    pub fn set_client_release(&self, release: Option<AfkClientRelease>) {
        *self.inner.release.write() = release;
    }

    /// The game versions and the recommended one the content index publishes with the client
    /// (`None`: nothing usable published, the built-in list applies). Applies to every config
    /// written and every settings page loaded from now on; stored choices are never rewritten.
    pub fn set_versions(&self, versions: Option<VersionCatalog>) {
        self.inner.store.set_versions(versions.unwrap_or_default());
    }

    /// The game versions accounts choose from now.
    pub fn versions(&self) -> Arc<VersionCatalog> {
        self.inner.store.versions()
    }

    fn session(&self, account_id: &str) -> Arc<Session> {
        self.inner
            .sessions
            .lock()
            .entry(account_id.to_owned())
            .or_insert_with(|| Session::spawn(account_id.to_owned(), self.inner.env.clone()))
            .clone()
    }

    fn existing(&self, account_id: &str) -> Option<Arc<Session>> {
        self.inner.sessions.lock().get(account_id).cloned()
    }

    fn all_sessions(&self) -> Vec<Arc<Session>> {
        self.inner.sessions.lock().values().cloned().collect()
    }

    fn is_active(&self, account_id: &str) -> bool {
        self.existing(account_id).is_some_and(|s| s.is_active())
    }

    // --- The client build ----------------------------------------------------------------------

    /// What the account's card shows about the AFK client: the preparation it waits for (once
    /// there is something to show), or why its last connect could not get the client.
    pub fn client_step(&self, account_id: &str) -> Option<ClientStep> {
        let prep = self.inner.prep.lock();
        if let Some(preparation) = &prep.current
            && preparation.waiters.contains_key(account_id)
        {
            return preparation.step;
        }
        prep.failed
            .get(account_id)
            .map(|&problem| ClientStep::Failed { problem })
    }

    /// Accounts waiting for the client now.
    fn waiting_ids(&self) -> Vec<String> {
        self.inner
            .prep
            .lock()
            .current
            .as_ref()
            .map(|p| p.waiters.keys().cloned().collect())
            .unwrap_or_default()
    }

    fn emit_client(&self, account_id: &str, step: Option<ClientStep>) {
        self.inner.env.emit(account_id, AfkEventKind::Client(step));
    }

    /// Waits until the client is ready for `ids`, joining the preparation in progress or starting
    /// one. With `start`, each account's session starts once the client is ready (a connect).
    /// `run`: a bulk run's token; cancelling the run takes the accounts it added off the wait.
    /// Fails with [`AfkError::Cancelled`] when the accounts stopped waiting and with
    /// [`AfkError::ClientUnavailable`] when the client cannot be had.
    async fn wait_for_client(
        &self,
        ids: &[String],
        start: bool,
        run: Option<&CancellationToken>,
    ) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        let Joined {
            mut outcome,
            added,
            left,
        } = self.join_preparation(ids, start);
        let all_left = async {
            for token in &left {
                token.cancelled().await;
            }
        };
        let run_cancelled = async {
            match run {
                Some(token) => token.cancelled().await,
                None => std::future::pending().await,
            }
        };
        tokio::select! {
            ready = outcome.wait_for(Option::is_some) => {
                match ready.ok().and_then(|value| (*value).clone()) {
                    Some(PrepOutcome::Ready) => Ok(()),
                    Some(PrepOutcome::Failed { problem, details }) => {
                        Err(AfkError::ClientUnavailable { problem, details })
                    }
                    Some(PrepOutcome::Cancelled) | None => Err(AfkError::Cancelled),
                }
            }
            () = all_left => Err(AfkError::Cancelled),
            () = run_cancelled => {
                for (id, serial) in &added {
                    self.leave_run_wait(id, *serial);
                }
                Err(AfkError::Cancelled)
            }
        }
    }

    /// Puts `ids` on the preparation in progress, starting one when none runs (or the one there
    /// is being stopped). A new wait forgets the account's last failure.
    fn join_preparation(&self, ids: &[String], start: bool) -> Joined {
        let mut events = Vec::new();
        let (joined, spawn) = {
            let mut guard = self.inner.prep.lock();
            let prep = &mut *guard;
            let forgotten: Vec<&String> = ids
                .iter()
                .filter(|id| prep.failed.remove(id.as_str()).is_some())
                .collect();
            let mut spawn = None;
            if prep
                .current
                .as_ref()
                .is_none_or(|current| current.cancel.is_cancelled())
            {
                prep.serial += 1;
                let (sender, receiver) = watch::channel(None);
                let cancel = CancellationToken::new();
                prep.current = Some(Preparation {
                    serial: prep.serial,
                    waiters: HashMap::new(),
                    step: None,
                    cancel: cancel.clone(),
                    outcome: receiver,
                });
                spawn = Some((prep.serial, sender, cancel));
            }
            let preparation = prep.current.as_mut().expect("a preparation was just set");
            let mut added = Vec::new();
            let mut left = Vec::new();
            for id in ids {
                match preparation.waiters.get_mut(id) {
                    Some(waiter) => {
                        waiter.start |= start;
                        left.push(waiter.left.clone());
                    }
                    None => {
                        prep.serial += 1;
                        let token = CancellationToken::new();
                        preparation.waiters.insert(
                            id.clone(),
                            Waiter {
                                start,
                                serial: prep.serial,
                                left: token.clone(),
                            },
                        );
                        added.push((id.clone(), prep.serial));
                        left.push(token);
                    }
                }
            }
            // Cards that showed a failure, or join a preparation that shows a step, change now.
            for id in ids {
                let newly = added.iter().any(|(added, _)| added == id);
                if forgotten.contains(&id) || (newly && preparation.step.is_some()) {
                    events.push((id.clone(), preparation.step));
                }
            }
            let joined = Joined {
                outcome: preparation.outcome.clone(),
                added,
                left,
            };
            (joined, spawn)
        };
        for (id, step) in events {
            self.emit_client(&id, step);
        }
        if let Some((serial, sender, cancel)) = spawn {
            let manager = self.clone();
            tokio::spawn(async move { manager.run_preparation(serial, sender, cancel).await });
        }
        joined
    }

    /// The account stops waiting for the client and its last failure is forgotten. The last
    /// waiter to leave stops the preparation (and its download).
    fn leave_wait(&self, account_id: &str) {
        let changed = {
            let mut prep = self.inner.prep.lock();
            let mut changed = prep.failed.remove(account_id).is_some();
            if let Some(preparation) = prep.current.as_mut()
                && let Some(waiter) = preparation.waiters.remove(account_id)
            {
                waiter.left.cancel();
                if preparation.waiters.is_empty() {
                    preparation.cancel.cancel();
                }
                changed |= preparation.step.is_some();
            }
            changed
        };
        if changed {
            self.emit_client(account_id, None);
        }
    }

    /// A cancelled bulk run takes the waiter it added off the wait, unless a connect took it over
    /// since (that connect still wants the client).
    fn leave_run_wait(&self, account_id: &str, serial: u64) {
        let shown = {
            let mut prep = self.inner.prep.lock();
            let Some(preparation) = prep.current.as_mut() else {
                return;
            };
            let ours = preparation
                .waiters
                .get(account_id)
                .is_some_and(|w| w.serial == serial && !w.start);
            if !ours {
                return;
            }
            if let Some(waiter) = preparation.waiters.remove(account_id) {
                waiter.left.cancel();
            }
            if preparation.waiters.is_empty() {
                preparation.cancel.cancel();
            }
            preparation.step.is_some()
        };
        if shown {
            self.emit_client(account_id, None);
        }
    }

    /// Shows `step` on every waiting account's card.
    fn set_step(&self, step: ClientStep) {
        let waiters: Vec<String> = {
            let mut prep = self.inner.prep.lock();
            let Some(preparation) = prep.current.as_mut() else {
                return;
            };
            if preparation.step == Some(step) {
                return;
            }
            preparation.step = Some(step);
            preparation.waiters.keys().cloned().collect()
        };
        for id in waiters {
            self.emit_client(&id, Some(step));
        }
    }

    /// The background task of one preparation: gets the client, then tells its waiters (starting
    /// the sessions of those that asked) before the outcome wakes the callers.
    async fn run_preparation(
        self,
        serial: u64,
        sender: watch::Sender<Option<PrepOutcome>>,
        cancel: CancellationToken,
    ) {
        let outcome = self.prepare(&cancel).await;
        if let PrepOutcome::Failed { problem, details } = &outcome {
            tracing::warn!(?problem, %details, "the AFK client could not be prepared");
        }
        let (waiters, shown, forgotten) = {
            let mut prep = self.inner.prep.lock();
            // A newer preparation replaced this stopped one: its waiters are not ours.
            let ours = prep.current.as_ref().is_some_and(|p| p.serial == serial);
            let (waiters, shown) = match prep.current.take_if(|_| ours) {
                Some(preparation) => (preparation.waiters, preparation.step.is_some()),
                None => (HashMap::new(), false),
            };
            let mut forgotten = Vec::new();
            match &outcome {
                // The client works now: older failures no longer say anything true.
                PrepOutcome::Ready => forgotten = prep.failed.drain().map(|(id, _)| id).collect(),
                PrepOutcome::Failed { problem, .. } => {
                    for id in waiters.keys() {
                        prep.failed.insert(id.clone(), *problem);
                    }
                }
                PrepOutcome::Cancelled => {}
            }
            (waiters, shown, forgotten)
        };
        for id in forgotten.iter().filter(|id| !waiters.contains_key(*id)) {
            self.emit_client(id, None);
        }
        for (id, waiter) in &waiters {
            match &outcome {
                PrepOutcome::Ready => {
                    // Started before the card stops showing the client, so it goes straight on
                    // to "starting".
                    if waiter.start && self.inner.store.account(id).is_some() {
                        self.session(id).start();
                    }
                    if shown {
                        self.emit_client(id, None);
                    }
                }
                PrepOutcome::Failed { problem, .. } => {
                    self.emit_client(id, Some(ClientStep::Failed { problem: *problem }))
                }
                PrepOutcome::Cancelled => {
                    if shown {
                        self.emit_client(id, None);
                    }
                }
            }
        }
        let _ = sender.send(Some(outcome));
    }

    /// Gets a client ready for new sessions. While sessions run, an installed build is used as it
    /// is (legacy: never update under a running client). Otherwise the known build is installed
    /// when needed (falling back to an installed one when that fails, for offline starts). With
    /// nothing installed and no build known, the content index is refreshed once first; a known
    /// build that cannot be had while nothing is installed gets one more look at the index too.
    async fn prepare(&self, cancel: &CancellationToken) -> PrepOutcome {
        let runtime = &self.inner.runtime;
        if self.all_sessions().iter().any(|s| s.is_running()) && runtime.resolve().is_some() {
            return PrepOutcome::Ready;
        }
        let mut release = self.inner.release.read().clone();
        let mut refreshed = false;
        if release.is_none() && runtime.resolve().is_none() {
            // The index this launcher has may predate the client's first publish, or it was
            // offline when it last looked.
            match self.refresh_release(cancel).await {
                Ok(found) => {
                    release = found;
                    refreshed = true;
                }
                Err(error) => return PrepOutcome::from_error(error),
            }
        }
        let mut result = self.ensure(release.as_ref(), cancel).await;
        if let Err(error) = &result
            && !refreshed
            && release.is_some()
            && !error.is_cancelled()
            && !error.is_network()
        {
            // The known build is gone from the mirrors, or a newer one replaced it since.
            tracing::info!(%error, "AFK client build unavailable, checking the content index again");
            match self.refresh_release(cancel).await {
                Ok(Some(newer)) if release.as_ref() != Some(&newer) => {
                    result = self.ensure(Some(&newer), cancel).await;
                }
                Ok(None) => result = Err(AfkError::NoClientRelease),
                Err(refresh) if refresh.is_cancelled() => result = Err(refresh),
                // The same build, or the index could not be reached: the first error stands.
                Ok(Some(_)) | Err(_) => {}
            }
        }
        match result {
            Ok(_) => {
                self.prune().await;
                PrepOutcome::Ready
            }
            Err(error) => PrepOutcome::from_error(error),
        }
    }

    /// Refreshes the content index for the client build it publishes (cards show "checking").
    async fn refresh_release(
        &self,
        cancel: &CancellationToken,
    ) -> Result<Option<AfkClientRelease>> {
        let Some(source) = self.inner.release_source.clone() else {
            return Ok(None);
        };
        self.set_step(ClientStep::Checking);
        let found = tokio::select! {
            found = source() => found?,
            () = cancel.cancelled() => return Err(AfkError::Cancelled),
        };
        self.set_client_release(found.clone());
        Ok(found)
    }

    /// [`AfkRuntime::ensure`] with the waiters' progress and the preparation's cancel.
    async fn ensure(
        &self,
        release: Option<&AfkClientRelease>,
        cancel: &CancellationToken,
    ) -> Result<ClientExe> {
        let options = DownloadOptions {
            cancel: cancel.clone(),
            progress: Some(self.progress_fn()),
            progress_interval: PROGRESS_INTERVAL,
            ..DownloadOptions::default()
        };
        self.inner
            .runtime
            .ensure(self.inner.downloader.as_ref(), release, &options)
            .await
    }

    /// Download progress → the waiters' step: downloading, then installing once the file is in.
    fn progress_fn(&self) -> ProgressFn {
        let manager = self.clone();
        Arc::new(move |progress: DownloadProgress| {
            let step = if progress.total_files > 0 && progress.done_files == progress.total_files {
                ClientStep::Installing
            } else {
                ClientStep::Downloading {
                    done: progress.done_bytes,
                    total: progress.total_bytes,
                }
            };
            manager.set_step(step);
            if let Some(external) = &manager.inner.download_progress {
                external(progress);
            }
        })
    }

    /// Removes old client builds no session uses.
    async fn prune(&self) {
        let in_use: Vec<String> = self
            .all_sessions()
            .iter()
            .filter_map(|s| s.client_version())
            .collect();
        let runtime = self.inner.runtime.clone();
        let pruned = tokio::task::spawn_blocking(move || runtime.prune(&in_use)).await;
        if let Ok(removed) = pruned
            && !removed.is_empty()
        {
            tracing::info!(?removed, "removed old AFK client builds");
        }
    }

    // --- Sessions ------------------------------------------------------------------------------

    /// Connects one account now (a pending reconnect is replaced by an immediate one). When the
    /// client has to be fetched first the account waits for it, its card showing the progress
    /// ([`Self::client_step`]); [`Self::disconnect`] ends the wait with [`AfkError::Cancelled`].
    pub async fn connect(&self, account_id: &str) -> Result<()> {
        if self.inner.store.account(account_id).is_none() {
            return Err(AfkError::AccountNotFound(account_id.to_owned()));
        }
        if self.existing(account_id).is_some_and(|s| s.is_running()) {
            return Ok(());
        }
        self.wait_for_client(&[account_id.to_owned()], true, None)
            .await
    }

    /// Disconnects (`/quit`, killed after the grace period) or cancels a pending reconnect. An
    /// account waiting for the client stops waiting, and a bulk run skips it.
    pub fn disconnect(&self, account_id: &str) {
        self.bulk_reached(account_id);
        self.leave_wait(account_id);
        if let Some(session) = self.existing(account_id) {
            session.stop();
        }
    }

    /// Types a line into the account's client.
    pub fn send(&self, account_id: &str, line: impl Into<String>) -> Result<()> {
        self.existing(account_id)
            .ok_or(AfkError::NotRunning)?
            .send(line.into())
    }

    /// "Connect all": every inactive account, the first immediately, then one per stagger
    /// interval with a live countdown. Cancel with [`Self::cancel_bulk`].
    pub async fn connect_all(&self) -> Result<BulkOutcome> {
        let ids = self
            .inner
            .store
            .accounts()
            .into_iter()
            .map(|a| a.id)
            .collect();
        self.run_staggered(ids).await
    }

    /// Connects the auto-start accounts, staggered like [`Self::connect_all`].
    pub async fn auto_start(&self) -> Result<BulkOutcome> {
        self.auto_start_with(&[]).await
    }

    /// Connects the auto-start accounts and `also` (the accounts a launcher update stopped, see
    /// [`Self::resume_candidates`]) in one staggered run, in account order.
    pub async fn auto_start_with(&self, also: &[String]) -> Result<BulkOutcome> {
        let ids = self
            .inner
            .store
            .accounts()
            .into_iter()
            .filter(|a| a.auto_start || also.contains(&a.id))
            .map(|a| a.id)
            .collect();
        self.run_staggered(ids).await
    }

    /// Connects these accounts, staggered like [`Self::connect_all`] and in account order (ids
    /// of removed accounts are skipped).
    pub async fn connect_staggered(&self, ids: &[String]) -> Result<BulkOutcome> {
        let ids = self
            .inner
            .store
            .accounts()
            .into_iter()
            .filter(|a| ids.contains(&a.id))
            .map(|a| a.id)
            .collect();
        self.run_staggered(ids).await
    }

    /// The accounts to bring back after the launcher restarts (for an update): every account in
    /// game, connecting, waiting to reconnect or waiting for the client, and those a running bulk
    /// connect has yet to reach, in account order. Accounts being stopped or stopped for good are
    /// left out.
    pub fn resume_candidates(&self) -> Vec<String> {
        let queued = self
            .inner
            .bulk
            .lock()
            .as_ref()
            .map(|run| run.remaining.clone())
            .unwrap_or_default();
        let waiting = self.waiting_ids();
        self.inner
            .store
            .accounts()
            .into_iter()
            .map(|a| a.id)
            .filter(|id| {
                queued.contains(id)
                    || waiting.contains(id)
                    || matches!(
                        self.snapshot(id).state,
                        SessionState::Starting
                            | SessionState::Connecting
                            | SessionState::Connected { .. }
                            | SessionState::Reconnecting { .. }
                    )
            })
            .collect()
    }

    /// Cancels a running bulk connect (accounts already started keep running; the ones it had
    /// waiting for the client stop waiting).
    pub fn cancel_bulk(&self) {
        if let Some(run) = self.inner.bulk.lock().as_ref() {
            run.token.cancel();
        }
    }

    pub fn bulk_running(&self) -> bool {
        self.inner.bulk.lock().is_some()
    }

    /// The bulk run is done with this account (started, skipped, failed or stopped by the
    /// player).
    fn bulk_reached(&self, account_id: &str) {
        if let Some(run) = self.inner.bulk.lock().as_mut() {
            run.remaining.retain(|id| id != account_id);
        }
    }

    /// The running bulk connect still has this account to connect.
    fn in_bulk(&self, account_id: &str) -> bool {
        self.inner
            .bulk
            .lock()
            .as_ref()
            .is_some_and(|run| run.remaining.iter().any(|id| id == account_id))
    }

    async fn run_staggered(&self, ids: Vec<String>) -> Result<BulkOutcome> {
        let token = {
            let mut bulk = self.inner.bulk.lock();
            if bulk.is_some() {
                return Err(AfkError::Busy);
            }
            let token = CancellationToken::new();
            *bulk = Some(BulkRun {
                token: token.clone(),
                remaining: ids.clone(),
            });
            token
        };
        let _guard = BulkGuard(&self.inner.bulk);

        let mut outcome = BulkOutcome::default();
        // The run's accounts wait for the client together, every card showing it. A client that
        // cannot be had ends the run, and each of those accounts says why.
        let idle: Vec<String> = ids
            .iter()
            .filter(|id| !self.is_active(id))
            .cloned()
            .collect();
        if let Err(error) = self.wait_for_client(&idle, false, Some(&token)).await {
            if error.is_cancelled() {
                outcome.cancelled = true;
            } else {
                tracing::warn!(%error, "bulk connect could not get the AFK client");
                outcome.failed = idle.into_iter().filter(|id| self.in_bulk(id)).collect();
            }
            return Ok(outcome);
        }

        let mut started_one = false;
        for id in ids {
            if token.is_cancelled() {
                outcome.cancelled = true;
                break;
            }
            // Stopped (or removed) by the player since the run started.
            if !self.in_bulk(&id) {
                continue;
            }
            if self.is_active(&id) {
                self.bulk_reached(&id);
                outcome.skipped.push(id);
                continue;
            }
            if started_one {
                if !self.countdown(&id, &token).await {
                    outcome.cancelled = true;
                    break;
                }
                // Stopped while it waited for its turn.
                if !self.in_bulk(&id) {
                    continue;
                }
                // Connected by hand while waiting.
                if self.is_active(&id) {
                    self.bulk_reached(&id);
                    outcome.skipped.push(id);
                    continue;
                }
            }
            let result = self.connect(&id).await;
            self.bulk_reached(&id);
            match result {
                Ok(()) => outcome.started.push(id),
                // Stopped while its client was prepared.
                Err(error) if error.is_cancelled() => {}
                Err(error) => {
                    tracing::warn!(account = %id, %error, "bulk connect could not start an account");
                    outcome.failed.push(id);
                }
            }
            started_one = true;
        }
        Ok(outcome)
    }

    /// Waits one stagger interval, emitting the remaining whole seconds. False when cancelled;
    /// ends early when the account leaves the run or connects meanwhile.
    async fn countdown(&self, account_id: &str, token: &CancellationToken) -> bool {
        let tick = Duration::from_secs(1);
        let mut remaining = self.inner.config.stagger;
        let mut completed = true;
        while !remaining.is_zero() {
            if token.is_cancelled() {
                completed = false;
                break;
            }
            if self.is_active(account_id) || !self.in_bulk(account_id) {
                break;
            }
            let secs = remaining.as_secs() + u64::from(remaining.subsec_nanos() > 0);
            self.inner.env.emit(
                account_id,
                AfkEventKind::Countdown {
                    secs: u32::try_from(secs).unwrap_or(u32::MAX),
                },
            );
            let step = remaining.min(tick);
            tokio::select! {
                () = token.cancelled() => {
                    completed = false;
                    break;
                }
                () = tokio::time::sleep(step) => remaining -= step,
            }
        }
        self.inner
            .env
            .emit(account_id, AfkEventKind::Countdown { secs: 0 });
        completed
    }

    /// Stops everything (cancelling a bulk connect and every wait for the client) and waits until
    /// every client has exited (each is killed after the grace period). Call before the launcher
    /// exits.
    pub async fn stop_all(&self) {
        self.cancel_bulk();
        for id in self.waiting_ids() {
            self.leave_wait(&id);
        }
        let sessions = self.all_sessions();
        for session in &sessions {
            session.stop();
        }
        let wait = self.inner.config.stop_grace + Duration::from_secs(3);
        futures::future::join_all(sessions.iter().map(|s| s.wait_idle(wait))).await;
    }

    /// Stops everything and ends the session actors.
    pub async fn shutdown(&self) {
        self.stop_all().await;
        let sessions: Vec<Arc<Session>> =
            self.inner.sessions.lock().drain().map(|(_, s)| s).collect();
        for session in sessions {
            session.shutdown().await;
        }
    }

    /// True while any client runs, connects or waits to reconnect, an account waits for the
    /// client, or a bulk connect is in progress: the launcher must not hide or exit then.
    pub fn any_active(&self) -> bool {
        self.bulk_running()
            || !self.waiting_ids().is_empty()
            || self.all_sessions().iter().any(|s| s.is_active())
    }

    /// How many accounts are in game and how many are on their way there (waiting for the client
    /// counts as on the way).
    pub fn activity(&self) -> AfkActivity {
        let snapshots = self.snapshots();
        let mut activity = AfkActivity::of(snapshots.iter().map(|snapshot| &snapshot.state));
        let waiting = self.waiting_ids();
        activity.connecting += snapshots
            .iter()
            .filter(|s| waiting.contains(&s.account_id) && !s.state.is_active())
            .count();
        activity
    }

    /// State of every account, in account order.
    pub fn snapshots(&self) -> Vec<SessionSnapshot> {
        self.inner
            .store
            .accounts()
            .iter()
            .map(|a| self.snapshot(&a.id))
            .collect()
    }

    pub fn snapshot(&self, account_id: &str) -> SessionSnapshot {
        self.existing(account_id).map_or_else(
            || SessionSnapshot {
                account_id: account_id.to_owned(),
                state: SessionState::Stopped,
                stats: SessionStats::default(),
                running: false,
                client_version: None,
            },
            |s| s.snapshot(),
        )
    }

    /// The account's console buffer (last 500 lines, masked).
    pub fn console(&self, account_id: &str) -> Vec<ConsoleLine> {
        self.existing(account_id)
            .map(|s| s.console())
            .unwrap_or_default()
    }

    // --- Accounts ------------------------------------------------------------------------------

    pub fn accounts(&self) -> Vec<AfkAccount> {
        self.inner.store.accounts()
    }

    pub fn add_account(&self, nickname: &str) -> Result<AfkAccount> {
        self.inner.store.add_account(nickname)
    }

    /// Removes the account and its keychain entry, stops its client, then deletes its folder. The
    /// account leaves the list first, so nothing (a bulk connect reaching it, a click, an
    /// automatic restart, the client it waited for) can start it again while its client is being
    /// stopped.
    pub async fn remove_account(&self, account_id: &str) -> Result<()> {
        let store = self.inner.store.clone();
        let id = account_id.to_owned();
        let removed = tokio::task::spawn_blocking(move || store.remove_account(&id))
            .await
            .map_err(|e| AfkError::InvalidData(format!("remove task failed: {e}")))??;
        self.bulk_reached(account_id);
        self.leave_wait(account_id);
        let session = self.inner.sessions.lock().remove(account_id);
        if let Some(session) = session {
            session.stop();
            session
                .wait_idle(self.inner.config.stop_grace + Duration::from_secs(3))
                .await;
            session.shutdown().await;
        }
        if !removed {
            return Err(AfkError::AccountNotFound(account_id.to_owned()));
        }
        let dir = self.inner.store.paths().account_dir(account_id);
        if let Err(error) = std::fs::remove_dir_all(&dir)
            && error.kind() != std::io::ErrorKind::NotFound
        {
            tracing::warn!(account = %account_id, %error, "could not delete the AFK account folder");
        }
        Ok(())
    }

    /// The settings page (including login commands; may block on the keychain).
    pub fn settings_form(&self, account_id: &str) -> Result<AccountSettings> {
        self.inner.store.settings_form(account_id)
    }

    /// Validates and saves the settings page (may block on the keychain). A running client keeps
    /// its settings until it reconnects; automatic restarts always use the saved settings.
    pub fn save_settings(
        &self,
        account_id: &str,
        settings: &AccountSettings,
    ) -> Result<SavedSettings> {
        let account = self.inner.store.save_settings(account_id, settings)?;
        Ok(SavedSettings {
            account,
            reconnect_needed: self.settings_changed(account_id),
        })
    }

    /// The settings page for the UI, secrets left in the keychain (may block on the keychain).
    pub fn settings_page(&self, account_id: &str) -> Result<SettingsPage> {
        self.inner.store.settings_page(account_id)
    }

    /// Validates and saves a page from [`Self::settings_page`] (may block on the keychain).
    pub fn save_settings_page(
        &self,
        account_id: &str,
        page: SettingsPage,
    ) -> Result<SavedSettings> {
        let account = self.inner.store.save_settings_page(account_id, page)?;
        Ok(SavedSettings {
            account,
            reconnect_needed: self.settings_changed(account_id),
        })
    }

    /// True while the account's client runs with settings older than the saved ones: they apply
    /// after [`Self::reconnect`] (or the next automatic restart).
    pub fn settings_changed(&self, account_id: &str) -> bool {
        self.existing(account_id)
            .and_then(|s| s.launched_revision())
            .is_some_and(|launched| launched != self.inner.store.revision(account_id))
    }

    /// Restarts the account's client so it picks up the saved settings: an active session is
    /// stopped (`/quit`, killed after the grace period) and launched again. An idle account just
    /// connects.
    pub async fn reconnect(&self, account_id: &str) -> Result<()> {
        if self.inner.store.account(account_id).is_none() {
            return Err(AfkError::AccountNotFound(account_id.to_owned()));
        }
        if let Some(session) = self.existing(account_id).filter(|s| s.is_active()) {
            session.stop();
            let wait = self.inner.config.stop_grace + Duration::from_secs(3);
            if !session.wait_idle(wait).await {
                tracing::warn!(account = %account_id, "AFK client did not stop before reconnecting");
            }
        }
        self.connect(account_id).await
    }

    pub fn afk_settings(&self) -> AfkSettings {
        self.inner.store.afk_settings()
    }

    pub fn set_show_script_commands(&self, show: bool) -> Result<bool> {
        self.inner.store.set_show_script_commands(show)
    }
}

/// Compile-time check: the app shell spawns these futures on a multi-threaded runtime.
#[cfg(test)]
#[allow(dead_code)]
fn manager_futures_are_send(manager: &AfkManager) {
    fn send<T: Send>(_: T) {}
    send(manager.connect("id"));
    send(manager.connect_all());
    send(manager.auto_start());
    send(manager.auto_start_with(&[]));
    send(manager.connect_staggered(&[]));
    send(manager.stop_all());
    send(manager.shutdown());
    send(manager.remove_account("id"));
    send(manager.reconnect("id"));
    send(
        manager
            .clone()
            .run_preparation(0, watch::channel(None).0, CancellationToken::new()),
    );
}
