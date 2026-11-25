//! "Hızlı oyun açılışı" (AOT-CACHE): Java's AOT cache for Fabric starts on Java 25+, behind the
//! signed config's `features.aotCache` (full caches) and `features.aotLite` (AOT-LITE: classes
//! only; it wins when both are on), both off until Studio turns them on. How a cache is keyed,
//! trained, made and dropped lives in `pl_mc::launch::aot` (tested there); this module wires it to
//! launches:
//!
//! - [`for_launch`]: a launch of an eligible instance (Mojang's runtime, no wrapper) gets its
//!   cache, or records when it has none; a recording that waits for its create step means a
//!   normal start.
//! - [`game_ended`]: a training game that crashed gives its key up; a cached game that crashed
//!   before it got to play, or whose JVM itself died later (TELEM-JVM), drops its cache for good.
//!   With no game left, [`run_pending`].
//! - [`run_pending`]: the create steps, one at a time, below normal priority, only while no game
//!   runs; a launch stops one ([`pause`]) and it runs again after that game.
//!
//! The files live in `cache/aot` (Ayarlar → Veri's cleanup removes them on request; they are
//! made again).

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::time::Duration;

use parking_lot::Mutex;
use pl_mc::install::InstalledVersion;
use pl_mc::java::JavaSource;
use pl_mc::launch::LaunchCommand;
use pl_mc::launch::aot::{AotMode, AotPlan, AotStore, CreateOutcome, TRAINING_ROOM, launch_key};
use pl_mc::launch::crash::CrashDiagnosis;
use pl_net::CancellationToken;
use pl_telemetry::AotUse;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// The applied content's `features.aotCache` (full caches).
static FULL: AtomicBool = AtomicBool::new(false);
/// The applied content's `features.aotLite` (lite caches).
static LITE: AtomicBool = AtomicBool::new(false);
/// The create run going now (one at a time) with its number, to stop it when a game starts.
static CREATING: Mutex<Option<(u64, CancellationToken)>> = Mutex::new(None);
static RUNS: AtomicU64 = AtomicU64::new(0);
/// After the launcher starts, recordings left from before wait this long for their create step.
const FIRST_CREATE: Duration = Duration::from_secs(90);

/// The signed config's `features.aotCache` and `features.aotLite`, from every applied content
/// snapshot.
pub(crate) fn apply_flags(full: bool, lite: bool) {
    FULL.store(full, Ordering::Relaxed);
    LITE.store(lite, Ordering::Relaxed);
}

/// The kind of cache launches get: lite while `features.aotLite` is on (with `aotCache` too: the
/// mode without the JVM crashes wins), full while only `features.aotCache` is; in debug builds
/// `PROJECTS_AOT_LITE=1` or `PROJECTS_AOT_CACHE=1` whatever the config says. `None`: no cache.
fn mode() -> Option<AotMode> {
    if LITE.load(Ordering::Relaxed) || debug_env("PROJECTS_AOT_LITE") {
        Some(AotMode::Lite)
    } else if FULL.load(Ordering::Relaxed) || debug_env("PROJECTS_AOT_CACHE") {
        Some(AotMode::Full)
    } else {
        None
    }
}

/// Whether launches get a cache of either kind.
fn on() -> bool {
    mode().is_some()
}

/// `<name>=1` in a debug build's environment (tests); always false in release builds.
fn debug_env(name: &str) -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var(name).is_ok_and(|value| value == "1")
    }
    #[cfg(not(debug_assertions))]
    {
        let _ = name;
        false
    }
}

pub(crate) fn store(state: &AppState) -> AotStore {
    AotStore::new(
        state
            .root
            .join(pl_data::usage::CACHE_DIR)
            .join(pl_mc::launch::aot::DIR_NAME),
    )
}

/// What a launch does about its cache, kept until its game ends.
#[derive(Debug, Clone)]
pub(crate) struct AotLaunch {
    pub instance: String,
    pub key: String,
    pub plan: AotPlan,
    /// The kind of cache (its training start's options depend on it).
    pub mode: AotMode,
    store: AotStore,
}

/// The cache plan of a launch about to start with `command` (built without any cache option).
/// `None`: both features are off or this launch cannot have a cache (not Fabric, Java older than
/// 25, a Java the player chose, a wrapper, an agent).
pub(crate) async fn for_launch(
    state: &AppState,
    instance_id: &str,
    installed: &InstalledVersion,
    command: &LaunchCommand,
    wrapped: bool,
) -> Option<AotLaunch> {
    let mode = mode()?;
    if wrapped || !matches!(installed.java.source, JavaSource::Mojang { .. }) {
        return None;
    }
    // `<home>/bin/javaw.exe`
    let home = installed.java.path.parent()?.parent()?.to_path_buf();
    let store = store(state);
    let command = command.clone();
    let (major, client) = (installed.java.info.major, installed.client_jar.clone());
    let instance = instance_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let key = launch_key(&command, major, &home, &client, mode)?;
        let _ = std::fs::create_dir_all(store.dir());
        let room = crate::commands::launch::free_space(store.dir())
            .is_some_and(|free| free >= TRAINING_ROOM);
        let plan = store.plan(&instance, &key, room);
        tracing::info!(%key, mode = mode_name(mode), plan = plan_name(&plan), "AOT cache plan");
        Some(AotLaunch {
            instance,
            key,
            plan,
            mode,
            store,
        })
    })
    .await
    .ok()
    .flatten()
}

/// A start's plan as usage data names it (TELEM-JVM).
pub(crate) fn usage(plan: &AotPlan) -> AotUse {
    match plan {
        AotPlan::Off => AotUse::Off,
        AotPlan::Use { .. } => AotUse::Use,
        AotPlan::Train { .. } => AotUse::Train,
    }
}

fn mode_name(mode: AotMode) -> &'static str {
    match mode {
        AotMode::Full => "full",
        AotMode::Lite => "lite",
    }
}

fn plan_name(plan: &AotPlan) -> &'static str {
    match plan {
        AotPlan::Off => "off",
        AotPlan::Use { .. } => "use",
        AotPlan::Train { .. } => "train",
    }
}

impl AotLaunch {
    /// A training start's command is final: counted, and what the create step repeats is kept.
    /// False when that could not be written (the start then goes without a recording).
    pub(crate) fn begin_training(&self, java: &Path, command: &LaunchCommand) -> bool {
        match self
            .store
            .begin_training(&self.instance, &self.key, java, command.jvm_options())
        {
            Ok(()) => true,
            Err(error) => {
                tracing::warn!(%error, "AOT training not prepared; a normal start");
                false
            }
        }
    }
}

/// A game of `launch` ended: `crash` its crash diagnosis, `played` when it got to play.
pub(crate) fn game_ended(
    app: &AppHandle,
    launch: AotLaunch,
    crash: Option<&CrashDiagnosis>,
    played: bool,
) {
    let crashed = crash.is_some();
    match &launch.plan {
        AotPlan::Train { config } => {
            let recorded = config.is_file();
            tracing::info!(crashed, recorded, "AOT training game ended");
            launch
                .store
                .training_ended(&launch.instance, &launch.key, crashed);
        }
        AotPlan::Use { .. } if crashed && !played => {
            tracing::warn!(
                instance = %launch.instance,
                "a game with an AOT cache crashed while it opened; that cache is not used again"
            );
            launch.store.give_up(&launch.instance, &launch.key);
        }
        // TELEM-JVM: the JVM itself, or the code it compiled, died with a cache in its start. The
        // cache is the one thing of ours in there: the next starts go without it (until the setup
        // changes and a new one is made), and usage data shows whether such crashes go with it.
        AotPlan::Use { .. } if crash.is_some_and(|crash| crash.trace.in_jvm()) => {
            tracing::warn!(
                instance = %launch.instance,
                "the JVM of a game with an AOT cache crashed; that cache is not used again"
            );
            launch.store.give_up(&launch.instance, &launch.key);
        }
        _ => {}
    }
    run_pending(app);
}

/// A game starts: a running create step stops (its recording stays for after the game).
pub(crate) fn pause() {
    if let Some((_, cancel)) = CREATING.lock().take() {
        cancel.cancel();
    }
}

/// Recordings left from before the launcher started get their create step a little later.
pub(crate) fn start(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_CREATE).await;
        run_pending(&app);
    });
}

/// Makes the caches whose recordings wait, one after the other, while no game runs (a game that
/// starts stops it, [`pause`]). Nothing while both features are off. A recording repeats its own
/// training's options (a lite one's `-XX:-AOTClassLinking` among them), whatever the mode now.
pub(crate) fn run_pending(app: &AppHandle) {
    if !on() {
        return;
    }
    let state = app.state::<AppState>();
    if state.games.lock().any() {
        return;
    }
    let (run, cancel) = {
        let mut creating = CREATING.lock();
        if creating.is_some() {
            return;
        }
        let run = RUNS.fetch_add(1, Ordering::Relaxed);
        let cancel = CancellationToken::new();
        *creating = Some((run, cancel.clone()));
        (run, cancel)
    };
    let store = store(&state);
    tauri::async_runtime::spawn(async move {
        let listed = store.clone();
        let pending = tauri::async_runtime::spawn_blocking(move || listed.pending())
            .await
            .unwrap_or_default();
        for item in pending {
            if cancel.is_cancelled() {
                break;
            }
            match pl_mc::launch::aot::create(&store, &item, &cancel).await {
                CreateOutcome::Cancelled => break,
                CreateOutcome::Created(_) | CreateOutcome::Failed => {}
            }
        }
        let mut creating = CREATING.lock();
        // A run started after this one was paused keeps its place.
        if creating
            .as_ref()
            .is_some_and(|(current, _)| *current == run)
        {
            *creating = None;
        }
    });
}
