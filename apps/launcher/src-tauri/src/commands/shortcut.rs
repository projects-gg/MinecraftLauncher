//! "Masaüstüne kısayol": a desktop shortcut that starts one instance through
//! `projects://play/<instance>` (built by `pl_desktop`). The picture is the instance's icon
//! tile, drawn by the UI and saved next to the launcher's other per-machine files.

use std::path::PathBuf;

use base64::Engine;
use pl_desktop::ShortcutSpec;
use serde::Serialize;
use tauri::State;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// Folder of the shortcut pictures. Outside the data folder, which can be moved, so shortcuts
/// keep their picture.
fn icon_dir() -> Option<PathBuf> {
    dirs::data_local_dir().map(|dir| dir.join("gg.projects.launcher").join("shortcuts"))
}

/// The program a shortcut starts. An AppImage runs from a temporary mount, so its own path is
/// used instead.
fn launcher_executable() -> std::io::Result<PathBuf> {
    #[cfg(target_os = "linux")]
    if let Some(appimage) = std::env::var_os("APPIMAGE").filter(|p| !p.is_empty()) {
        return Ok(PathBuf::from(appimage));
    }
    std::env::current_exe()
}

/// Save the picture the shortcut shows: an icon on Windows, a PNG on Linux (macOS shortcuts
/// show the system's link picture).
fn save_icon(instance_id: &str, png: &[u8]) -> Result<Option<PathBuf>, String> {
    if cfg!(target_os = "macos") {
        return Ok(None);
    }
    let dir = icon_dir().ok_or("no local data folder")?;
    std::fs::create_dir_all(&dir).map_err(|e| format!("{}: {e}", dir.display()))?;
    let (bytes, extension) = if cfg!(windows) {
        (
            pl_desktop::png_to_ico(png).map_err(|e| e.to_string())?,
            "ico",
        )
    } else {
        (png.to_vec(), "png")
    };
    let path = dir.join(format!("{instance_id}.{extension}"));
    std::fs::write(&path, bytes).map_err(|e| format!("{}: {e}", path.display()))?;
    Ok(Some(path))
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ShortcutView {
    /// File name of the shortcut on the desktop.
    pub file_name: String,
}

fn failed(details: impl Into<String>) -> CmdError {
    CmdError::new("KISAYOL", "Masaüstü kısayolu oluşturulamadı.", details)
}

/// Create (or refresh) the desktop shortcut of `instance_id`. `icon_png` is the instance's icon
/// tile as base64 PNG (at most 256×256); without it the launcher's own icon is used.
#[tauri::command]
#[specta::specta]
pub async fn instance_shortcut_create(
    state: State<'_, AppState>,
    instance_id: String,
    icon_png: Option<String>,
) -> CmdResult<ShortcutView> {
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let link = pl_desktop::play_link(&instance.id).ok_or_else(|| {
        failed(format!(
            "instance id {:?} cannot travel in a link",
            instance.id
        ))
    })?;
    let desktop = dirs::desktop_dir().ok_or_else(|| failed("no desktop folder"))?;
    let target = launcher_executable().map_err(|e| failed(e.to_string()))?;
    let icon = match icon_png {
        Some(encoded) => {
            let png = base64::engine::general_purpose::STANDARD
                .decode(encoded.trim())
                .map_err(|e| failed(format!("icon: {e}")))?;
            let id = instance.id.clone();
            tauri::async_runtime::spawn_blocking(move || save_icon(&id, &png))
                .await
                .map_err(CmdError::internal)?
                .unwrap_or_else(|error| {
                    // The shortcut still works with the launcher's own picture.
                    tracing::warn!(%error, "shortcut picture not saved");
                    None
                })
        }
        None => None,
    };
    let spec = ShortcutSpec {
        description: format!(
            "{} kurulumunu Projects Launcher ile başlatır.",
            instance.name
        ),
        name: instance.name.clone(),
        target,
        link,
        icon,
    };
    let path =
        tauri::async_runtime::spawn_blocking(move || pl_desktop::create_shortcut(&desktop, &spec))
            .await
            .map_err(CmdError::internal)?
            .map_err(|e| failed(e.to_string()))?;
    tracing::info!(instance = %instance_id, path = %path.display(), "desktop shortcut created");
    Ok(ShortcutView {
        file_name: path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .unwrap_or_default(),
    })
}
