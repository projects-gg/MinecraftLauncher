//! "Özel" in "Yeni Kurulum" (CUSTOM-VER, `features.customVersions`): the versions other installers
//! (OptiFine's, Forge's, Fabric's, a modpack's own) put in the data folder, and that folder.
//! The decisions live in `pl_mc::version::custom`; this module gathers what the launcher runs
//! itself (so its own loader profiles stay out of the list) and keeps installers happy with the
//! folder.

use std::collections::HashSet;
use std::io::Write;
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use pl_lang::pick;
use pl_mc::loaders::{LoaderKind, loader_profile_id};
use pl_mc::version::VersionManifest;
use pl_mc::version::custom::{self, CustomKind, CustomVersion};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// `features.customVersions` of the applied content (off until Studio turns it on).
static ENABLED: AtomicBool = AtomicBool::new(false);

/// Follows the signed config: off, nothing is listed and no such instance is made (instances made
/// while it was on still launch).
pub(crate) fn apply_flag(on: bool) {
    ENABLED.store(on, Ordering::SeqCst);
}

pub(crate) fn enabled() -> bool {
    ENABLED.load(Ordering::SeqCst)
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomVersionView {
    pub id: String,
    /// The Minecraft version it runs on.
    pub minecraft: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub kind: CustomKind,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomVersionsView {
    /// Newest first.
    pub versions: Vec<CustomVersionView>,
    /// The folder an installer is pointed at (the data folder, which holds `versions/`).
    pub folder: String,
}

/// Mojang's list (the cached copy when Mojang cannot be reached).
async fn manifest(state: &AppState) -> CmdResult<VersionManifest> {
    match pl_mc::version::load_manifest(&state.mc, false).await {
        Ok(manifest) => Ok(manifest),
        Err(error) if error.is_offline() => pl_mc::version::load_manifest(&state.mc, true)
            .await
            .map_err(CmdError::from),
        Err(error) => Err(error.into()),
    }
}

/// The loader profiles the launcher runs itself for instances and kept sets: theirs, not "Özel"
/// (an instance that launches another installer's version does not run one).
async fn launcher_ids(state: &AppState) -> HashSet<String> {
    let Ok((refs, _)) = crate::commands::cleanup::game_refs(state).await else {
        return HashSet::new();
    };
    refs.iter()
        .filter(|game| game.custom.is_none())
        .filter_map(|game| match (game.loader, game.loader_version.as_deref()) {
            (Some(kind), Some(version)) if kind != LoaderKind::Vanilla => {
                loader_profile_id(kind, &game.minecraft, version).ok()
            }
            _ => None,
        })
        .collect()
}

/// Forge's installer refuses a data folder without `launcher_profiles.json` and OptiFine's ends
/// with an error window: a minimal one is written when there is none (one that exists is never
/// touched).
fn ensure_profiles_file(root: &Path) {
    let path = root.join("launcher_profiles.json");
    if path.exists() {
        return;
    }
    let written = std::fs::create_dir_all(root).and_then(|()| {
        std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
            .and_then(|mut file| file.write_all(b"{\n  \"profiles\": {}\n}\n"))
    });
    match written {
        Ok(()) => tracing::info!("wrote a minimal launcher_profiles.json for other installers"),
        Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => {}
        Err(error) => tracing::warn!(%error, "could not write launcher_profiles.json"),
    }
}

fn view(version: CustomVersion) -> CustomVersionView {
    CustomVersionView {
        id: version.id,
        minecraft: version.minecraft,
        loader: version.loader,
        loader_version: version.loader_version,
        kind: version.kind,
    }
}

/// The versions other installers put in the data folder, newest first; empty while
/// `features.customVersions` is off.
#[tauri::command]
#[specta::specta]
pub async fn custom_versions(state: State<'_, AppState>) -> CmdResult<CustomVersionsView> {
    let root = state.mc.dirs.root().to_path_buf();
    let folder = root.display().to_string();
    if !enabled() {
        return Ok(CustomVersionsView {
            versions: Vec::new(),
            folder,
        });
    }
    let manifest = manifest(&state).await?;
    let launcher = launcher_ids(&state).await;
    let dirs = state.mc.dirs.clone();
    let versions = tokio::task::spawn_blocking(move || {
        ensure_profiles_file(&root);
        // The launcher's Fabric and Quilt profiles from before it marked them are its own.
        match custom::adopt_unmarked_profiles(&dirs) {
            Ok(0) => {}
            Ok(marked) => tracing::info!(marked, "marked the launcher's older loader profiles"),
            Err(error) => {
                tracing::warn!(%error, "could not mark the launcher's older loader profiles")
            }
        }
        custom::scan(&dirs, &manifest, &launcher)
    })
    .await
    .map_err(CmdError::internal)?;
    Ok(CustomVersionsView {
        versions: versions.into_iter().map(view).collect(),
        folder,
    })
}

/// Opens the data folder (the one installers ask for), with `launcher_profiles.json` in it.
#[tauri::command]
#[specta::specta]
pub async fn custom_versions_open_folder(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<()> {
    let root = state.mc.dirs.root().to_path_buf();
    if enabled() {
        ensure_profiles_file(&root);
    }
    app.opener()
        .open_path(root.to_string_lossy(), None::<&str>)
        .map_err(|error| {
            CmdError::new(
                "KLASOR-ACILMADI",
                pick(
                    "Klasör açılamadı. Dosya gezgininden elle açabilirsin.",
                    "Couldn't open the folder. You can open it yourself in your file manager.",
                ),
                error.to_string(),
            )
        })
}

/// SURUM-YOK for another installer's version that the data folder no longer has.
pub(crate) fn missing(id: &str) -> CmdError {
    CmdError::new(
        "SURUM-YOK",
        pick(
            "Bu özel sürüm veri klasöründe yok. Sürümü yeniden kur, klasör olarak veri klasörünü seç.",
            "This custom version isn't in the data folder anymore. Install it again and choose the data folder as its folder.",
        ),
        format!("custom version {id} not found"),
    )
}

/// BULUNAMADI for a file another installer put in place that no server offers (OptiFine's jar).
pub(crate) fn files_missing(details: impl Into<String>) -> CmdError {
    let mut error = CmdError::new(
        "BULUNAMADI",
        pick(
            "Bu özel sürümün bazı dosyaları eksik. Sürümü yeniden kur, klasör olarak veri klasörünü seç.",
            "Some files of this custom version are missing. Install it again and choose the data folder as its folder.",
        ),
        details,
    );
    error.usage = Some("BULUNAMADI.custom-version".to_owned());
    error
}

/// Another installer's version `id` for a new instance (its game version and loader): refused
/// while `features.customVersions` is off or when the data folder no longer has it.
pub(crate) async fn describe(state: &AppState, id: &str) -> CmdResult<CustomVersion> {
    if !enabled() {
        return Err(CmdError::new(
            "OZEL-KAPALI",
            pick(
                "Özel sürümler şu an kullanılamıyor.",
                "Custom versions aren't available right now.",
            ),
            "features.customVersions is off",
        ));
    }
    let manifest = manifest(state).await?;
    let dirs = state.mc.dirs.clone();
    let wanted = id.to_owned();
    tokio::task::spawn_blocking(move || custom::describe(&dirs, &wanted, &manifest))
        .await
        .map_err(CmdError::internal)?
        .ok_or_else(|| missing(id))
}
