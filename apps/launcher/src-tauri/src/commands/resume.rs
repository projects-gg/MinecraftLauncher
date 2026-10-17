//! "Devam et": the world or server an instance played last (1.20+ quick play). The choice is
//! made in `pl_mc::launch::resume`; this only reads the instance and maps the view.

use pl_mc::launch::{QuickPlay, ResumeTarget};
use serde::Serialize;
use tauri::State;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ResumeKind {
    World,
    Server,
}

/// Where "Devam et" goes.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ResumeView {
    pub kind: ResumeKind,
    /// The world's or the server's name.
    pub name: String,
    /// When it was played (ISO time).
    pub last_played_at: Option<String>,
    /// The world's or the server's icon (data URL).
    pub icon_url: Option<String>,
    /// Handed back to `launch_instance` (`settings.quickPlay`).
    pub quick_play: QuickPlay,
}

fn png_data_url(bytes: &[u8]) -> String {
    use base64::Engine;
    format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    )
}

fn iso(ms: Option<i64>) -> Option<String> {
    ms.and_then(|ms| jiff::Timestamp::from_millisecond(ms).ok())
        .map(|t| t.to_string())
}

fn view(target: ResumeTarget) -> ResumeView {
    let quick_play = target.quick_play();
    match target {
        ResumeTarget::World {
            name,
            last_played_ms,
            icon_png,
            ..
        } => ResumeView {
            kind: ResumeKind::World,
            name,
            last_played_at: iso(last_played_ms),
            icon_url: icon_png.as_deref().map(png_data_url),
            quick_play,
        },
        ResumeTarget::Server {
            name,
            last_played_ms,
            icon_png,
            ..
        } => ResumeView {
            kind: ResumeKind::Server,
            name,
            last_played_at: iso(last_played_ms),
            icon_url: icon_png.as_deref().map(png_data_url),
            quick_play,
        },
    }
}

/// The world or server "Devam et" joins for `instance_id`; `None` before 1.20, before the
/// first world or server, and while the version was never installed.
#[tauri::command]
#[specta::specta]
pub async fn resume_target(
    state: State<'_, AppState>,
    instance_id: String,
) -> CmdResult<Option<ResumeView>> {
    let instance = state
        .instances
        .get(&instance_id)
        .await
        .map_err(CmdError::from)?;
    if !pl_mc::launch::supports_quick_play(&state.mc, &instance.mc_version).await {
        return Ok(None);
    }
    let game_dir = state
        .instances
        .game_dir(&instance_id)
        .map_err(CmdError::from)?;
    let logs = pl_mc::gamefiles::list_game_logs(&state.root.join("logs"), &instance_id)
        .await
        .map(|logs| logs.into_iter().map(|log| log.path).collect())
        .unwrap_or_default();
    match pl_mc::launch::resume_target(&game_dir, logs).await {
        Ok(target) => Ok(target.map(view)),
        Err(error) => {
            tracing::debug!(%error, "no resume target");
            Ok(None)
        }
    }
}
