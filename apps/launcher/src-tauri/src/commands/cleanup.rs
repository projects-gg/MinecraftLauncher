//! Settings → Veri → Temizlik: version sets an instance has not played for a long time, shared
//! game files no instance uses, leftovers (caches, old logs, crash reports) and the launcher
//! trash; plus the automatic tidy-up shortly after start and once a day.
//!
//! The decisions live in the crates: `pl_content::sets` (which sets are stale and may go),
//! `pl_mc::shared_files` (reference counting of versions, loader profiles, Java runtimes,
//! libraries and assets), `pl_data::housekeeping` (every age and size rule) and
//! `pl_mc::gamefiles::LauncherTrash`. This module gathers the inputs (instances, their parked
//! sets, what is running) and maps views. Nothing runs while a game runs or is being prepared,
//! while an install or backup runs, or during another data job (a move).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, SystemTime};

use pl_content::VersionSets;
use pl_data::housekeeping::{self, CleanupState, InUse, KeptSets, LeftoverKind, Places, Policy};
use pl_mc::gamefiles::{LauncherTrash, TrashKind, TrashLimits};
use pl_mc::instance::Instance;
use pl_mc::java::JavaSetting;
use pl_mc::loaders::LoaderKind;
use pl_mc::shared_files::{self, GameRef, ScanOptions, SharedGroup, SharedItem};
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::commands::data::{DataJob, begin_job};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use pl_lang::pick;

/// The first automatic round starts this long after the launcher (never slows the start).
const FIRST_TIDY: Duration = Duration::from_secs(90);
/// ...and the launcher looks again this often while it stays open (a round runs only when one
/// is due, [`Policy::round_every`]).
const TIDY_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// A round that found the launcher busy runs this long after it is free again (a game that just
/// closed is often started again soon).
const AFTER_BUSY: Duration = Duration::from_secs(3 * 60);
/// A round's game file deletion stops after this; the rest goes next time.
const ROUND_BUDGET: Duration = Duration::from_secs(2 * 60);
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

/// A round found the launcher busy and waits for it to be free ([`status_changed`]).
static WAITING: AtomicBool = AtomicBool::new(false);
/// A round is already scheduled after a busy spell.
static SCHEDULED: AtomicBool = AtomicBool::new(false);
/// The running round's stop: a game starting or getting ready, or an install, stops it.
static RUNNING: std::sync::Mutex<Option<CancellationToken>> = std::sync::Mutex::new(None);

fn iso_ms(ms: i64) -> Option<String> {
    jiff::Timestamp::from_millisecond(ms)
        .ok()
        .map(|t| t.to_string())
}

/// The trash limits for the trash at `trash_dir`: 30 days, and a size cap from its disk.
pub(crate) fn trash_limits(trash_dir: &Path) -> TrashLimits {
    TrashLimits {
        max_bytes: Some(housekeeping::trash_cap(pl_data::usage::disk_size(
            trash_dir,
        ))),
        ..TrashLimits::default()
    }
}

fn launcher_trash(state: &AppState) -> LauncherTrash {
    LauncherTrash::new(state.root.join("trash"))
}

/// The version sets the player put back from the trash ([`KeptSets`]).
async fn kept_sets(state: &AppState) -> KeptSets {
    let root = state.root.clone();
    tokio::task::spawn_blocking(move || KeptSets::load(&root))
        .await
        .unwrap_or_default()
}

fn set_key(instance_id: &str, minecraft: &str) -> String {
    format!("{instance_id}/{minecraft}")
}

/// Remembers that a version set came back from the trash ([`KeptSets`]): it is not offered as an
/// old set again until it goes unused for the threshold since then.
pub(crate) async fn remember_kept_set(state: &AppState, instance_id: &str, minecraft: &str) {
    let mut kept = kept_sets(state).await;
    kept.keep(
        set_key(instance_id, minecraft),
        jiff::Timestamp::now().as_millisecond(),
    );
    let root = state.root.clone();
    if let Ok(Err(error)) = tokio::task::spawn_blocking(move || kept.save(&root)).await {
        tracing::warn!(%error, "could not remember the set put back");
    }
}

// ---------------------------------------------------------------------------------------------
// What blocks a cleanup

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CleanupBlock {
    /// A game runs.
    GameRunning,
    /// A game is being prepared (downloads, loader install).
    Preparing,
    /// An install, version switch, repair or world backup runs.
    Busy,
    /// The data folder is being moved (or an earlier move finishes).
    DataJob,
}

fn blocked(app: &AppHandle, state: &AppState) -> Option<CleanupBlock> {
    if state.games.lock().any() {
        Some(CleanupBlock::GameRunning)
    } else if state.launch_cancel.lock().is_some() {
        Some(CleanupBlock::Preparing)
    } else if crate::commands::modrinth::jobs_running(app)
        || crate::commands::library::jobs_running(app)
        || crate::commands::java::job_running(app)
    {
        Some(CleanupBlock::Busy)
    } else if crate::commands::data::job_running(app) {
        Some(CleanupBlock::DataJob)
    } else {
        None
    }
}

fn block_error(block: CleanupBlock) -> CmdError {
    let (code, message) = match block {
        CleanupBlock::GameRunning => (
            "TEMIZLIK-OYUN",
            pick(
                "Oyun açıkken temizlik yapılamaz. Önce oyunu kapat.",
                "You can't clean up while the game is open. Close the game first.",
            ),
        ),
        CleanupBlock::Preparing => (
            "TEMIZLIK-HAZIRLIK",
            pick(
                "Oyun hazırlanırken temizlik yapılamaz. Bitmesini bekle.",
                "You can't clean up while a game is getting ready. Wait for it to finish.",
            ),
        ),
        CleanupBlock::Busy => (
            "TEMIZLIK-ISLEM",
            pick(
                "Arka planda bir kurulum ya da yedekleme sürüyor. Bitince tekrar dene.",
                "An install or backup is running in the background. Try again when it's done.",
            ),
        ),
        CleanupBlock::DataJob => (
            "TEMIZLIK-VERI",
            pick(
                "Veri klasöründe başka bir işlem sürüyor. Bitince tekrar dene.",
                "Something else is working in the data folder. Try again when it's done.",
            ),
        ),
    };
    CmdError::new(code, message, format!("{block:?}"))
}

fn ensure_quiet(app: &AppHandle, state: &AppState) -> CmdResult<()> {
    match blocked(app, state) {
        Some(block) => Err(block_error(block)),
        None => Ok(()),
    }
}

/// Settings → Veri listens for this instead of asking every few seconds; it then asks for
/// [`cleanup_status`] once. Nothing listens (and nothing runs) while that page is closed.
pub(crate) const STATUS_CHANGED: &str = "cleanup-status-changed";

/// What [`blocked`] reads changed: a game started or ended, began or stopped getting ready, or an
/// install, backup, Java download or data job began or ended (the free space moves with them).
/// The automatic round listens too: a game starting stops its deletion between items, and a
/// round that waited for a busy launcher runs [`AFTER_BUSY`] after it is free.
pub(crate) fn status_changed(app: &AppHandle) {
    let _ = app.emit(STATUS_CHANGED, ());
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let block = blocked(app, &state);
    if matches!(
        block,
        Some(CleanupBlock::GameRunning | CleanupBlock::Preparing | CleanupBlock::Busy)
    ) {
        let running = RUNNING.lock().unwrap_or_else(|p| p.into_inner());
        if let Some(cancel) = running.as_ref() {
            cancel.cancel();
            WAITING.store(true, Ordering::Release);
        }
    }
    if block.is_none() && WAITING.load(Ordering::Acquire) && !SCHEDULED.swap(true, Ordering::AcqRel)
    {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(AFTER_BUSY).await;
            SCHEDULED.store(false, Ordering::Release);
            round(&app).await;
        });
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CleanupStatusView {
    /// Why cleaning is not possible right now (the lists can still be shown).
    pub blocked: Option<CleanupBlock>,
    /// Free space on the data folder's disk.
    pub free_bytes: Option<u64>,
    /// Size of that disk.
    pub disk_bytes: Option<u64>,
}

/// Whether cleaning is possible right now, and the data folder's disk.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_status(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<CleanupStatusView> {
    let root = state.root.clone();
    let (free_bytes, disk_bytes) = tauri::async_runtime::spawn_blocking(move || {
        (
            pl_data::usage::free_space(&root),
            pl_data::usage::disk_size(&root),
        )
    })
    .await
    .map_err(CmdError::internal)?;
    Ok(CleanupStatusView {
        blocked: blocked(&app, &state),
        free_bytes,
        disk_bytes,
    })
}

// ---------------------------------------------------------------------------------------------
// Old version sets

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OldSetView {
    /// `<instance id>/<minecraft>`, unique in the list.
    pub key: String,
    pub instance_id: String,
    pub instance_name: String,
    pub minecraft: String,
    pub loader: Option<LoaderKind>,
    /// When the set was last played (else parked or prepared).
    pub last_used_at: Option<String>,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OldSetsView {
    /// Longest unused first.
    pub sets: Vec<OldSetView>,
    pub total_bytes: u64,
    pub older_than_days: u32,
}

/// The threshold the UI offers (30/60/90), kept within a sane range.
fn threshold(days: u32) -> u32 {
    days.clamp(7, 365)
}

fn cutoff(days: u32) -> jiff::Timestamp {
    let ms = jiff::Timestamp::now()
        .as_millisecond()
        .saturating_sub(i64::from(threshold(days)) * DAY_MS);
    jiff::Timestamp::from_millisecond(ms).unwrap_or(jiff::Timestamp::UNIX_EPOCH)
}

async fn old_sets(state: &AppState, days: u32) -> CmdResult<OldSetsView> {
    let instances = state.instances.list().await.map_err(CmdError::from)?;
    let cutoff = cutoff(days);
    let kept = kept_sets(state).await;
    let mut sets: Vec<(Option<jiff::Timestamp>, OldSetView)> = Vec::new();
    for instance in instances {
        let Ok(game_dir) = state.instances.game_dir(&instance.id) else {
            continue;
        };
        let stale = match VersionSets::new(&game_dir).stale(cutoff).await {
            Ok(stale) => stale,
            Err(error) => {
                tracing::debug!(instance = %instance.id, %error, "could not read the parked sets");
                continue;
            }
        };
        for set in stale {
            // The instance's own version is never offered, whatever its metadata says.
            if set.info.minecraft == instance.mc_version {
                continue;
            }
            let key = set_key(&instance.id, &set.info.minecraft);
            // Put back from the trash lately: the player wants it.
            if kept.kept_since(&key, cutoff.as_millisecond()) {
                continue;
            }
            sets.push((
                set.last_used,
                OldSetView {
                    key,
                    instance_id: instance.id.clone(),
                    instance_name: instance.name.clone(),
                    loader: LoaderKind::from_modrinth_name(&set.info.loader)
                        .filter(|kind| *kind != LoaderKind::Vanilla),
                    minecraft: set.info.minecraft,
                    last_used_at: set.last_used.map(|t| t.to_string()),
                    size_bytes: set.size_bytes,
                },
            ));
        }
    }
    sets.sort_by_key(|(used, _)| *used);
    let sets: Vec<OldSetView> = sets.into_iter().map(|(_, view)| view).collect();
    Ok(OldSetsView {
        total_bytes: sets.iter().map(|s| s.size_bytes).sum(),
        older_than_days: threshold(days),
        sets,
    })
}

/// Version sets not played for `older_than_days` days, across every instance.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_old_sets(
    state: State<'_, AppState>,
    older_than_days: u32,
) -> CmdResult<OldSetsView> {
    old_sets(&state, older_than_days).await
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetRef {
    pub instance_id: String,
    pub minecraft: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemovedSetView {
    pub instance_id: String,
    pub minecraft: String,
    /// The launcher trash item ("Geri al" puts it back).
    pub trash_id: String,
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RemovedSetsView {
    pub removed: Vec<RemovedSetView>,
    /// Bytes moved into the trash.
    pub moved_bytes: u64,
    /// Sets left alone (played meanwhile, now active, part of a switch, or in use).
    pub skipped: u32,
}

/// Moves the chosen old sets into the launcher trash (recoverable for 30 days). Each set is
/// checked again under the instance's lock: still not played for `older_than_days` days, not the
/// active version, no unfinished switch. Refused while a game runs or is being prepared.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_remove_sets(
    app: AppHandle,
    state: State<'_, AppState>,
    sets: Vec<SetRef>,
    older_than_days: u32,
) -> CmdResult<RemovedSetsView> {
    ensure_quiet(&app, &state)?;
    let _job = begin_job(&app, DataJob::Cleaning)?;
    let trash = launcher_trash(&state);
    let cutoff = cutoff(older_than_days);
    let kept = kept_sets(&state).await;
    let mut by_instance: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for set in sets {
        by_instance
            .entry(set.instance_id)
            .or_default()
            .push(set.minecraft);
    }
    let mut view = RemovedSetsView {
        removed: Vec::new(),
        moved_bytes: 0,
        skipped: 0,
    };
    for (instance_id, versions) in by_instance {
        let lock = crate::commands::modrinth::instance_lock(&app, &instance_id);
        let _instance_guard = lock.lock().await;
        let _switching = crate::commands::versions::SWITCHES.lock().await;
        // A game may have started while waiting for the locks.
        ensure_quiet_for_sets(&app, &state)?;
        let skip_all = |view: &mut RemovedSetsView, versions: &[String]| {
            view.skipped += u32::try_from(versions.len()).unwrap_or(u32::MAX);
        };
        let Ok(instance) = state.instances.get(&instance_id).await else {
            skip_all(&mut view, &versions);
            continue;
        };
        let (Ok(game_dir), Ok(instance_dir)) = (
            state.instances.game_dir(&instance_id),
            state.instances.instance_dir(&instance_id),
        ) else {
            skip_all(&mut view, &versions);
            continue;
        };
        let sets = VersionSets::new(&game_dir);
        let stale: HashSet<String> = match sets.stale(cutoff).await {
            Ok(list) => list.into_iter().map(|s| s.info.minecraft).collect(),
            Err(_) => HashSet::new(),
        };
        for minecraft in versions {
            if minecraft == instance.mc_version
                || !stale.contains(&minecraft)
                || kept.kept_since(&set_key(&instance_id, &minecraft), cutoff.as_millisecond())
            {
                view.skipped += 1;
                continue;
            }
            let dir = match sets.removable(&minecraft).await {
                Ok(Some(dir)) => dir,
                Ok(None) | Err(_) => {
                    view.skipped += 1;
                    continue;
                }
            };
            match trash
                .put(&dir, TrashKind::VersionSet, &instance_id, &minecraft)
                .await
            {
                Ok(entry) => {
                    // Its Modrinth records were only a cache for that set.
                    let addons =
                        crate::commands::versions::parked_addons_path(&instance_dir, &minecraft);
                    let _ = tokio::fs::remove_file(addons).await;
                    tracing::info!(instance = %instance_id, %minecraft, bytes = entry.size, "moved an old version set to the trash");
                    view.moved_bytes += entry.size;
                    view.removed.push(RemovedSetView {
                        instance_id: instance_id.clone(),
                        minecraft,
                        trash_id: entry.id,
                        size_bytes: entry.size,
                    });
                }
                Err(error) => {
                    tracing::warn!(instance = %instance_id, %minecraft, %error, "could not move an old version set to the trash");
                    view.skipped += 1;
                }
            }
        }
    }
    purge_trash_later(trash);
    Ok(view)
}

/// [`ensure_quiet`] minus the data job this cleanup holds itself.
fn ensure_quiet_for_sets(app: &AppHandle, state: &AppState) -> CmdResult<()> {
    match blocked(app, state) {
        Some(CleanupBlock::DataJob) | None => Ok(()),
        Some(block) => Err(block_error(block)),
    }
}

#[derive(Debug, Clone, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestoredSetsView {
    /// Back where they were.
    pub restored: u32,
    /// Left in the trash: that version has a new set now (or is the active one).
    pub taken: u32,
    /// Not put back: gone from the trash, instance deleted, or a file error.
    pub failed: u32,
}

/// "Geri al" (after a set cleanup, or from the trash list): puts version sets back from the
/// trash. A set whose version was started again meanwhile (a new set exists, or it is active)
/// stays in the trash. Sets that come back are not offered as old sets again until they go
/// unused for the threshold since then ([`KeptSets`]).
#[tauri::command]
#[specta::specta]
pub async fn cleanup_restore_sets(
    app: AppHandle,
    state: State<'_, AppState>,
    trash_ids: Vec<String>,
) -> CmdResult<RestoredSetsView> {
    ensure_quiet(&app, &state)?;
    let _job = begin_job(&app, DataJob::Cleaning)?;
    let trash = launcher_trash(&state);
    let mut kept = kept_sets(&state).await;
    let mut view = RestoredSetsView::default();
    for id in trash_ids {
        let entry = match trash.get(&id).await {
            Ok(entry) if entry.kind == TrashKind::VersionSet => entry,
            _ => {
                view.failed += 1;
                continue;
            }
        };
        let lock = crate::commands::modrinth::instance_lock(&app, &entry.instance_id);
        let _instance_guard = lock.lock().await;
        let _switching = crate::commands::versions::SWITCHES.lock().await;
        let (Ok(instance), Ok(game_dir)) = (
            state.instances.get(&entry.instance_id).await,
            state.instances.game_dir(&entry.instance_id),
        ) else {
            view.failed += 1;
            continue;
        };
        let sets = VersionSets::new(&game_dir);
        let taken = instance.mc_version == entry.name
            || tokio::fs::try_exists(sets.set_dir(&entry.name))
                .await
                .unwrap_or(true);
        if taken {
            tracing::info!(instance = %entry.instance_id, minecraft = %entry.name, "a new set took the old one's place; it stays in the trash");
            view.taken += 1;
            continue;
        }
        match trash.restore(&id).await {
            Ok(_) => {
                tracing::info!(instance = %entry.instance_id, minecraft = %entry.name, "put a version set back from the trash");
                view.restored += 1;
                kept.keep(
                    set_key(&entry.instance_id, &entry.name),
                    jiff::Timestamp::now().as_millisecond(),
                );
            }
            Err(error) => {
                tracing::warn!(%id, %error, "could not put a version set back");
                view.failed += 1;
            }
        }
    }
    if view.restored > 0 {
        let root = state.root.clone();
        let saved = tokio::task::spawn_blocking(move || kept.save(&root)).await;
        if let Ok(Err(error)) = saved {
            tracing::warn!(%error, "could not remember the sets put back");
        }
    }
    Ok(view)
}

// ---------------------------------------------------------------------------------------------
// Shared files no instance uses

/// What every instance needs: its active version and every parked set (and both sides of an
/// unfinished switch, loader unknown), plus custom Java executables that may live in
/// `runtimes/`. Fails when the instances cannot be listed: without them nothing can be judged.
async fn game_refs(state: &AppState) -> CmdResult<(Vec<GameRef>, Vec<PathBuf>)> {
    let instances = state.instances.list().await.map_err(CmdError::from)?;
    let mut refs = Vec::new();
    let mut protected = Vec::new();
    for instance in &instances {
        refs.push(GameRef::new(
            instance.mc_version.clone(),
            Some(instance.loader.kind),
            instance.loader.version.clone(),
        ));
        if let JavaSetting::Custom { path } = &instance.java {
            protected.push(path.clone());
        }
        let Ok(game_dir) = state.instances.game_dir(&instance.id) else {
            continue;
        };
        let sets = VersionSets::new(&game_dir);
        if let Ok(Some(journal)) = sets.pending().await {
            for minecraft in [journal.from, journal.to] {
                refs.push(GameRef::new(minecraft, None, None));
            }
        }
        for info in sets.parked().await.unwrap_or_default() {
            refs.push(parked_ref(instance, &info));
        }
    }
    Ok((refs, protected))
}

/// A parked set's version. A set without loader metadata in a modded instance keeps every
/// profile of its game version (its loader is unknown).
fn parked_ref(instance: &Instance, info: &pl_content::SetInfo) -> GameRef {
    let unknown = info.loader.trim().is_empty()
        && info.loader_version.trim().is_empty()
        && instance.loader.kind != LoaderKind::Vanilla;
    let loader = if unknown {
        None
    } else {
        LoaderKind::from_modrinth_name(&info.loader)
    };
    GameRef::new(
        info.minecraft.clone(),
        loader,
        Some(info.loader_version.clone()),
    )
}

/// What the automatic round keeps: [`game_refs`] read strictly (an instance file, a parked set or
/// a switch journal that cannot be read now stops the round's game file part: what it needs is
/// unknown), plus what the launcher trash may bring back within its 30 days (deleted instances
/// with their parked sets and Java, version sets cleaned up).
async fn round_refs(state: &AppState) -> CmdResult<(Vec<GameRef>, Vec<PathBuf>)> {
    let unknown = |what: &str| {
        CmdError::new(
            "TEMIZLIK-OKUNAMADI",
            "",
            format!("{what} could not be read; the game files wait"),
        )
    };
    let instances = state.instances.list().await.map_err(CmdError::from)?;
    // An instance folder the list left out (its file unreadable) may need anything.
    let root = state.instances.root().to_path_buf();
    let folders = tokio::task::spawn_blocking(move || {
        std::fs::read_dir(&root)
            .map(|entries| {
                entries
                    .filter_map(Result::ok)
                    .filter(|e| {
                        e.path()
                            .join(pl_mc::instance::store::INSTANCE_FILE)
                            .exists()
                    })
                    .count()
            })
            .unwrap_or(0)
    })
    .await
    .map_err(CmdError::internal)?;
    if folders != instances.len() {
        return Err(unknown("an instance"));
    }
    let mut refs = Vec::new();
    let mut protected = Vec::new();
    for instance in &instances {
        let game_dir = state
            .instances
            .game_dir(&instance.id)
            .map_err(CmdError::from)?;
        instance_refs(instance, &game_dir, &mut refs, &mut protected)
            .await
            .map_err(|()| unknown("a version set"))?;
    }
    let trash = launcher_trash(state);
    for entry in trash.list(None).await.map_err(CmdError::from)? {
        let item = trash.item_path(&entry);
        match entry.kind {
            TrashKind::Instance => {
                let file = item.join(pl_mc::instance::store::INSTANCE_FILE);
                let instance: Instance = tokio::fs::read(&file)
                    .await
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                    .ok_or_else(|| unknown("a deleted instance"))?;
                let game_dir = item.join(pl_mc::instance::store::GAME_DIR_NAME);
                instance_refs(&instance, &game_dir, &mut refs, &mut protected)
                    .await
                    .map_err(|()| unknown("a deleted instance's set"))?;
            }
            TrashKind::VersionSet => {
                let (info, _) = pl_content::sets::read_set_dir(&item)
                    .await
                    .map_err(|_| unknown("a trashed version set"))?;
                // Its loader is not judged: every profile of its game version stays.
                if let Some(info) = info {
                    refs.push(GameRef::new(info.minecraft, None, None));
                }
            }
            _ => {}
        }
    }
    Ok((refs, protected))
}

/// One instance's versions (active, parked, both sides of an unfinished switch) and custom Java;
/// `Err` when its sets cannot be read.
async fn instance_refs(
    instance: &Instance,
    game_dir: &Path,
    refs: &mut Vec<GameRef>,
    protected: &mut Vec<PathBuf>,
) -> Result<(), ()> {
    refs.push(GameRef::new(
        instance.mc_version.clone(),
        Some(instance.loader.kind),
        instance.loader.version.clone(),
    ));
    if let JavaSetting::Custom { path } = &instance.java {
        protected.push(path.clone());
    }
    let sets = VersionSets::new(game_dir);
    if let Some(journal) = sets.pending().await.map_err(|_| ())? {
        for minecraft in [journal.from, journal.to] {
            refs.push(GameRef::new(minecraft, None, None));
        }
    }
    for info in sets.parked().await.map_err(|_| ())? {
        refs.push(parked_ref(instance, &info));
    }
    Ok(())
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SharedFilesView {
    pub items: Vec<SharedItem>,
    /// Bytes freed only when all of a group's items go (files hard-linked into each).
    pub groups: Vec<SharedGroup>,
    pub total_bytes: u64,
}

/// Game versions, loader profiles, Java runtimes, libraries and assets no instance (and no
/// parked set) uses, with the bytes each frees.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_shared_files(state: State<'_, AppState>) -> CmdResult<SharedFilesView> {
    let (refs, protected) = game_refs(&state).await?;
    let dirs = state.mc.dirs.clone();
    let scan = tauri::async_runtime::spawn_blocking(move || {
        shared_files::scan(
            &dirs,
            &refs,
            &ScanOptions {
                protected,
                ..ScanOptions::default()
            },
        )
    })
    .await
    .map_err(CmdError::internal)?
    .map_err(|error| {
        CmdError::new(
            "TEMIZLIK-TARAMA",
            pick(
                "Oyun dosyaları taranamadı.",
                "Couldn't scan the game files.",
            ),
            error.to_string(),
        )
    })?;
    Ok(SharedFilesView {
        total_bytes: scan.total_bytes(),
        items: scan.items,
        groups: scan.groups,
    })
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CleanupProgressView {
    pub done: u64,
    pub total: u64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CleanupResultView {
    pub freed_bytes: u64,
    pub removed_files: u64,
}

/// Deletes the chosen shared items (judged again first: an item something started using
/// meanwhile stays). They download again when needed.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_delete_shared(
    app: AppHandle,
    state: State<'_, AppState>,
    ids: Vec<String>,
    on_progress: Channel<CleanupProgressView>,
) -> CmdResult<CleanupResultView> {
    ensure_quiet(&app, &state)?;
    let _job = begin_job(&app, DataJob::Cleaning)?;
    let (refs, protected) = game_refs(&state).await?;
    let dirs = state.mc.dirs.clone();
    let report = tauri::async_runtime::spawn_blocking(move || {
        let progress = |done: u64, total: u64| {
            let _ = on_progress.send(CleanupProgressView { done, total });
        };
        shared_files::delete(
            &dirs,
            &refs,
            &ScanOptions {
                protected,
                ..ScanOptions::default()
            },
            &ids,
            &CancellationToken::new(),
            &progress,
        )
    })
    .await
    .map_err(CmdError::internal)?
    .map_err(|error| {
        CmdError::new(
            "TEMIZLIK-SIL",
            pick(
                "Bazı dosyalar silinemedi. Tekrar dene.",
                "Some files couldn't be deleted. Try again.",
            ),
            error.to_string(),
        )
    })?;
    Ok(CleanupResultView {
        freed_bytes: report.freed_bytes,
        removed_files: report.removed_files,
    })
}

// ---------------------------------------------------------------------------------------------
// Leftovers: caches, old logs, crash reports, unfinished downloads

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LeftoverKindView {
    Cache,
    StartCache,
    Downloads,
    Logs,
    CrashReports,
}

impl LeftoverKindView {
    fn kind(self) -> LeftoverKind {
        match self {
            Self::Cache => LeftoverKind::Cache,
            Self::StartCache => LeftoverKind::StartCache,
            Self::Downloads => LeftoverKind::Downloads,
            Self::Logs => LeftoverKind::Logs,
            Self::CrashReports => LeftoverKind::CrashReports,
        }
    }

    fn of(kind: LeftoverKind) -> Self {
        match kind {
            LeftoverKind::Cache => Self::Cache,
            LeftoverKind::StartCache => Self::StartCache,
            LeftoverKind::Downloads => Self::Downloads,
            LeftoverKind::Logs => Self::Logs,
            LeftoverKind::CrashReports => Self::CrashReports,
        }
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LeftoverView {
    pub kind: LeftoverKindView,
    pub bytes: u64,
    pub files: u64,
}

/// Where the cleanup looks, and what the running game holds right now.
async fn places(app: &AppHandle, state: &AppState) -> (Places, InUse) {
    let instances = state.instances.list().await.unwrap_or_default();
    let game_dirs: Vec<PathBuf> = instances
        .iter()
        .filter_map(|instance| state.instances.game_dir(&instance.id).ok())
        .collect();
    let last_played: HashMap<String, i64> = instances
        .iter()
        .filter_map(|instance| {
            Some((
                instance.id.clone(),
                instance.last_played_at?.as_millisecond(),
            ))
        })
        .collect();
    let mut in_use = InUse::default();
    for running in state.games.lock().iter() {
        if let Ok(game_dir) = state.instances.game_dir(&running.instance_id) {
            in_use.game_dirs.push(game_dir);
        }
        if let Some(log) = running.process.log_file() {
            in_use.files.push(log);
        }
    }
    let places = Places {
        root: state.root.clone(),
        game_dirs,
        temp_dir: Some(std::env::temp_dir()),
        app_name: app.package_info().name.clone(),
        app_version: app.package_info().version.to_string(),
        last_played,
    };
    (places, in_use)
}

/// What cleaning caches, old logs, crash reports and unfinished downloads would free.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_leftovers(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Vec<LeftoverView>> {
    let (places, in_use) = places(&app, &state).await;
    let groups = tauri::async_runtime::spawn_blocking(move || {
        housekeeping::scan_leftovers(&places, &Policy::default(), &in_use, SystemTime::now())
    })
    .await
    .map_err(CmdError::internal)?;
    Ok(groups
        .into_iter()
        .map(|group| LeftoverView {
            kind: LeftoverKindView::of(group.kind),
            bytes: group.bytes,
            files: group.files,
        })
        .collect())
}

/// Cleans the chosen leftover groups. With the cache, unused news media go too.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_clean_leftovers(
    app: AppHandle,
    state: State<'_, AppState>,
    kinds: Vec<LeftoverKindView>,
) -> CmdResult<CleanupResultView> {
    ensure_quiet(&app, &state)?;
    let _job = begin_job(&app, DataJob::Cleaning)?;
    let mut result = CleanupResultView {
        freed_bytes: 0,
        removed_files: 0,
    };
    if kinds.contains(&LeftoverKindView::Cache) {
        let content_dir = state.root.join(pl_data::usage::CACHE_DIR).join("content");
        let size = |dir: PathBuf| {
            tauri::async_runtime::spawn_blocking(move || pl_data::usage::dir_size(&dir))
        };
        let before = size(content_dir.clone()).await.unwrap_or(0);
        match state.content.gc().await {
            Ok(gc) => result.removed_files += (gc.removed_docs + gc.removed_media) as u64,
            Err(error) => tracing::warn!(%error, "content cache cleanup failed"),
        }
        let after = size(content_dir).await.unwrap_or(0);
        result.freed_bytes += before.saturating_sub(after);
    }
    let (places, in_use) = places(&app, &state).await;
    let chosen: Vec<LeftoverKind> = kinds.iter().map(|k| k.kind()).collect();
    let report = tauri::async_runtime::spawn_blocking(move || {
        housekeeping::clean_leftovers(
            &places,
            &Policy::default(),
            &in_use,
            SystemTime::now(),
            &chosen,
        )
    })
    .await
    .map_err(CmdError::internal)?;
    result.freed_bytes += report.freed_bytes;
    result.removed_files += report.removed_files;
    Ok(result)
}

// ---------------------------------------------------------------------------------------------
// The launcher trash

/// A version set in the launcher trash.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrashedSetView {
    pub trash_id: String,
    pub instance_id: String,
    pub instance_name: String,
    pub minecraft: String,
    pub deleted_at: Option<String>,
    pub size_bytes: u64,
}

/// An instance deleted from the library, in the launcher trash ("Geri al": `instance_restore`).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrashedInstanceView {
    pub trash_id: String,
    pub name: String,
    pub deleted_at: Option<String>,
    pub size_bytes: u64,
}

/// Anything else in the launcher trash: a world, a backup, a screenshot, the folders "Yeniden
/// yükle" set aside, or a version set whose instance is itself in the trash.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrashedFileView {
    pub trash_id: String,
    pub instance_id: String,
    /// The instance it came from (also when that instance waits in the trash too).
    pub instance_name: Option<String>,
    pub kind: TrashKind,
    /// A world's own name; a file name for the others (the UI words them).
    pub name: String,
    pub file_name: String,
    pub deleted_at: Option<String>,
    pub size_bytes: u64,
    /// "Geri al" can put it back here (worlds, backups and screenshots of an existing
    /// instance, through `trash_restore`).
    pub restorable: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TrashSummaryView {
    /// Everything listed below (expired items about to go are left out).
    pub items: u32,
    pub bytes: u64,
    /// The size cap: over it, the oldest items go first.
    pub cap_bytes: u64,
    /// When the oldest item is deleted by itself.
    pub next_expiry_at: Option<String>,
    /// Version sets in the trash (of instances that still exist), newest first.
    pub sets: Vec<TrashedSetView>,
    /// Deleted instances, newest first (their pages are gone, so they are listed here).
    pub instances: Vec<TrashedInstanceView>,
    /// Everything else, newest first (the instance pages list their own worlds, backups and
    /// screenshots too).
    pub files: Vec<TrashedFileView>,
}

/// What the launcher trash holds: the version sets, deleted instances and everything else in it.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_trash(state: State<'_, AppState>) -> CmdResult<TrashSummaryView> {
    let trash = launcher_trash(&state);
    let usage = trash.usage().await.map_err(|e| {
        CmdError::new(
            "COP-LISTE",
            pick("Çöp kutusu okunamadı.", "Couldn't read the trash."),
            e.to_string(),
        )
    })?;
    let limits = trash_limits(trash.dir());
    let retention = i64::try_from(limits.max_age.as_millis()).unwrap_or(i64::MAX);
    let names: HashMap<String, String> = state
        .instances
        .list()
        .await
        .map(|list| list.into_iter().map(|i| (i.id, i.name)).collect())
        .unwrap_or_default();
    let now = jiff::Timestamp::now().as_millisecond();
    let mut entries: Vec<_> = trash
        .list(None)
        .await
        .unwrap_or_default()
        .into_iter()
        // Expired ones are about to go (the purge runs in the background).
        .filter(|e| e.deleted_at_ms.saturating_add(retention) > now)
        .collect();
    entries.sort_by_key(|e| std::cmp::Reverse(e.deleted_at_ms));
    let items = u32::try_from(entries.len()).unwrap_or(u32::MAX);
    let bytes = entries.iter().map(|e| e.size).sum();
    // Deleted instances keep their names in the trash: their sets and files are named after them.
    let trashed_names: HashMap<String, String> = entries
        .iter()
        .filter(|e| e.kind == TrashKind::Instance)
        .map(|e| (e.instance_id.clone(), e.name.clone()))
        .collect();
    let instances = entries
        .iter()
        .filter(|e| e.kind == TrashKind::Instance)
        .map(|entry| TrashedInstanceView {
            trash_id: entry.id.clone(),
            name: entry.name.clone(),
            deleted_at: iso_ms(entry.deleted_at_ms),
            size_bytes: entry.size,
        })
        .collect();
    let mut sets = Vec::new();
    let mut files = Vec::new();
    for entry in entries {
        let live_name = names.get(&entry.instance_id).cloned();
        match entry.kind {
            TrashKind::Instance => {}
            TrashKind::VersionSet if live_name.is_some() => sets.push(TrashedSetView {
                instance_name: live_name.unwrap_or_default(),
                deleted_at: iso_ms(entry.deleted_at_ms),
                trash_id: entry.id,
                instance_id: entry.instance_id,
                minecraft: entry.name,
                size_bytes: entry.size,
            }),
            kind => files.push(TrashedFileView {
                restorable: live_name.is_some()
                    && matches!(
                        kind,
                        TrashKind::World | TrashKind::Backup | TrashKind::Screenshot
                    ),
                instance_name: live_name.or_else(|| trashed_names.get(&entry.instance_id).cloned()),
                deleted_at: iso_ms(entry.deleted_at_ms),
                trash_id: entry.id,
                instance_id: entry.instance_id,
                kind,
                name: entry.name,
                file_name: entry.file_name,
                size_bytes: entry.size,
            }),
        }
    }
    Ok(TrashSummaryView {
        items,
        bytes,
        cap_bytes: limits.max_bytes.unwrap_or(u64::MAX),
        next_expiry_at: usage
            .oldest_deleted_ms
            .and_then(|ms| iso_ms(ms.saturating_add(retention))),
        sets,
        instances,
        files,
    })
}

/// Deletes one item of the launcher trash for good (frees its space now).
#[tauri::command]
#[specta::specta]
pub async fn cleanup_delete_trashed_set(
    app: AppHandle,
    state: State<'_, AppState>,
    trash_id: String,
) -> CmdResult<CleanupResultView> {
    let _job = begin_job(&app, DataJob::Cleaning)?;
    let trash = launcher_trash(&state);
    let not_found = |detail: String| {
        CmdError::new(
            "COP-YOK",
            pick("Bu öğe çöp kutusunda yok.", "This item isn't in the trash."),
            detail,
        )
    };
    let entry = trash
        .get(&trash_id)
        .await
        .map_err(|e| not_found(e.to_string()))?;
    trash.delete(&trash_id).await.map_err(|e| {
        CmdError::new(
            "COP-SIL",
            pick(
                "Kalıcı olarak silinemedi. Tekrar dene.",
                "Couldn't delete it for good. Try again.",
            ),
            e.to_string(),
        )
    })?;
    tracing::info!(instance = %entry.instance_id, name = %entry.name, kind = ?entry.kind, bytes = entry.size, "deleted an item from the trash for good");
    Ok(CleanupResultView {
        freed_bytes: entry.size,
        removed_files: 1,
    })
}

/// Deletes everything in the launcher trash for good.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_empty_trash(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<CleanupResultView> {
    let _job = begin_job(&app, DataJob::Cleaning)?;
    let report = launcher_trash(&state).empty().await.map_err(|e| {
        CmdError::new(
            "COP-BOSALT",
            pick(
                "Çöp kutusu boşaltılamadı. Tekrar dene.",
                "Couldn't empty the trash. Try again.",
            ),
            e.to_string(),
        )
    })?;
    tracing::info!(
        removed = report.removed,
        freed = report.freed_bytes,
        "emptied the launcher trash"
    );
    Ok(CleanupResultView {
        freed_bytes: report.freed_bytes,
        removed_files: u64::try_from(report.removed).unwrap_or(u64::MAX),
    })
}

/// Applies the trash limits in the background (after a cleanup added to it).
fn purge_trash_later(trash: LauncherTrash) {
    tauri::async_runtime::spawn(async move {
        let limits = trash_limits(trash.dir());
        if let Err(error) = trash.purge(limits).await {
            tracing::warn!(%error, "could not purge the launcher trash");
        }
    });
}

// ---------------------------------------------------------------------------------------------
// The reminder and the automatic tidy-up

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CleanupSummaryView {
    /// Old sets + unused shared files + leftovers (the trash empties itself).
    pub reclaimable_bytes: u64,
    pub free_bytes: Option<u64>,
}

/// How much a cleanup could free, for the occasional reminder.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_summary(
    app: AppHandle,
    state: State<'_, AppState>,
    older_than_days: u32,
) -> CmdResult<CleanupSummaryView> {
    let sets = old_sets(&state, older_than_days)
        .await
        .map(|v| v.total_bytes)
        .unwrap_or(0);
    let shared = cleanup_shared_files(state.clone())
        .await
        .map(|v| v.total_bytes)
        .unwrap_or(0);
    let leftovers: u64 = cleanup_leftovers(app.clone(), state.clone())
        .await
        .map(|list| list.iter().map(|l| l.bytes).sum())
        .unwrap_or(0);
    let status = cleanup_status(app, state).await?;
    Ok(CleanupSummaryView {
        reclaimable_bytes: sets + shared + leftovers,
        free_bytes: status.free_bytes,
    })
}

/// Starts the automatic cleanup: a round shortly after start, then at most once a day
/// ([`Policy::round_every`], counted across restarts in `cleanup-state.json`). A round never runs
/// while a game runs or gets ready, an install or a data job runs: it waits for
/// [`status_changed`] to say the launcher is free and runs [`AFTER_BUSY`] later. Usage data's
/// daily batch rides on the same daily wake (TELEMETRY: no timer of its own); nothing else wakes
/// up for the cleanup.
pub fn start_housekeeping(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_TIDY).await;
        loop {
            crate::telemetry::tick(&app);
            round(&app).await;
            tokio::time::sleep(TIDY_EVERY).await;
        }
    });
}

fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

async fn cleanup_state(state: &AppState) -> CleanupState {
    let root = state.root.clone();
    tokio::task::spawn_blocking(move || CleanupState::load(&root))
        .await
        .unwrap_or_default()
}

/// One automatic round when one is due: the trash limits and junk always; with "Otomatik
/// Temizlik" on also old logs and crash reports, the cache caps, idle start caches and the shared
/// game files unused for [`Policy::unused_age`] ([`housekeeping::sweep_shared`]). A round a game
/// interrupts waits for the launcher to be free again; one that ran out of its budget goes on
/// the next day.
async fn round(app: &AppHandle) {
    let state = app.state::<AppState>();
    let policy = Policy::default();
    let record = cleanup_state(&state).await;
    if !record.round_due(now_ms(), policy.round_every) {
        WAITING.store(false, Ordering::Release);
        return;
    }
    // The stop is in place before the last look, so a launch that starts from here on stops it
    // (and marks the round as waiting again).
    WAITING.store(false, Ordering::Release);
    let cancel = CancellationToken::new();
    *RUNNING.lock().unwrap_or_else(|p| p.into_inner()) = Some(cancel.clone());
    let job = if blocked(app, &state).is_some() {
        None
    } else {
        begin_job(app, DataJob::Cleaning).ok()
    };
    let Some(_job) = job else {
        RUNNING.lock().unwrap_or_else(|p| p.into_inner()).take();
        WAITING.store(true, Ordering::Release);
        return;
    };
    let budget = {
        let cancel = cancel.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(ROUND_BUDGET).await;
            cancel.cancel();
        })
    };
    let (places, in_use) = places(app, &state).await;
    // Without everything the instances and the trash need nothing can be judged: the shared
    // files wait for a later round.
    let refs = if record.auto {
        round_refs(&state)
            .await
            .map_err(|error| tracing::info!(?error, "the game files wait for a later round"))
            .ok()
    } else {
        None
    };
    let dirs = state.mc.dirs.clone();
    let deep = record.auto;
    let work = cancel.clone();
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let now = SystemTime::now();
        let cap = housekeeping::trash_cap(pl_data::usage::disk_size(&places.root));
        let deep = deep && !work.is_cancelled();
        let tidied = housekeeping::tidy(&places, &policy, &in_use, now, Some(cap), deep);
        // Judged and deleted on a copy of the record, which keeps no lock meanwhile; its unused
        // marks are stored after (use marks a launch wrote meanwhile stay).
        let swept = refs.and_then(|(refs, protected)| {
            let mut copy = CleanupState::read(&places.root).ok()?;
            if !copy.auto || work.is_cancelled() {
                return None;
            }
            let swept = housekeeping::sweep_shared(
                &dirs, &refs, &protected, &mut copy, &policy, now, &work,
            );
            if let Err(error) =
                CleanupState::update(&places.root, |record| record.adopt_unused(&copy))
            {
                tracing::warn!(%error, "could not keep the cleanup record");
            }
            Some(swept)
        });
        (tidied, swept)
    })
    .await;
    budget.abort();
    RUNNING.lock().unwrap_or_else(|p| p.into_inner()).take();
    let interrupted = WAITING.load(Ordering::Acquire);
    match outcome {
        Ok((tidied, swept)) => {
            if tidied.trash.removed > 0 || tidied.junk.removed_files > 0 {
                tracing::info!(
                    trash_items = tidied.trash.removed,
                    trash_bytes = tidied.trash.freed_bytes,
                    junk_files = tidied.junk.removed_files,
                    junk_bytes = tidied.junk.freed_bytes,
                    "tidied the data folder"
                );
            }
            if let Some(Err(error)) = swept
                && !matches!(error, pl_mc::Error::Cancelled)
            {
                tracing::warn!(%error, "the game file cleanup did not run");
            }
        }
        Err(error) => tracing::warn!(%error, "the cleanup round did not run"),
    }
    if !interrupted {
        let root = state.root.clone();
        let stamp = now_ms();
        let saved = tokio::task::spawn_blocking(move || {
            CleanupState::update(&root, |record| record.last_round_ms = stamp)
        })
        .await;
        if let Ok(Err(error)) = saved {
            tracing::warn!(%error, "could not keep the cleanup record");
        }
    }
    tidy_skin_history(app).await;
}

/// Marks the version a game was readied with and its Java as used ([`CleanupState`]): the
/// automatic cleanup keeps them (and what they need) for [`Policy::unused_age`] even when no
/// instance refers to them any more. A small write in the background, off the launch's path.
pub(crate) fn note_use(state: &AppState, version_id: &str, java: &Path) {
    let root = state.root.clone();
    let dirs = state.mc.dirs.clone();
    let version = version_id.to_owned();
    let java = java.to_path_buf();
    tauri::async_runtime::spawn_blocking(move || {
        let component = pl_mc::java::managed::managed_runtime_of(&dirs, &java).map(|(c, _)| c);
        let marked = CleanupState::update(&root, |record| {
            record.mark_used(&version, component.as_deref(), now_ms());
        });
        if let Err(error) = marked {
            tracing::debug!(%error, "could not mark the game files as used");
        }
    });
}

/// Whether "Otomatik Temizlik" is on (the default).
#[tauri::command]
#[specta::specta]
pub async fn cleanup_auto(state: State<'_, AppState>) -> CmdResult<bool> {
    Ok(cleanup_state(&state).await.auto)
}

/// Turns "Otomatik Temizlik" on or off (off also stops a round that is deleting game files);
/// returns what is kept.
#[tauri::command]
#[specta::specta]
pub async fn cleanup_set_auto(state: State<'_, AppState>, on: bool) -> CmdResult<bool> {
    if !on && let Some(cancel) = RUNNING.lock().unwrap_or_else(|p| p.into_inner()).as_ref() {
        cancel.cancel();
    }
    let root = state.root.clone();
    tauri::async_runtime::spawn_blocking(move || {
        CleanupState::update(&root, |record| {
            record.auto = on;
            record.auto
        })
    })
    .await
    .map_err(CmdError::internal)?
    .map_err(CmdError::from)
}

/// The skin history's share of the tidy-up: histories of accounts gone from this computer for
/// half a year, pictures no history uses (`pl_auth::SkinHistory::tidy`).
async fn tidy_skin_history(app: &AppHandle) {
    let handle = app.clone();
    let tidied = tauri::async_runtime::spawn_blocking(move || {
        handle.state::<AppState>().auth.tidy_skin_history()
    })
    .await;
    match tidied {
        Ok(Some(report)) if report.histories > 0 || report.files > 0 => tracing::info!(
            histories = report.histories,
            files = report.files,
            bytes = report.bytes,
            "tidied the skin history"
        ),
        Ok(_) => {}
        Err(error) => tracing::warn!(%error, "the skin history tidy-up did not run"),
    }
}
