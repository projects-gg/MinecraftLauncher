//! Launcher self-updates on top of tauri-plugin-updater.
//!
//! 1. Our channel file (`v1/releases/<channel>.json`, Tauri's static format plus `rollout`,
//!    `mandatory`, `minVersion`) is fetched from the content mirrors in order.
//! 2. `ReleaseChannel::decide` applies staged rollout (stable per-install bucket) and the
//!    minimum version.
//! 3. The Tauri updater downloads the signed bundle from the mirror that answered and installs it.

use std::sync::Arc;

use pl_content::{ReleaseChannel, UpdateDecision, rollout_bucket};
use serde::Serialize;
use tauri::ipc::Channel;
use tauri::{AppHandle, State};
use tauri_plugin_updater::UpdaterExt;

use crate::error::{CmdError, CmdResult};
use crate::state::{AppState, CONTENT_MIRRORS};

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateStatus {
    /// "upToDate" | "available" | "mandatory" | "heldBack" | "unavailable"
    pub state: String,
    pub current: String,
    pub version: Option<String>,
    pub notes: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct UpdateProgress {
    pub downloaded: u64,
    pub total: Option<u64>,
    pub done: bool,
}

/// Stable random id of this installation, used only for the rollout bucket.
fn install_id(state: &AppState) -> String {
    let path = state.root.join("install-id");
    if let Ok(id) = std::fs::read_to_string(&path)
        && !id.trim().is_empty()
    {
        return id.trim().to_owned();
    }
    let id = uuid::Uuid::new_v4().to_string();
    let _ = std::fs::write(&path, &id);
    id
}

fn channel_urls(channel: &str) -> Vec<String> {
    let channel = if channel == "beta" { "beta" } else { "stable" };
    CONTENT_MIRRORS
        .iter()
        .map(|base| format!("{base}releases/{channel}.json"))
        .collect()
}

async fn fetch_channel(state: &AppState, channel: &str) -> Option<(ReleaseChannel, String)> {
    let urls = channel_urls(channel);
    match state
        .http
        .get_json_from_mirrors::<ReleaseChannel>(&urls)
        .await
    {
        Ok((release, index)) => Some((release, urls[index].clone())),
        Err(error) => {
            tracing::info!(%error, "update channel unavailable");
            None
        }
    }
}

#[tauri::command]
#[specta::specta]
pub async fn update_check(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: String,
) -> CmdResult<UpdateStatus> {
    let current_text = app.package_info().version.to_string();
    let current = semver::Version::parse(&current_text).map_err(CmdError::internal)?;
    let Some((release, _)) = fetch_channel(&state, &channel).await else {
        return Ok(UpdateStatus {
            state: "unavailable".into(),
            current: current_text,
            version: None,
            notes: None,
        });
    };
    let decision = release.decide(&current, rollout_bucket(&install_id(&state)));
    let state_text = match decision {
        UpdateDecision::UpToDate => "upToDate",
        UpdateDecision::HeldBack => "heldBack",
        UpdateDecision::Available => "available",
        UpdateDecision::Mandatory => "mandatory",
        UpdateDecision::Invalid => "unavailable",
    };
    Ok(UpdateStatus {
        state: state_text.into(),
        current: current_text,
        version: Some(release.version.clone()),
        notes: release.notes.clone(),
    })
}

/// Download the update of `channel` in the background (signature verified by the updater) and
/// keep it ready; `update_apply` installs it. Progress streams over `on_progress`.
#[tauri::command]
#[specta::specta]
pub async fn update_download(
    app: AppHandle,
    state: State<'_, AppState>,
    channel: String,
    on_progress: Channel<UpdateProgress>,
) -> CmdResult<bool> {
    let Some((_, url)) = fetch_channel(&state, &channel).await else {
        return Err(CmdError::new(
            "GUNCELLEME-YOK",
            "Güncelleme sunucusuna ulaşılamadı. Daha sonra tekrar dene.",
            "no channel mirror answered",
        ));
    };
    let endpoint = url.parse().map_err(CmdError::internal)?;
    let updater = app
        .updater_builder()
        .endpoints(vec![endpoint])
        .map_err(CmdError::internal)?
        .build()
        .map_err(CmdError::internal)?;
    let Some(update) = updater
        .check()
        .await
        .map_err(|e| CmdError::new("GUNCELLEME", "Güncelleme denetlenemedi.", e.to_string()))?
    else {
        return Ok(false);
    };
    let downloaded = Arc::new(std::sync::atomic::AtomicU64::new(0));
    let progress = on_progress.clone();
    let counter = Arc::clone(&downloaded);
    let bytes = update
        .download(
            move |chunk, total| {
                let done = counter.fetch_add(chunk as u64, std::sync::atomic::Ordering::Relaxed)
                    + chunk as u64;
                let _ = progress.send(UpdateProgress {
                    downloaded: done,
                    total,
                    done: false,
                });
            },
            move || {
                let _ = on_progress.send(UpdateProgress {
                    downloaded: downloaded.load(std::sync::atomic::Ordering::Relaxed),
                    total: None,
                    done: true,
                });
            },
        )
        .await
        .map_err(|e| {
            CmdError::new(
                "GUNCELLEME-INDIR",
                "Güncelleme indirilemedi.",
                e.to_string(),
            )
        })?;
    *state.pending_update.lock() = Some((update, bytes));
    Ok(true)
}

/// Install the downloaded update. On Windows the installer takes over and the app exits; on
/// macOS/Linux the app restarts itself. The update is kept when it cannot be installed now, so
/// "try again" works (the mandatory update screen has no other way forward).
#[tauri::command]
#[specta::specta]
pub fn update_apply(app: AppHandle, state: State<'_, AppState>) -> CmdResult<()> {
    let Some((update, bytes)) = state.pending_update.lock().take() else {
        return Err(CmdError::new(
            "GUNCELLEME-HAZIR-DEGIL",
            "Hazır bir güncelleme yok.",
            "no pending update",
        ));
    };
    if state.afk.any_active() || state.running.lock().is_some() {
        // Keep it for later rather than killing a game or AFK sessions.
        *state.pending_update.lock() = Some((update, bytes));
        return Err(CmdError::new(
            "GUNCELLEME-MESGUL",
            "Oyun ya da AFK hesapları açıkken güncelleme kurulamaz. Onları kapattıktan sonra tekrar dene.",
            "busy",
        ));
    }
    if let Err(error) = update.install(&bytes) {
        *state.pending_update.lock() = Some((update, bytes));
        return Err(CmdError::new(
            "GUNCELLEME-KURULUM",
            "Güncelleme kurulamadı. Tekrar dene.",
            error.to_string(),
        ));
    }
    app.restart();
}

#[tauri::command]
#[specta::specta]
pub fn app_restart(app: AppHandle) {
    app.restart();
}
