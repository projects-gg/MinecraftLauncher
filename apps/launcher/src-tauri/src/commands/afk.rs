use std::sync::Arc;

use pl_afk::{
    AfkAccount, AfkError, AfkEventKind, AfkSettings, ConsoleLine, ConsoleLineKind, EventSink,
    PatternProblem, SessionState, SessionStats, SettingsPage, ValidationIssue, VersionOption,
};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// Where AFK sessions report: every event goes to the UI as `afk-event`; a state change also
/// refreshes the tray's AFK summary. That runs on a blocking thread, never on the session's task:
/// it reads every session and waits for the main thread to set the tooltip.
pub fn event_sink(app: &AppHandle) -> EventSink {
    let app = app.clone();
    Arc::new(move |event| {
        let _ = app.emit("afk-event", &event);
        #[cfg(desktop)]
        if matches!(event.kind, AfkEventKind::State(_)) {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || crate::shell::refresh_afk_tray(&app));
        }
    })
}

impl From<pl_afk::AfkError> for CmdError {
    fn from(error: pl_afk::AfkError) -> Self {
        let details = error.to_string();
        let (code, message) = match &error {
            AfkError::AccountNotFound(_) => (
                "AFK-HESAP-YOK",
                "Bu AFK hesabı artık yok. Listeyi yenileyip tekrar dene.",
            ),
            AfkError::InvalidNickname => (
                "AFK-AD",
                "Oyuncu adı 3-16 karakter olmalı ve yalnızca harf, rakam ya da alt çizgi içermeli.",
            ),
            AfkError::DuplicateNickname => ("AFK-AD", "Bu isimde bir AFK hesabı zaten var."),
            AfkError::Secret(_) => (
                "AFK-ANAHTAR",
                "Giriş komutlarına bilgisayarın güvenli anahtar deposundan ulaşılamadı. Tekrar dene; sürerse bilgisayarı yeniden başlat.",
            ),
            AfkError::LoginCommandsChanged => (
                "AFK-AYAR-DEGISTI",
                "Giriş komutları bu sırada değişmiş. Ayarları kapatıp yeniden aç.",
            ),
            AfkError::InvalidSettings(_) => ("AFK-AYAR", "Ayarlarda eksik ya da hatalı bilgi var."),
            AfkError::Io { .. } => (
                "AFK-DISK",
                "AFK ayarları diske yazılamadı. Diskte yer olduğundan ve antivirüsün engellemediğinden emin ol.",
            ),
            AfkError::NotRunning => ("AFK-BAGLI-DEGIL", "Bu hesap şu an bağlı değil."),
            _ => ("AFK", "AFK işlemi tamamlanamadı."),
        };
        CmdError::new(code, message, details)
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkAccountView {
    pub id: String,
    pub nickname: String,
    pub server: String,
    pub auto_start: bool,
    /// The game version the client connects with: `auto` or a listed value (the recommended one
    /// when the account follows it or stored a version the list does not know).
    pub mc_version: String,
    /// `mc_version` is the recommended version (no version chip then).
    pub mc_version_recommended: bool,
    pub state: SessionState,
    pub stats: SessionStats,
    pub running: bool,
    /// The running client was launched with older settings; they apply after a reconnect.
    pub settings_changed: bool,
    /// Show the date next to the time in the live console (chat log settings).
    pub show_date: bool,
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
    let versions = state.afk.versions();
    state
        .afk
        .accounts()
        .into_iter()
        .map(|account| {
            let snapshot = state.afk.snapshot(&account.id);
            let version = versions.resolve(&account.minecraft_version);
            AfkAccountView {
                server: server_label(&account),
                settings_changed: state.afk.settings_changed(&account.id),
                show_date: account.chat_log.show_date,
                id: account.id,
                nickname: account.nickname,
                auto_start: account.auto_start,
                mc_version: version.value,
                mc_version_recommended: version.recommended,
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

fn connect_error(error: AfkError) -> CmdError {
    if matches!(error, AfkError::AccountNotFound(_)) {
        return error.into();
    }
    CmdError::new(
        "AFK-BAGLAN",
        "AFK hesabı başlatılamadı. AFK istemcisi henüz indirilmemiş olabilir; internet bağlantını kontrol et.",
        error.to_string(),
    )
}

#[tauri::command]
#[specta::specta]
pub async fn afk_connect(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.afk.connect(&id).await.map_err(connect_error)
}

#[tauri::command]
#[specta::specta]
pub fn afk_disconnect(state: State<'_, AppState>, id: String) {
    state.afk.disconnect(&id);
}

/// Stops the account's client (if any) and connects it again, so saved settings apply.
#[tauri::command]
#[specta::specta]
pub async fn afk_reconnect(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    state.afk.reconnect(&id).await.map_err(connect_error)
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
    state
        .afk
        .console(&id)
        .into_iter()
        .filter_map(line_view)
        .collect()
}

#[tauri::command]
#[specta::specta]
pub fn afk_any_active(state: State<'_, AppState>) -> bool {
    state.afk.any_active()
}

/// Opens the account's folder in the file manager: its chat log, alerts and console log are there.
#[tauri::command]
#[specta::specta]
pub fn afk_open_folder(app: AppHandle, state: State<'_, AppState>, id: String) -> CmdResult<()> {
    let store = state.afk.store();
    let account = store
        .account(&id)
        .ok_or_else(|| AfkError::AccountNotFound(id.clone()))?;
    let dir = store.paths().account_dir(&account.id);
    std::fs::create_dir_all(&dir).map_err(CmdError::internal)?;
    app.opener()
        .open_path(dir.to_string_lossy(), None::<&str>)
        .map_err(|error| CmdError::new("KLASOR", "Klasör açılamadı.", error.to_string()))
}

// --- Settings sheet ---------------------------------------------------------------------------

/// Everything the settings sheet of one account needs.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkSettingsView {
    /// Login commands that carry a password are listed by name only; the password stays in the
    /// OS keychain.
    pub settings: SettingsPage,
    /// Game versions the AFK client can join, in picker order: `auto` first, then the versions
    /// newest first; one is flagged recommended (the content index's list, else the built-in one).
    pub versions: Vec<VersionOption>,
    /// The account stored a version the list does not know: the page shows (and the client uses)
    /// the recommended one instead.
    pub unknown_version: Option<String>,
    /// False when the OS keychain is unavailable: login commands then only last until the launcher
    /// closes.
    pub keychain_available: bool,
    /// Command names whose arguments are passwords (lowercase): the sheet masks such lines while
    /// they are typed, and saved ones come back as stored lines.
    pub sensitive_commands: Vec<String>,
}

/// Outcome of saving the settings sheet. Incomplete settings are an expected outcome, not an error.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum AfkSettingsSaveResult {
    /// Saved. `settings` is the stored page (normalized; passwords still left out).
    Saved {
        settings: Box<SettingsPage>,
        /// The running client keeps its old settings until it reconnects.
        #[serde(rename = "reconnectNeeded")]
        reconnect_needed: bool,
    },
    /// Nothing was saved.
    Invalid { issues: Vec<ValidationIssue> },
}

/// Reads one account's settings (blocks on the keychain, so it runs off the async workers).
#[tauri::command]
#[specta::specta]
pub async fn afk_settings(state: State<'_, AppState>, id: String) -> CmdResult<AfkSettingsView> {
    let afk = state.afk.clone();
    let versions = afk.versions();
    let unknown_version = afk
        .store()
        .account(&id)
        .and_then(|account| versions.resolve(&account.minecraft_version).unknown);
    let settings = tauri::async_runtime::spawn_blocking(move || afk.settings_page(&id))
        .await
        .map_err(CmdError::internal)??;
    Ok(AfkSettingsView {
        settings,
        versions: versions.options(),
        unknown_version,
        keychain_available: !state.ephemeral_secrets,
        sensitive_commands: pl_afk::sensitive_command_names()
            .iter()
            .map(|name| (*name).to_owned())
            .collect(),
    })
}

/// Validates and saves one account's settings. The UI's checks are only a convenience: the same
/// rules (and the clamping of every number) run here before anything is written.
#[tauri::command]
#[specta::specta]
pub async fn afk_save_settings(
    state: State<'_, AppState>,
    id: String,
    settings: SettingsPage,
) -> CmdResult<AfkSettingsSaveResult> {
    let afk = state.afk.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let saved = afk.save_settings_page(&id, settings)?;
        Ok::<_, AfkError>((saved.reconnect_needed, afk.settings_page(&id)?))
    })
    .await
    .map_err(CmdError::internal)?;
    match result {
        Ok((reconnect_needed, settings)) => Ok(AfkSettingsSaveResult::Saved {
            settings: Box::new(settings),
            reconnect_needed,
        }),
        Err(AfkError::InvalidSettings(issues)) => Ok(AfkSettingsSaveResult::Invalid { issues }),
        Err(error) => Err(error.into()),
    }
}

/// Checks an auto-respond pattern as it is typed (the same check runs again on save). `None`: the
/// client can use it.
#[tauri::command]
#[specta::specta]
pub fn afk_check_pattern(pattern: String, is_regex: bool) -> Option<PatternProblem> {
    pl_afk::check_pattern(&pattern, is_regex)
}

/// Account-independent AFK preferences.
#[tauri::command]
#[specta::specta]
pub fn afk_global_settings(state: State<'_, AppState>) -> AfkSettings {
    state.afk.afk_settings()
}

/// Shows or hides login-script lines in every console (lines with passwords stay hidden).
#[tauri::command]
#[specta::specta]
pub fn afk_set_show_script_commands(state: State<'_, AppState>, show: bool) -> CmdResult<()> {
    state.afk.set_show_script_commands(show)?;
    Ok(())
}
