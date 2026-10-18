//! Modrinth browser and add-on management for instances: search, project pages, installs with
//! dependencies, update checks, removal and adding files by hand. Long jobs stream progress over
//! a channel and can be cancelled with `content_cancel`.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;
use pl_lang::pick;
use pl_mc::addons::{
    ADDON_INDEX_FILE, AddonEntry, AddonInstallOptions, AddonInstallReport, AddonProgress, Addons,
    cached_icon, index_key,
};
use pl_mc::gamefiles::ImportKind;
use pl_mc::instance::Instance;
use pl_mc::loaders::LoaderKind;
use pl_mc::modrinth::{
    ModrinthClient, ModrinthProject, ModrinthProjectType, ModrinthResolveTarget,
    ModrinthSearchQuery, ModrinthSearchResults, ModrinthSortIndex, ModrinthVersion,
    compatible_loaders,
};
use pl_net::{CancellationToken, NetError};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, Manager, State};

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

// ---------------------------------------------------------------------------------------------
// Jobs, errors and shared helpers (also used by `commands::modpack`)

/// Cancellation tokens of running content jobs, and one lock per instance: jobs on the same
/// instance run one after another so its add-on index is never written concurrently.
#[derive(Default)]
pub struct ContentJobs {
    tokens: Mutex<HashMap<String, CancellationToken>>,
    locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

/// The job registry, created on first use (no setup code needed).
fn content_jobs(app: &AppHandle) -> tauri::State<'_, ContentJobs> {
    if app.try_state::<ContentJobs>().is_none() {
        app.manage(ContentJobs::default());
    }
    app.state::<ContentJobs>()
}

/// A registered job. Its token is unregistered on drop.
pub(crate) struct JobGuard {
    app: AppHandle,
    id: String,
    pub token: CancellationToken,
}

impl Drop for JobGuard {
    fn drop(&mut self) {
        content_jobs(&self.app).tokens.lock().remove(&self.id);
    }
}

pub(crate) fn start_job(app: &AppHandle, job_id: &str) -> JobGuard {
    let token = CancellationToken::new();
    if let Some(previous) = content_jobs(app)
        .tokens
        .lock()
        .insert(job_id.to_owned(), token.clone())
    {
        previous.cancel();
    }
    JobGuard {
        app: app.clone(),
        id: job_id.to_owned(),
        token,
    }
}

/// Whether a content job runs: Modrinth installs and updates, modpack imports and installs,
/// version switches, mod repairs (a data folder move or cleanup waits for them).
pub(crate) fn jobs_running(app: &AppHandle) -> bool {
    !content_jobs(app).tokens.lock().is_empty()
}

pub(crate) fn instance_lock(app: &AppHandle, instance_id: &str) -> Arc<tokio::sync::Mutex<()>> {
    content_jobs(app)
        .locks
        .lock()
        .entry(instance_id.to_owned())
        .or_default()
        .clone()
}

/// Progress of a content or modpack job.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JobProgressView {
    /// resolving | downloading | finishing (add-ons);
    /// pack | extracting | downloading | finishing (imports); identifying | writing (exports)
    pub stage: String,
    /// 0–100; `None` while the amount of work is unknown.
    pub percent: Option<f64>,
    pub done_files: u64,
    pub total_files: u64,
    pub bytes_per_sec: u64,
}

impl JobProgressView {
    pub(crate) fn stage(stage: &str) -> Self {
        Self {
            stage: stage.to_owned(),
            percent: None,
            done_files: 0,
            total_files: 0,
            bytes_per_sec: 0,
        }
    }

    pub(crate) fn download(stage: &str, p: &pl_net::DownloadProgress) -> Self {
        Self {
            stage: stage.to_owned(),
            percent: percent(
                p.done_bytes,
                p.total_bytes,
                p.done_files as u64,
                p.total_files as u64,
            ),
            done_files: p.done_files as u64,
            total_files: p.total_files as u64,
            bytes_per_sec: p.bytes_per_sec,
        }
    }
}

pub(crate) fn percent(
    done_bytes: u64,
    total_bytes: u64,
    done_files: u64,
    total_files: u64,
) -> Option<f64> {
    if total_bytes > 0 {
        Some((done_bytes as f64 / total_bytes as f64 * 100.0).clamp(0.0, 100.0))
    } else if total_files > 0 {
        Some((done_files as f64 / total_files as f64 * 100.0).clamp(0.0, 100.0))
    } else {
        None
    }
}

/// Friendly errors for Modrinth and add-on operations (the UI shows only the message).
pub(crate) fn content_error(error: pl_mc::Error) -> CmdError {
    use pl_mc::Error as E;
    let details = error.to_string();
    let (code, message) = match &error {
        _ if error.is_cancelled() => ("IPTAL", pick("İşlem iptal edildi.", "Cancelled.")),
        // The disk: before anything was downloaded, or filled up on the way ("Yer aç").
        E::NoRoom(short) => return CmdError::no_room(short.needed, short.free, details),
        _ if error.is_disk_full() => return CmdError::disk_full(details),
        _ if error.is_offline() => (
            "MODRINTH-AG",
            pick(
                "Modrinth'e ulaşılamadı. İnternet bağlantını kontrol edip tekrar dene.",
                "Couldn't reach Modrinth. Check your internet connection and try again.",
            ),
        ),
        E::Net(NetError::Status { status: 429, .. }) => (
            "MODRINTH-YOGUN",
            pick(
                "Modrinth şu an çok yoğun. Biraz bekleyip tekrar dene.",
                "Modrinth is very busy right now. Wait a bit and try again.",
            ),
        ),
        E::Net(NetError::Status { status, .. }) if *status >= 500 => (
            "MODRINTH-SUNUCU",
            pick(
                "Modrinth şu an yanıt vermiyor. Biraz sonra tekrar dene.",
                "Modrinth isn't answering right now. Try again in a bit.",
            ),
        ),
        E::Net(NetError::Checksum { .. } | NetError::Size { .. }) => (
            "DOGRULAMA",
            pick(
                "İndirilen dosya doğrulanamadı. Tekrar dene.",
                "The downloaded file couldn't be checked. Try again.",
            ),
        ),
        E::Net(NetError::Batch { first, .. })
            if first.contains("checksum") || first.contains("size mismatch") =>
        {
            (
                "DOGRULAMA",
                pick(
                    "İndirilen dosyalar doğrulanamadı. Tekrar dene.",
                    "The downloaded files couldn't be checked. Try again.",
                ),
            )
        }
        E::Net(NetError::Batch { .. }) => (
            "INDIRME",
            pick(
                "Bazı dosyalar indirilemedi. Bağlantını kontrol edip tekrar dene.",
                "Some files couldn't be downloaded. Check your connection and try again.",
            ),
        ),
        E::Net(_) => (
            "MODRINTH",
            pick(
                "Modrinth ile bağlantıda bir sorun oldu. Tekrar dene.",
                "Something went wrong talking to Modrinth. Try again.",
            ),
        ),
        E::NotFound(_) => (
            "BULUNAMADI",
            pick(
                "Bu içerik Modrinth'te bulunamadı.",
                "This wasn't found on Modrinth.",
            ),
        ),
        E::Io { .. } | E::TrashUnavailable { .. } => (
            "DISK",
            pick(
                "Dosyalar kaydedilemedi. Diskte yer olduğundan ve antivirüsün engellemediğinden emin ol.",
                "Couldn't save the files. Make sure there's space on the disk and your antivirus isn't blocking it.",
            ),
        ),
        E::AlreadyExists(_) => ("VAR", "Bu adda bir dosya zaten var."),
        _ => (
            "ICERIK",
            pick(
                "İçerik işlenirken bir sorun oluştu. Tekrar dene.",
                "Something went wrong with this content. Try again.",
            ),
        ),
    };
    CmdError::new(code, message, details)
}

pub(crate) fn modrinth_client(state: &AppState) -> ModrinthClient {
    ModrinthClient::new(state.http.clone())
}

/// Where cached Modrinth project icons live (inside the asset-protocol scope).
pub(crate) fn icon_dir(state: &AppState) -> PathBuf {
    state.root.join("cache").join("modrinth").join("icons")
}

/// The add-on index of an instance lives with the launcher's other bookkeeping.
pub(crate) fn addon_index_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join(".launcher").join(ADDON_INDEX_FILE)
}

pub(crate) fn addons_for(state: &AppState, instance_id: &str) -> CmdResult<Addons> {
    let instance_dir = state
        .instances
        .instance_dir(instance_id)
        .map_err(CmdError::from)?;
    let game_dir = state
        .instances
        .game_dir(instance_id)
        .map_err(CmdError::from)?;
    Ok(Addons::new(game_dir, addon_index_path(&instance_dir)).with_icon_dir(icon_dir(state)))
}

/// Relative paths (`mods/x.jar`) the official pack installed into a game directory.
pub(crate) fn pack_owned(game_dir: &Path) -> HashSet<String> {
    let path = game_dir.join(".projects").join("pack-state.json");
    std::fs::read(path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<serde_json::Value>(&bytes).ok())
        .and_then(|v| {
            v.get("files")
                .and_then(|f| f.as_object())
                .map(|o| o.keys().cloned().collect())
        })
        .unwrap_or_default()
}

fn project_type_of(kind: ImportKind) -> ModrinthProjectType {
    match kind {
        ImportKind::Mod => ModrinthProjectType::Mod,
        ImportKind::ResourcePack => ModrinthProjectType::Resourcepack,
        ImportKind::ShaderPack => ModrinthProjectType::Shader,
    }
}

/// Versions that run on `instance`: its game version, and for mods its loader (plus the
/// loaders it can also run, see [`compatible_loaders`]).
fn target_for(instance: &Instance, kind: ImportKind) -> ModrinthResolveTarget {
    match kind {
        ImportKind::Mod => {
            ModrinthResolveTarget::for_loader(instance.loader.kind, instance.mc_version.clone())
        }
        _ => ModrinthResolveTarget {
            loaders: Vec::new(),
            game_versions: vec![instance.mc_version.clone()],
        },
    }
}

fn check_kind(instance: &Instance, kind: ImportKind) -> CmdResult<()> {
    if kind == ImportKind::Mod && instance.loader.kind == LoaderKind::Vanilla {
        return Err(CmdError::new(
            "VANILLA-MOD",
            pick(
                "Vanilla kurulumlara mod yüklenemez. Fabric, Quilt, Forge ya da NeoForge ile bir kurulum oluştur.",
                "Vanilla instances can't have mods. Make an instance with Fabric, Quilt, Forge or NeoForge.",
            ),
            "mods on a vanilla instance",
        ));
    }
    Ok(())
}

fn send(channel: &Channel<JobProgressView>, view: JobProgressView) {
    let _ = channel.send(view);
}

fn addon_progress(p: AddonProgress) -> JobProgressView {
    match p {
        AddonProgress::Resolving => JobProgressView::stage("resolving"),
        AddonProgress::Downloading {
            done_files,
            total_files,
            done_bytes,
            total_bytes,
            bytes_per_sec,
        } => JobProgressView {
            stage: "downloading".to_owned(),
            percent: percent(done_bytes, total_bytes, done_files, total_files),
            done_files,
            total_files,
            bytes_per_sec,
        },
        AddonProgress::Finishing => JobProgressView::stage("finishing"),
    }
}

// ---------------------------------------------------------------------------------------------
// Browsing

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentSearchRequest {
    pub project_type: ModrinthProjectType,
    pub query: String,
    pub sort: ModrinthSortIndex,
    pub category: Option<String>,
    pub offset: u32,
    /// 1–50.
    pub limit: u32,
    /// Show only what runs on this instance (its game version, and its loader for mods).
    pub instance_id: Option<String>,
}

/// Search Modrinth. With an instance, results are limited to what runs on it.
#[tauri::command]
#[specta::specta]
pub async fn modrinth_search(
    state: State<'_, AppState>,
    request: ContentSearchRequest,
) -> CmdResult<ModrinthSearchResults> {
    let mut query = ModrinthSearchQuery {
        query: Some(request.query),
        project_type: Some(request.project_type),
        categories: request
            .category
            .into_iter()
            .filter(|c| !c.is_empty())
            .collect(),
        index: request.sort,
        offset: request.offset,
        limit: request.limit.clamp(1, 50),
        ..ModrinthSearchQuery::default()
    };
    if let Some(id) = &request.instance_id {
        let instance = state.instances.get(id).await.map_err(CmdError::from)?;
        query.game_versions = vec![instance.mc_version.clone()];
        if request.project_type == ModrinthProjectType::Mod {
            query.loaders = compatible_loaders(instance.loader.kind, &instance.mc_version);
        }
    }
    modrinth_client(&state)
        .search(&query)
        .await
        .map_err(content_error)
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CategoryView {
    pub name: String,
    pub project_type: ModrinthProjectType,
    /// `categories`, `features`, `resolutions`, `performance impact`.
    pub header: String,
}

/// Search categories of every project type.
#[tauri::command]
#[specta::specta]
pub async fn modrinth_categories(state: State<'_, AppState>) -> CmdResult<Vec<CategoryView>> {
    let categories = modrinth_client(&state)
        .categories()
        .await
        .map_err(content_error)?;
    Ok(categories
        .into_iter()
        .filter(|c| c.project_type != ModrinthProjectType::Unknown)
        .map(|c| CategoryView {
            name: c.name,
            project_type: c.project_type,
            header: c.header,
        })
        .collect())
}

/// A project page (Markdown description, gallery, links).
#[tauri::command]
#[specta::specta]
pub async fn modrinth_project(
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<ModrinthProject> {
    modrinth_client(&state)
        .project(&id)
        .await
        .map_err(content_error)
}

/// Versions of a project, newest first. With an instance, only versions that run on it.
#[tauri::command]
#[specta::specta]
pub async fn modrinth_versions(
    state: State<'_, AppState>,
    project_id: String,
    instance_id: Option<String>,
    kind: Option<ImportKind>,
) -> CmdResult<Vec<ModrinthVersion>> {
    let target = match &instance_id {
        Some(id) => {
            let instance = state.instances.get(id).await.map_err(CmdError::from)?;
            target_for(&instance, kind.unwrap_or(ImportKind::Mod))
        }
        None => ModrinthResolveTarget::default(),
    };
    modrinth_client(&state)
        .project_versions(&project_id, &target.loaders, &target.game_versions)
        .await
        .map_err(content_error)
}

// ---------------------------------------------------------------------------------------------
// Installed add-ons

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentEntryView {
    #[serde(flatten)]
    pub entry: AddonEntry,
    /// Installed by the official pack: it is updated with the pack, not from here.
    pub from_pack: bool,
    /// Cached project icon (local path; the UI converts it with `convertFileSrc`).
    pub icon_path: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentScanView {
    pub entries: Vec<ContentEntryView>,
    /// Modrinth could not be reached; entries only carry what was known locally.
    pub offline: bool,
}

/// The add-ons of one kind in an instance and the Modrinth projects they belong to, optionally
/// with available updates. Offline it answers from the local index.
#[tauri::command]
#[specta::specta]
pub async fn content_scan(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    kind: ImportKind,
    check_updates: bool,
) -> CmdResult<ContentScanView> {
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let addons = addons_for(&state, &instance_id)?;
    let lock = instance_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    let client = modrinth_client(&state);
    let scan = if check_updates {
        addons
            .check_updates(&client, kind, &target_for(&instance, kind))
            .await
    } else {
        addons.identify(&client, kind).await
    }
    .map_err(content_error)?;

    let owned = pack_owned(addons.game_dir());
    let icons = icon_dir(&state);
    let entries = scan
        .entries
        .into_iter()
        .map(|mut entry| {
            let from_pack = owned.contains(&index_key(kind.folder(), &entry.file_name));
            if from_pack {
                entry.update = None;
            }
            let icon_path = entry
                .source
                .as_ref()
                .and_then(|s| cached_icon(&icons, &s.project_id))
                .map(|p| p.to_string_lossy().into_owned());
            ContentEntryView {
                entry,
                from_pack,
                icon_path,
            }
        })
        .collect();
    Ok(ContentScanView {
        entries,
        offline: scan.offline,
    })
}

/// Install a project into an instance: `version_id`, or else the best version for the instance.
/// Mods bring their required dependencies. An older version of the project is replaced.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn content_install(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    kind: ImportKind,
    project_id: String,
    version_id: Option<String>,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<AddonInstallReport> {
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    check_kind(&instance, kind)?;
    let addons = addons_for(&state, &instance_id)?;
    let job = start_job(&app, &job_id);
    send(&on_progress, JobProgressView::stage("resolving"));
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };

    let client = modrinth_client(&state);
    let target = target_for(&instance, kind);
    let project = client.project(&project_id).await.map_err(content_error)?;
    if project.project_type != project_type_of(kind) {
        return Err(CmdError::new(
            "TUR",
            pick(
                "Bu içerik bu bölüme yüklenemez.",
                "This can't go in this section.",
            ),
            format!("{:?} into {kind:?}", project.project_type),
        ));
    }
    let version = match version_id {
        Some(id) => {
            let version = client.version(&id).await.map_err(content_error)?;
            if version.project_id != project.id {
                return Err(CmdError::internal(format!(
                    "version {id} is not of {project_id}"
                )));
            }
            version
        }
        None => client
            .best_version(&project.id, &target)
            .await
            .map_err(content_error)?
            .ok_or_else(|| {
                CmdError::new(
                    "UYUMLU-SURUM-YOK",
                    pick(
                        "Bunun bu kurulumla uyumlu bir sürümü yok.",
                        "There's no version of this that works with this instance.",
                    ),
                    format!("{project_id} for {:?}", target),
                )
            })?,
    };

    let events = on_progress.clone();
    addons
        .install(
            &client,
            &state.mc.downloader,
            kind,
            vec![version],
            &target,
            AddonInstallOptions {
                with_dependencies: kind == ImportKind::Mod,
                concurrency: 6,
                cancel: job.token.clone(),
                progress: Some(Arc::new(move |p: AddonProgress| {
                    send(&events, addon_progress(p))
                })),
            },
        )
        .await
        .map_err(content_error)
}

/// Update add-ons to their newest compatible versions (`file_names` empty = every update).
/// Files of the official pack are left alone.
#[tauri::command]
#[specta::specta]
pub async fn content_update(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    kind: ImportKind,
    file_names: Vec<String>,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<AddonInstallReport> {
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    check_kind(&instance, kind)?;
    let addons = addons_for(&state, &instance_id)?;
    let job = start_job(&app, &job_id);
    send(&on_progress, JobProgressView::stage("resolving"));
    let lock = instance_lock(&app, &instance_id);
    let _guard = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        guard = lock.lock() => guard,
    };

    let client = modrinth_client(&state);
    let target = target_for(&instance, kind);
    let scan = addons
        .check_updates(&client, kind, &target)
        .await
        .map_err(content_error)?;
    if scan.offline {
        return Err(content_error(pl_mc::Error::Offline));
    }
    let owned = pack_owned(addons.game_dir());
    let wanted: HashSet<&str> = file_names.iter().map(String::as_str).collect();
    let mut ids: Vec<String> = scan
        .entries
        .iter()
        .filter(|e| wanted.is_empty() || wanted.contains(e.file_name.as_str()))
        .filter(|e| !owned.contains(&index_key(kind.folder(), &e.file_name)))
        .filter_map(|e| e.update.as_ref().map(|u| u.version_id.clone()))
        .collect();
    ids.sort();
    ids.dedup();
    if ids.is_empty() {
        return Ok(AddonInstallReport::default());
    }
    let roots = client.versions(&ids).await.map_err(content_error)?;
    let events = on_progress.clone();
    addons
        .install(
            &client,
            &state.mc.downloader,
            kind,
            roots,
            &target,
            AddonInstallOptions {
                with_dependencies: kind == ImportKind::Mod,
                concurrency: 6,
                cancel: job.token.clone(),
                progress: Some(Arc::new(move |p: AddonProgress| {
                    send(&events, addon_progress(p))
                })),
            },
        )
        .await
        .map_err(content_error)
}

/// Remove an add-on file (to the OS trash when possible).
#[tauri::command]
#[specta::specta]
pub async fn content_remove(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    kind: ImportKind,
    file_name: String,
) -> CmdResult<()> {
    let addons = addons_for(&state, &instance_id)?;
    if pack_owned(addons.game_dir()).contains(&index_key(kind.folder(), &file_name)) {
        return Err(CmdError::new(
            "PAKET-DOSYASI",
            pick(
                "Bu dosya resmi paketin parçası, paket güncellenince geri gelir. Kaldırmak yerine kapatabilirsin.",
                "This file is part of the official pack and comes back when the pack updates. You can turn it off instead.",
            ),
            file_name,
        ));
    }
    let lock = instance_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    addons
        .remove(kind, &file_name)
        .await
        .map(|_| ())
        .map_err(content_error)
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentAddView {
    pub added: Vec<String>,
    /// Files that were not added (wrong type or a file with that name exists).
    pub skipped: Vec<String>,
}

/// Copy files chosen by the player into the instance (`mods/`, `resourcepacks/`,
/// `shaderpacks/`). Existing files are never overwritten.
#[tauri::command]
#[specta::specta]
pub async fn content_add_files(
    state: State<'_, AppState>,
    instance_id: String,
    kind: ImportKind,
    paths: Vec<String>,
) -> CmdResult<ContentAddView> {
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let mut view = ContentAddView {
        added: Vec::new(),
        skipped: Vec::new(),
    };
    for path in paths {
        let path = PathBuf::from(path);
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default();
        match pl_mc::gamefiles::import_into(&game_dir, kind, &path).await {
            Ok(_) => view.added.push(name),
            Err(error @ pl_mc::Error::Io { .. }) => return Err(content_error(error)),
            Err(error) => {
                tracing::info!(%error, "file not added");
                view.skipped.push(name);
            }
        }
    }
    Ok(view)
}

/// Cancel a running content or modpack job.
#[tauri::command]
#[specta::specta]
pub fn content_cancel(app: AppHandle, job_id: String) {
    if let Some(token) = content_jobs(&app).tokens.lock().get(&job_id) {
        token.cancel();
    }
}
