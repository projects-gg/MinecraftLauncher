use std::sync::Arc;

use pl_afk::looks::{Hover, LineLooks, LookEvent, Span};
use pl_afk::{
    AfkAccount, AfkError, AfkEventKind, AfkSettings, ClientProblem, ClientStep, ConsoleLine,
    ConsoleLineKind, EventSink, PatternProblem, SessionState, SessionStats, SettingsPage,
    ValidationIssue, VersionOption,
};
use pl_cloud_afk::{CloudClick, CloudHover, CloudHoverEntity, CloudLineEvent, CloudSpan};
use pl_lang::pick;
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};
use tauri_plugin_opener::OpenerExt;

use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// Where AFK sessions report: every event goes to the UI as `afk-event`; a state change (or a
/// change in waiting for the client) also refreshes the tray's AFK summary. That runs on a
/// blocking thread, never on the session's task: it reads every session and waits for the main
/// thread to set the tooltip (only when its text changed).
pub fn event_sink(app: &AppHandle) -> EventSink {
    let app = app.clone();
    Arc::new(move |event| {
        let _ = app.emit("afk-event", &event);
        #[cfg(desktop)]
        if matches!(event.kind, AfkEventKind::State(_) | AfkEventKind::Client(_)) {
            let app = app.clone();
            tauri::async_runtime::spawn_blocking(move || crate::shell::refresh_afk_tray(&app));
        }
    })
}

/// The AFK manager's look at the content index when a connect finds no client installed and no
/// build known (`pl_afk::client`): the index refreshed now, and the client build it publishes for
/// this computer. No mirror reached is "offline"; mirrors that answered with nothing usable are a
/// broken download.
pub fn release_source(app: &AppHandle) -> pl_afk::ReleaseSource {
    let app = app.clone();
    Arc::new(move || {
        let app = app.clone();
        Box::pin(async move {
            let unusable = |offline: bool, problem: String| {
                if offline {
                    AfkError::ContentUnavailable(problem)
                } else {
                    AfkError::InvalidData(format!("content index: {problem}"))
                }
            };
            let snapshot = crate::commands::content::refresh_now(&app)
                .await
                .map_err(|failed| unusable(failed.offline, failed.message))?;
            if snapshot.from_cache {
                return Err(unusable(
                    snapshot.offline,
                    snapshot.problem.unwrap_or_default(),
                ));
            }
            let state = app.state::<AppState>();
            Ok(crate::commands::content::afk_client_release(
                &state, &snapshot,
            ))
        })
    })
}

/// What the player is told when a connect could not get the AFK client (the AFK page words the
/// same in place: `afkConnect.problem` in the UI's texts).
fn client_problem_message(problem: ClientProblem) -> &'static str {
    match problem {
        ClientProblem::NotPublished => pick(
            "AFK istemcisi henüz hazır değil. Biraz sonra tekrar dene.",
            "The AFK client isn't ready yet. Try again in a bit.",
        ),
        ClientProblem::Offline => pick(
            "AFK istemcisi indirilemedi. İnternet bağlantını kontrol et.",
            "Couldn't download the AFK client. Check your internet connection.",
        ),
        ClientProblem::DiskFull => pick(
            "Diskte yer kalmadı. Biraz yer açıp tekrar dene.",
            "The disk is full. Free up some space and try again.",
        ),
        ClientProblem::Blocked => pick(
            "AFK istemcisi kurulamadı. Antivirüsün engellemediğinden emin ol.",
            "Couldn't install the AFK client. Make sure your antivirus isn't blocking it.",
        ),
        ClientProblem::Broken => pick(
            "AFK istemcisi indirilemedi. Biraz sonra tekrar dene.",
            "Couldn't download the AFK client. Try again in a bit.",
        ),
    }
}

impl From<pl_afk::AfkError> for CmdError {
    fn from(error: pl_afk::AfkError) -> Self {
        let details = error.to_string();
        let (code, message) = match &error {
            AfkError::ClientUnavailable { .. }
            | AfkError::NoClientRelease
            | AfkError::ContentUnavailable(_)
            | AfkError::ClientNotInstalled => (
                "AFK-ISTEMCI",
                client_problem_message(ClientProblem::of(&error)),
            ),
            AfkError::Cancelled => ("IPTAL", pick("İşlem iptal edildi.", "Cancelled.")),
            AfkError::Busy => (
                "AFK-SIRADA",
                pick(
                    "AFK hesapları zaten sırayla bağlanıyor.",
                    "The AFK accounts are already connecting one by one.",
                ),
            ),
            AfkError::AccountNotFound(_) => (
                "AFK-HESAP-YOK",
                pick(
                    "Bu AFK hesabı artık yok. Listeyi yenileyip tekrar dene.",
                    "This AFK account is gone. Refresh the list and try again.",
                ),
            ),
            AfkError::InvalidNickname => (
                "AFK-AD",
                pick(
                    "Oyuncu adı 3-16 karakter olmalı ve yalnızca harf, rakam ya da alt çizgi içermeli.",
                    "The player name must be 3 to 16 characters: letters, numbers or underscores only.",
                ),
            ),
            AfkError::DuplicateNickname => (
                "AFK-AD",
                pick(
                    "Bu adla bir AFK hesabı zaten var.",
                    "There's already an AFK account with this name.",
                ),
            ),
            AfkError::Secret(_) => (
                "AFK-ANAHTAR",
                pick(
                    "Giriş komutlarına bilgisayarın güvenli anahtar deposundan ulaşılamadı. Tekrar dene. Sürerse bilgisayarı yeniden başlat.",
                    "Couldn't reach the login commands in your computer's secure storage. Try again. If it keeps happening, restart your computer.",
                ),
            ),
            AfkError::LoginCommandsChanged => (
                "AFK-AYAR-DEGISTI",
                pick(
                    "Giriş komutları bu sırada değişmiş. Ayarları kapatıp yeniden aç.",
                    "The login commands changed in the meantime. Close the settings and open them again.",
                ),
            ),
            AfkError::InvalidSettings(_) => (
                "AFK-AYAR",
                pick(
                    "Ayarlarda eksik ya da hatalı bilgi var.",
                    "Some settings are missing or wrong.",
                ),
            ),
            AfkError::Io { .. } => (
                "AFK-DISK",
                pick(
                    "AFK ayarları diske yazılamadı. Diskte yer olduğundan ve antivirüsün engellemediğinden emin ol.",
                    "Couldn't save the AFK settings. Make sure there's space on the disk and your antivirus isn't blocking it.",
                ),
            ),
            AfkError::NotRunning => (
                "AFK-BAGLI-DEGIL",
                pick(
                    "Bu hesap şu an bağlı değil.",
                    "This account isn't connected right now.",
                ),
            ),
            _ => (
                "AFK",
                pick(
                    "AFK işlemi tamamlanamadı.",
                    "Couldn't finish that for the AFK account.",
                ),
            ),
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
    /// The account waits for the AFK client (the step, with the download's progress), or its
    /// last connect could not get it (`failed`, with the problem to word).
    pub client: Option<ClientStep>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConsoleLineView {
    pub time: String,
    pub text: String,
    /// "normal" | "script" (sensitive lines never reach the UI)
    pub kind: String,
    /// The player whose chat line it is (a client with the console kinds), for the card of a
    /// command clicked in it.
    pub who: Option<String>,
    /// The line in the game's colours (console looks, AFK-UX): its parts and what they do, in
    /// 7/24 Bulut's form; the view checks them again (`@pl/ui/console`).
    pub st: Option<Vec<CloudSpan>>,
    pub ev: Option<Vec<CloudLineEvent>>,
}

/// The server's dialog of a desktop account as its console card shows it (DIALOG-CARD on this
/// computer): its JSON as pl-afk checked it (the view checks it again: `safeDialog`).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkDialogView {
    /// The client's revision of it: a press names it.
    pub rev: u32,
    /// Its JSON (empty while `wait`).
    pub dialog: String,
    /// The game's "waiting for response" screen in a dialog's place (client 4.2.5): a close of
    /// `rev` leaves it.
    pub wait: bool,
    /// When it opened (RFC 3339).
    pub at: String,
}

/// The client's answer to a press of the card: done, or why not (`stale`: another dialog now;
/// anything else: it did not work).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkDialogOutcome {
    pub ok: bool,
    pub why: Option<String>,
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
                client: state.afk.client_step(&account.id),
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

/// A connect's outcome for the UI. Stopping the account while it waited for the client is not a
/// failure. A client that could not be had is shown on the account's card (the AFK page words it
/// from the account's `client` step), so its error only backs that up.
fn connect_result(result: pl_afk::Result<()>) -> CmdResult<()> {
    match result {
        Ok(()) => Ok(()),
        Err(error) if error.is_cancelled() => Ok(()),
        Err(
            error @ (AfkError::AccountNotFound(_)
            | AfkError::ClientUnavailable { .. }
            | AfkError::NoClientRelease
            | AfkError::ContentUnavailable(_)
            | AfkError::ClientNotInstalled),
        ) => Err(error.into()),
        Err(error) => Err(CmdError::new(
            "AFK-BAGLAN",
            pick(
                "AFK hesabı başlatılamadı. Tekrar dene.",
                "The AFK account couldn't start. Try again.",
            ),
            error.to_string(),
        )),
    }
}

/// Connects one account. When the AFK client is missing it is fetched first (the content index
/// refreshed when the launcher knows no build), the card showing the progress; stopping the
/// account ends the wait.
#[tauri::command]
#[specta::specta]
pub async fn afk_connect(state: State<'_, AppState>, id: String) -> CmdResult<()> {
    connect_result(state.afk.connect(&id).await)
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
    connect_result(state.afk.reconnect(&id).await)
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

/// Largest looks of one console read, newest line first: a line past it shows plain (the view
/// reads the whole console after every event).
const LOOKS_BUDGET: usize = 48 * 1024;

fn span_view(span: &Span) -> CloudSpan {
    let units = u32::try_from(span.units).unwrap_or(u32::MAX);
    match span.ev.and_then(|index| u8::try_from(index).ok()) {
        Some(index) => CloudSpan::Event(units, span.code.clone(), index),
        None => CloudSpan::Plain(units, span.code.clone()),
    }
}

fn spans_view(spans: &[Span]) -> Vec<CloudSpan> {
    spans.iter().map(span_view).collect()
}

fn event_view(event: &LookEvent) -> CloudLineEvent {
    CloudLineEvent {
        click: event.click.as_ref().map(|click| CloudClick {
            a: click.a.clone(),
            v: click.v.clone(),
        }),
        hover: event.hover.as_ref().map(|hover| match hover {
            Hover::Text { text, st } => CloudHover::Text {
                text: text.clone(),
                st: st.as_deref().map(spans_view),
            },
            Hover::Entity { name, st } => CloudHover::Entity {
                entity: CloudHoverEntity {
                    label: None,
                    name: Some(name.clone()),
                    st: st.as_deref().map(spans_view),
                },
            },
        }),
    }
}

fn line_view(line: ConsoleLine, budget: &mut usize) -> Option<ConsoleLineView> {
    let kind = match line.kind {
        ConsoleLineKind::Normal => "normal",
        ConsoleLineKind::Script => "script",
        ConsoleLineKind::Sensitive => return None,
    };
    let looks: Option<&LineLooks> = line.looks.as_ref().filter(|looks| {
        let bytes = pl_afk::looks::json_bytes(looks);
        let fits = bytes <= *budget;
        if fits {
            *budget -= bytes;
        }
        fits
    });
    let st = looks.map(|looks| spans_view(&looks.st));
    let ev = looks
        .filter(|looks| !looks.ev.is_empty())
        .map(|looks| looks.ev.iter().map(event_view).collect());
    Some(ConsoleLineView {
        time: line.at.to_string(),
        text: line.text,
        kind: kind.to_owned(),
        who: line.who,
        st,
        ev,
    })
}

#[tauri::command]
#[specta::specta]
pub fn afk_console(state: State<'_, AppState>, id: String) -> Vec<ConsoleLineView> {
    let mut budget = LOOKS_BUDGET;
    let mut lines: Vec<ConsoleLineView> = state
        .afk
        .console(&id)
        .into_iter()
        .rev()
        .filter_map(|line| line_view(line, &mut budget))
        .collect();
    lines.reverse();
    lines
}

/// The server's dialog of the account, as its console card shows it (none while it has none).
#[tauri::command]
#[specta::specta]
pub fn afk_dialog(state: State<'_, AppState>, id: String) -> Option<AfkDialogView> {
    state.afk.dialog(&id).map(|dialog| AfkDialogView {
        rev: dialog.rev,
        dialog: dialog.json,
        wait: dialog.wait,
        at: dialog.at.to_string(),
    })
}

/// What the game's pause screen and quick actions key open for the account (client 4.2.5):
/// `{pause?, quick?}`, each `{label, labelSt?}`, as JSON (the view checks it again: `safeMenus`).
/// A press `{open: "pause" | "quick"}` opens one.
#[tauri::command]
#[specta::specta]
pub fn afk_dialog_menus(state: State<'_, AppState>, id: String) -> Option<String> {
    state.afk.dialog_menus(&id)
}

/// A press of the account's dialog card (`press`: its JSON as `@pl/ui` `dialogPress` built it):
/// checked by pl-afk and by the client, which answers.
#[tauri::command]
#[specta::specta]
pub async fn afk_dialog_press(
    state: State<'_, AppState>,
    id: String,
    press: String,
) -> CmdResult<AfkDialogOutcome> {
    let outcome = state.afk.press_dialog(&id, &press).await?;
    Ok(AfkDialogOutcome {
        ok: outcome.ok,
        why: outcome.why,
    })
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
        .map_err(|error| {
            CmdError::new(
                "KLASOR",
                pick("Klasör açılamadı.", "Couldn't open the folder."),
                error.to_string(),
            )
        })
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
