//! AFK+ (the AI tier of a 7/24 Bulut account): thin commands over `pl_cloud_afk`'s AFK+ calls.
//! Nothing of it shows while the service's `plusMode` is `off` (`cloud_afk_overview` → `plus` is
//! null then). Purchases, upgrades and plan changes open the site (`cloud_afk_open`).
//!
//! While the AFK+ panel is on screen its account is watched with `ai`: the AI's state, feed and
//! approvals stream as `cloud-afk-live` events, as a console's lines do.
//!
//! Refusals the panel says where they happen (a typed rule that could not be read, a reply the
//! game would not take, an approval that closed meanwhile) come back as outcomes, not errors.

use std::path::PathBuf;

use pl_cloud_afk::{
    CloudError, CloudNotifyPrefs, PlusAction, PlusApproval, PlusCatalog, PlusConfig,
    PlusControlAnswer, PlusDealPage, PlusDealSummary, PlusFeedPage, PlusHistory, PlusInboxLine,
    PlusInboxPage, PlusPanelView, PlusParseDraft, PlusQuota, PlusSaveOutcome, PlusSettings,
    PlusThreadPage, PlusWeeklyReport,
};
use pl_lang::{Lang, pick};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::commands::cloud_afk::{service, token};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// The panel's first look at an account: its settings, the mode's catalog, the quota and the
/// approvals (the last two wait while the account's machine is away).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_panel(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<PlusPanelView> {
    let token = token(&state)?;
    Ok(service(&app, &state).plus_panel(&token, &id).await?)
}

/// The settings as stored now (after a conflict, "Son halini yükle").
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_settings(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<PlusSettings> {
    let token = token(&state)?;
    Ok(service(&app, &state).plus_settings(&token, &id).await?)
}

/// A game mode's catalog (no session needed; kept an hour). None when it cannot be had now.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_catalog(
    app: AppHandle,
    state: State<'_, AppState>,
    mode: String,
) -> CmdResult<Option<PlusCatalog>> {
    Ok(service(&app, &state).plus_catalog(&mode).await)
}

/// Saves the settings (`rev`: the revision the panel started from).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_save(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    rev: u64,
    config: PlusConfig,
) -> CmdResult<PlusSaveOutcome> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_save(&token, &id, rev, config)
        .await?)
}

/// "Bu hale dön": the saved revision `rev` becomes the next one (`current`: the one on screen).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_restore(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    rev: u64,
    current: u64,
) -> CmdResult<PlusSaveOutcome> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_restore(&token, &id, rev, current)
        .await?)
}

/// The AI's switch, a player blocked, unblocked or forgotten, the spent total reset.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_control(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    action: PlusAction,
    name: Option<String>,
) -> CmdResult<PlusControlAnswer> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_control(&token, &id, action, name.as_deref())
        .await?)
}

/// Older feed lines (the socket brings the latest while the panel is open).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_feed(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    after: Option<String>,
) -> CmdResult<PlusFeedPage> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_feed(&token, &id, after.as_deref())
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_deals(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    after: Option<String>,
) -> CmdResult<PlusDealPage> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_deals(&token, &id, after.as_deref())
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_approvals(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Vec<PlusApproval>> {
    let token = token(&state)?;
    Ok(service(&app, &state).plus_approvals(&token, &id).await?)
}

/// How an answer to an approval ended.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlusAnswerOutcome {
    /// Stored: final, the machine hears it (now or on its return). Boxed: the other variant is small.
    Done { approval: Box<PlusApproval> },
    /// It was no longer waiting (answered elsewhere, expired, the deal gone): `state` says how.
    Closed { state: String },
}

/// "Onayla", "Reddet", or "Fiyatı düşür" (`approve` with `max_price` below the deal's price).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_answer(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    approval: String,
    approve: bool,
    max_price: Option<u64>,
) -> CmdResult<PlusAnswerOutcome> {
    let token = token(&state)?;
    match service(&app, &state)
        .plus_answer(&token, &id, &approval, approve, max_price)
        .await
    {
        Ok(approval) => Ok(PlusAnswerOutcome::Done {
            approval: Box::new(approval),
        }),
        Err(error) if error.code() == "approval_closed" => Ok(PlusAnswerOutcome::Closed {
            state: error.extra_str("state").unwrap_or("closed").to_owned(),
        }),
        Err(error) if error.code() == "approval_not_found" => Ok(PlusAnswerOutcome::Closed {
            state: "closed".into(),
        }),
        Err(error) => Err(error.into()),
    }
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_quota(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<PlusQuota> {
    let token = token(&state)?;
    Ok(service(&app, &state).plus_quota(&token, &id).await?)
}

/// "Mesajlar" (QoL Q1): the conversations of the last 7 days (live from the machine).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_inbox(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    after: Option<String>,
) -> CmdResult<PlusInboxPage> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_inbox(&token, &id, after.as_deref())
        .await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_thread(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
    after: Option<String>,
) -> CmdResult<PlusThreadPage> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_thread(&token, &id, &name, after.as_deref())
        .await?)
}

/// How the owner's reply ended.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlusReplyOutcome {
    Sent {
        line: PlusInboxLine,
    },
    /// The text rule said no: `reason` (`link`, `phone`, `email`, `iban`, `tckn`, `card`, `words`,
    /// `marker`, `length`, `form`).
    Rejected {
        reason: String,
    },
    /// The player wrote more than 24 hours ago.
    NotContacted,
    /// Too many replies now; `retry_after` seconds when the service said.
    Limited {
        retry_after: Option<u64>,
    },
    /// The account's machine is not connected now.
    Offline,
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_reply(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
    text: String,
) -> CmdResult<PlusReplyOutcome> {
    let token = token(&state)?;
    match service(&app, &state)
        .plus_reply(&token, &id, &name, &text)
        .await
    {
        Ok(line) => Ok(PlusReplyOutcome::Sent { line }),
        Err(CloudError::Invalid("reply")) => Ok(PlusReplyOutcome::Rejected {
            reason: "length".into(),
        }),
        Err(error) => match error.code() {
            "text_rejected" | "invalid" => Ok(PlusReplyOutcome::Rejected {
                reason: error.reason().unwrap_or("form").to_owned(),
            }),
            "not_contacted" => Ok(PlusReplyOutcome::NotContacted),
            "rate_limited" => Ok(PlusReplyOutcome::Limited {
                retry_after: error.retry_after(),
            }),
            "host_offline" | "unavailable" => Ok(PlusReplyOutcome::Offline),
            _ => Err(error.into()),
        },
    }
}

/// The conversation was read up to `up_to` (Unix seconds).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_read(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
    up_to: i64,
) -> CmdResult<()> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_read(&token, &id, &name, up_to)
        .await?)
}

/// "Şimdi devret": the assistant answers in this conversation again.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_hand_back(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
) -> CmdResult<()> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_hand_back(&token, &id, &name)
        .await?)
}

/// How "Kural oluştur" (QoL Q2) ended.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlusParseOutcome {
    Draft {
        draft: PlusParseDraft,
        issues: Vec<String>,
    },
    /// Nothing could be read: `no_item`, `no_amount`, `many_items` or `unsafe`.
    Failed { reason: String },
    /// The day's typed rules are used up (`daily`), or too many in an hour.
    Limited { daily: bool },
    /// The account's machine is not connected now.
    Offline,
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_parse(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    text: String,
) -> CmdResult<PlusParseOutcome> {
    let token = token(&state)?;
    let lang = Lang::current().pick("tr", "en");
    match service(&app, &state)
        .plus_parse(&token, &id, &text, lang)
        .await
    {
        Ok(answer) => Ok(PlusParseOutcome::Draft {
            draft: answer.draft,
            issues: answer.issues,
        }),
        Err(CloudError::Invalid("typed rule")) => Ok(PlusParseOutcome::Failed {
            reason: "no_item".into(),
        }),
        Err(error) => match error.code() {
            "parse_failed" | "invalid" => Ok(PlusParseOutcome::Failed {
                reason: error.reason().unwrap_or("no_item").to_owned(),
            }),
            "rate_limited" => Ok(PlusParseOutcome::Limited {
                daily: error.reason() == Some("daily"),
            }),
            "host_offline" | "unavailable" => Ok(PlusParseOutcome::Offline),
            _ => Err(error.into()),
        },
    }
}

/// "Ayar geçmişi" (QoL Q7), newest first.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_history(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<PlusHistory> {
    let token = token(&state)?;
    Ok(service(&app, &state).plus_history(&token, &id).await?)
}

/// The trade summary (QoL Q4) of `from..to` (Unix seconds; none: the last 30 days).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_summary(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    from: Option<i64>,
    to: Option<i64>,
) -> CmdResult<PlusDealSummary> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_deal_summary(&token, &id, from, to)
        .await?)
}

/// The weekly report (QoL Q4) of every AFK+ account of the player; `week` `YYYY-Www` (none: the
/// last whole week).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_report(
    app: AppHandle,
    state: State<'_, AppState>,
    week: Option<String>,
) -> CmdResult<PlusWeeklyReport> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_weekly_report(&token, week.as_deref())
        .await?)
}

/// A table written to the player's Downloads folder.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlusExport {
    pub file_name: String,
}

fn downloads() -> Option<PathBuf> {
    dirs::download_dir().or_else(|| dirs::home_dir().map(|home| home.join("Downloads")))
}

/// "Tabloyu indir": the range's deals as a CSV table in the Downloads folder (Turkish headers with
/// `;` as Turkish Excel reads them, or English with `,`; UTF-8 with a byte order mark).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_export(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    name: String,
    from: Option<i64>,
    to: Option<i64>,
) -> CmdResult<PlusExport> {
    let token = token(&state)?;
    let lang = Lang::current().pick("tr", "en");
    let bytes = service(&app, &state)
        .plus_deals_csv(&token, &id, from, to, lang)
        .await?;
    let Some(dir) = downloads() else {
        return Err(export_error("no Downloads folder"));
    };
    let day = jiff::Zoned::now().strftime("%Y-%m-%d").to_string();
    let written = tauri::async_runtime::spawn_blocking(move || {
        std::fs::create_dir_all(&dir)?;
        let file_name = pl_cloud_afk::table_file_name(&dir, &name, &day);
        std::fs::write(dir.join(&file_name), &bytes)?;
        Ok::<_, std::io::Error>(file_name)
    })
    .await
    .map_err(CmdError::internal)?;
    match written {
        Ok(file_name) => Ok(PlusExport { file_name }),
        Err(error) => Err(export_error(&error.to_string())),
    }
}

fn export_error(details: &str) -> CmdError {
    CmdError::new(
        "AFKPLUS-TABLO",
        pick(
            "Tablo kaydedilemedi. İndirilenler klasörünü kontrol et.",
            "Couldn't save the table. Check your Downloads folder.",
        ),
        details.to_owned(),
    )
}

/// "Klasörde göster" for a table "Tabloyu indir" saved (only such a file, in Downloads).
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_plus_show_export(app: AppHandle, file_name: String) -> CmdResult<()> {
    let safe = pl_cloud_afk::is_table_file_name(&file_name);
    let Some(path) = downloads().map(|dir| dir.join(&file_name)).filter(|_| safe) else {
        return Err(export_error("not an AFK+ table"));
    };
    app.opener()
        .reveal_item_in_dir(&path)
        .map_err(|error| export_error(&error.to_string()))
}

/// The player's notification switches and quiet hours (all their cloud accounts).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_notify_prefs(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<CloudNotifyPrefs> {
    let token = token(&state)?;
    Ok(service(&app, &state).notify_prefs(&token).await?)
}

#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_set_notify_prefs(
    app: AppHandle,
    state: State<'_, AppState>,
    prefs: CloudNotifyPrefs,
) -> CmdResult<CloudNotifyPrefs> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .set_notify_prefs(&token, &prefs)
        .await?)
}

/// The game mode whose catalog names the items of AFK notices until an AFK+ panel was opened (the
/// only verified profile of v1; a mode the service does not offer just reads nothing).
const NOTICE_MODE: &str = "projects-smp";

/// An AFK notice about an item gets the item's names when a catalog this launcher read knows it
/// (the page's toast and the in-game card say "Elytra", not an id). Without one the Projects
/// mode's catalog is read for the next notice; this one goes on at once.
pub fn name_items(app: &AppHandle, notice: &mut pl_social::AfkNotice) {
    use tauri::Manager;
    let Some(item) = notice.get("item").map(str::to_owned) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let cloud = service(app, &state);
    if let Some((tr, en)) = cloud.cached_item_name(&item) {
        notice.item_name = Some(pl_social::AfkItemName { tr, en });
        return;
    }
    let cloud = cloud.clone();
    tauri::async_runtime::spawn(async move {
        let _ = cloud.plus_catalog(NOTICE_MODE).await;
    });
}

/// The player saw an AFK notice (its toast with the window in front, or the panel showing it): the
/// friends service keeps it from the phone.
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_notice_seen(state: State<'_, AppState>, id: String) {
    state.social.afk_seen(&id);
}

/// The AFK+ panel is on screen: the AI's state, feed and approvals stream as `cloud-afk-live`.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_watch(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<()> {
    let token = token(&state)?;
    Ok(service(&app, &state).watch_ai(&token, &id).await?)
}

/// The panel left the screen (closed, the window hidden).
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_plus_unwatch(app: AppHandle, state: State<'_, AppState>, id: String) {
    service(&app, &state).unwatch_ai(&id);
}
