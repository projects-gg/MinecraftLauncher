//! Background preparation ("Arka planda hazırla"): while the launcher is idle, on screen or in the
//! tray, the selected instance's game files, mod loader, Java and pack files download gently, so
//! OYNA starts almost at once.
//!
//! - Only the instance the command bar shows, only with the setting on (the default), and only on
//!   an unmetered connection, outside battery saver, with no full-screen app in front and nothing
//!   else running that the player started (`pl_net::background`).
//! - Runs follow events: the start, a choice in the bar, new content, a launch or a game ending,
//!   the connection or battery saver changing. Nothing polls while idle; a running preparation
//!   looks once a second whether it may go on, and the player's own launch stops it at once
//!   (`yield_to_launch`), taking over its finished and half-done files.
//! - The steps are the launch's own (`launch::ready_game_files` in background mode): no account,
//!   no mod check, no process. What they would have told the player (a pack update, mod changes
//!   adopted) waits for the instance's next launch (`take_notices`).
//! - A ready instance costs two small files: its install marker and the cached content index.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use pl_mc::install::space;
use pl_mc::instance::Instance;
use pl_net::CancellationToken;
use pl_net::background::{self, Conditions, Hold, Outcome, Schedule, Trigger};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Emitter, Listener, Manager};
use tokio::sync::Notify;

use crate::commands::instances::read_marker;
use crate::commands::launch::{self, EventSink, LaunchEventView, Readying};
use crate::commands::official::{self, Followed};
use crate::error::CmdError;
use crate::state::AppState;

/// The event the command bar follows.
const STATUS_EVENT: &str = "prepare-status";
/// The setting and its instance, in the data folder.
const CONFIG_FILE: &str = "prepare.json";
/// How often a running preparation looks whether it may go on.
const WATCH_EVERY: Duration = Duration::from_secs(1);
/// How long the player's launch waits for a stopping preparation to close its files.
const YIELD_WAIT: Duration = Duration::from_secs(3);
/// Progress reaches the page at most this often (and only while the window is on screen).
const PROGRESS_EVERY: Duration = Duration::from_millis(500);

/// What the command bar shows (`prepare-status`, `prepare_status`).
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum PrepareStatusView {
    /// Nothing to show.
    Idle,
    /// Downloading for the instance; `percent` (0–100) never goes back within a run.
    #[serde(rename_all = "camelCase")]
    Preparing { instance_id: String, percent: f64 },
    /// A run just put in place everything the next launch needs (sent once, never kept).
    #[serde(rename_all = "camelCase")]
    Ready { instance_id: String },
}

/// The setting and the instance it applies to. Kept in the data folder, so a launcher started in
/// the tray prepares before its page ever asks.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Config {
    enabled: bool,
    instance_id: Option<String>,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            enabled: true,
            instance_id: None,
        }
    }
}

/// A run in progress.
struct Running {
    instance_id: String,
    cancel: CancellationToken,
    /// Fires once the run has ended (its files closed, the instance lock free).
    ended: CancellationToken,
}

/// Managed state of the background preparation.
pub struct Prepare {
    inner: Mutex<Inner>,
    /// The scheduler sleeps on this until an event (or the due time) comes.
    wake: Notify,
}

struct Inner {
    config: Config,
    /// The instance's version as the page last told it (`None` until the first call).
    version: Option<String>,
    schedule: Schedule,
    running: Option<Running>,
    status: PrepareStatusView,
    /// When progress last reached the page, and at which whole percent.
    shown: Option<(Instant, u32)>,
    /// What runs found on their way, for each instance's next launch.
    notices: HashMap<String, Vec<LaunchEventView>>,
    /// Whether the connection and battery saver allowed a run at the last look (a change to
    /// allowed makes a run due).
    system_allows: bool,
}

fn system_allows() -> bool {
    let network = background::network_hint().is_some_and(|hint| hint.online() && hint.unmetered());
    network && background::power_saver() != Some(true)
}

/// Debug builds (and measurement builds) prepare only with this set to `1`: an end-to-end run or a
/// measurement must not start downloading a whole game behind the tester's back.
const E2E_ENV: &str = "PROJECTS_E2E_PREPARE";

fn build_allows() -> bool {
    !cfg!(debug_assertions) || std::env::var_os(E2E_ENV).is_some_and(|value| value == "1")
}

/// Starts background preparation with the launcher: its setting, the system watchers, the content
/// listener and the scheduler (the first look comes once the start has settled).
pub fn start(app: &AppHandle) {
    let root = app.state::<AppState>().root.clone();
    let active = build_allows();
    app.manage(Prepare {
        inner: Mutex::new(Inner {
            config: load_config(&root),
            version: None,
            schedule: Schedule::default(),
            running: None,
            status: PrepareStatusView::Idle,
            shown: None,
            notices: HashMap::new(),
            system_allows: active && system_allows(),
        }),
        wake: Notify::new(),
    });
    if !active {
        // The commands still answer (nothing to show); nothing runs or watches.
        tracing::info!(
            "background preparation is off in this debug build ({E2E_ENV}=1 turns it on)"
        );
        return;
    }
    let handle = app.clone();
    let network = background::watch_network(move || system_changed(&handle));
    let handle = app.clone();
    let power = background::watch_power(move || system_changed(&handle));
    tracing::debug!(network, power, "background preparation watches the system");
    let handle = app.clone();
    app.listen_any("content-changed", move |_| content_changed(&handle));
    trigger(app, Trigger::Started);
    let handle = app.clone();
    tauri::async_runtime::spawn(async move { scheduler(handle).await });
}

fn trigger(app: &AppHandle, trigger: Trigger) {
    let Some(prepare) = app.try_state::<Prepare>() else {
        return;
    };
    prepare
        .inner
        .lock()
        .schedule
        .trigger(trigger, Instant::now());
    prepare.wake.notify_one();
}

/// The connection or battery saver changed (a system thread): becoming allowed makes a run due; a
/// running one notices the opposite within a second.
fn system_changed(app: &AppHandle) {
    let allows = system_allows();
    let Some(prepare) = app.try_state::<Prepare>() else {
        return;
    };
    let became = {
        let mut inner = prepare.inner.lock();
        let became = allows && !inner.system_allows;
        inner.system_allows = allows;
        became
    };
    if became {
        tracing::debug!("the connection and battery saver allow background preparation again");
        trigger(app, Trigger::System);
    }
}

/// New content arrived (a pack update): a look. A run brings content itself and uses it already.
fn content_changed(app: &AppHandle) {
    let Some(prepare) = app.try_state::<Prepare>() else {
        return;
    };
    if prepare.inner.lock().running.is_none() {
        trigger(app, Trigger::Content);
    }
}

/// The player pressed OYNA: a running preparation stops at once and the launch waits (briefly)
/// until its files are closed and the instance lock is free.
pub async fn yield_to_launch(app: &AppHandle) {
    let Some(prepare) = app.try_state::<Prepare>() else {
        return;
    };
    let running = prepare
        .inner
        .lock()
        .running
        .as_ref()
        .map(|run| (run.cancel.clone(), run.ended.clone()));
    let Some((cancel, ended)) = running else {
        return;
    };
    cancel.cancel();
    match tokio::time::timeout(YIELD_WAIT, ended.cancelled()).await {
        Ok(()) => tracing::info!("background preparation stopped for a launch"),
        Err(_) => tracing::warn!("background preparation still stopping when the launch began"),
    }
}

/// A launch ended without starting the game (an error, a cancel, a question).
pub fn launch_ended(app: &AppHandle) {
    trigger(app, Trigger::LaunchEnded);
}

/// The game closed.
pub fn game_exited(app: &AppHandle) {
    trigger(app, Trigger::GameExited);
}

/// What background runs found for the instance (a pack update, mod changes adopted, a version the
/// pack dropped), taken by its launch to tell the player as it would have.
pub fn take_notices(app: &AppHandle, instance_id: &str) -> Vec<LaunchEventView> {
    app.try_state::<Prepare>()
        .and_then(|prepare| prepare.inner.lock().notices.remove(instance_id))
        .unwrap_or_default()
}

/// Settings → İndirmeler "Arka planda hazırla" and the instance the command bar shows, with its
/// version (`mc/loader/loader version`). Called whenever one of them changes: a choice makes a run
/// due soon, a run for another instance (or with the setting off) stops at once.
#[tauri::command]
#[specta::specta]
pub fn prepare_configure(
    app: AppHandle,
    enabled: bool,
    instance_id: Option<String>,
    version: Option<String>,
) {
    let Some(prepare) = app.try_state::<Prepare>() else {
        return;
    };
    let config = Config {
        enabled,
        instance_id,
    };
    let (changed, chosen, stop) = {
        let mut inner = prepare.inner.lock();
        let changed = inner.config != config;
        // The first call after the start only tells what the start already looks at.
        let version_changed = inner.version.is_some() && inner.version != version;
        inner.version = version;
        inner.config = config.clone();
        let stop = inner
            .running
            .as_ref()
            .filter(|run| {
                !enabled || config.instance_id.as_deref() != Some(run.instance_id.as_str())
            })
            .map(|run| run.cancel.clone());
        (changed, changed || version_changed, stop)
    };
    if let Some(stop) = stop {
        tracing::info!("background preparation stopped: the choice changed");
        stop.cancel();
    }
    if changed {
        save_config(&app.state::<AppState>().root, &config);
    }
    if chosen && enabled && config.instance_id.is_some() {
        trigger(&app, Trigger::Chosen);
    }
}

/// What the command bar shows now (after the page loaded or came back on screen).
#[tauri::command]
#[specta::specta]
pub fn prepare_status(app: AppHandle) -> PrepareStatusView {
    app.try_state::<Prepare>()
        .map(|prepare| prepare.inner.lock().status.clone())
        .unwrap_or(PrepareStatusView::Idle)
}

/// Waits for the due run (no timer without one), runs it, schedules what follows.
async fn scheduler(app: AppHandle) {
    let prepare = app.state::<Prepare>();
    loop {
        let due = prepare.inner.lock().schedule.due();
        let fire = match due {
            None => {
                prepare.wake.notified().await;
                false
            }
            Some(at) => {
                tokio::select! {
                    () = prepare.wake.notified() => false,
                    () = tokio::time::sleep_until(tokio::time::Instant::from_std(at)) => true,
                }
            }
        };
        if !fire {
            continue;
        }
        prepare.inner.lock().schedule.start();
        let outcome = run(&app).await;
        prepare
            .inner
            .lock()
            .schedule
            .finish(outcome, Instant::now());
    }
}

/// What the launcher and the system say now.
fn conditions(app: &AppHandle, state: &AppState) -> Conditions {
    let enabled = app.try_state::<Prepare>().is_some_and(|prepare| {
        let inner = prepare.inner.lock();
        inner.config.enabled && inner.config.instance_id.is_some()
    });
    Conditions {
        enabled,
        game: state.running.lock().is_some() || state.launch_cancel.lock().is_some(),
        busy: crate::commands::modrinth::jobs_running(app)
            || crate::commands::library::jobs_running(app)
            || crate::commands::java::job_running(app)
            || crate::commands::data::job_running(app),
        full_screen: background::full_screen(),
        power_saver: background::power_saver(),
        network: background::network_hint(),
    }
}

fn still_chosen(app: &AppHandle, instance_id: &str) -> bool {
    app.try_state::<Prepare>().is_some_and(|prepare| {
        let inner = prepare.inner.lock();
        inner.config.enabled && inner.config.instance_id.as_deref() == Some(instance_id)
    })
}

/// The disk filled up meanwhile (something else wrote to it): the reserve is no longer free.
fn disk_tight(dirs: &[&Path]) -> bool {
    dirs.iter()
        .any(|dir| launch::free_space(dir).is_none_or(space::background_tight))
}

/// One run for the chosen instance, start to end.
async fn run(app: &AppHandle) -> Outcome {
    let state = app.state::<AppState>();
    let config = app.state::<Prepare>().inner.lock().config.clone();
    let Some(instance_id) = config.instance_id.filter(|_| config.enabled) else {
        return Outcome::Held(Hold::Off);
    };
    if let Some(hold) = conditions(app, &state).hold() {
        tracing::debug!(?hold, "background preparation waits");
        return Outcome::Held(hold);
    }
    // Whatever holds the instance (a mod change, a version switch) goes first. The run takes the
    // lock itself, only while it changes the instance (`Readying::Background`).
    let lock = crate::commands::modrinth::instance_lock(app, &instance_id);
    if lock.try_lock().is_err() {
        return Outcome::Held(Hold::Busy);
    }
    let Ok(mut instance) = state.instances.get(&instance_id).await else {
        // Deleted meanwhile: the page chooses another one.
        return Outcome::Held(Hold::Off);
    };
    let (Ok(instance_dir), Ok(game_dir)) = (
        state.instances.instance_dir(&instance_id),
        state.instances.game_dir(&instance_id),
    ) else {
        return Outcome::Failed;
    };
    if up_to_date(&state, &instance, &instance_dir, &game_dir).await {
        tracing::debug!(instance = %instance_id, "background preparation: nothing to do");
        return Outcome::Ready;
    }

    let cancel = CancellationToken::new();
    let ended = CancellationToken::new();
    app.state::<Prepare>().inner.lock().running = Some(Running {
        instance_id: instance_id.clone(),
        cancel: cancel.clone(),
        ended: ended.clone(),
    });
    tracing::info!(instance = %instance_id, "background preparation started");
    let started = Instant::now();
    let sink = sink(app, &instance_id);
    let data_root: PathBuf = state.mc.dirs.root().to_path_buf();
    let work = launch::ready_game_files(
        app,
        &state,
        &mut instance,
        (instance_dir.as_path(), game_dir.as_path()),
        Readying::Background { lock: &lock },
        &sink,
        &cancel,
    );
    tokio::pin!(work);
    // Stopped steps get their usual moment to end by themselves (the pack sync is never dropped
    // half-way): the run is awaited to its end, it is never cut off.
    let mut held = None;
    let result = loop {
        tokio::select! {
            result = &mut work => break result,
            () = tokio::time::sleep(WATCH_EVERY), if held.is_none() && !cancel.is_cancelled() => {
                let hold = conditions(app, &state)
                    .hold()
                    // Anyone else asking for this instance (the instance page, a mod change):
                    // the registry and this run hold the only other handles of its lock.
                    .or_else(|| (Arc::strong_count(&lock) > 2).then_some(Hold::Busy))
                    .or_else(|| {
                        disk_tight(&[data_root.as_path(), game_dir.as_path()])
                            .then_some(Hold::Disk)
                    });
                if let Some(hold) = hold {
                    tracing::info!(?hold, "background preparation paused");
                    held = Some(hold);
                    cancel.cancel();
                } else if !still_chosen(app, &instance_id) {
                    // Another instance was chosen just as this run began.
                    cancel.cancel();
                }
            }
        }
    };
    app.state::<Prepare>().inner.lock().running = None;
    ended.cancel();

    let outcome = match (result, held) {
        (Ok(_), _) => Outcome::Ready,
        (Err(_), Some(hold)) => Outcome::Held(hold),
        // The player's launch, another instance, the setting turned off, or the instance changed
        // meanwhile (each brings what follows itself).
        (Err(_), None) if cancel.is_cancelled() => Outcome::Stopped,
        (Err(error), None) if error.code == launch::CHANGED => Outcome::Stopped,
        (Err(error), None) if error.code == launch::OFFLINE => Outcome::Held(Hold::Offline),
        (Err(error), None) if error.code == CmdError::DISK_FULL => Outcome::Held(Hold::Disk),
        (Err(error), None) => {
            tracing::warn!(code = %error.code, details = %error.details, "background preparation failed");
            Outcome::Failed
        }
    };
    tracing::info!(
        instance = %instance_id,
        ?outcome,
        after_ms = started.elapsed().as_millis() as u64,
        "background preparation ended"
    );
    finish_status(app, &instance_id, outcome == Outcome::Ready);
    outcome
}

/// Nothing for the next launch to download, as far as two small files tell: the install marker
/// matches the instance's version and, for a pack instance, the installed pack version is the one
/// the cached content index offers (the launch still checks every file).
async fn up_to_date(
    state: &AppState,
    instance: &Instance,
    instance_dir: &Path,
    game_dir: &Path,
) -> bool {
    let installed = read_marker(instance_dir).is_some_and(|marker| {
        marker.mc_version == instance.mc_version
            && marker.loader == instance.loader.kind
            && marker.loader_version == instance.loader.version
    });
    if !installed {
        return false;
    }
    let Some(managed) = &instance.managed_pack else {
        return true;
    };
    // Without content nothing is known to follow; new content makes a run due when it arrives.
    let Ok(snapshot) = state.content.load_cached().await else {
        return true;
    };
    let family =
        official::instance_family(&snapshot.index, instance, Some(game_dir)).unwrap_or_default();
    match official::followed_pack(&snapshot.index, instance, &family) {
        Followed::Member(pack) | Followed::InPlace(pack) => {
            pack.id == managed.pack_id
                && pack.latest == managed.version
                && pack
                    .minecraft()
                    .is_none_or(|minecraft| minecraft == instance.mc_version)
        }
        Followed::Dropped { .. } => true,
    }
}

/// Where a run's steps report: progress for the command bar, notices for the next launch.
fn sink(app: &AppHandle, instance_id: &str) -> EventSink {
    let app = app.clone();
    let instance_id = instance_id.to_owned();
    Arc::new(move |event| match event {
        LaunchEventView::Progress(progress) => show_progress(&app, &instance_id, progress.percent),
        LaunchEventView::PackUpdated { .. }
        | LaunchEventView::ModsNotice(_)
        | LaunchEventView::PackVersionDropped { .. } => keep_notice(&app, &instance_id, event),
        _ => {}
    })
}

/// Keeps the newest notice of each kind for the instance's next launch.
fn keep_notice(app: &AppHandle, instance_id: &str, notice: LaunchEventView) {
    let Some(prepare) = app.try_state::<Prepare>() else {
        return;
    };
    let mut inner = prepare.inner.lock();
    let kept = inner.notices.entry(instance_id.to_owned()).or_default();
    kept.retain(|old| std::mem::discriminant(old) != std::mem::discriminant(&notice));
    kept.push(notice);
}

fn window_on_screen(app: &AppHandle) -> bool {
    app.get_webview_window("main").is_some_and(|window| {
        window.is_visible().unwrap_or(false) && !window.is_minimized().unwrap_or(false)
    })
}

/// The run's progress: kept for `prepare_status`, sent to the page while the window is on screen
/// (a page out of sight asks again when it comes back), at most twice a second and on whole
/// percents.
fn show_progress(app: &AppHandle, instance_id: &str, percent: f64) {
    let Some(prepare) = app.try_state::<Prepare>() else {
        return;
    };
    let percent = percent.clamp(0.0, 100.0);
    let send = {
        let mut inner = prepare.inner.lock();
        let percent = match &inner.status {
            PrepareStatusView::Preparing {
                instance_id: id,
                percent: before,
            } if id == instance_id => before.max(percent),
            _ => percent,
        };
        inner.status = PrepareStatusView::Preparing {
            instance_id: instance_id.to_owned(),
            percent,
        };
        let whole = percent.floor() as u32;
        let now = Instant::now();
        let due = inner
            .shown
            .is_none_or(|(at, shown)| whole != shown && now.duration_since(at) >= PROGRESS_EVERY);
        if due {
            inner.shown = Some((now, whole));
            Some(inner.status.clone())
        } else {
            None
        }
    };
    if let Some(status) = send
        && window_on_screen(app)
    {
        let _ = app.emit(STATUS_EVENT, status);
    }
}

/// The run ended: "ready" once when it showed progress and finished, otherwise back to nothing.
fn finish_status(app: &AppHandle, instance_id: &str, ready: bool) {
    let Some(prepare) = app.try_state::<Prepare>() else {
        return;
    };
    let send = {
        let mut inner = prepare.inner.lock();
        let shown = matches!(inner.status, PrepareStatusView::Preparing { .. });
        inner.status = PrepareStatusView::Idle;
        inner.shown = None;
        match (shown, ready) {
            (false, _) => None,
            (true, true) => Some(PrepareStatusView::Ready {
                instance_id: instance_id.to_owned(),
            }),
            (true, false) => Some(PrepareStatusView::Idle),
        }
    };
    if let Some(status) = send {
        let _ = app.emit(STATUS_EVENT, status);
    }
}

fn load_config(root: &Path) -> Config {
    std::fs::read(root.join(CONFIG_FILE))
        .ok()
        .and_then(|bytes| serde_json::from_slice(&bytes).ok())
        .unwrap_or_default()
}

fn save_config(root: &Path, config: &Config) {
    let saved = serde_json::to_vec(config)
        .map_err(|error| error.to_string())
        .and_then(|bytes| {
            pl_mc::paths::write_atomic_blocking(&root.join(CONFIG_FILE), &bytes)
                .map_err(|error| error.to_string())
        });
    if let Err(error) = saved {
        tracing::warn!(%error, "background preparation setting not saved");
    }
}
