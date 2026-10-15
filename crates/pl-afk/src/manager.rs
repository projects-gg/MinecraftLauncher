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
use crate::process::{ClientSpawner, ProcessSpawner};
use crate::restart::RestartSchedule;
use crate::runtime::{AfkClientRelease, AfkRuntime, ClientExe};
use crate::session::{
    AfkEventKind, EventSink, Session, SessionEnv, SessionSnapshot, SessionState, SessionStats,
};
use crate::store::AccountStore;

/// Timing knobs (production defaults; tests shrink them).
#[derive(Debug, Clone)]
pub struct ManagerConfig {
    /// Gap between accounts in a bulk connect (legacy: 15 s).
    pub stagger: Duration,
    /// Automatic restart back-off (legacy: 15/30/45/60 s, then 60 s).
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
    /// The client runs with the old settings until it reconnects (legacy hint).
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
    bulk: Mutex<Option<CancellationToken>>,
    prepare_lock: tokio::sync::Mutex<()>,
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
struct BulkGuard<'a>(&'a Mutex<Option<CancellationToken>>);

impl Drop for BulkGuard<'_> {
    fn drop(&mut self) {
        *self.0.lock() = None;
    }
}

impl AfkManager {
    /// Creates the manager. No client runs yet, so login scripts left behind by a crash are
    /// deleted here.
    pub fn new(
        store: Arc<AccountStore>,
        runtime: Arc<AfkRuntime>,
        events: EventSink,
        options: ManagerOptions,
    ) -> Self {
        let purged = purge_login_scripts(&store.paths().accounts_dir());
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
        let ids = self
            .inner
            .store
            .accounts()
            .into_iter()
            .filter(|a| a.auto_start)
            .map(|a| a.id)
            .collect();
        self.run_staggered(ids).await
    }

    /// Cancels a running bulk connect (accounts already started keep running).
    pub fn cancel_bulk(&self) {
        if let Some(token) = self.inner.bulk.lock().as_ref() {
            token.cancel();
        }
    }

    pub fn bulk_running(&self) -> bool {
        self.inner.bulk.lock().is_some()
    }

    async fn run_staggered(&self, ids: Vec<String>) -> Result<BulkOutcome> {
        let token = {
            let mut bulk = self.inner.bulk.lock();
            if bulk.is_some() {
                return Err(AfkError::Busy);
            }
            let token = CancellationToken::new();
            *bulk = Some(token.clone());
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
                    outcome.skipped.push(id);
                    continue;
                }
            }
            match self.connect(&id).await {
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

    /// Stops the account's client, then removes the account, its keychain entry and its folder.
    pub async fn remove_account(&self, account_id: &str) -> Result<()> {
        let session = self.inner.sessions.lock().remove(account_id);
        if let Some(session) = session {
            session.stop();
            session
                .wait_idle(self.inner.config.stop_grace + Duration::from_secs(3))
                .await;
            session.shutdown().await;
        }
        let store = self.inner.store.clone();
        let id = account_id.to_owned();
        let removed = tokio::task::spawn_blocking(move || store.remove_account(&id))
            .await
            .map_err(|e| AfkError::InvalidData(format!("remove task failed: {e}")))??;
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
        let reconnect_needed = self.existing(account_id).is_some_and(|s| s.is_running());
        Ok(SavedSettings {
            account,
            reconnect_needed,
        })
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
    send(manager.stop_all());
    send(manager.shutdown());
    send(manager.remove_account("id"));
}
