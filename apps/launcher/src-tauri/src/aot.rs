//! "Hızlı oyun açılışı" (AOT-CACHE): Java's AOT cache for Fabric starts on Java 25+, behind the
//! signed config's `features.aotCache` (off until Studio turns it on). How a cache is keyed,
//! trained, made and dropped lives in `pl_mc::launch::aot` (tested there); this module wires it to
//! launches:
//!
//! - [`for_launch`]: a launch of an eligible instance (Mojang's runtime, no wrapper) gets its
//!   cache, or records when it has none; a recording that waits for its create step means a
//!   normal start.
//! - [`game_ended`]: a training game that crashed gives its key up; a cached game that crashed
//!   before it got to play drops its cache for good. With no game left, [`run_pending`].
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
use pl_mc::launch::aot::{AotPlan, AotStore, CreateOutcome, TRAINING_ROOM, launch_key};
use pl_net::CancellationToken;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// The applied content's `features.aotCache`.
static FLAG: AtomicBool = AtomicBool::new(false);
/// The create run going now (one at a time) with its number, to stop it when a game starts.
static CREATING: Mutex<Option<(u64, CancellationToken)>> = Mutex::new(None);
static RUNS: AtomicU64 = AtomicU64::new(0);
/// After the launcher starts, recordings left from before wait this long for their create step.
const FIRST_CREATE: Duration = Duration::from_secs(90);

/// The signed config's `features.aotCache`, from every applied content snapshot.
pub(crate) fn apply_flag(on: bool) {
    FLAG.store(on, Ordering::Relaxed);
}

/// Whether launches get the cache: the flag, or `PROJECTS_AOT_CACHE=1` in debug builds.
fn on() -> bool {
    FLAG.load(Ordering::Relaxed) || debug_on()
}

/// `PROJECTS_AOT_CACHE=1` (debug builds only): the cache on whatever the config says, for tests.
fn debug_on() -> bool {
    #[cfg(debug_assertions)]
    {
        std::env::var("PROJECTS_AOT_CACHE").is_ok_and(|value| value == "1")
    }
    #[cfg(not(debug_assertions))]
    {
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
    store: AotStore,
}

/// The cache plan of a launch about to start with `command` (built without any cache option).
/// `None`: the feature is off or this launch cannot have a cache (not Fabric, Java older than 25,
/// a Java the player chose, a wrapper, an agent).
pub(crate) async fn for_launch(
    state: &AppState,
    instance_id: &str,
    installed: &InstalledVersion,
    command: &LaunchCommand,
    wrapped: bool,
) -> Option<AotLaunch> {
    if !on() || wrapped || !matches!(installed.java.source, JavaSource::Mojang { .. }) {
        return None;
    }
    // `<home>/bin/javaw.exe`
    let home = installed.java.path.parent()?.parent()?.to_path_buf();
    let store = store(state);
    let command = command.clone();
    let (major, client) = (installed.java.info.major, installed.client_jar.clone());
    let instance = instance_id.to_owned();
    tauri::async_runtime::spawn_blocking(move || {
        let key = launch_key(&command, major, &home, &client)?;
        let _ = std::fs::create_dir_all(store.dir());
        let room = crate::commands::launch::free_space(store.dir())
            .is_some_and(|free| free >= TRAINING_ROOM);
        let plan = store.plan(&instance, &key, room);
        tracing::info!(%key, plan = plan_name(&plan), "AOT cache plan");
        Some(AotLaunch {
            instance,
            key,
            plan,
            store,
        })
    })
    .await
    .ok()
    .flatten()
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

/// A game of `launch` ended: `crashed` with a crash diagnosis, `played` when it got to play.
pub(crate) fn game_ended(app: &AppHandle, launch: AotLaunch, crashed: bool, played: bool) {
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
/// starts stops it, [`pause`]). Nothing while the feature is off.
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
