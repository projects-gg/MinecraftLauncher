//! Modpacks: preview and import pack files (`.mrpack`, Prism/MultiMC, CurseForge) as new
//! instances, install Modrinth modpacks, and export an instance as `.mrpack`. Progress streams
//! over a channel; `content_cancel` stops a job (a half-imported instance is removed).

use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::Duration;

use pl_mc::addons::AddonSource;
use pl_mc::instance::{Instance, InstanceIcon, InstanceLoader, MemorySettings, NewInstance};
use pl_mc::loaders::LoaderKind;
use pl_mc::modpack::{
    CurseForgeModLink, ImportPendingFiles, MODPACK_STATE_FILE, ModpackState, MrEnvSupport,
    MrpackExport, PackPreview, identify_hosted_files, mrpack_download_items, read_pack_contents,
};
use pl_mc::modrinth::{ModrinthHashAlgorithm, ModrinthProjectType, ModrinthResolveTarget};
use pl_net::{
    CancellationToken, Checksum, DownloadItem, DownloadOptions, HashAlgo, ProgressFn, VerifyMode,
};
use serde::{Deserialize, Serialize};
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::commands::instances::{InstanceView, read_marker, view};
use crate::commands::modrinth::{
    JobProgressView, addons_for, content_error, instance_lock, modrinth_client, start_job,
};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

fn send(channel: &Channel<JobProgressView>, view: JobProgressView) {
    let _ = channel.send(view);
}

/// Friendly errors for pack files.
fn pack_error(error: pl_mc::Error) -> CmdError {
    use pl_mc::Error as E;
    let details = error.to_string();
    match &error {
        _ if error.is_cancelled() || error.is_offline() => content_error(error),
        E::Unsupported(_) => CmdError::new(
            "PAKET-TANINMADI",
            "Bu dosya tanınan bir mod paketi değil. .mrpack, Prism/MultiMC ya da CurseForge zip dosyası seç.",
            details,
        ),
        E::Invalid(_) | E::Json { .. } | E::Archive { .. } => CmdError::new(
            "PAKET-BOZUK",
            "Mod paketi bozuk ya da eksik görünüyor.",
            details,
        ),
        _ => content_error(error),
    }
}

/// Where an instance keeps the files of its Modrinth modpack for updates.
pub(crate) fn modpack_state_path(instance_dir: &Path) -> PathBuf {
    instance_dir.join(".launcher").join(MODPACK_STATE_FILE)
}

/// The Modrinth modpack version an `.mrpack` file is (by its hash), when Modrinth knows it.
/// Best effort: offline or unknown packs give `None`.
async fn identify_pack_file(state: &AppState, archive: &Path) -> Option<AddonSource> {
    let sha1 = pl_net::hash_file(archive, HashAlgo::Sha1).await.ok()?;
    let client = modrinth_client(state);
    let version = match client
        .version_from_hash(&sha1, ModrinthHashAlgorithm::Sha1)
        .await
    {
        Ok(found) => found?,
        Err(error) => {
            tracing::debug!(%error, "could not identify the pack file on Modrinth");
            return None;
        }
    };
    let project = client.project(&version.project_id).await.ok()?;
    (project.project_type == ModrinthProjectType::Modpack).then_some(AddonSource {
        project_id: project.id,
        version_id: version.id,
        version_number: Some(version.version_number),
        title: Some(project.title),
        icon_url: project.icon_url,
    })
}

/// What a pack file would create (read without extracting it).
#[tauri::command]
#[specta::specta]
pub async fn modpack_preview(path: String) -> CmdResult<PackPreview> {
    pl_mc::modpack::preview_pack(Path::new(&path))
        .await
        .map_err(|error| match error {
            pl_mc::Error::Io { .. } => CmdError::new(
                "DOSYA-OKUNAMADI",
                "Dosya açılamadı. Dosyanın hâlâ yerinde olduğundan emin ol.",
                error.to_string(),
            ),
            error => pack_error(error),
        })
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModpackImportRequest {
    pub path: String,
    /// Instance name; the pack's own name when empty.
    pub name: Option<String>,
    /// Built-in icon key.
    pub icon: String,
    /// Optional files of an `.mrpack` (their `path`) the player leaves out.
    #[serde(default)]
    pub skip_optional: Vec<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModpackImportView {
    pub instance: InstanceView,
    /// CurseForge mods that have to be downloaded by hand (name and page), when the pack lists
    /// them.
    pub manual_mods: Vec<CurseForgeModLink>,
    pub manual_count: u64,
    /// Files written into the new instance (bundled and downloaded).
    pub files: u64,
    /// Archive entries left out because their paths were unsafe.
    pub skipped: u64,
    /// Parts of the source instance the launcher cannot reproduce.
    pub unsupported: Vec<String>,
}

/// Import a pack file as a new instance.
#[tauri::command]
#[specta::specta]
pub async fn modpack_import(
    app: AppHandle,
    state: State<'_, AppState>,
    request: ModpackImportRequest,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<ModpackImportView> {
    let job = start_job(&app, &job_id);
    import_archive(
        &app,
        &state,
        Path::new(&request.path),
        request.name,
        request.icon,
        None,
        &request.skip_optional,
        &job.token,
        &on_progress,
    )
    .await
}

/// Download a Modrinth modpack (`version_id`, else its newest release) and import it as a new
/// instance.
#[tauri::command]
#[specta::specta]
#[allow(clippy::too_many_arguments)]
pub async fn modpack_install(
    app: AppHandle,
    state: State<'_, AppState>,
    project_id: String,
    version_id: Option<String>,
    name: Option<String>,
    icon: String,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<ModpackImportView> {
    let job = start_job(&app, &job_id);
    send(&on_progress, JobProgressView::stage("pack"));
    let client = modrinth_client(&state);
    let project = client.project(&project_id).await.map_err(content_error)?;
    if project.project_type != ModrinthProjectType::Modpack {
        return Err(CmdError::new(
            "TUR",
            "Bu içerik bir mod paketi değil.",
            format!("{:?}", project.project_type),
        ));
    }
    let version = match version_id {
        Some(id) => client.version(&id).await.map_err(content_error)?,
        None => client
            .best_version(&project.id, &ModrinthResolveTarget::default())
            .await
            .map_err(content_error)?
            .ok_or_else(|| {
                CmdError::new(
                    "SURUM-YOK",
                    "Bu mod paketinin indirilebilir bir sürümü yok.",
                    project_id.clone(),
                )
            })?,
    };
    if version.project_id != project.id {
        return Err(CmdError::internal("the version belongs to another project"));
    }
    let dest = download_pack_version(&state, &version, &job.token, &on_progress, "pack").await?;

    let source = AddonSource {
        project_id: project.id.clone(),
        version_id: version.id.clone(),
        version_number: Some(version.version_number.clone()),
        title: Some(project.title.clone()),
        icon_url: project.icon_url.clone(),
    };
    let name = name
        .filter(|n| !n.trim().is_empty())
        .or_else(|| Some(project.title.clone()));
    let result = import_archive(
        &app,
        &state,
        &dest,
        name,
        icon,
        Some(source),
        &[],
        &job.token,
        &on_progress,
    )
    .await;
    let _ = tokio::fs::remove_file(&dest).await;
    result
}

/// Downloads the `.mrpack` of a Modrinth modpack version into `cache/modrinth/packs/` (kept
/// when it is already there and valid) and returns its path. Progress goes out as `stage`.
pub(crate) async fn download_pack_version(
    state: &AppState,
    version: &pl_mc::modrinth::ModrinthVersion,
    cancel: &CancellationToken,
    on_progress: &Channel<JobProgressView>,
    stage: &'static str,
) -> CmdResult<PathBuf> {
    let file = version
        .files
        .iter()
        .find(|f| f.primary && f.filename.ends_with(".mrpack"))
        .or_else(|| {
            version
                .files
                .iter()
                .find(|f| f.filename.ends_with(".mrpack"))
        })
        .ok_or_else(|| {
            CmdError::new(
                "PAKET-DOSYASI-YOK",
                "Bu sürümün mod paketi dosyası yok.",
                version.id.clone(),
            )
        })?;
    let cache = state.root.join("cache").join("modrinth").join("packs");
    let dest = cache.join(format!("{}.mrpack", version.id));
    let checksum = match (&file.hashes.sha512, &file.hashes.sha1) {
        (Some(h), _) if !h.is_empty() => Some(Checksum::new(HashAlgo::Sha512, h)),
        (_, Some(h)) if !h.is_empty() => Some(Checksum::new(HashAlgo::Sha1, h)),
        _ => None,
    };
    let events = on_progress.clone();
    let progress: ProgressFn = Arc::new(move |p: pl_net::DownloadProgress| {
        send(&events, JobProgressView::download(stage, &p))
    });
    state
        .mc
        .downloader
        .download(
            DownloadItem::new(file.url.clone(), &dest)
                .checksum(checksum)
                .size(Some(file.size)),
            &DownloadOptions {
                concurrency: 1,
                verify: VerifyMode::Full,
                cancel: cancel.clone(),
                progress: Some(progress),
                progress_interval: Duration::from_millis(150),
            },
        )
        .await
        .map_err(|e| content_error(e.into()))?;
    Ok(dest)
}

#[allow(clippy::too_many_arguments)]
async fn import_archive(
    app: &AppHandle,
    state: &AppState,
    archive: &Path,
    name: Option<String>,
    icon: String,
    modpack: Option<AddonSource>,
    skip_optional: &[String],
    cancel: &CancellationToken,
    progress: &Channel<JobProgressView>,
) -> CmdResult<ModpackImportView> {
    send(progress, JobProgressView::stage("extracting"));
    let preview = pl_mc::modpack::preview_pack(archive)
        .await
        .map_err(pack_error)?;
    let modded = preview.loader != LoaderKind::Vanilla;
    let name = name
        .map(|n| n.trim().to_owned())
        .filter(|n| !n.is_empty())
        .unwrap_or_else(|| preview.name.clone());
    let created = state
        .instances
        .create(NewInstance {
            name,
            mc_version: preview.mc_version.clone(),
            loader: InstanceLoader {
                kind: preview.loader,
                version: if modded {
                    preview.loader_version.clone()
                } else {
                    None
                },
            },
            icon: Some(InstanceIcon::Builtin { key: icon }),
            group: None,
            // "Otomatik" unless the pack brings its own memory settings (below).
            memory: None,
            managed_pack: None,
        })
        .await
        .map_err(CmdError::from)?;

    let lock = instance_lock(app, &created.id);
    let _guard = lock.lock().await;
    let outcome = fill_instance(
        state,
        &created,
        archive,
        modpack,
        skip_optional,
        cancel,
        progress,
    )
    .await;
    match outcome {
        Ok((mut instance, imported)) => {
            send(progress, JobProgressView::stage("finishing"));
            // The pack's own server list comes first; an empty one gets the Projects server.
            crate::commands::instances::seed_default_server_once(state, &mut instance).await;
            let (manual_mods, manual_count) = match &imported.pending {
                ImportPendingFiles::CurseForgeNeedsApiKey { files, links } => {
                    (links.clone(), files.len() as u64)
                }
                _ => (Vec::new(), 0),
            };
            let downloaded = match &imported.pending {
                ImportPendingFiles::Modrinth { files } => files.len() as u64,
                _ => 0,
            };
            Ok(ModpackImportView {
                instance: view(state, &instance),
                manual_mods,
                manual_count,
                files: imported.extracted_files + downloaded,
                skipped: imported.skipped_entries.len() as u64,
                unsupported: imported.unsupported,
            })
        }
        Err(error) => {
            // Never leave a half-imported instance behind.
            if let Ok(dir) = state.instances.instance_dir(&created.id) {
                let _ = tokio::fs::remove_dir_all(&dir).await;
            }
            Err(error)
        }
    }
}

/// Extract the pack into the new instance, download its hosted files (without the optional ones
/// the player left out) and apply its settings. An `.mrpack` known to Modrinth also gets its
/// update state (`.launcher/modpack.json`).
async fn fill_instance(
    state: &AppState,
    created: &Instance,
    archive: &Path,
    modpack: Option<AddonSource>,
    skip_optional: &[String],
    cancel: &CancellationToken,
    progress: &Channel<JobProgressView>,
) -> CmdResult<(Instance, pl_mc::modpack::ImportedInstance)> {
    let game_dir = state
        .instances
        .game_dir(&created.id)
        .map_err(CmdError::from)?;
    let mut imported = pl_mc::modpack::import_pack(archive, &game_dir, cancel.clone())
        .await
        .map_err(pack_error)?;

    // Optional files the player left out are not downloaded; their keys are remembered.
    let mut optional_off = std::collections::BTreeSet::new();
    if let ImportPendingFiles::Modrinth { files } = &mut imported.pending {
        files.retain(|file| {
            let skip = file.client_support() == MrEnvSupport::Optional
                && skip_optional.contains(&file.path);
            if skip {
                optional_off.insert(file.optional_key());
            }
            !skip
        });
    }

    let addons = addons_for(state, &created.id)?;
    if let ImportPendingFiles::Modrinth { files } = &imported.pending {
        let items = mrpack_download_items(&game_dir, files, true).map_err(pack_error)?;
        let events = progress.clone();
        let on_download: ProgressFn = Arc::new(move |p: pl_net::DownloadProgress| {
            send(&events, JobProgressView::download("downloading", &p))
        });
        state
            .mc
            .downloader
            .download_all(
                items,
                &DownloadOptions {
                    concurrency: 8,
                    verify: VerifyMode::Full,
                    cancel: cancel.clone(),
                    progress: Some(on_download),
                    progress_interval: Duration::from_millis(150),
                },
            )
            .await
            .map_err(|e| content_error(e.into()))?;
        if let Err(error) = addons.record_pack_files(files).await {
            tracing::warn!(%error, "could not record pack files");
        }
    }
    // A pack file from disk may be a Modrinth modpack too: then it can be updated like one.
    let modpack = match modpack {
        Some(source) => Some(source),
        None if imported.format == pl_mc::modpack::ModpackFormat::Mrpack => {
            identify_pack_file(state, archive).await
        }
        None => None,
    };
    if let Some(source) = &modpack
        && imported.format == pl_mc::modpack::ModpackFormat::Mrpack
    {
        let instance_dir = state
            .instances
            .instance_dir(&created.id)
            .map_err(CmdError::from)?;
        let (archive, source) = (archive.to_path_buf(), source.clone());
        let saved = tauri::async_runtime::spawn_blocking(move || {
            let contents = read_pack_contents(&archive, &optional_off)?;
            ModpackState::from_contents(
                &source.project_id,
                &source.version_id,
                source.version_number.clone(),
                &contents,
            )
            .save(&modpack_state_path(&instance_dir))
        })
        .await;
        if !matches!(saved, Ok(Ok(()))) {
            tracing::warn!("could not record the modpack's files for updates");
        }
    }
    if modpack.is_some()
        && let Err(error) = addons.set_modpack(modpack).await
    {
        tracing::warn!(%error, "could not record the modpack source");
    }

    let mut instance = created.clone();
    if let (Some(min), Some(max)) = (imported.min_memory_mb, imported.max_memory_mb) {
        instance.memory = pl_mc::instance::clamp_memory(
            &MemorySettings {
                min_mb: min,
                max_mb: max,
            },
            pl_mc::instance::system_memory_mb(),
        );
        instance.memory_auto = false;
    }
    if let Some(args) = &imported.jvm_args {
        instance.jvm_args = args.trim().to_owned();
    }
    if let Some(notes) = &imported.notes {
        instance.notes = notes.clone();
    } else if let Some(summary) = &imported.summary {
        instance.notes = summary.clone();
    }
    if let Some(played) = imported.total_played_secs {
        instance.play_time_secs = played;
    }
    if instance != *created {
        state
            .instances
            .update(&instance)
            .await
            .map_err(CmdError::from)?;
    }
    Ok((instance, imported))
}

#[derive(Debug, Clone, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModpackExportRequest {
    /// Destination file (`.mrpack` is added when missing).
    pub path: String,
    pub name: String,
    pub version: String,
    pub summary: Option<String>,
    /// Bundle the worlds (`saves/`).
    pub include_worlds: bool,
    /// Bundle mod settings (`config/`, `defaultconfigs/`, `kubejs/`).
    pub include_config: bool,
    /// Bundle game options and the server list.
    pub include_options: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModpackExportView {
    pub path: String,
    /// Files referenced by their Modrinth download (not bundled).
    pub hosted_files: u64,
    pub bundled_files: u64,
    pub bundled_bytes: u64,
    /// Modrinth was unreachable: every mod was bundled into the file.
    pub offline: bool,
}

/// Loader version for an export: the instance's, else what was installed, else the newest
/// stable one.
async fn export_loader_version(
    state: &AppState,
    instance: &Instance,
    instance_dir: &Path,
) -> CmdResult<Option<String>> {
    if instance.loader.kind == LoaderKind::Vanilla {
        return Ok(None);
    }
    if let Some(version) = instance.loader.version.clone().filter(|v| !v.is_empty()) {
        return Ok(Some(version));
    }
    if let Some(marker) = read_marker(instance_dir)
        && marker.loader == instance.loader.kind
        && let Some(version) = marker.loader_version
    {
        return Ok(Some(version));
    }
    let versions =
        pl_mc::loaders::list_loader_versions(&state.mc, instance.loader.kind, &instance.mc_version)
            .await
            .map_err(content_error)?;
    Ok(versions.into_iter().find(|v| v.stable).map(|v| v.version))
}

/// Files in `folder` of the game directory that Modrinth may host.
fn hosted_candidates(game_dir: &Path, folder: &str, extension: &str) -> Vec<String> {
    let Ok(entries) = std::fs::read_dir(game_dir.join(folder)) else {
        return Vec::new();
    };
    let mut out: Vec<String> = entries
        .filter_map(|e| e.ok())
        .filter(|e| e.file_type().is_ok_and(|t| t.is_file()))
        .filter_map(|e| e.file_name().to_str().map(str::to_owned))
        .filter(|name| name.to_ascii_lowercase().ends_with(extension))
        .map(|name| format!("{folder}/{name}"))
        .collect();
    out.sort();
    out
}

/// Export an instance as a Modrinth `.mrpack`. Mods, resource packs and shader packs Modrinth
/// hosts are referenced by download link; everything else chosen is bundled.
#[tauri::command]
#[specta::specta]
pub async fn modpack_export(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    request: ModpackExportRequest,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<ModpackExportView> {
    let job = start_job(&app, &job_id);
    send(&on_progress, JobProgressView::stage("identifying"));
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let instance_dir = state
        .instances
        .instance_dir(&instance_id)
        .map_err(CmdError::from)?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let loader_version = export_loader_version(&state, &instance, &instance_dir).await?;
    if instance.loader.kind != LoaderKind::Vanilla && loader_version.is_none() {
        return Err(CmdError::new(
            "YUKLEYICI-YOK",
            "Mod yükleyici sürümü bulunamadı. Kurulumu bir kez başlatıp tekrar dene.",
            format!("{:?} {}", instance.loader.kind, instance.mc_version),
        ));
    }

    let mut candidates = hosted_candidates(&game_dir, "mods", ".jar");
    candidates.extend(hosted_candidates(&game_dir, "resourcepacks", ".zip"));
    candidates.extend(hosted_candidates(&game_dir, "shaderpacks", ".zip"));
    let client = modrinth_client(&state);
    let identified = tokio::select! {
        _ = job.token.cancelled() => return Err(content_error(pl_mc::Error::Cancelled)),
        found = identify_hosted_files(&client, &game_dir, &candidates) => found,
    };
    let (hosted, offline) = match identified {
        Ok(found) => (found.files, false),
        Err(error) if error.is_offline() => (Vec::new(), true),
        Err(error) => return Err(content_error(error)),
    };

    let mut overrides: Vec<String> = ["mods", "resourcepacks", "shaderpacks"]
        .iter()
        .map(|s| (*s).to_owned())
        .collect();
    if request.include_config {
        overrides.extend(["config", "defaultconfigs", "kubejs"].map(str::to_owned));
    }
    if request.include_options {
        overrides.extend(
            [
                "options.txt",
                "optionsof.txt",
                "optionsshaders.txt",
                "servers.dat",
            ]
            .map(str::to_owned),
        );
    }
    if request.include_worlds {
        overrides.push("saves".to_owned());
    }
    let overrides: Vec<String> = overrides
        .into_iter()
        .filter(|rel| game_dir.join(rel).exists())
        .collect();

    let mut dest = PathBuf::from(&request.path);
    if dest
        .extension()
        .is_none_or(|ext| !ext.eq_ignore_ascii_case("mrpack"))
    {
        let mut name = dest.file_name().unwrap_or_default().to_os_string();
        name.push(".mrpack");
        dest.set_file_name(name);
    }
    let name = request.name.trim();
    let export = MrpackExport {
        name: if name.is_empty() {
            instance.name.clone()
        } else {
            name.to_owned()
        },
        version_id: Some(request.version.trim().to_owned())
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| "1.0.0".to_owned()),
        summary: request.summary.filter(|s| !s.trim().is_empty()),
        mc_version: instance.mc_version.clone(),
        loader: instance.loader.kind,
        loader_version,
        files: hosted,
        overrides,
    };
    send(&on_progress, JobProgressView::stage("writing"));
    let lock = instance_lock(&app, &instance_id);
    let _guard = lock.lock().await;
    let summary = pl_mc::modpack::write_mrpack(&game_dir, &export, &dest, job.token.clone())
        .await
        .map_err(content_error)?;
    Ok(ModpackExportView {
        path: summary.path.to_string_lossy().into_owned(),
        hosted_files: summary.index_files,
        bundled_files: summary.override_files,
        bundled_bytes: summary.override_bytes,
        offline,
    })
}
