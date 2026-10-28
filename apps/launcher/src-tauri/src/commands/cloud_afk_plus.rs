//! AFK+ (the AI tier of a 7/24 Bulut account): thin commands over `pl_cloud_afk`'s AFK+ calls.
//! Nothing of it shows while the service's `plusMode` is `off` (`cloud_afk_overview` → `plus` is
//! null then). Purchases, upgrades and plan changes open the site (`cloud_afk_open`).
//!
//! While the AFK+ panel is on screen its account is watched with `ai`: the AI's state, new audit
//! log entries, the orders' progress and approvals stream as `cloud-afk-live` events, as a
//! console's lines do.
//!
//! Refusals the panel says where they happen (a typed order the rules refused, an inventory item
//! that cannot be picked, a reply the game would not take, an approval that closed meanwhile) come
//! back as outcomes, not errors.

use std::path::PathBuf;

use pl_cloud_afk::{
    CloudError, CloudNotifyPrefs, PlusAction, PlusApproval, PlusAuditEntry, PlusAuditOutcome,
    PlusCheckOutcome, PlusConfig, PlusControlAnswer, PlusHistory, PlusInboxLine, PlusInboxPage,
    PlusItemData, PlusNewCustomItem, PlusNewOrder, PlusOrderOutcome, PlusOrderPatch,
    PlusOrderSettings, PlusPanelView, PlusPickOutcome, PlusQuota, PlusReceiptPage,
    PlusReceiptSummary, PlusSaveOutcome, PlusSettings, PlusThreadPage, PlusWeeklyReport,
};
use pl_lang::{Lang, pick};
use serde::Serialize;
use tauri::{AppHandle, State};
use tauri_plugin_opener::OpenerExt;

use crate::commands::cloud_afk::{service, token};
use crate::error::{CmdError, CmdResult};
use crate::state::AppState;

/// The launcher's cache folder (`<data>/cache`): AFK+'s item data and the audit logs it kept.
fn cache_root(state: &AppState) -> PathBuf {
    state.root.join("cache")
}

/// Signed out: the audit logs this computer kept go too (counterparts' names and masked
/// messages stay only while the player is signed in, and 7 days at most).
pub(crate) async fn forget_kept(app: &AppHandle, state: &AppState) {
    service(app, state)
        .plus_audit_prune(&cache_root(state), Vec::new())
        .await;
}

/// The panel's first look at an account: its settings, the quota and the approvals (the last two
/// wait while the account's machine is away).
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

/// The game's item data for a Minecraft version (the account's; none: the newest), checked and
/// cached. None when it cannot be had now (the order form offers to try again).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_item_data(
    app: AppHandle,
    state: State<'_, AppState>,
    minecraft: Option<String>,
) -> CmdResult<Option<PlusItemData>> {
    let root = cache_root(&state);
    Ok(service(&app, &state)
        .plus_item_data(&root, minecraft.as_deref())
        .await)
}

/// Saves the settings (`rev`: the revision the panel started from). The orders and custom items
/// are not sent: they have their own routes.
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

/// "Kontrol et": the player's words become an order's card (nothing is stored). `in_game`: the
/// player said the words' money is the game's ("Bu oyun parası", after the money question).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_check(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    text: String,
    in_game: bool,
) -> CmdResult<PlusCheckOutcome> {
    let token = token(&state)?;
    let lang = Lang::current().pick("tr", "en");
    Ok(service(&app, &state)
        .plus_check(&token, &id, &text, lang, in_game)
        .await?)
}

/// "Kaydet" of an order's card or form (or "Geri al" of a deleted one): the order, the new custom
/// items it names and a chip's settings change, in one write.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_order_create(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    order: PlusNewOrder,
    custom_items: Vec<PlusNewCustomItem>,
    settings: PlusOrderSettings,
) -> CmdResult<PlusOrderOutcome> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_order_create(&token, &id, order, custom_items, settings)
        .await?)
}

/// An order's change: "Düzenle", "Durdur", "Sürdür", "Süreyi uzat".
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_order_patch(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    order: String,
    patch: PlusOrderPatch,
) -> CmdResult<PlusOrderOutcome> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_order_patch(&token, &id, &order, patch)
        .await?)
}

/// "Sil": at once, or once the order's trade ended.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_order_delete(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    order: String,
) -> CmdResult<PlusOrderOutcome> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_order_delete(&token, &id, &order)
        .await?)
}

/// "Envanterden seç": the item in a slot of the bot's inventory (5-45, as the live inventory
/// shows it), its id and look as the view showed them.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_pick(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    slot: u32,
    item: String,
    cmd: Option<i64>,
    model: Option<String>,
) -> CmdResult<PlusPickOutcome> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_pick(&token, &id, slot, &item, cmd, model.as_deref())
        .await?)
}

/// A page of the audit log ("Akış"): `after` the newest entries past the last one kept, `before`
/// older ones; `group` `deals`, `messages` or `warnings`. While the machine cannot be asked, what
/// this computer kept.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_audit(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    before: Option<i64>,
    after: Option<i64>,
    group: Option<String>,
) -> CmdResult<PlusAuditOutcome> {
    let token = token(&state)?;
    let root = cache_root(&state);
    Ok(service(&app, &state)
        .plus_audit(&token, &id, &root, before, after, group.as_deref())
        .await?)
}

/// What this computer kept of an account's audit log, newest first (the panel's first frame).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_audit_kept(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
) -> CmdResult<Vec<PlusAuditEntry>> {
    let root = cache_root(&state);
    Ok(service(&app, &state).plus_audit_kept(&root, &id).await)
}

/// Keeps entries the live stream brought.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_audit_keep(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    items: Vec<PlusAuditEntry>,
) -> CmdResult<()> {
    let root = cache_root(&state);
    service(&app, &state)
        .plus_audit_keep(&root, &id, items)
        .await;
    Ok(())
}

/// Drops the audit logs kept for accounts that are not AFK+ any more (`keep`: those that are).
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_audit_prune(
    app: AppHandle,
    state: State<'_, AppState>,
    keep: Vec<String>,
) -> CmdResult<()> {
    let root = cache_root(&state);
    service(&app, &state).plus_audit_prune(&root, keep).await;
    Ok(())
}

/// Completed deals (receipts, 90 days), newest first.
#[tauri::command]
#[specta::specta]
pub async fn cloud_afk_plus_receipts(
    app: AppHandle,
    state: State<'_, AppState>,
    id: String,
    before: Option<String>,
) -> CmdResult<PlusReceiptPage> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_receipts(&token, &id, before.as_deref())
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

/// "Onayla", "Reddet", "Fiyatı düşür" (`approve` with `max_price` below the deal's price), "Takas
/// oldu" / "Olmadı" (a handoff), "Evet, bu" / "Hayır" (a first sight).
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
) -> CmdResult<PlusReceiptSummary> {
    let token = token(&state)?;
    Ok(service(&app, &state)
        .plus_receipt_summary(&token, &id, from, to)
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

/// "Tabloyu indir": the range's receipts as a CSV table in the Downloads folder (Turkish headers
/// with `;` as Turkish Excel reads them, or English with `,`; UTF-8 with a byte order mark).
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
        .plus_receipts_csv(&token, &id, from, to, lang)
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

/// An AFK notice about an item gets the item's names when this launcher knows them (the page's
/// toast and the in-game card say "Elytra", not an id): a custom item by the account's settings
/// as read, a game item by the item data read this session. Without them the account's item data
/// is read for the next notice; this one goes on at once (the words fall back to the id read
/// plainly, or "özel eşya").
pub fn name_items(app: &AppHandle, notice: &mut pl_social::AfkNotice) {
    use tauri::Manager;
    let Some(item) = notice.get("item").map(str::to_owned) else {
        return;
    };
    let Some(state) = app.try_state::<AppState>() else {
        return;
    };
    let cloud = service(app, &state);
    let acct = notice.get("acct").map(str::to_owned);
    if let Some((tr, en)) = cloud.plus_item_names(acct.as_deref(), &item) {
        notice.item_name = Some(pl_social::AfkItemName { tr, en });
        return;
    }
    if item.starts_with("custom:") {
        return;
    }
    let minecraft = acct.as_deref().and_then(|acct| cloud.plus_minecraft(acct));
    let root = cache_root(&state);
    let cloud = cloud.clone();
    tauri::async_runtime::spawn(async move {
        let _ = cloud.plus_item_data(&root, minecraft.as_deref()).await;
    });
}

/// The player saw an AFK notice (its toast with the window in front, or the panel showing it): the
/// friends service keeps it from the phone.
#[tauri::command]
#[specta::specta]
pub fn cloud_afk_notice_seen(state: State<'_, AppState>, id: String) {
    state.social.afk_seen(&id);
}

/// The AFK+ panel is on screen: the AI's state, audit log entries, orders' progress and approvals
/// stream as `cloud-afk-live`.
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
