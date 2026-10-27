//! 7/24 Bulut's cloud features (CAFK-FEAT phase 1) over `pl_cloud_afk`: the live inventory (its
//! stream while a view is on screen, the moves), the account's inventory choice, "Dene" for an
//! auto-reply, the console download and "Ayarları kopyala". Each shows in the UI only when the
//! service offers it (`CloudOverview.features`, `CloudAccount.features`).

use std::path::{Path, PathBuf};

use pl_cloud_afk::{
    CloudAccount, CloudCopyOutcome, CloudCopySecret, CloudCopySource, CloudInvError, CloudLogRange,
    CloudReplyTry, CloudSaveOutcome,
};
use pl_lang::{Lang, pick};
use serde::{Deserialize, Serialize};
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use super::cloud_afk::{service, token};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// An inventory view is on screen: the account's inventory streams as `cloud-afk-live` events
/// (`invState`, `inventory`, `window`).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_inventory_watch(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    let token = token(&state)?;
    Ok(service(&app, &state).watch_inventory(&token, &id).await?)
}

/// The inventory view left the screen (closed, another tab, the window hidden).
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_inventory_unwatch(app: AppHandle, state: State<'_, AppState>, id: String) {
    service(&app, &state).unwatch_inventory(&id);
}

/// A move (`op`: the view's `InvOp` as JSON, `opId` its own id). Null: sent, its answer comes as an
/// `invResult` event; else why it could not go out (its shape, or `host_offline`).
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_inv_op(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    op_id: String,
    op: String,
) -> Option<CloudInvError> {
    service(&app, &state).inv_op(&id, &op_id, &op)
}

/// The inventory view on or off for this account ("Aç"; the settings' switch saves with the rest).
/// It applies at the client's next start.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_set_inventory(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    enabled: bool,
) -> CmdResult<CloudSaveOutcome> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .set_inventory(&token, &id, enabled)
        .await?)
}

/// "Dene": what the rule would answer to `sample` written by `sender` (the account's owner, else
/// "Oyuncu"), on this computer.
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_try_reply(
    pattern: String,
    regex: bool,
    from: String,
    reply: String,
    sample: String,
    sender: String,
) -> CloudReplyTry {
    pl_cloud_afk::try_reply(&pattern, regex, &from, &reply, &sample, &sender)
}

/// A console download written to the Downloads folder.
#[derive(Debug, Clone, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudConsoleFile {
    pub file_name: String,
}

fn downloads() -> Option<PathBuf> {
    dirs::download_dir().or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
}

/// `Ahmet_AFK-konsol-2026-10-26.txt`, or with ` (2)` … when that name is taken.
fn free_name(dir: &Path, name: &str) -> String {
    if !dir.join(name).exists() {
        return name.to_owned();
    }
    let stem = name.trim_end_matches(".txt");
    (2..1000)
        .map(|n| format!("{stem} ({n}).txt"))
        .find(|candidate| !dir.join(candidate).exists())
        .unwrap_or_else(|| name.to_owned())
}

fn download_error(details: &str) -> CmdError {
    CmdError::new(
        "BULUT-INDIR",
        pick(
            "Konsol kaydedilemedi. İndirilenler klasörünü kontrol et.",
            "Couldn't save the console. Check your Downloads folder.",
        ),
        details.to_owned(),
    )
}

/// "İndir": the masked console of `range` as a `.txt` in the Downloads folder (UTF-8 with a byte
/// order mark, CRLF, Turkey time; kind names in the launcher's language). Null: no line in that
/// range, nothing written.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_console_download(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
    range: CloudLogRange,
) -> CmdResult<Option<CloudConsoleFile>> {
    let token = token(&state)?;
    let lang = Lang::current().pick("tr", "en");
    let now = jiff::Timestamp::now().as_second();
    let Some(text) = service(&app, &state)
        .console_download(&token, &id, range, lang, now)
        .await?
    else {
        return Ok(None);
    };
    let Some(dir) = downloads() else {
        return Err(download_error("no Downloads folder"));
    };
    let wanted = pl_cloud_afk::console_file_name(&name, lang, now);
    let written = tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)?;
        let file_name = free_name(&dir, &wanted);
        std::fs::write(dir.join(&file_name), text.as_bytes())?;
        Ok::<_, std::io::Error>(file_name)
    })
    .await
    .map_err(CmdError::internal)?;
    match written {
        Ok(file_name) => Ok(Some(CloudConsoleFile { file_name })),
        Err(error) => Err(download_error(&error.to_string())),
    }
}

/// "Klasörde göster" for a console download (only such a file, in Downloads).
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_show_download(app: AppHandle, file_name: String) -> CmdResult<()> {
    let safe = pl_cloud_afk::is_console_file_name(&file_name);
    let Some(path) = downloads().map(|dir| dir.join(&file_name)).filter(|_| safe) else {
        return Err(download_error("not a console download"));
    };
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|error| download_error(&error.to_string()))
}

/// "Ayarları kopyala": the source's secret rows this computer has no words for.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_copy_source(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<CloudCopySource> {
    let token = token(&state)?;
    Ok(service(&app, &state).copy_source(&token, &id).await?)
}

/// How copying to one account ended: the account as stored now, or the refusal's code (the
/// launcher's `BULUT-…` codes, worded by the bulk toast).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudCopyResult {
    pub id: String,
    pub account: Option<CloudAccount>,
    pub code: Option<String>,
}

/// "Ayarları kopyala": one change per target, in order (owner name, anti-AFK, commands, replies;
/// the server and version with `withServer`). Every secret row is sealed again for its target.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_copy_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    source: String,
    targets: Vec<String>,
    with_server: bool,
    secrets: Vec<CloudCopySecret>,
) -> CmdResult<Vec<CloudCopyResult>> {
    let token = token(&state)?;
    let outcomes = service(&app, &state)
        .copy_settings(&token, &source, &targets, with_server, &secrets)
        .await?;
    Ok(outcomes
        .into_iter()
        .map(|(id, outcome)| match outcome {
            CloudCopyOutcome::Copied(account) => CloudCopyResult {
                id,
                account: Some(*account),
                code: None,
            },
            CloudCopyOutcome::Failed(error) => CloudCopyResult {
                id,
                account: None,
                code: Some(CmdError::from(error).code),
            },
            CloudCopyOutcome::Unfit => CloudCopyResult {
                id,
                account: None,
                code: Some("BULUT".into()),
            },
        })
        .collect())
}
