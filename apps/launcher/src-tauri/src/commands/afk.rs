use pl_afk::{AfkAccount, ConsoleLine, ConsoleLineKind, SessionState, SessionStats};
use serde::Serialize;
use tauri::{AppHandle, Manager, State};

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

impl From<pl_afk::AfkError> for CmdError {
    fn from(error: pl_afk::AfkError) -> Self {
        CmdError::new("AFK", "AFK işlemi tamamlanamadı.", error.to_string())
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkAccountView {
    pub id: String,
    pub nickname: String,
    pub server: String,
    pub auto_start: bool,
    pub mc_version: String,
    pub state: SessionState,
    pub stats: SessionStats,
    pub running: bool,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleLineView {
    pub time: String,
    pub text: String,
    /// "normal" | "script" (sensitive lines never reach the UI)
    pub kind: String,
}

fn server_label(account: &AfkAccount) -> String {
    let host = if account.server_host.trim().is_empty() {
        "play.projects.gg"
    } else {
        account.server_host.trim()
    };
    match account.server_port {
        p if p > 0 && p != 25565 => format!("{host}:{p}"),
        _ => host.to_owned(),
    }
}

fn overview(state: &AppState) -> Vec<AfkAccountView> {
    state
        .afk
        .accounts()
        .into_iter()
        .map(|account| {
            let snapshot = state.afk.snapshot(&account.id);
            AfkAccountView {
                server: server_label(&account),
                id: account.id,
                nickname: account.nickname,
                auto_start: account.auto_start,
                mc_version: account.minecraft_version,
                state: snapshot.state,
                stats: snapshot.stats,
                running: snapshot.running,
            }
        })
        .collect()
}

#[tauri::command]
#[specta::specta]
pub fn afk_overview(state: State<'_, AppState>) -> Vec<AfkAccountView> {
    overview(&state)
}

#[tauri::command]
#[specta::specta]
pub fn afk_add(state: State<'_, AppState>, nickname: String) -> CmdResult<()> {
    state.afk.add_account(nickname.trim())?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn afk_remove(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.afk.remove_account(&id).await?;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub async fn afk_connect(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.afk.connect(&id).await.map_err(|e| {
        CmdError::new(
            "AFK-BAGLAN",
            "AFK hesabı başlatılamadı. AFK istemcisi henüz indirilmemiş olabilir; internet bağlantını kontrol et.",
            e.to_string(),
        )
    })
}

#[tauri::command]
#[specta::specta]
pub fn afk_disconnect(state: State<'_, AppState>, id: String) {
    state.afk.disconnect(&id);
}

/// Staggered connect of every idle account. Runs in the background (15 s between accounts);
/// progress arrives as AFK events.
#[tauri::command]
#[specta::specta]
pub fn afk_connect_all(app: AppHandle) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if let Err(error) = state.afk.connect_all().await {
            tracing::warn!(%error, "AFK connect-all failed");
        }
    });
}

#[tauri::command]
#[specta::specta]
pub async fn afk_stop_all(state: State<'_, AppState>) -> CmdResult<()> {
    state.afk.cancel_bulk();
    state.afk.stop_all().await;
    Ok(())
}

#[tauri::command]
#[specta::specta]
pub fn afk_send(state: State<'_, AppState>, id: String, line: String) -> CmdResult<()> {
    state.afk.send(&id, line)?;
    Ok(())
}

fn line_view(line: ConsoleLine) -> Option<ConsoleLineView> {
    let kind = match line.kind {
        ConsoleLineKind::Normal => "normal",
        ConsoleLineKind::Script => "script",
        ConsoleLineKind::Sensitive => return None,
    };
    Some(ConsoleLineView {
        time: line.at.to_string(),
        text: line.text,
        kind: kind.to_owned(),
    })
}

#[tauri::command]
#[specta::specta]
pub fn afk_console(state: State<'_, AppState>, id: String) -> Vec<ConsoleLineView> {
    state.afk.console(&id).into_iter().filter_map(line_view).collect()
}

#[tauri::command]
#[specta::specta]
pub fn afk_any_active(state: State<'_, AppState>) -> bool {
    state.afk.any_active()
}
