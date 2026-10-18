//! Data packs of a world (`saves/<world>/datapacks/`): listing them, adding one from Modrinth
//! (the version made for the instance's Minecraft version) and removing one (to the system's
//! trash, like add-ons).

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use pl_mc::gamefiles::{self, ResourcePackInfo};
use pl_mc::modrinth::ModrinthResolveTarget;
use pl_net::{Checksum, DownloadItem, DownloadOptions, HashAlgo, ProgressFn, VerifyMode};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};

use crate::commands::modrinth::{JobProgressView, content_error, modrinth_client, start_job};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;
use pl_lang::pick;

/// One data pack of a world.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DatapackView {
    pub file_name: String,
    /// The file name without `.zip`.
    pub name: String,
    /// `pack.mcmeta` description as plain text.
    pub description: Option<String>,
    pub size_bytes: u64,
    pub folder: bool,
    /// `pack.png` as a data URL.
    pub icon: Option<String>,
}

fn view(pack: ResourcePackInfo) -> DatapackView {
    use base64::Engine;
    let name = pack
        .file_name
        .strip_suffix(".zip")
        .or_else(|| pack.file_name.strip_suffix(".ZIP"))
        .unwrap_or(&pack.file_name)
        .to_owned();
    DatapackView {
        name,
        description: pack.description,
        size_bytes: pack.size,
        folder: pack.is_dir,
        icon: pack.icon_png.map(|png| {
            format!(
                "data:image/png;base64,{}",
                base64::engine::general_purpose::STANDARD.encode(png)
            )
        }),
        file_name: pack.file_name,
    }
}

/// The world's data packs folder; the world must exist.
fn datapacks_dir(state: &AppState, instance_id: &str, world: &str) -> CmdResult<PathBuf> {
    let game_dir = state
        .instances
        .game_dir(instance_id)
        .map_err(CmdError::from)?;
    let missing = |details: String| {
        CmdError::new(
            "DUNYA-YOK",
            pick("Bu dünya bulunamadı.", "This world wasn't found."),
            details,
        )
    };
    let dir = gamefiles::datapacks_dir(&game_dir, world).map_err(|e| missing(e.to_string()))?;
    let world_dir = dir.parent().map(PathBuf::from).unwrap_or_default();
    if !world_dir.join("level.dat").is_file() {
        return Err(missing(format!("{world} has no level.dat")));
    }
    Ok(dir)
}

/// The data packs of a world, by name.
#[tauri::command]
#[specta::specta]
pub async fn datapacks_list(
    state: State<'_, AppState>,
    instance_id: String,
    world: String,
) -> CmdResult<Vec<DatapackView>> {
    let dir = datapacks_dir(&state, &instance_id, &world)?;
    let packs = gamefiles::list_datapacks(&dir).await.map_err(|e| {
        CmdError::new(
            "VERI-PAKETI-LISTE",
            pick("Veri paketleri okunamadı.", "Couldn't read the data packs."),
            e.to_string(),
        )
    })?;
    Ok(packs.into_iter().map(view).collect())
}

/// Adds a Modrinth data pack to a world: the newest version made for the instance's Minecraft
/// version. Stoppable with `content_cancel`.
#[tauri::command]
#[specta::specta]
pub async fn datapack_install(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    world: String,
    project_id: String,
    job_id: String,
    on_progress: Channel<JobProgressView>,
) -> CmdResult<DatapackView> {
    let job = start_job(&app, &job_id);
    let _ = on_progress.send(JobProgressView::stage("resolving"));
    let dir = datapacks_dir(&state, &instance_id, &world)?;
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let target = ModrinthResolveTarget {
        loaders: vec!["datapack".to_owned()],
        game_versions: vec![instance.mc_version.clone()],
    };
    let version = modrinth_client(&state)
        .best_version(&project_id, &target)
        .await
        .map_err(content_error)?
        .ok_or_else(|| {
            CmdError::new(
                "VERI-PAKETI-SURUM",
                pick(
                    "Bu veri paketinin bu Minecraft sürümüne uygun bir sürümü yok.",
                    "This data pack has no version for this Minecraft version.",
                ),
                format!("{project_id} for {}", instance.mc_version),
            )
        })?;
    let file = version
        .primary_file()
        .filter(|f| f.filename.to_ascii_lowercase().ends_with(".zip"))
        .ok_or_else(|| {
            CmdError::new(
                "VERI-PAKETI-DOSYA",
                pick(
                    "Bu veri paketinin indirilebilir bir dosyası yok.",
                    "This data pack has no file to download.",
                ),
                version.id.clone(),
            )
        })?;
    let name = pl_mc::paths::safe_name(&file.filename)
        .map_err(|e| {
            CmdError::new(
                "VERI-PAKETI-DOSYA",
                pick(
                    "Bu veri paketinin indirilebilir bir dosyası yok.",
                    "This data pack has no file to download.",
                ),
                e.to_string(),
            )
        })?
        .to_owned();
    let dest = dir.join(&name);
    let checksum = match (&file.hashes.sha512, &file.hashes.sha1) {
        (Some(h), _) if !h.is_empty() => Some(Checksum::new(HashAlgo::Sha512, h)),
        (_, Some(h)) if !h.is_empty() => Some(Checksum::new(HashAlgo::Sha1, h)),
        _ => None,
    };
    let events = on_progress.clone();
    let progress: ProgressFn = Arc::new(move |p: pl_net::DownloadProgress| {
        let _ = events.send(JobProgressView::download("downloading", &p));
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
                cancel: job.token.clone(),
                progress: Some(progress),
                progress_interval: Duration::from_millis(150),
            },
        )
        .await
        .map_err(|e| content_error(e.into()))?;
    tracing::info!(%project_id, file = %name, %world, "data pack added");
    let packs = gamefiles::list_datapacks(&dir).await.unwrap_or_default();
    Ok(packs
        .into_iter()
        .find(|p| p.file_name == name)
        .map(view)
        .unwrap_or(DatapackView {
            name: name.trim_end_matches(".zip").to_owned(),
            description: None,
            size_bytes: file.size,
            folder: false,
            icon: None,
            file_name: name,
        }))
}

/// Removes a data pack from a world (to the system's trash when there is one).
#[tauri::command]
#[specta::specta]
pub async fn datapack_remove(
    state: State<'_, AppState>,
    instance_id: String,
    world: String,
    file_name: String,
) -> CmdResult<()> {
    let dir = datapacks_dir(&state, &instance_id, &world)?;
    let name = pl_mc::paths::safe_name(&file_name).map_err(|e| {
        CmdError::new(
            "AD",
            pick("Bu dosya bulunamadı.", "This file wasn't found."),
            e.to_string(),
        )
    })?;
    gamefiles::remove_path(&dir.join(name), true)
        .await
        .map(|_| ())
        .map_err(|e| {
            CmdError::new(
                "VERI-PAKETI-SIL",
                pick(
                    "Veri paketi kaldırılamadı. Oyun açıksa kapatıp tekrar dene.",
                    "Couldn't remove the data pack. If the game is open, close it and try again.",
                ),
                e.to_string(),
            )
        })
}
