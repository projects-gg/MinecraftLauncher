//! The mod center of an instance: the pack's recommended mods (items) with the player's
//! choices, the changes the launcher adopted from the mods folder, files it could not read,
//! and applying a new selection (dry run first, then downloads and removals with progress).
//!
//! The player's own mods ("Kendi modların") and the Modrinth browser ("Keşfet") use the
//! add-on commands in `commands::modrinth`; moving own mods to another version lives in
//! `commands::versions`.

use std::collections::BTreeMap;
use std::path::Path;
use std::sync::Arc;

use pl_content::{
    ItemCategory, ItemStatus, ManualChange, PackManifest, PackRef, PackSelection, PackState,
    SyncIssueKind, SyncPhase, SyncPlan, SyncPolicy, SyncProgress, SyncProgressFn, SyncSummary,
    apply_sync, plan_sync,
};
use pl_lang::pick;
use pl_mc::instance::Instance;
use pl_mc::loaders::LoaderKind;
use pl_net::{CancellationToken, DownloadOptions, VerifyMode};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::commands::modrinth::{
    JobProgressView, content_error, instance_lock, percent, start_job,
};
use crate::commands::official::{self, Followed, pack_error};
use crate::commands::versions::{ensure_switch_finished, refuse_while_running};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// Section of an item in the mod center (mirrors `pl_content::ItemCategory`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ModCategory {
    Performance,
    Visual,
    Interface,
    Gameplay,
    Library,
    Other,
}

impl From<ItemCategory> for ModCategory {
    fn from(category: ItemCategory) -> Self {
        match category {
            ItemCategory::Performance => Self::Performance,
            ItemCategory::Visual => Self::Visual,
            ItemCategory::Interface => Self::Interface,
            ItemCategory::Gameplay => Self::Gameplay,
            ItemCategory::Library => Self::Library,
            ItemCategory::Other => Self::Other,
        }
    }
}

/// A change the player made to a pack mod outside the launcher (mirrors
/// `pl_content::ManualChange`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ModChange {
    /// Deleted: switched off.
    Removed,
    /// Renamed to `.disabled`: switched off, the copy is kept.
    Disabled,
    /// A disabled copy renamed back: switched on.
    Enabled,
    /// Replaced by another file: kept until "Önerilene döndür".
    Replaced,
}

fn change_view(change: ManualChange) -> Option<ModChange> {
    match change {
        ManualChange::Removed => Some(ModChange::Removed),
        ManualChange::Disabled => Some(ModChange::Disabled),
        ManualChange::Enabled => Some(ModChange::Enabled),
        ManualChange::Replaced => Some(ModChange::Replaced),
        ManualChange::Other => None,
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ModIssueKind {
    /// The file could not be read.
    Unreadable,
    /// A folder is where a file belongs.
    NotAFile,
    /// The file could not be renamed or deleted (another program has it open).
    Busy,
}

/// A file the sync left alone because it could not handle it.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModIssueView {
    /// File name ("copy details" only).
    pub file_name: String,
    /// The game folder it is in (`mods`, `config`, …).
    pub folder: String,
    /// The recommended item the file belongs to, by its title (what the UI names it by).
    pub title: Option<String>,
    pub kind: ModIssueKind,
    /// Technical details, for "copy details" only.
    pub detail: String,
}

/// One recommended mod (pack item).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModCenterItemView {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub category: ModCategory,
    pub icon_url: Option<String>,
    /// The mod's official page (`https://` only), opened in the browser ("Mod sayfası").
    pub page_url: Option<String>,
    /// Modrinth project, when the pack names it (opens the project page).
    pub project_id: Option<String>,
    /// File name in its folder.
    pub file_name: String,
    /// Folder of the file: `mods`, `resourcepacks`, `shaderpacks`, …
    pub folder: String,
    /// Installed only because other mods need it; never a choice.
    pub library: bool,
    pub default_enabled: bool,
    /// Ids of the items installed together with this one.
    pub requires: Vec<String>,
    /// The player's choice.
    pub selected: bool,
    /// Installed after applying: selected, or needed by an enabled item.
    pub enabled: bool,
    /// Added to the pack after the player's set was made and not confirmed yet ("Yeni").
    pub is_new: bool,
    /// Ids of enabled items that need this one.
    pub required_by: Vec<String>,
    /// The launcher's in-game overlay needs it (Fabric API while "Oyun içi katman" is on): it is
    /// installed for that too.
    pub launcher_needs: bool,
    pub installed: bool,
    /// Installed, and the pack has a newer file for it ("Güncelleme var").
    pub update_available: bool,
    pub size_bytes: u64,
    /// A change the player made in the mods folder that the launcher adopted.
    pub manual: Option<ModChange>,
    /// The player replaced the file: it is kept until "Önerilene döndür".
    pub modified: bool,
    /// Switched off and kept as a disabled copy.
    pub disabled_copy: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModCenterPackView {
    pub id: String,
    pub name: String,
    /// Newest version of the pack.
    pub version: String,
    /// The version the instance has installed (`None` before the first sync).
    pub installed_version: Option<String>,
    pub minecraft: String,
}

/// The pack no longer offers the instance's Minecraft version.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DroppedVersionView {
    pub minecraft: String,
    /// The family's recommended Minecraft version.
    pub recommended: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModCenterView {
    pub instance_id: String,
    pub mc_version: String,
    pub loader: LoaderKind,
    /// The instance follows a pack, so it has recommended mods.
    pub managed: bool,
    pub pack: Option<ModCenterPackView>,
    pub items: Vec<ModCenterItemView>,
    /// The mods folder looked wiped: the next sync installs the selection again.
    pub reset: bool,
    pub issues: Vec<ModIssueView>,
    pub dropped: Option<DroppedVersionView>,
    /// The recommended mods could not be loaded (no network and nothing cached).
    pub unavailable: bool,
    /// The game of this instance is running: changes wait until it is closed.
    pub game_running: bool,
}

/// Choices from the mod center.
#[derive(Debug, Clone, Default, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ModSelectionInput {
    /// Item id → selected (items not listed keep their choice).
    pub items: BTreeMap<String, bool>,
    /// Items whose replaced file goes back to the pack's ("Önerilene döndür").
    pub revert: Vec<String>,
}

impl From<ModSelectionInput> for PackSelection {
    fn from(input: ModSelectionInput) -> Self {
        PackSelection {
            items: input.items,
            groups: BTreeMap::new(),
            revert: input.revert.into_iter().collect(),
        }
    }
}

/// What applying a selection would do.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModCenterPreviewView {
    pub items: Vec<ModCenterItemView>,
    pub summary: SyncSummary,
    /// Files would be downloaded, renamed or deleted.
    pub changes_files: bool,
    /// Applying records something even without file changes (choices, new mods seen).
    pub records_choices: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModCenterApplyView {
    pub downloaded: u32,
    pub restored: u32,
    pub removed: u32,
    /// Files that could not be changed (they stay; the next sync tries again).
    pub issues: u32,
}

/// Changes the launcher adopted, shown as a non-blocking notice after a launch sync.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModsNoticeView {
    pub changes: Vec<ModChangeView>,
    /// The mods folder looked wiped and the selection was installed again.
    pub reset: bool,
    /// Files that could not be read or changed (details in the mod center).
    pub issues: u32,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModChangeView {
    pub item_id: String,
    pub title: String,
    pub change: ModChange,
    /// Titles of enabled mods that need it (why it came back).
    pub required_by: Vec<String>,
}

/// Where an instance's recommended mods come from.
pub(crate) enum PackSource {
    NotManaged,
    Unavailable,
    Dropped(DroppedVersionView),
    Ready {
        pack: Box<PackRef>,
        manifest: Box<PackManifest>,
    },
}

/// The manifest of the pack an instance follows for its Minecraft version (content cache
/// first, then the network).
pub(crate) async fn pack_source(
    state: &AppState,
    instance: &Instance,
    game_dir: &Path,
) -> PackSource {
    if instance.managed_pack.is_none() {
        return PackSource::NotManaged;
    }
    let Some(snapshot) = official::cached_or_fresh(state).await else {
        return PackSource::Unavailable;
    };
    let family =
        official::instance_family(&snapshot.index, instance, Some(game_dir)).unwrap_or_default();
    let pack = match official::followed_pack(&snapshot.index, instance, &family) {
        Followed::Member(pack) | Followed::InPlace(pack) => pack.clone(),
        Followed::Dropped { recommended } => {
            return PackSource::Dropped(DroppedVersionView {
                minecraft: instance.mc_version.clone(),
                recommended: recommended
                    .and_then(|pack| pack.minecraft())
                    .map(str::to_owned),
            });
        }
    };
    match state.content.pack_manifest(&pack).await {
        Ok(manifest) => PackSource::Ready {
            pack: Box::new(pack),
            manifest: Box::new(manifest),
        },
        Err(error) => {
            tracing::warn!(%error, "pack manifest unavailable for the mod center");
            PackSource::Unavailable
        }
    }
}

fn split_path(path: &str) -> (String, String) {
    match path.rsplit_once('/') {
        Some((folder, name)) => (folder.to_owned(), name.to_owned()),
        None => (String::new(), path.to_owned()),
    }
}

pub(crate) fn item_views(
    manifest: &PackManifest,
    statuses: &[ItemStatus],
) -> Vec<ModCenterItemView> {
    statuses
        .iter()
        // A launcher setting's items ("Oyun içi katman") have their one place there.
        .filter(|status| !pl_content::LauncherItems::owns(&status.id))
        .filter_map(|status| {
            let item = manifest.projects.item(&status.id)?;
            let (folder, file_name) = split_path(&item.path);
            Some(ModCenterItemView {
                id: item.id.clone(),
                title: item.title.clone(),
                summary: item.summary.clone(),
                category: item.category.into(),
                icon_url: item.icon_url.clone(),
                page_url: item.page().map(str::to_owned),
                project_id: item.modrinth.as_ref().map(|m| m.project_id.clone()),
                file_name,
                folder,
                library: item.library,
                default_enabled: item.default_enabled,
                requires: item.requires.clone(),
                selected: status.selected,
                enabled: status.effectively_enabled,
                is_new: status.is_new,
                required_by: status.required_by.clone(),
                launcher_needs: status.launcher_needs,
                installed: status.installed,
                update_available: status.update_available,
                size_bytes: status.size,
                manual: status.manual.and_then(change_view),
                modified: status.modified,
                disabled_copy: status.disabled_copy,
            })
        })
        .collect()
}

fn issue_views(plan: &SyncPlan, manifest: &PackManifest) -> Vec<ModIssueView> {
    plan.issues
        .iter()
        .map(|issue| {
            let (folder, file_name) = split_path(&issue.path);
            let path = issue.path.strip_suffix(".disabled").unwrap_or(&issue.path);
            ModIssueView {
                file_name,
                folder,
                title: manifest
                    .projects
                    .item_for_path(path)
                    .map(|item| item.title.clone()),
                kind: match issue.kind {
                    SyncIssueKind::Unreadable => ModIssueKind::Unreadable,
                    SyncIssueKind::NotAFile => ModIssueKind::NotAFile,
                    SyncIssueKind::Busy => ModIssueKind::Busy,
                },
                detail: issue.detail.clone(),
            }
        })
        .collect()
}

/// The notice a launch sync shows for adopted changes, a wiped mods folder or unreadable
/// files; `None` when there is nothing to say. `failed` counts renames and deletions that
/// failed while applying.
pub(crate) fn notice(
    plan: &SyncPlan,
    manifest: &PackManifest,
    failed: usize,
) -> Option<ModsNoticeView> {
    let title = |id: &str| manifest.projects.item(id).map(|item| item.title.clone());
    let changes: Vec<ModChangeView> = plan
        .adopted
        .iter()
        .filter_map(|adopted| {
            let change = change_view(adopted.change)?;
            let status = plan.items.iter().find(|s| s.id == adopted.item);
            let required_by = status
                .filter(|s| s.effectively_enabled && !s.selected)
                .map(|s| s.required_by.iter().filter_map(|id| title(id)).collect())
                .unwrap_or_default();
            Some(ModChangeView {
                item_id: adopted.item.clone(),
                title: title(&adopted.item)?,
                change,
                required_by,
            })
        })
        .collect();
    let issues = plan.issues.len() + failed;
    (!changes.is_empty() || plan.reset || issues > 0).then(|| ModsNoticeView {
        changes,
        reset: plan.reset,
        issues: u32::try_from(issues).unwrap_or(u32::MAX),
    })
}

/// Background-sync policy: files whose size and recorded hash match are not hashed again; the
/// launcher's own items follow their settings ("Oyun içi katman", crate::overlay).
pub(crate) fn quick() -> SyncPolicy {
    SyncPolicy {
        repair: false,
        quick_check: true,
        items: crate::overlay::pack_items(),
    }
}

/// Download options for pack syncs started from the mod center or a version switch.
pub(crate) fn sync_options(cancel: &CancellationToken) -> DownloadOptions {
    DownloadOptions {
        concurrency: 8,
        verify: VerifyMode::Full,
        cancel: cancel.clone(),
        ..DownloadOptions::default()
    }
}

/// Sync progress as job progress for the UI.
pub(crate) fn sync_progress(channel: Channel<JobProgressView>) -> SyncProgressFn {
    Arc::new(move |p: SyncProgress| {
        let _ = channel.send(JobProgressView {
            stage: match p.phase {
                SyncPhase::Downloading => "downloading",
                SyncPhase::Removing => "removing",
                SyncPhase::Finished => "finishing",
            }
            .to_owned(),
            percent: percent(
                p.done_bytes,
                p.total_bytes,
                p.done_files as u64,
                p.total_files as u64,
            ),
            done_files: p.done_files as u64,
            total_files: p.total_files as u64,
            bytes_per_sec: p.bytes_per_sec,
        });
    })
}

fn game_running(state: &AppState, instance_id: &str) -> bool {
    state
        .running
        .lock()
        .as_ref()
        .is_some_and(|running| running.instance_id == instance_id)
}

fn not_available() -> CmdError {
    CmdError::new(
        "PAKET-YOK",
        pick(
            "Önerilen modlar şu an yüklenemiyor. İnternet bağlantını kontrol edip tekrar dene.",
            "The recommended mods can't be loaded right now. Check your internet connection and try again.",
        ),
        "no pack manifest for this instance",
    )
}

/// The mod center of an instance (a dry run of the next sync: adopted changes and issues show
/// up before the next launch).
#[tauri::command]
#[specta::specta]
pub async fn mod_center(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<ModCenterView> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let mut view = ModCenterView {
        instance_id: instance.id.clone(),
        mc_version: instance.mc_version.clone(),
        loader: instance.loader.kind,
        managed: instance.managed_pack.is_some(),
        pack: None,
        items: Vec::new(),
        reset: false,
        issues: Vec::new(),
        dropped: None,
        unavailable: false,
        game_running: game_running(&state, &instance_id),
    };
    match pack_source(&state, &instance, &game_dir).await {
        PackSource::NotManaged => {}
        PackSource::Unavailable => view.unavailable = true,
        PackSource::Dropped(dropped) => view.dropped = Some(dropped),
        PackSource::Ready { pack, manifest } => {
            let previous = PackState::load(&game_dir).await.map_err(pack_error)?;
            let plan = plan_sync(&manifest, &game_dir, previous.as_ref(), None, &quick())
                .await
                .map_err(pack_error)?;
            view.items = item_views(&manifest, &plan.items);
            view.issues = issue_views(&plan, &manifest);
            view.reset = plan.reset;
            view.pack = Some(ModCenterPackView {
                id: pack.id.clone(),
                name: manifest.name.clone(),
                version: manifest.version_id.clone(),
                installed_version: previous
                    .map(|state| state.version_id)
                    .filter(|version| !version.is_empty()),
                minecraft: manifest.dependencies.minecraft.clone(),
            });
        }
    }
    Ok(view)
}

/// What applying `selection` would download and remove (nothing changes on disk).
#[tauri::command]
#[specta::specta]
pub async fn mod_center_preview(
    state: State<'_, AppState>,
    instance_id: String,
    selection: ModSelectionInput,
) -> CmdResult<ModCenterPreviewView> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let PackSource::Ready { manifest, .. } = pack_source(&state, &instance, &game_dir).await else {
        return Err(not_available());
    };
    let previous = PackState::load(&game_dir).await.map_err(pack_error)?;
    let selection: PackSelection = selection.into();
    let plan = plan_sync(
        &manifest,
        &game_dir,
        previous.as_ref(),
        Some(&selection),
        &quick(),
    )
    .await
    .map_err(pack_error)?;
    Ok(ModCenterPreviewView {
        items: item_views(&manifest, &plan.items),
        summary: plan.summary(),
        changes_files: plan.changes_files(),
        records_choices: plan.state_changed,
    })
}

/// Applies a selection: downloads, renames back and removals, with progress. Refused while the
/// instance's game is running.
#[tauri::command]
#[specta::specta]
pub async fn mod_center_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    selection: ModSelectionInput,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<ModCenterApplyView> {
    refuse_while_running(&state, &instance_id)?;
    let job = start_job(&app, &job_id);
    let _ = on_progress.send(JobProgressView::stage("resolving"));
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };
    apply_selection(
        &state,
        &instance_id,
        selection.into(),
        Some(on_progress),
        &job.token,
    )
    .await
}

/// Plans and applies `selection` for an instance (the caller holds the instance lock).
pub(crate) async fn apply_selection(
    state: &AppState,
    instance_id: &str,
    selection: PackSelection,
    on_progress: Option<Channel<JobProgressView>>,
    cancel: &CancellationToken,
) -> CmdResult<ModCenterApplyView> {
    refuse_while_running(state, instance_id)?;
    let mut instance = state
        .instances
        .get(instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(instance_id)
        .map_err(CmdError::from)?;
    let PackSource::Ready { manifest, .. } = pack_source(state, &instance, &game_dir).await else {
        return Err(not_available());
    };
    let previous = PackState::load(&game_dir).await.map_err(pack_error)?;
    let plan = plan_sync(
        &manifest,
        &game_dir,
        previous.as_ref(),
        Some(&selection),
        &quick(),
    )
    .await
    .map_err(pack_error)?;
    // Nothing starts that cannot fit on the disk (the launch's margin; "Yer aç").
    let downloads = plan
        .downloads
        .iter()
        .map(|download| download.size)
        .fold(0, u64::saturating_add);
    pl_mc::install::space::ensure_room(&game_dir, downloads).map_err(CmdError::from)?;
    let report = apply_sync(
        &plan,
        &state.mc.downloader,
        &sync_options(cancel),
        on_progress.map(sync_progress),
    )
    .await
    .map_err(|error| {
        // A disk that fills up on the way says so (and offers "Yer aç").
        let details = error.to_string();
        if pl_net::mentions_disk_full(&details) {
            CmdError::disk_full(details)
        } else {
            pack_error(error)
        }
    })?;
    // A mod switched on here gets its first settings now, before the next launch.
    crate::commands::modsets::seed_mod_settings(
        &game_dir,
        &plan.placed_mods(&manifest.projects.items),
    )
    .await;
    tracing::info!(
        instance = %instance_id,
        summary = %plan.summary().describe_tr(),
        "mod center selection applied"
    );
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    Ok(ModCenterApplyView {
        downloaded: count(report.downloaded),
        restored: count(report.restored),
        removed: count(
            plan.deletes
                .iter()
                .filter(|delete| !delete.replaced)
                .count()
                .min(report.deleted),
        ),
        issues: count(report.issues.len()),
    })
}

/// The player saw the mod center: new mods are no longer flagged and adopted changes are
/// acknowledged (no files change).
#[tauri::command]
#[specta::specta]
pub async fn mod_center_mark_seen(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<()> {
    let lock = instance_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let PackSource::Ready { manifest, .. } = pack_source(&state, &instance, &game_dir).await else {
        return Ok(());
    };
    let Some(mut pack_state) = PackState::load(&game_dir).await.map_err(pack_error)? else {
        return Ok(());
    };
    let before = pack_state.clone();
    pack_state.mark_items_seen(&manifest);
    if pack_state != before {
        pack_state.save(&game_dir).await.map_err(pack_error)?;
    }
    Ok(())
}

/// The pack item a mods-folder file belongs to (`mods/<file>` without `.disabled`), from the
/// active pack state.
pub(crate) async fn item_for_mod_file(game_dir: &Path, file_name: &str) -> Option<String> {
    let base = file_name.strip_suffix(".disabled").unwrap_or(file_name);
    let state = PackState::load(game_dir).await.ok().flatten()?;
    let record = state.files.get(&format!("mods/{base}"))?;
    record.item.clone()
}
