//! Ready-made mod sets ("Hazır setler"): the catalog the team publishes in Studio, the sets an
//! instance has, adding, updating and removing a set (the pack sync engine, see
//! `pl_content::modsets`) and new instances started from a set.
//!
//! Nothing here runs on its own: a set changes files only when the player acts. The only work
//! outside these commands is turning on, right before a launch, the resource packs a set
//! installed while the instance had no `options.txt` yet ([`activate_pending_packs`]).

use std::collections::BTreeSet;
use std::path::Path;

use pl_content::{
    AddedSet, ModSet, ModSetCatalog, ModSetItemKind, ModSetPurpose, ModSetStates, ModSetVariant,
    PackItem, PackLoader, PackSelection, SetApplyReport, SetChange, SyncPlan, apply_set,
    keep_shared_files, plan_set, plan_set_removal, provided_projects,
};
use pl_lang::pick;
use pl_mc::addons::{AddonIndex, index_key};
use pl_mc::instance::{Instance, PackListChange};
use pl_mc::loaders::LoaderKind;
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::commands::instances::{InstanceView, NewInstanceRequest, create_instance, view};
use crate::commands::modcenter::{
    ModCenterItemView, ModCenterPreviewView, ModIssueKind, ModIssueView, ModSelectionInput,
    PackSource, item_views, pack_source, sync_options, sync_progress,
};
use crate::commands::modrinth::{
    JobProgressView, addon_index_path, content_error, instance_lock, pack_owned, start_job,
};
use crate::commands::official::{self, pack_error};
use crate::commands::versions::{ensure_switch_finished, refuse_while_running};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// What a set is for (mirrors `pl_content::ModSetPurpose`): its card's mark.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SetPurpose {
    Pvp,
    Performance,
    Visual,
    Other,
}

impl From<ModSetPurpose> for SetPurpose {
    fn from(purpose: ModSetPurpose) -> Self {
        match purpose {
            ModSetPurpose::Pvp => Self::Pvp,
            ModSetPurpose::Performance => Self::Performance,
            ModSetPurpose::Visual => Self::Visual,
            ModSetPurpose::Other => Self::Other,
        }
    }
}

/// Kind of a set item (from its file's folder).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SetItemKind {
    Mod,
    ResourcePack,
    ShaderPack,
}

impl From<ModSetItemKind> for SetItemKind {
    fn from(kind: ModSetItemKind) -> Self {
        match kind {
            ModSetItemKind::Mod => Self::Mod,
            ModSetItemKind::ResourcePack => Self::ResourcePack,
            ModSetItemKind::ShaderPack => Self::ShaderPack,
        }
    }
}

/// One item of a set, as the cards and the add dialog show it.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetItemPreview {
    pub id: String,
    pub title: String,
    pub summary: Option<String>,
    pub kind: SetItemKind,
    pub icon_url: Option<String>,
    pub page_url: Option<String>,
    pub project_id: Option<String>,
    pub default_enabled: bool,
    /// Installed only because another item needs it; never a choice.
    pub library: bool,
    pub requires: Vec<String>,
    pub size_bytes: u64,
}

/// One Minecraft version (and loader) of a set.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetVariantView {
    pub minecraft: String,
    /// `None`: resource and shader packs only, for any loader (vanilla too).
    pub loader: Option<LoaderKind>,
    /// The loader version a new instance from the set gets.
    pub loader_version: Option<String>,
    pub items: Vec<SetItemPreview>,
    /// What adding the set without changes installs (its default items and what they need).
    pub mods: u32,
    pub resource_packs: u32,
    pub shader_packs: u32,
    pub download_bytes: u64,
}

/// A set card.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetCardView {
    pub id: String,
    /// Turkish wording; the `*_en` fields carry the English one when the team wrote it.
    pub name: String,
    pub name_en: Option<String>,
    pub summary: Option<String>,
    pub summary_en: Option<String>,
    pub purpose: SetPurpose,
    /// Newest Minecraft version first.
    pub variants: Vec<SetVariantView>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetCatalogView {
    pub sets: Vec<SetCardView>,
    /// The sets could not be loaded (no network and nothing cached).
    pub unavailable: bool,
}

/// A set an instance has.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSetView {
    pub id: String,
    pub name: String,
    pub name_en: Option<String>,
    pub summary: Option<String>,
    pub summary_en: Option<String>,
    pub purpose: SetPurpose,
    /// Installed for the instance's Minecraft version.
    pub installed: bool,
    /// The catalog has a variant for this instance (needed to install or update it).
    pub available: bool,
    /// The team changed the set's files since it was installed ("Güncelleme var").
    pub update_available: bool,
    /// The catalog no longer has the set: it keeps working and can be removed.
    pub retired: bool,
    /// The set's items under the current choices, like the mod center's.
    pub items: Vec<ModCenterItemView>,
    /// Items the instance already has from elsewhere (the official pack, own mods).
    pub present: Vec<SetItemPreview>,
    /// Applying now would change files (a file is missing, something it needed went away).
    pub pending: bool,
    pub issues: Vec<ModIssueView>,
    /// Bytes of the files the set installed here.
    pub size_bytes: u64,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstanceSetsView {
    pub instance_id: String,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub sets: Vec<InstanceSetView>,
    /// The game of this instance is running: changes wait until it is closed.
    pub game_running: bool,
    /// The catalog could not be loaded (the instance's sets still show from its records).
    pub unavailable: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetApplyView {
    pub downloaded: u32,
    pub restored: u32,
    pub removed: u32,
    /// Files that could not be changed (they stay; the next apply tries again).
    pub issues: u32,
}

/// A new instance started from a set.
#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetInstanceRequest {
    pub set_id: String,
    /// The variant's Minecraft version.
    pub minecraft: String,
    pub name: String,
    pub icon: String,
    /// Choices made before creating (items not listed keep their default).
    #[serde(default)]
    pub selection: Option<ModSelectionInput>,
    /// Settings → Oyun defaults, as for a new instance.
    #[serde(default)]
    pub max_memory_mb: Option<u32>,
    #[serde(default)]
    pub jvm_args: Option<String>,
}

fn loader_kind(loader: Option<PackLoader>) -> LoaderKind {
    match loader {
        None => LoaderKind::Vanilla,
        Some(PackLoader::Fabric) => LoaderKind::Fabric,
        Some(PackLoader::Quilt) => LoaderKind::Quilt,
        Some(PackLoader::Forge) => LoaderKind::Forge,
        Some(PackLoader::NeoForge) => LoaderKind::NeoForge,
    }
}

fn pack_loader(kind: LoaderKind) -> Option<PackLoader> {
    match kind {
        LoaderKind::Vanilla => None,
        LoaderKind::Fabric => Some(PackLoader::Fabric),
        LoaderKind::Quilt => Some(PackLoader::Quilt),
        LoaderKind::Forge => Some(PackLoader::Forge),
        LoaderKind::NeoForge => Some(PackLoader::NeoForge),
    }
}

fn item_preview(variant: &ModSetVariant, item: &PackItem) -> SetItemPreview {
    SetItemPreview {
        id: item.id.clone(),
        title: item.title.clone(),
        summary: item.summary.clone(),
        kind: ModSetVariant::item_kind(item).into(),
        icon_url: item.icon_url.clone(),
        page_url: item.page().map(str::to_owned),
        project_id: item.modrinth.as_ref().map(|m| m.project_id.clone()),
        default_enabled: item.default_enabled,
        library: item.library,
        requires: item.requires.clone(),
        size_bytes: variant.file(&item.path).map_or(0, |file| file.file_size),
    }
}

fn variant_view(variant: &ModSetVariant) -> SetVariantView {
    let defaults = variant.default_items();
    let count = |kind: ModSetItemKind| {
        let n = defaults
            .iter()
            .filter(|item| !item.library && ModSetVariant::item_kind(item) == kind)
            .count();
        u32::try_from(n).unwrap_or(u32::MAX)
    };
    SetVariantView {
        minecraft: variant.minecraft().to_owned(),
        loader: variant.loader().map(|loader| loader_kind(Some(loader))),
        loader_version: variant.loader_version().map(str::to_owned),
        items: variant
            .items
            .iter()
            .map(|item| item_preview(variant, item))
            .collect(),
        mods: count(ModSetItemKind::Mod),
        resource_packs: count(ModSetItemKind::ResourcePack),
        shader_packs: count(ModSetItemKind::ShaderPack),
        download_bytes: variant.default_download_size(),
    }
}

fn card_view(set: &ModSet) -> SetCardView {
    let english = set.en.as_ref();
    let mut variants: Vec<&ModSetVariant> = set.variants.iter().collect();
    variants.sort_by(|a, b| pl_content::compare_minecraft_versions(b.minecraft(), a.minecraft()));
    SetCardView {
        id: set.id.clone(),
        name: set.name.clone(),
        name_en: english.and_then(|en| en.name.clone()),
        summary: set.summary.clone(),
        summary_en: english.and_then(|en| en.summary.clone()),
        purpose: set.purpose.into(),
        variants: variants.into_iter().map(variant_view).collect(),
    }
}

/// The catalog this launcher may offer (sets that need a newer launcher are left out). `None`
/// when it cannot be loaded.
async fn catalog(app: &AppHandle, state: &AppState) -> Option<ModSetCatalog> {
    let snapshot = official::cached_or_fresh(state).await?;
    match state.content.mod_sets(&snapshot.index).await {
        Ok(mut catalog) => {
            let launcher = app.package_info().version.clone();
            catalog.sets.retain(|set| set.supported_by(&launcher));
            Some(catalog)
        }
        Err(error) => {
            tracing::warn!(%error, "ready-made mod sets unavailable");
            None
        }
    }
}

fn not_available() -> CmdError {
    CmdError::new(
        "SET-YOK",
        pick(
            "Hazır setler şu an yüklenemiyor. İnternet bağlantını kontrol edip tekrar dene.",
            "The ready-made sets can't be loaded right now. Check your internet connection and try again.",
        ),
        "no mod set catalog",
    )
}

fn set_missing(set_id: &str) -> CmdError {
    CmdError::new(
        "SET-YOK",
        pick(
            "Bu set artık sunulmuyor.",
            "This set isn't offered anymore.",
        ),
        format!("mod set {set_id} is not in the catalog"),
    )
}

fn no_variant(set: &ModSet, instance: &Instance) -> CmdError {
    CmdError::new(
        "SET-SURUM",
        pick(
            "Bu set, kurulumun sürümü için hazır değil.",
            "This set isn't ready for this instance's version.",
        ),
        format!(
            "mod set {} has no variant for {} {:?}",
            set.id, instance.mc_version, instance.loader.kind
        ),
    )
}

/// The Modrinth projects the instance already has outside `set_id`: the official pack's items
/// (on or off), the player's own Modrinth files, and projects other sets installed from another
/// file than `variant` uses (two copies of one mod would stop the game).
async fn provided_for(
    state: &AppState,
    instance: &Instance,
    game_dir: &Path,
    states: &ModSetStates,
    set_id: &str,
    variant: &ModSetVariant,
) -> BTreeSet<String> {
    let pack = match pack_source(state, instance, game_dir).await {
        PackSource::Ready { manifest, .. } => Some(manifest),
        _ => None,
    };
    let pack_files = pack_owned(game_dir);
    let index = match state.instances.instance_dir(&instance.id) {
        Ok(dir) => AddonIndex::load(&addon_index_path(&dir)).await,
        Err(_) => AddonIndex::default(),
    };
    let own: Vec<String> = index
        .files
        .iter()
        .filter(|(key, _)| !pack_files.contains(*key) && states.owner(key).is_none())
        .filter_map(|(_, record)| record.source.as_ref().map(|s| s.project_id.clone()))
        .collect();
    let mut provided = provided_projects(pack.as_deref(), own.iter().map(String::as_str));
    for (id, other) in states.sets.iter().filter(|(id, _)| id.as_str() != set_id) {
        let Some(applied) = &other.applied else {
            continue;
        };
        let _ = id;
        for item in &applied.items {
            let Some(project) = item.modrinth.as_ref().map(|m| &m.project_id) else {
                continue;
            };
            if !other.state.files.contains_key(&item.path) {
                continue;
            }
            let ours = variant
                .items
                .iter()
                .find(|mine| mine.modrinth.as_ref().map(|m| &m.project_id) == Some(project));
            if ours.is_some_and(|mine| mine.path != item.path) {
                provided.insert(project.clone());
            }
        }
    }
    provided
}

/// The items `variant` leaves out for this instance (their project is already there).
fn present_items(variant: &ModSetVariant, plan: &SyncPlan) -> Vec<SetItemPreview> {
    variant
        .items
        .iter()
        .filter(|item| !item.library && !plan.items.iter().any(|status| status.id == item.id))
        .map(|item| item_preview(variant, item))
        .collect()
}

fn issue_views(plan: &SyncPlan, variant: &ModSetVariant) -> Vec<ModIssueView> {
    plan.issues
        .iter()
        .map(|issue| {
            let (folder, file_name) = issue
                .path
                .rsplit_once('/')
                .map(|(folder, name)| (folder.to_owned(), name.to_owned()))
                .unwrap_or_else(|| (String::new(), issue.path.clone()));
            let path = issue.path.strip_suffix(".disabled").unwrap_or(&issue.path);
            ModIssueView {
                file_name,
                folder,
                title: variant
                    .items
                    .iter()
                    .find(|item| item.path == path)
                    .map(|item| item.title.clone()),
                kind: match issue.kind {
                    pl_content::SyncIssueKind::Unreadable => ModIssueKind::Unreadable,
                    pl_content::SyncIssueKind::NotAFile => ModIssueKind::NotAFile,
                    pl_content::SyncIssueKind::Busy => ModIssueKind::Busy,
                },
                detail: issue.detail.clone(),
            }
        })
        .collect()
}

fn game_running(state: &AppState, instance_id: &str) -> bool {
    state
        .running
        .lock()
        .as_ref()
        .is_some_and(|running| running.instance_id == instance_id)
}

/// The ready-made sets this launcher offers, with what each variant installs.
#[tauri::command]
#[specta::specta]
pub async fn mod_sets_catalog(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<SetCatalogView> {
    Ok(match catalog(&app, &state).await {
        Some(catalog) => SetCatalogView {
            sets: catalog.sets.iter().map(card_view).collect(),
            unavailable: false,
        },
        None => SetCatalogView {
            sets: Vec::new(),
            unavailable: true,
        },
    })
}

/// The sets of an instance with their items under the current choices (a dry run: nothing
/// changes on disk).
#[tauri::command]
#[specta::specta]
pub async fn mod_sets_instance(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<InstanceSetsView> {
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
    let states = ModSetStates::load(&game_dir).await.map_err(pack_error)?;
    let catalog = if states.sets.is_empty() {
        None
    } else {
        catalog(&app, &state).await
    };
    let loader = pack_loader(instance.loader.kind);
    let mut sets = Vec::new();
    for (id, added) in &states.sets {
        let from_catalog = catalog.as_ref().and_then(|catalog| catalog.set(id));
        let current = from_catalog.and_then(|set| set.variant_for(&instance.mc_version, loader));
        // Plan with the catalog's variant when there is one, else with what is installed.
        let planned = current.or(added.applied.as_ref());
        let mut view = InstanceSetView {
            id: id.clone(),
            name: from_catalog.map_or_else(|| added.name.clone(), |set| set.name.clone()),
            name_en: from_catalog.and_then(|set| set.en.as_ref()?.name.clone()),
            summary: from_catalog.and_then(|set| set.summary.clone()),
            summary_en: from_catalog.and_then(|set| set.en.as_ref()?.summary.clone()),
            purpose: from_catalog.map_or(added.purpose, |set| set.purpose).into(),
            installed: added.installed(),
            available: current.is_some(),
            update_available: current.is_some_and(|variant| added.has_update(variant)),
            retired: catalog.is_some() && from_catalog.is_none(),
            items: Vec::new(),
            present: Vec::new(),
            pending: false,
            issues: Vec::new(),
            size_bytes: 0,
        };
        if let Some(variant) = planned.filter(|_| added.installed()) {
            let set = ModSet {
                id: id.clone(),
                name: view.name.clone(),
                summary: None,
                purpose: added.purpose,
                en: None,
                variants: Vec::new(),
                min_launcher_version: None,
            };
            // The installed content, so the view shows what is there (the update is offered).
            let installed = added.applied.as_ref().unwrap_or(variant);
            let provided = provided_for(&state, &instance, &game_dir, &states, id, installed).await;
            match plan_set(&game_dir, &set, installed, Some(added), None, &provided).await {
                Ok(mut plan) => {
                    keep_shared_files(&mut plan, &states, id);
                    let manifest = installed.manifest(id, &view.name, &provided);
                    view.items = item_views(&manifest, &plan.items);
                    view.present = present_items(installed, &plan);
                    view.pending = plan.changes_files();
                    view.issues = issue_views(&plan, installed);
                }
                Err(error) => tracing::warn!(%error, set = %id, "could not plan a mod set"),
            }
        }
        view.size_bytes = owned_size(&game_dir, added).await;
        sets.push(view);
    }
    Ok(InstanceSetsView {
        instance_id: instance.id.clone(),
        mc_version: instance.mc_version.clone(),
        loader: instance.loader.kind,
        sets,
        game_running: game_running(&state, &instance_id),
        unavailable: !states.sets.is_empty() && catalog.is_none(),
    })
}

/// Bytes of the files a set recorded that are still on disk.
async fn owned_size(game_dir: &Path, added: &AddedSet) -> u64 {
    let mut total = 0u64;
    for (path, record) in &added.state.files {
        let mut file = game_dir.join(path);
        if record.disabled {
            file.as_mut_os_string().push(".disabled");
        }
        if let Ok(meta) = tokio::fs::metadata(&file).await {
            total = total.saturating_add(meta.len());
        }
    }
    total
}

/// The set and the variant an instance would install, plus the planning inputs.
struct Planned {
    set: ModSet,
    variant: ModSetVariant,
    plan: SyncPlan,
}

async fn plan_for(
    app: &AppHandle,
    state: &AppState,
    instance: &Instance,
    game_dir: &Path,
    set_id: &str,
    selection: Option<&PackSelection>,
) -> CmdResult<Planned> {
    let catalog = catalog(app, state).await.ok_or_else(not_available)?;
    let set = catalog
        .set(set_id)
        .cloned()
        .ok_or_else(|| set_missing(set_id))?;
    let variant = set
        .variant_for(&instance.mc_version, pack_loader(instance.loader.kind))
        .cloned()
        .ok_or_else(|| no_variant(&set, instance))?;
    let states = ModSetStates::load(game_dir).await.map_err(pack_error)?;
    let provided = provided_for(state, instance, game_dir, &states, set_id, &variant).await;
    let mut plan = plan_set(
        game_dir,
        &set,
        &variant,
        states.sets.get(set_id),
        selection,
        &provided,
    )
    .await
    .map_err(pack_error)?;
    keep_shared_files(&mut plan, &states, set_id);
    Ok(Planned { set, variant, plan })
}

/// What adding a set, applying choices or updating it would do (nothing changes on disk).
#[tauri::command]
#[specta::specta]
pub async fn mod_set_preview(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    set_id: String,
    selection: Option<ModSelectionInput>,
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
    let selection: Option<PackSelection> = selection.map(Into::into);
    let Planned { set, variant, plan } = plan_for(
        &app,
        &state,
        &instance,
        &game_dir,
        &set_id,
        selection.as_ref(),
    )
    .await?;
    let states = ModSetStates::load(&game_dir).await.map_err(pack_error)?;
    let provided = provided_for(&state, &instance, &game_dir, &states, &set_id, &variant).await;
    let manifest = variant.manifest(&set.id, &set.name, &provided);
    Ok(ModCenterPreviewView {
        items: item_views(&manifest, &plan.items),
        summary: plan.summary(),
        changes_files: plan.changes_files(),
        records_choices: plan.state_changed,
    })
}

/// Adds a set to an instance, installs it for the instance's version, applies the player's
/// choices or brings the catalog's newer content (only what the player chose changes; new
/// items stay off). Refused while the instance's game runs.
#[tauri::command]
#[specta::specta]
pub async fn mod_set_apply(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    set_id: String,
    selection: Option<ModSelectionInput>,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<SetApplyView> {
    refuse_while_running(&state, &instance_id)?;
    let job = start_job(&app, &job_id);
    let _ = on_progress.send(JobProgressView::stage("resolving"));
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    ensure_switch_finished(&state, &mut instance).await?;
    install_set(
        &app,
        &state,
        &instance,
        &set_id,
        selection.map(Into::into),
        Some(on_progress),
        &job.token,
    )
    .await
}

/// Plans and applies a set for an instance (the caller holds the instance lock).
async fn install_set(
    app: &AppHandle,
    state: &AppState,
    instance: &Instance,
    set_id: &str,
    selection: Option<PackSelection>,
    on_progress: Option<Channel<JobProgressView>>,
    cancel: &CancellationToken,
) -> CmdResult<SetApplyView> {
    refuse_while_running(state, &instance.id)?;
    let game_dir = state
        .instances
        .game_dir(&instance.id)
        .map_err(CmdError::from)?;
    let Planned { set, variant, plan } =
        plan_for(app, state, instance, &game_dir, set_id, selection.as_ref()).await?;
    let downloads = plan
        .downloads
        .iter()
        .map(|download| download.size)
        .fold(0, u64::saturating_add);
    pl_mc::install::space::ensure_room(&game_dir, downloads).map_err(CmdError::from)?;
    let report = apply_set(
        &plan,
        set_id,
        SetChange::Install {
            name: set.name.clone(),
            purpose: set.purpose,
            variant,
        },
        &state.mc.downloader,
        &sync_options(cancel),
        on_progress.map(sync_progress),
    )
    .await
    .map_err(apply_error)?;
    settle_packs(&game_dir, &report).await;
    tracing::info!(
        instance = %instance.id,
        set = %set_id,
        summary = %plan.summary().describe_tr(),
        "mod set applied"
    );
    Ok(apply_view(&plan, &report))
}

fn apply_error(error: pl_content::Error) -> CmdError {
    // A disk that fills up on the way says so (and offers "Yer aç").
    let details = error.to_string();
    if pl_net::mentions_disk_full(&details) {
        CmdError::disk_full(details)
    } else {
        pack_error(error)
    }
}

fn apply_view(plan: &SyncPlan, report: &SetApplyReport) -> SetApplyView {
    let count = |n: usize| u32::try_from(n).unwrap_or(u32::MAX);
    SetApplyView {
        downloaded: count(report.sync.downloaded),
        restored: count(report.sync.restored),
        removed: count(
            plan.deletes
                .iter()
                .filter(|delete| !delete.replaced)
                .count()
                .min(report.sync.deleted),
        ),
        issues: count(report.sync.issues.len()),
    }
}

/// After a set changed files: packs it removed leave the game's list, packs it installed are
/// turned on (now, or at the first launch when there is no `options.txt` yet), and the first
/// shader pack it brought is chosen in Iris when none is.
async fn settle_packs(game_dir: &Path, report: &SetApplyReport) {
    if !report.removed_resource_packs.is_empty()
        && let Err(error) =
            pl_mc::instance::disable_resource_packs(game_dir, &report.removed_resource_packs).await
    {
        tracing::warn!(%error, "could not take removed packs out of options.txt");
    }
    activate_pending_packs(game_dir).await;
    if let Some(shader) = report.new_shader_packs.first() {
        match pl_mc::instance::choose_shader_once(game_dir, shader).await {
            Ok(true) => tracing::info!("chose a set's shader pack in Iris"),
            Ok(false) => {}
            Err(error) => tracing::warn!(%error, "could not choose a set's shader pack"),
        }
    }
}

/// Whether a ready-made set is added to the game directory. Such an instance gets the official
/// pack's key table like the pack's own instances (a new `options.txt`, the clash check): the
/// sets bring the pack's WI Zoom and Gamma Utils, whose own keys clash with the game's.
pub(crate) async fn has_sets(game_dir: &Path) -> bool {
    ModSetStates::load(game_dir)
        .await
        .is_ok_and(|states| !states.sets.is_empty())
}

/// Turns on the resource packs sets installed that wait for `options.txt` (each once). Called
/// after a set changes files and right before a launch, after the first `options.txt` is
/// written. Never fails a launch.
pub(crate) async fn activate_pending_packs(game_dir: &Path) {
    let Ok(mut states) = ModSetStates::load(game_dir).await else {
        return;
    };
    let pending = states.pending_resource_packs();
    if pending.is_empty() {
        return;
    }
    match pl_mc::instance::enable_resource_packs(game_dir, &pending).await {
        Ok(PackListChange::Done | PackListChange::Unreadable) => {
            states.mark_resource_packs_done(&pending);
            if let Err(error) = states.save(game_dir).await {
                tracing::warn!(%error, "could not record the turned-on resource packs");
            }
        }
        Ok(PackListChange::NoOptionsFile) => {}
        Err(error) => tracing::warn!(%error, "could not turn on a set's resource packs"),
    }
}

/// Removes a set from an instance: its files go while unchanged (files the player changed stay
/// as their own). Refused while the instance's game runs.
#[tauri::command]
#[specta::specta]
pub async fn mod_set_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    set_id: String,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<SetApplyView> {
    refuse_while_running(&state, &instance_id)?;
    let job = start_job(&app, &job_id);
    let _ = on_progress.send(JobProgressView::stage("removing"));
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };
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
    let states = ModSetStates::load(&game_dir).await.map_err(pack_error)?;
    let Some(added) = states.sets.get(&set_id) else {
        return Ok(SetApplyView {
            downloaded: 0,
            restored: 0,
            removed: 0,
            issues: 0,
        });
    };
    let mut plan = plan_set_removal(&game_dir, &set_id, added)
        .await
        .map_err(pack_error)?;
    keep_shared_files(&mut plan, &states, &set_id);
    let report = apply_set(
        &plan,
        &set_id,
        SetChange::Remove,
        &state.mc.downloader,
        &sync_options(&job.token),
        Some(sync_progress(on_progress)),
    )
    .await
    .map_err(apply_error)?;
    settle_packs(&game_dir, &report).await;
    // Another set may have left a mod out because this one brought it: it gets its own now.
    let remaining: Vec<String> = ModSetStates::load(&game_dir)
        .await
        .map(|states| {
            states
                .sets
                .iter()
                .filter(|(_, set)| set.installed())
                .map(|(id, _)| id.clone())
                .collect()
        })
        .unwrap_or_default();
    for other in remaining {
        match plan_for(&app, &state, &instance, &game_dir, &other, None).await {
            Ok(Planned { set, variant, plan }) if plan.changes_files() => {
                let result = apply_set(
                    &plan,
                    &other,
                    SetChange::Install {
                        name: set.name.clone(),
                        purpose: set.purpose,
                        variant,
                    },
                    &state.mc.downloader,
                    &sync_options(&job.token),
                    None,
                )
                .await;
                match result {
                    Ok(report) => settle_packs(&game_dir, &report).await,
                    Err(error) => {
                        tracing::warn!(%error, set = %other, "could not complete a remaining set");
                    }
                }
            }
            Ok(_) => {}
            Err(error) => tracing::debug!(?error, set = %other, "remaining set not re-planned"),
        }
    }
    tracing::info!(instance = %instance_id, set = %set_id, "mod set removed");
    Ok(apply_view(&plan, &report))
}

/// The player looked at a set's items: new ones are no longer flagged (no files change).
#[tauri::command]
#[specta::specta]
pub async fn mod_set_mark_seen(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    set_id: String,
) -> CmdResult<()> {
    let lock = instance_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let mut states = ModSetStates::load(&game_dir).await.map_err(pack_error)?;
    let Some(added) = states.sets.get_mut(&set_id) else {
        return Ok(());
    };
    let Some(variant) = added.applied.clone() else {
        return Ok(());
    };
    let manifest = variant.manifest(&set_id, &added.name, &BTreeSet::new());
    let before = added.state.clone();
    added.state.mark_items_seen(&manifest);
    if added.state != before {
        states.save(&game_dir).await.map_err(pack_error)?;
    }
    Ok(())
}

/// A new instance started from a set: the variant's Minecraft version and loader, then the set
/// installed into it. A failed install leaves no half-made instance behind.
#[tauri::command]
#[specta::specta]
pub async fn instance_create_from_set(
    app: AppHandle,
    state: State<'_, AppState>,
    request: SetInstanceRequest,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<InstanceView> {
    let job = start_job(&app, &job_id);
    let _ = on_progress.send(JobProgressView::stage("resolving"));
    let catalog = catalog(&app, &state).await.ok_or_else(not_available)?;
    let set = catalog
        .set(&request.set_id)
        .cloned()
        .ok_or_else(|| set_missing(&request.set_id))?;
    let variant = set
        .variants
        .iter()
        .find(|variant| variant.minecraft() == request.minecraft.trim())
        .cloned()
        .ok_or_else(|| {
            CmdError::new(
                "SET-SURUM",
                pick(
                    "Bu set, seçilen sürüm için hazır değil.",
                    "This set isn't ready for the chosen version.",
                ),
                format!("mod set {} has no {} variant", set.id, request.minecraft),
            )
        })?;
    // Nothing starts that cannot fit: the set's default download with the launch's margin.
    pl_mc::install::space::ensure_room(state.instances.root(), variant.default_download_size())
        .map_err(CmdError::from)?;
    let created = create_instance(
        &state,
        NewInstanceRequest {
            name: request.name.clone(),
            icon: request.icon.clone(),
            mc_version: variant.minecraft().to_owned(),
            loader: loader_kind(variant.loader()),
            loader_version: variant.loader_version().map(str::to_owned),
            max_memory_mb: request.max_memory_mb,
            jvm_args: request.jvm_args.clone(),
        },
    )
    .await?;
    let lock = instance_lock(&app, &created.id);
    let _guard = lock.lock().await;
    let outcome = install_set(
        &app,
        &state,
        &created,
        &set.id,
        request.selection.map(Into::into),
        Some(on_progress),
        &job.token,
    )
    .await;
    match outcome {
        Ok(_) => Ok(view(&state, &created)),
        Err(error) => {
            // Never leave a half-made instance behind.
            if let Ok(dir) = state.instances.instance_dir(&created.id) {
                let _ = tokio::fs::remove_dir_all(&dir).await;
            }
            Err(error)
        }
    }
}

/// The set a file of an instance belongs to (`folder/name`, `.disabled` ignored), for badges.
pub(crate) fn set_of_file<'a>(
    states: &'a ModSetStates,
    folder: &str,
    file_name: &str,
) -> Option<(&'a str, &'a AddedSet)> {
    states.owner(&index_key(folder, file_name))
}

/// The badge of a file that came from a set.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SetBadgeView {
    pub id: String,
    pub name: String,
    pub purpose: SetPurpose,
}

impl SetBadgeView {
    pub(crate) fn of(id: &str, set: &AddedSet) -> Self {
        Self {
            id: id.to_owned(),
            name: set.name.clone(),
            purpose: set.purpose.into(),
        }
    }
}

/// The title a set gives one of its files (`folder/name`), when it has one.
pub(crate) fn set_item_title(set: &AddedSet, folder: &str, file_name: &str) -> Option<String> {
    let key = index_key(folder, file_name);
    let item = set.state.files.get(&key)?.item.as_deref()?;
    set.applied
        .as_ref()?
        .items
        .iter()
        .find(|candidate| candidate.id == item)
        .map(|candidate| candidate.title.clone())
}
