//! All AFK sessions of the launcher.
//!
//! [`AfkManager`] owns one session actor per account, prepares the client build, runs the
//! staggered bulk connects (also used for auto-start, which the legacy launcher started all at
//! once) and is the single entry point the app shell talks to. It is cheap to clone.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::Duration;

use parking_lot::{Mutex, RwLock};
use pl_net::{CancellationToken, DownloadOptions, Downloader, ProgressFn};
use serde::{Deserialize, Serialize};

use crate::console::ConsoleLine;
use crate::error::{AfkError, Result};
use crate::form::AccountSettings;
use crate::housekeeping::{DEFAULT_LOG_CAP_BYTES, purge_login_scripts};
use crate::model::{AfkAccount, AfkSettings};
use crate::orphans;
use crate::page::SettingsPage;
use crate::process::{ClientSpawner, ProcessSpawner};
use crate::restart::RestartSchedule;
use crate::runtime::{AfkClientRelease, AfkRuntime, ClientExe};
use crate::session::{
    AfkActivity, AfkEventKind, EventSink, Session, SessionEnv, SessionSnapshot, SessionState,
    SessionStats,
};
use crate::store::AccountStore;
use crate::versions::VersionCatalog;

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
}

impl Default for ManagerConfig {
    fn default() -> Self {
        Self {
            stagger: Duration::from_secs(15),
            restart: RestartSchedule::default(),
            stop_grace: Duration::from_secs(4),
            log_cap_bytes: DEFAULT_LOG_CAP_BYTES,
        }
    }
}

/// Optional parts of a manager.
#[derive(Default)]
pub struct ManagerOptions {
    pub config: ManagerConfig,
    /// Needed to install client builds; without it only installed builds are used.
    pub downloader: Option<Downloader>,
    pub download_progress: Option<ProgressFn>,
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
    release: RwLock<Option<AfkClientRelease>>,
    sessions: Mutex<HashMap<String, Arc<Session>>>,
    bulk: Mutex<Option<BulkRun>>,
    prepare_lock: tokio::sync::Mutex<()>,
}

/// A staggered bulk connect in progress.
struct BulkRun {
    token: CancellationToken,
    /// Accounts it has not reached yet (the one it is connecting included).
    remaining: Vec<String>,
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
        });
        Self {
            inner: Arc::new(Inner {
                store,
                runtime,
                env,
                config: options.config,
                downloader: options.downloader,
                download_progress: options.download_progress,
                release: RwLock::new(None),
                sessions: Mutex::new(HashMap::new()),
                bulk: Mutex::new(None),
                prepare_lock: tokio::sync::Mutex::new(()),
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

    /// Installs/updates the client when needed and returns the build new sessions will run. While
    /// sessions run, an installed build is used as-is (legacy: never update under a running
    /// client). Concurrent callers wait for one preparation.
    pub async fn prepare_client(&self) -> Result<ClientExe> {
        let _guard = self.inner.prepare_lock.lock().await;
        let runtime = &self.inner.runtime;
        let sessions = self.all_sessions();
        if sessions.iter().any(|s| s.is_running())
            && let Some(client) = runtime.resolve()
        {
            return Ok(client);
        }
        let release = self.inner.release.read().clone();
        let options = DownloadOptions {
            progress: self.inner.download_progress.clone(),
            ..DownloadOptions::default()
        };
        let client = runtime
            .ensure(self.inner.downloader.as_ref(), release.as_ref(), &options)
            .await?;

        let in_use: Vec<String> = sessions.iter().filter_map(|s| s.client_version()).collect();
        let runtime = runtime.clone();
        let pruned = tokio::task::spawn_blocking(move || runtime.prune(&in_use)).await;
        if let Ok(removed) = pruned
            && !removed.is_empty()
        {
            tracing::info!(?removed, "removed old AFK client builds");
        }
        Ok(client)
    }

    /// Connects one account now (a pending reconnect is replaced by an immediate one).
    pub async fn connect(&self, account_id: &str) -> Result<()> {
        if self.inner.store.account(account_id).is_none() {
            return Err(AfkError::AccountNotFound(account_id.to_owned()));
        }
        if self.existing(account_id).is_some_and(|s| s.is_running()) {
            return Ok(());
        }
        self.prepare_client().await?;
        self.session(account_id).start();
        Ok(())
    }

    /// Disconnects (`/quit`, killed after the grace period) or cancels a pending reconnect.
    pub fn disconnect(&self, account_id: &str) {
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
    /// game, connecting or waiting to reconnect, and those a running bulk connect has yet to
    /// reach, in account order. Accounts being stopped or stopped for good are left out.
    pub fn resume_candidates(&self) -> Vec<String> {
        let queued = self
            .inner
            .bulk
            .lock()
            .as_ref()
            .map(|run| run.remaining.clone())
            .unwrap_or_default();
        self.inner
            .store
            .accounts()
            .into_iter()
            .map(|a| a.id)
            .filter(|id| {
                queued.contains(id)
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

    /// Cancels a running bulk connect (accounts already started keep running).
    pub fn cancel_bulk(&self) {
        if let Some(run) = self.inner.bulk.lock().as_ref() {
            run.token.cancel();
        }
    }

    pub fn bulk_running(&self) -> bool {
        self.inner.bulk.lock().is_some()
    }

    /// The bulk run is done with this account (started, skipped or failed).
    fn bulk_reached(&self, account_id: &str) {
        if let Some(run) = self.inner.bulk.lock().as_mut() {
            run.remaining.retain(|id| id != account_id);
        }
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
        let mut started_one = false;
        for id in ids {
            if token.is_cancelled() {
                outcome.cancelled = true;
                break;
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
                Err(error) => {
                    tracing::warn!(account = %id, %error, "bulk connect could not start an account");
                    outcome.failed.push(id);
                }
            }
            started_one = true;
        }
        Ok(outcome)
    }

    /// Waits one stagger interval, emitting the remaining whole seconds. False when cancelled.
    async fn countdown(&self, account_id: &str, token: &CancellationToken) -> bool {
        let tick = Duration::from_secs(1);
        let mut remaining = self.inner.config.stagger;
        let mut completed = true;
        while !remaining.is_zero() {
            if token.is_cancelled() {
                completed = false;
                break;
            }
            if self.is_active(account_id) {
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

    /// Stops everything (cancelling a bulk connect) and waits until every client has exited
    /// (each is killed after the grace period). Call before the launcher exits.
    pub async fn stop_all(&self) {
        self.cancel_bulk();
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

    /// True while any client runs, connects or waits to reconnect, or a bulk connect is in
    /// progress: the launcher must not hide or exit then.
    pub fn any_active(&self) -> bool {
        self.bulk_running() || self.all_sessions().iter().any(|s| s.is_active())
    }

    /// How many accounts are in game and how many are on their way there.
    pub fn activity(&self) -> AfkActivity {
        AfkActivity::of(self.snapshots().iter().map(|snapshot| &snapshot.state))
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
    /// automatic restart) can start it again while its client is being stopped.
    pub async fn remove_account(&self, account_id: &str) -> Result<()> {
        let store = self.inner.store.clone();
        let id = account_id.to_owned();
        let removed = tokio::task::spawn_blocking(move || store.remove_account(&id))
            .await
            .map_err(|e| AfkError::InvalidData(format!("remove task failed: {e}")))??;
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
    send(manager.prepare_client());
    send(manager.connect("id"));
    send(manager.connect_all());
    send(manager.auto_start());
    send(manager.auto_start_with(&[]));
    send(manager.connect_staggered(&[]));
    send(manager.stop_all());
    send(manager.shutdown());
    send(manager.remove_account("id"));
    send(manager.reconnect("id"));
}
