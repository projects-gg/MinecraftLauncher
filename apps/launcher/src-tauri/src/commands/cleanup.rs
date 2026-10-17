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
use std::time::{Duration, SystemTime};

use pl_content::VersionSets;
use pl_data::housekeeping::{self, InUse, KeptSets, LeftoverKind, Places, Policy};
use pl_mc::gamefiles::{LauncherTrash, TrashKind, TrashLimits};
use pl_mc::instance::Instance;
use pl_mc::java::JavaSetting;
use pl_mc::loaders::LoaderKind;
use pl_mc::shared_files::{self, GameRef, ScanOptions, SharedGroup, SharedItem};
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::commands::data::{DataJob, begin_job};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// The automatic tidy-up starts this long after the launcher (never slows the start).
const FIRST_TIDY: Duration = Duration::from_secs(90);
/// ...and then runs this often while the launcher stays open.
const TIDY_EVERY: Duration = Duration::from_secs(24 * 60 * 60);
/// When something was busy, the tidy-up tries again after this.
const TIDY_RETRY: Duration = Duration::from_secs(60 * 60);
const DAY_MS: i64 = 24 * 60 * 60 * 1000;

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
    if state.running.lock().is_some() {
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
            "Oyun açıkken temizlik yapılamaz. Önce oyunu kapat.",
        ),
        CleanupBlock::Preparing => (
            "TEMIZLIK-HAZIRLIK",
            "Oyun hazırlanırken temizlik yapılamaz. Bitmesini bekle.",
        ),
        CleanupBlock::Busy => (
            "TEMIZLIK-ISLEM",
            "Arka planda bir kurulum ya da yedekleme sürüyor. Bitince tekrar dene.",
        ),
        CleanupBlock::DataJob => (
            "TEMIZLIK-VERI",
            "Veri klasöründe başka bir işlem sürüyor. Bitince tekrar dene.",
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
            "Oyun dosyaları taranamadı.",
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
            "Bazı dosyalar silinemedi. Tekrar dene.",
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
    Downloads,
    Logs,
    CrashReports,
}

impl LeftoverKindView {
    fn kind(self) -> LeftoverKind {
        match self {
            Self::Cache => LeftoverKind::Cache,
            Self::Downloads => LeftoverKind::Downloads,
            Self::Logs => LeftoverKind::Logs,
            Self::CrashReports => LeftoverKind::CrashReports,
        }
    }

    fn of(kind: LeftoverKind) -> Self {
        match kind {
            LeftoverKind::Cache => Self::Cache,
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
    let game_dirs: Vec<PathBuf> = state
        .instances
        .list()
        .await
        .unwrap_or_default()
        .iter()
        .filter_map(|instance| state.instances.game_dir(&instance.id).ok())
        .collect();
    let mut in_use = InUse::default();
    if let Some(running) = state.running.lock().as_ref() {
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
    let usage = trash
        .usage()
        .await
        .map_err(|e| CmdError::new("COP-LISTE", "Çöp kutusu okunamadı.", e.to_string()))?;
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
    let not_found = |detail: String| CmdError::new("COP-YOK", "Bu öğe çöp kutusunda yok.", detail);
    let entry = trash
        .get(&trash_id)
        .await
        .map_err(|e| not_found(e.to_string()))?;
    trash.delete(&trash_id).await.map_err(|e| {
        CmdError::new(
            "COP-SIL",
            "Kalıcı olarak silinemedi. Tekrar dene.",
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
            "Çöp kutusu boşaltılamadı. Tekrar dene.",
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

/// Starts the automatic tidy-up: shortly after start, then once a day while the launcher stays
/// open. A round that finds the launcher busy (game, install, data job) tries again later.
pub fn start_housekeeping(app: &AppHandle) {
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(FIRST_TIDY).await;
        loop {
            let wait = if tidy(&app).await {
                TIDY_EVERY
            } else {
                TIDY_RETRY
            };
            tokio::time::sleep(wait).await;
        }
    });
}

/// One round of the automatic tidy-up; false when it had to wait for a busy launcher.
async fn tidy(app: &AppHandle) -> bool {
    let state = app.state::<AppState>();
    // The trash limits and junk rules never touch what a game uses, but the launcher stays
    // quiet while it prepares one (downloads write `.part` files) or moves the data folder.
    if matches!(
        blocked(app, &state),
        Some(CleanupBlock::Preparing | CleanupBlock::Busy | CleanupBlock::DataJob)
    ) {
        return false;
    }
    let Ok(_job) = begin_job(app, DataJob::Cleaning) else {
        return false;
    };
    let (places, in_use) = places(app, &state).await;
    let outcome = tauri::async_runtime::spawn_blocking(move || {
        let cap = housekeeping::trash_cap(pl_data::usage::disk_size(&places.root));
        housekeeping::tidy(
            &places,
            &Policy::default(),
            &in_use,
            SystemTime::now(),
            Some(cap),
        )
    })
    .await;
    match outcome {
        Ok(report) => {
            if report.trash.removed > 0 || report.junk.removed_files > 0 {
                tracing::info!(
                    trash_items = report.trash.removed,
                    trash_bytes = report.trash.freed_bytes,
                    junk_files = report.junk.removed_files,
                    junk_bytes = report.junk.freed_bytes,
                    "tidied the data folder"
                );
            }
        }
        Err(error) => tracing::warn!(%error, "the tidy-up did not run"),
    }
    tidy_skin_history(app).await;
    true
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
