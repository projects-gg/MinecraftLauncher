//! Files dropped onto the launcher window: what each one is (shown while it is still being
//! dragged) and copying it where it belongs in an instance: mods into the active set's `mods/`,
//! resource packs into `resourcepacks/`, shader packs into `shaderpacks/`. Modpacks are handed
//! back to the UI, which opens the import flow. The kind is read from the file's contents
//! (`pl_mc::gamefiles::classify_files`).

use std::path::PathBuf;
use std::time::Duration;

use pl_mc::gamefiles::{DroppedFile, DroppedKind, classify_files, import_dropped};
use serde::Serialize;
use tauri::{AppHandle, State};

use crate::commands::modrinth::instance_lock;
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// Most paths looked at per drop (a whole folder of files is not a drop to sort).
const MAX_FILES: usize = 64;
/// How long a drop waits for a version switch or a mod change of the instance to finish.
const LOCK_WAIT: Duration = Duration::from_secs(4);

/// One dropped file and what it is.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DroppedFileView {
    pub path: String,
    pub name: String,
    pub kind: DroppedKind,
}

impl From<&DroppedFile> for DroppedFileView {
    fn from(file: &DroppedFile) -> Self {
        Self {
            path: file.path.to_string_lossy().into_owned(),
            name: file.file_name.clone(),
            kind: file.kind,
        }
    }
}

fn paths_of(paths: Vec<String>) -> Vec<PathBuf> {
    paths
        .into_iter()
        .filter(|p| !p.trim().is_empty())
        .take(MAX_FILES)
        .map(PathBuf::from)
        .collect()
}

/// What the dragged files are (called as soon as a drag enters the window).
#[tauri::command]
#[specta::specta]
pub async fn files_classify(paths: Vec<String>) -> Vec<DroppedFileView> {
    classify_files(paths_of(paths))
        .await
        .iter()
        .map(DroppedFileView::from)
        .collect()
}

/// What happened to the files of a drop.
#[derive(Debug, Clone, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DropResultView {
    /// Copied into the instance.
    pub added: Vec<DroppedFileView>,
    /// A file of the same name is already there; nothing was replaced.
    pub existing: Vec<DroppedFileView>,
    /// Not a mod, resource pack, shader pack or modpack.
    pub unknown: Vec<DroppedFileView>,
    /// Mods dropped onto an instance without a mod loader (not copied).
    pub needs_loader: Vec<DroppedFileView>,
    /// Mods not copied because the instance's mods were being changed (version switch, update).
    pub busy: Vec<DroppedFileView>,
    /// Modpacks: the UI opens the import flow with them.
    pub modpacks: Vec<DroppedFileView>,
    /// Copies that failed (disk full, file locked); details in the log.
    pub failed: Vec<DroppedFileView>,
}

/// Copy dropped files into `instance_id`. Mods wait briefly for a running version switch or mod
/// change of the instance (they would land in the wrong set); packs are shared by all versions
/// and go in at once.
#[tauri::command]
#[specta::specta]
pub async fn files_drop(
    app: AppHandle,
    state: State<'_, AppState>,
    instance_id: String,
    paths: Vec<String>,
) -> CmdResult<DropResultView> {
    let mut instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let files = classify_files(paths_of(paths)).await;
    let mut result = DropResultView::default();

    let (mods, others): (Vec<&DroppedFile>, Vec<&DroppedFile>) =
        files.iter().partition(|f| f.kind == DroppedKind::Mod);
    let mut copy: Vec<&DroppedFile> = Vec::new();
    for file in others {
        match file.kind {
            DroppedKind::Unknown => result.unknown.push(file.into()),
            DroppedKind::Modpack => result.modpacks.push(file.into()),
            _ => copy.push(file),
        }
    }

    let lock = instance_lock(&app, &instance_id);
    let guard = if mods.is_empty() {
        None
    } else if !instance.is_modded() {
        result
            .needs_loader
            .extend(mods.iter().map(|f| DroppedFileView::from(*f)));
        None
    } else {
        match tokio::time::timeout(LOCK_WAIT, lock.lock()).await {
            Ok(guard) => {
                // A switch a crash interrupted is finished first, so the mods reach the set
                // that is really active.
                match crate::commands::versions::ensure_switch_finished(&state, &mut instance).await
                {
                    Ok(()) => {
                        copy.extend(mods.iter().copied());
                        Some(guard)
                    }
                    Err(error) => {
                        tracing::warn!(details = %error.details, "dropped mods wait for an unfinished version switch");
                        result
                            .busy
                            .extend(mods.iter().map(|f| DroppedFileView::from(*f)));
                        None
                    }
                }
            }
            Err(_) => {
                result
                    .busy
                    .extend(mods.iter().map(|f| DroppedFileView::from(*f)));
                None
            }
        }
    };

    for file in copy {
        match import_dropped(&game_dir, file).await {
            Ok(dest) => {
                tracing::info!(file = %file.file_name, to = %dest.display(), "dropped file added");
                result.added.push(file.into());
            }
            Err(pl_mc::Error::AlreadyExists(_)) => result.existing.push(file.into()),
            Err(error) => {
                tracing::warn!(file = %file.file_name, %error, "dropped file not added");
                result.failed.push(file.into());
            }
        }
    }
    drop(guard);
    Ok(result)
}
