//! The AFK+ panel's calls on [`CloudService`]: its first look at an account (settings, quota and
//! approvals together), saving settings and orders with the launcher's checks first, typed orders
//! ("Kontrol et"), picking an item from the bot's inventory, the audit log with what this computer
//! kept of it, the game's item data, the AI's controls, approvals, the QoL v1 routes and the AI's
//! live stream.
//!
//! Nothing polls: the panel reads when it opens and after an action; while it is on screen the
//! account is watched with `ai: true` and the socket brings the rest. The settings last read are
//! kept per account: the checks before a write run against them, and AFK notices name a custom
//! item by them.

use std::collections::{BTreeMap, HashMap};
use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::client::Bearer;
use crate::error::{CloudError, Result};
use crate::plus::{
    CloudNotifyPrefs, MAX_ORDER_TEXT, MAX_PAUSE_SECONDS, MAX_REPLY, MIN_ORDER_TEXT, PlusAction,
    PlusApproval, PlusAuditEntry, PlusConfig, PlusControlAnswer, PlusHistory, PlusInboxLine,
    PlusInboxPage, PlusIssue, PlusNewCustomItem, PlusNewOrder, PlusOrder, PlusOrderPatch,
    PlusOrderSettings, PlusOrderWritten, PlusPick, PlusQuota, PlusReceiptPage, PlusReceiptSummary,
    PlusSettings, PlusThreadPage, PlusWeeklyReport, PlusWritten, refusal_issues,
};
use crate::plus_audit;
use crate::plus_check::{
    PlusCheckContext, PlusCheckFeatures, PlusCheckLimits, PlusModeFacts, PlusProgress,
    check_config_value, clean_text,
};
use crate::plus_items::{ItemDataCache, PlusItemData, item_data_dir};
use crate::service::CloudService;

fn unix_now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |elapsed| {
            i64::try_from(elapsed.as_secs()).unwrap_or(i64::MAX)
        })
}

/// What AFK+ keeps of this session: the item data, and each account's settings as last read or
/// written.
#[derive(Default)]
pub(crate) struct PlusCache {
    pub(crate) items: ItemDataCache,
    settings: Mutex<HashMap<String, PlusSettings>>,
}

impl PlusCache {
    pub(crate) fn forget(&self) {
        self.settings.lock().clear();
        self.items.clear();
    }

    fn remember(&self, id: &str, settings: &PlusSettings) {
        self.settings.lock().insert(id.to_owned(), settings.clone());
    }

    fn settings(&self, id: &str) -> Option<PlusSettings> {
        self.settings.lock().get(id).cloned()
    }

    /// A write's answer over what was read: the settings, their revision and the judge, and the
    /// server view when the answer brings one (a save's: its money name as the service names it);
    /// else the server read stays, as Studio's rules and the orders' states do.
    fn written(
        &self,
        id: &str,
        config: PlusConfig,
        ai_rev: u64,
        server: Option<crate::plus::PlusServer>,
    ) -> PlusSettings {
        let mut guard = self.settings.lock();
        let entry = guard.entry(id.to_owned()).or_default();
        if entry.ai_rev <= ai_rev {
            entry.config = config;
            entry.ai_rev = ai_rev;
            if let Some(server) = server {
                entry.server = server;
            }
        }
        entry.clone()
    }
}

/// The AFK+ panel's first look at an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlusPanelView {
    pub settings: PlusSettings,
    /// None while the account's machine cannot be asked (`machine_away`).
    pub quota: Option<PlusQuota>,
    /// Pending ones first, then the last 24 hours' answers.
    pub approvals: Vec<PlusApproval>,
    /// The account's machine is not connected now: the quota, the log and "Mesajlar" wait.
    pub machine_away: bool,
}

/// How a settings save (or "Bu hale dön") ended. Problems are worded where they are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlusSaveOutcome {
    Saved {
        settings: PlusSettings,
    },
    /// Something the service would refuse, found here or said by it.
    Invalid {
        issues: Vec<PlusIssue>,
    },
    /// The settings changed elsewhere since the panel read them: their newer state.
    Conflict {
        settings: PlusSettings,
    },
}

/// How an order write ended (a new order, a change, a pause, a delete).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlusOrderOutcome {
    /// Stored: the settings with it; `order` the stored order (none for a delete); `deferred` a
    /// delete that waits for the order's trade to end.
    Saved {
        settings: Box<PlusSettings>,
        order: Option<Box<PlusOrder>>,
        deferred: bool,
    },
    /// Refused: where (`order.limit`, `customItem.name`, `orders` with `limit_orders`).
    Invalid { issues: Vec<PlusIssue> },
    /// The order is gone (deleted elsewhere).
    Gone,
    /// Too many writes: `daily` the AFK+ day's 100.
    Limited {
        daily: bool,
        retry_after: Option<u64>,
    },
}

/// How "Kontrol et" ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlusCheckOutcome {
    /// The card: the service's answer (`{draft, questions, notes, left}`) as JSON text, so the
    /// panel answers its questions on exactly what the service wrote.
    Draft { answer: String },
    /// The rules or the agent said no: `length`, `link`, `command`, `personal`, `unsafe`,
    /// `loosen` (with `rule`), `too_big`, `orders_full`, `no_item`, `many_items`; or asked first:
    /// `question` with `rule: "rmt"` (money that may be the game's: checked again with `in_game`).
    Rejected {
        reason: String,
        rule: Option<String>,
    },
    /// `rate` (a fourth check within a minute) or `daily` (the AFK+ day's checks used).
    Limited {
        reason: String,
        retry_after: Option<u64>,
    },
    /// The machine's AI cannot read it now: `agent`, `paused`, `off`, `budget`, `timeout`.
    Unavailable { reason: String },
    /// The account's machine is not connected now.
    Offline,
    /// Studio switched typed orders off.
    Off,
}

/// How "Envanterden seç" ended.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlusPickOutcome {
    Picked {
        pick: PlusPick,
    },
    /// `empty`, `stale`, `unparsed`, `container`, `not_in_game`, `inventory_off`, `custom_off`.
    Failed {
        reason: String,
    },
    Offline,
    Limited {
        retry_after: Option<u64>,
    },
}

/// A page of the audit log.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum PlusAuditOutcome {
    /// From the machine (and kept here): newest first; `more` when older ones are left; `from`
    /// the oldest AFK+ day it keeps.
    Page {
        items: Vec<PlusAuditEntry>,
        more: bool,
        from: Option<i64>,
    },
    /// The machine cannot be asked: what this computer kept, newest first.
    Kept {
        items: Vec<PlusAuditEntry>,
    },
    /// Studio switched the audit log off.
    Off,
    Limited {
        retry_after: Option<u64>,
    },
}

/// The machine's own refusals of a read: the panel shows the rest and says the machine is away.
fn machine_away(error: &CloudError) -> bool {
    matches!(error.code(), "host_offline" | "unavailable")
}

/// What the checks run against: the account's settings as read (its server's profile, Studio's
/// rules, the stored document and the orders' progress).
pub fn check_context(settings: &PlusSettings, now: i64) -> PlusCheckContext {
    let server = &settings.server;
    let facts = PlusModeFacts {
        max_deal: Some(server.money.max_deal),
        ad_templates: server.ads.iter().map(|ad| ad.id.clone()).collect(),
        approval_above: None,
    };
    let mut modes = BTreeMap::new();
    if !server.mode.is_empty() && server.mode != "generic" {
        modes.insert(server.mode.clone(), facts.clone());
    }
    let rules = &settings.rules;
    PlusCheckContext {
        now,
        modes,
        stored_mode: Some(settings.config.mode.clone()),
        stored_profile: (settings.config.mode == server.mode).then_some(facts),
        return_rows: None,
        features: PlusCheckFeatures {
            item_pay: Some(rules.features.item_pay),
            custom_items: Some(rules.features.custom_items),
            currency: Some(rules.features.currency),
        },
        limits: PlusCheckLimits {
            order_max: rules.order_max,
            order_store: rules.order_store,
            custom_items_max: rules.custom_items_max,
            order_days_max: rules.order_days_max,
            order_days_default: rules.order_days_default,
        },
        stored: serde_json::to_value(&settings.config).ok(),
        progress: settings
            .orders
            .iter()
            .map(|(id, view)| {
                let at = (view.at > 0).then_some(view.at);
                (
                    id.clone(),
                    PlusProgress {
                        got: u64::from(view.got),
                        at,
                    },
                )
            })
            .collect(),
    }
}

/// The settings a `PUT` sends (everything but the orders and custom items), checked and as the
/// service would store them; or what it would refuse. The money's name goes as the player's
/// settings have it: cleaned, null once cleared (the stored document leaves it out), and not at
/// all where they leave it out (the service keeps the stored one; an older one knows no such key).
pub fn checked_settings(
    config: &PlusConfig,
    settings: &PlusSettings,
    now: i64,
) -> std::result::Result<PlusConfig, Vec<PlusIssue>> {
    let mut input =
        serde_json::to_value(config).map_err(|_| vec![PlusIssue::new("config", "type")])?;
    if let Some(object) = input.as_object_mut() {
        object.remove("orders");
        object.remove("customItems");
    }
    let stored = check_config_value(&input, &check_context(settings, checks_now(settings, now)))
        .map_err(|refusal| vec![refusal.issue()])?;
    let mut sent: PlusConfig =
        serde_json::from_value(stored).map_err(|_| vec![PlusIssue::new("config", "type")])?;
    match config.trade.currency {
        None => sent.trade.currency = None,
        Some(None) => sent.trade.currency = Some(None),
        Some(Some(_)) => {}
    }
    Ok(sent)
}

/// A refusal on the count of active orders (`limit_orders`, scope `active`).
fn is_active_cap(issue: &PlusIssue) -> bool {
    issue.reason == "limit_orders" && issue.problem.as_deref() == Some("active")
}

/// The clock the checks here run on: this computer's, or later when what the service stamped on
/// the stored settings says its clock is ahead. A computer whose clock is behind would otherwise
/// refuse the stored orders (`at` "in the future") and a break set elsewhere; the service checks
/// again with its own clock.
fn checks_now(settings: &PlusSettings, now: i64) -> i64 {
    let config = &settings.config;
    config
        .orders
        .iter()
        .map(|order| order.at)
        .chain(config.custom_items.iter().map(|item| item.at))
        .chain(config.pause_until.map(|until| until - MAX_PAUSE_SECONDS))
        .fold(now, i64::max)
}

/// A refusal about the order being written, on the form's fields: `orders[i].limit` →
/// `order.limit` for the written order's `i` (any `i` for the service's answer to a write: it
/// refuses only what is written), a new custom item's `customItems[j].name` → `customItem.name`
/// (`j` from `new_custom`). Another order's or a stored item's stays as it is (the form says it
/// under its fields).
fn on_the_form(mut issue: PlusIssue, order: Option<usize>, new_custom: usize) -> PlusIssue {
    let split = |field: &str, list: &str| -> Option<(usize, String)> {
        let tail = field.strip_prefix(list)?.strip_prefix('[')?;
        let (index, after) = tail.split_once(']')?;
        Some((index.parse().ok()?, after.to_owned()))
    };
    if let Some((index, after)) = split(&issue.field, "orders") {
        if order.is_none_or(|order| index == order) {
            issue.field = format!("order{after}");
        }
    } else if let Some((index, after)) = split(&issue.field, "customItems")
        && index >= new_custom
    {
        issue.field = format!("customItem{after}");
    }
    issue
}

/// An id no order of the document uses (the service makes the real one).
fn spare_id(config: &PlusConfig) -> String {
    (0u32..)
        .map(|n| format!("new{n}"))
        .find(|id| config.orders.iter().all(|order| &order.id != id))
        .unwrap_or_else(|| "new".into())
}

/// The document as it would be after a new order (and the custom items it names), checked.
pub fn checked_new_order(
    settings: &PlusSettings,
    order: &PlusNewOrder,
    custom_items: &[PlusNewCustomItem],
    choice: &PlusOrderSettings,
    now: i64,
) -> std::result::Result<(), Vec<PlusIssue>> {
    let mut candidate = settings.config.clone();
    if let Some(pay) = &choice.pay {
        candidate.pay = pay.clone();
    }
    if let Some(cap) = choice.per_deal_max {
        candidate.limits.per_deal_max = Some(cap);
    }
    let id = spare_id(&candidate);
    candidate.orders.push(order.stored(&id, now));
    let written = candidate.orders.len() - 1;
    let new_custom = candidate.custom_items.len();
    for item in custom_items {
        if candidate.custom_items.iter().any(|known| known.k == item.k) {
            continue;
        }
        candidate.custom_items.push(crate::plus::PlusCustomItem {
            k: item.k.clone(),
            name: item.name.clone(),
            base: item.base.clone(),
            fp: item.fp.clone(),
            look: item.look.clone(),
            from: item.from.clone(),
            at: now,
        });
    }
    check_document(&candidate, settings, now, written, new_custom)
}

/// The document as it would be after an order's change, checked.
pub fn checked_patch(
    settings: &PlusSettings,
    order: &str,
    patch: &PlusOrderPatch,
    now: i64,
) -> std::result::Result<(), Vec<PlusIssue>> {
    let mut candidate = settings.config.clone();
    let Some(written) = candidate.orders.iter().position(|o| o.id == order) else {
        // Not in what was read: the service says whether it is gone.
        return Ok(());
    };
    let slot = &mut candidate.orders[written];
    *slot = patch.applied(slot, now, settings.rules.order_days_default);
    let new_custom = candidate.custom_items.len();
    check_document(&candidate, settings, now, written, new_custom)
}

fn check_document(
    candidate: &PlusConfig,
    settings: &PlusSettings,
    now: i64,
    written: usize,
    new_custom: usize,
) -> std::result::Result<(), Vec<PlusIssue>> {
    let input =
        serde_json::to_value(candidate).map_err(|_| vec![PlusIssue::new("config", "type")])?;
    check_config_value(&input, &check_context(settings, checks_now(settings, now)))
        .map(|_| ())
        .map_err(|refusal| vec![on_the_form(refusal.issue(), Some(written), new_custom)])
}

/// "Kontrol et"'s answer in the panel's terms.
pub fn check_outcome(result: Result<String>) -> Result<PlusCheckOutcome> {
    let error = match result {
        Ok(answer) => return Ok(PlusCheckOutcome::Draft { answer }),
        Err(error) => error,
    };
    Ok(match error.code() {
        "order_rejected" => PlusCheckOutcome::Rejected {
            reason: error.reason().unwrap_or("unsafe").to_owned(),
            rule: error.extra_str("rule").map(str::to_owned),
        },
        "rate_limited" => PlusCheckOutcome::Limited {
            reason: if error.reason() == Some("daily") {
                "daily".into()
            } else {
                "rate".into()
            },
            retry_after: error.retry_after(),
        },
        "ai_unavailable" => PlusCheckOutcome::Unavailable {
            reason: error.reason().unwrap_or("agent").to_owned(),
        },
        "host_offline" | "unavailable" => PlusCheckOutcome::Offline,
        "feature_off" => PlusCheckOutcome::Off,
        _ => return Err(error),
    })
}

/// "Envanterden seç"'s answer in the panel's terms.
pub fn pick_outcome(result: Result<PlusPick>) -> Result<PlusPickOutcome> {
    let error = match result {
        Ok(pick) => return Ok(PlusPickOutcome::Picked { pick }),
        Err(error) => error,
    };
    Ok(match error.code() {
        "pick_failed" => PlusPickOutcome::Failed {
            reason: error.reason().unwrap_or("unparsed").to_owned(),
        },
        "host_offline" | "unavailable" => PlusPickOutcome::Offline,
        "rate_limited" => PlusPickOutcome::Limited {
            retry_after: error.retry_after(),
        },
        "feature_off" => PlusPickOutcome::Failed {
            reason: "custom_off".into(),
        },
        _ => return Err(error),
    })
}

impl CloudService {
    fn cache(&self) -> &PlusCache {
        &self.inner.plus
    }

    /// The panel's first look: the settings (they must read), and with them the quota and the
    /// approvals (each may wait for the machine).
    pub async fn plus_panel(&self, token: &Bearer, id: &str) -> Result<PlusPanelView> {
        let client = &self.inner.client;
        let (settings, quota, approvals) = tokio::join!(
            client.plus_settings(token, id),
            client.plus_quota(token, id),
            client.plus_approvals(token, id)
        );
        let settings = settings?;
        self.cache().remember(id, &settings);
        let mut away = false;
        let quota = match quota {
            Ok(quota) => Some(quota),
            Err(error) if machine_away(&error) => {
                away = true;
                None
            }
            Err(error) => {
                tracing::debug!(%error, "the AFK+ quota is not readable now");
                None
            }
        };
        let mut approvals = match approvals {
            Ok(list) => list.approvals,
            Err(error) => {
                away |= machine_away(&error);
                tracing::debug!(%error, "the AFK+ approvals are not readable now");
                Vec::new()
            }
        };
        approvals.sort_by(|a, b| {
            b.is_pending()
                .cmp(&a.is_pending())
                .then(b.created_at.cmp(&a.created_at))
        });
        Ok(PlusPanelView {
            settings,
            quota,
            approvals,
            machine_away: away,
        })
    }

    /// The settings as stored now.
    pub async fn plus_settings(&self, token: &Bearer, id: &str) -> Result<PlusSettings> {
        let settings = self.inner.client.plus_settings(token, id).await?;
        self.cache().remember(id, &settings);
        Ok(settings)
    }

    /// The settings the checks run against: as read, or read now.
    async fn known_settings(&self, token: &Bearer, id: &str) -> Result<PlusSettings> {
        match self.cache().settings(id) {
            Some(settings) => Ok(settings),
            None => self.plus_settings(token, id).await,
        }
    }

    /// The settings an order write goes from, with the launcher's check of it. A refusal on the
    /// count of active orders reads them again once first: the orders' states here are those read
    /// when the panel opened, and one may have finished since (the live stream tells the page).
    async fn checked_write(
        &self,
        token: &Bearer,
        id: &str,
        check: impl Fn(&PlusSettings) -> std::result::Result<(), Vec<PlusIssue>>,
    ) -> Result<std::result::Result<PlusSettings, Vec<PlusIssue>>> {
        let settings = self.known_settings(token, id).await?;
        match check(&settings) {
            Ok(()) => Ok(Ok(settings)),
            Err(issues) if issues.iter().any(is_active_cap) => {
                let fresh = self.plus_settings(token, id).await?;
                Ok(check(&fresh).map(|()| fresh))
            }
            Err(issues) => Ok(Err(issues)),
        }
    }

    /// Saves the settings (`rev`: the revision the panel started from). The launcher's checks
    /// come first; a change made elsewhere meanwhile comes back as `Conflict`.
    pub async fn plus_save(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        config: PlusConfig,
    ) -> Result<PlusSaveOutcome> {
        let settings = self.known_settings(token, id).await?;
        let config = match checked_settings(&config, &settings, unix_now()) {
            Ok(config) => config,
            Err(issues) => return Ok(PlusSaveOutcome::Invalid { issues }),
        };
        let answer = self.inner.client.put_plus(token, id, rev, &config).await;
        self.settings_written(token, id, answer).await
    }

    /// "Bu hale dön" (QoL Q7): the revision `rev`'s settings become the next revision; `current`
    /// is the revision on screen. The orders and custom items stay as they are.
    pub async fn plus_restore(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        current: u64,
    ) -> Result<PlusSaveOutcome> {
        let answer = self
            .inner
            .client
            .plus_restore(token, id, rev, current)
            .await;
        self.settings_written(token, id, answer).await
    }

    async fn settings_written(
        &self,
        token: &Bearer,
        id: &str,
        answer: Result<PlusWritten>,
    ) -> Result<PlusSaveOutcome> {
        match answer {
            Ok(written) => {
                let mut settings =
                    self.cache()
                        .written(id, written.config, written.ai_rev, written.server);
                settings.judge = written.judge;
                self.cache().remember(id, &settings);
                Ok(PlusSaveOutcome::Saved { settings })
            }
            Err(error) => match refusal_issues(&error) {
                Some(issues) => Ok(PlusSaveOutcome::Invalid { issues }),
                None if error.code() == "rev_conflict" => Ok(PlusSaveOutcome::Conflict {
                    settings: self.plus_settings(token, id).await?,
                }),
                None => Err(error),
            },
        }
    }

    /// "Kontrol et": the player's words become an order's card (nothing is stored). The free
    /// rules run at the service (the panel already hinted them); a text outside 3-200 characters
    /// never leaves. `in_game`: the player said the words' money is the game's ("Bu oyun parası").
    pub async fn plus_check(
        &self,
        token: &Bearer,
        id: &str,
        text: &str,
        lang: &str,
        in_game: bool,
    ) -> Result<PlusCheckOutcome> {
        let cleaned = clean_text(text).replace('\n', " ");
        let length = cleaned.chars().count();
        if !(MIN_ORDER_TEXT..=MAX_ORDER_TEXT).contains(&length) {
            return Ok(PlusCheckOutcome::Rejected {
                reason: "length".into(),
                rule: None,
            });
        }
        check_outcome(
            self.inner
                .client
                .check_order(token, id, &cleaned, lang, in_game)
                .await,
        )
    }

    /// A new order ("Kaydet"), or a deleted one put back ("Geri al": with its id and time).
    pub async fn plus_order_create(
        &self,
        token: &Bearer,
        id: &str,
        order: PlusNewOrder,
        custom_items: Vec<PlusNewCustomItem>,
        choice: PlusOrderSettings,
    ) -> Result<PlusOrderOutcome> {
        let checked = self
            .checked_write(token, id, |settings| {
                checked_new_order(settings, &order, &custom_items, &choice, unix_now())
            })
            .await?;
        let settings = match checked {
            Ok(settings) => settings,
            Err(issues) => return Ok(PlusOrderOutcome::Invalid { issues }),
        };
        let client = &self.inner.client;
        let mut rev = settings.ai_rev;
        for attempt in 0..2 {
            match client
                .create_order(token, id, rev, &order, &custom_items, &choice)
                .await
            {
                Err(error) if attempt == 0 && error.code() == "rev_conflict" => {
                    rev = self.fresh_rev(token, id, &error).await?;
                }
                answer => return self.order_written(id, answer),
            }
        }
        unreachable!("the loop returns on its second pass")
    }

    /// A change of an order: "Düzenle", "Durdur" (`on: false`), "Sürdür", "Süreyi uzat".
    pub async fn plus_order_patch(
        &self,
        token: &Bearer,
        id: &str,
        order: &str,
        patch: PlusOrderPatch,
    ) -> Result<PlusOrderOutcome> {
        let checked = self
            .checked_write(token, id, |settings| {
                checked_patch(settings, order, &patch, unix_now())
            })
            .await?;
        let settings = match checked {
            Ok(settings) => settings,
            Err(issues) => return Ok(PlusOrderOutcome::Invalid { issues }),
        };
        let client = &self.inner.client;
        let mut rev = settings.ai_rev;
        for attempt in 0..2 {
            match client.patch_order(token, id, rev, order, &patch).await {
                Err(error) if attempt == 0 && error.code() == "rev_conflict" => {
                    rev = self.fresh_rev(token, id, &error).await?;
                }
                answer => return self.order_written(id, answer),
            }
        }
        unreachable!("the loop returns on its second pass")
    }

    /// "Sil": at once, or once its trade ended (`deferred`).
    pub async fn plus_order_delete(
        &self,
        token: &Bearer,
        id: &str,
        order: &str,
    ) -> Result<PlusOrderOutcome> {
        let settings = self.known_settings(token, id).await?;
        let client = &self.inner.client;
        let mut rev = settings.ai_rev;
        for attempt in 0..2 {
            match client.delete_order(token, id, rev, order).await {
                Err(error) if attempt == 0 && error.code() == "rev_conflict" => {
                    rev = self.fresh_rev(token, id, &error).await?;
                }
                answer => return self.order_written(id, answer),
            }
        }
        unreachable!("the loop returns on its second pass")
    }

    /// The revision a conflict names (the service sends it), else read again.
    async fn fresh_rev(&self, token: &Bearer, id: &str, error: &CloudError) -> Result<u64> {
        match error.extra("aiRev").and_then(Value::as_u64) {
            Some(rev) => Ok(rev),
            None => Ok(self.plus_settings(token, id).await?.ai_rev),
        }
    }

    fn order_written(
        &self,
        id: &str,
        answer: Result<PlusOrderWritten>,
    ) -> Result<PlusOrderOutcome> {
        match answer {
            Ok(written) => {
                let settings = self
                    .cache()
                    .written(id, written.config, written.ai_rev, None);
                Ok(PlusOrderOutcome::Saved {
                    settings: Box::new(settings),
                    order: written.order.map(Box::new),
                    deferred: written.deferred,
                })
            }
            Err(error) => {
                if let Some(issues) = refusal_issues(&error) {
                    return Ok(PlusOrderOutcome::Invalid {
                        issues: issues
                            .into_iter()
                            .map(|issue| on_the_form(issue, None, 0))
                            .collect(),
                    });
                }
                match error.code() {
                    "order_not_found" => Ok(PlusOrderOutcome::Gone),
                    "rate_limited" => Ok(PlusOrderOutcome::Limited {
                        daily: error.reason() == Some("daily_writes"),
                        retry_after: error.retry_after(),
                    }),
                    _ => Err(error),
                }
            }
        }
    }

    /// "Envanterden seç": the item in a slot of the bot's inventory, as an order's item.
    pub async fn plus_pick(
        &self,
        token: &Bearer,
        id: &str,
        slot: u32,
        item: &str,
        cmd: Option<i64>,
        model: Option<&str>,
    ) -> Result<PlusPickOutcome> {
        pick_outcome(
            self.inner
                .client
                .pick_item(token, id, slot, item, cmd, model)
                .await,
        )
    }

    /// "Yapay zekâyı durdur" / "başlat", a player blocked, unblocked or forgotten, "Harcananı
    /// sıfırla". The account's AI state goes into what the page shows.
    pub async fn plus_control(
        &self,
        token: &Bearer,
        id: &str,
        action: PlusAction,
        name: Option<&str>,
    ) -> Result<PlusControlAnswer> {
        let answer = self
            .inner
            .client
            .plus_control(token, id, action, name)
            .await?;
        if let Some(ai) = &answer.ai
            && let Some(account) = self.inner.known.lock().accounts.get_mut(id)
        {
            account.ai = Some(ai.clone());
        }
        if let Some(mut settings) = self.cache().settings(id) {
            let config = &mut settings.config;
            match action {
                PlusAction::Pause => config.enabled = false,
                // "Başlat" also ends the wait for the first start (the service drops `ready`).
                PlusAction::Resume => {
                    config.enabled = true;
                    config.ready = None;
                }
                PlusAction::Block | PlusAction::Unblock => {
                    if let Some(name) = name {
                        config.blocked.retain(|n| !n.eq_ignore_ascii_case(name));
                        if action == PlusAction::Block {
                            config.blocked.push(name.to_owned());
                        }
                    }
                }
                PlusAction::Forget | PlusAction::ResetSpent => {}
            }
            settings.ai_rev = settings.ai_rev.max(answer.ai_rev);
            self.cache().remember(id, &settings);
        }
        Ok(answer)
    }

    /// A page of the audit log ("Akış"): from the machine, kept here as it comes; while the
    /// machine cannot be asked, what this computer kept.
    pub async fn plus_audit(
        &self,
        token: &Bearer,
        id: &str,
        cache_root: &Path,
        before: Option<i64>,
        after: Option<i64>,
        group: Option<&str>,
    ) -> Result<PlusAuditOutcome> {
        let answer = self
            .inner
            .client
            .plus_audit(token, id, before, after, Some(50), group)
            .await;
        match answer {
            Ok(page) => {
                let dir = plus_audit::audit_dir(cache_root);
                let account = id.to_owned();
                let items = page.items.clone();
                let today = plus_audit::plus_day(unix_now());
                let _ = tokio::task::spawn_blocking(move || {
                    plus_audit::keep(&dir, &account, &items, today)
                })
                .await;
                Ok(PlusAuditOutcome::Page {
                    items: page.items,
                    more: page.more,
                    from: page.from,
                })
            }
            Err(error) if machine_away(&error) => Ok(PlusAuditOutcome::Kept {
                items: self.plus_audit_kept(cache_root, id).await,
            }),
            Err(error) => match error.code() {
                "feature_off" => Ok(PlusAuditOutcome::Off),
                "rate_limited" => Ok(PlusAuditOutcome::Limited {
                    retry_after: error.retry_after(),
                }),
                _ => Err(error),
            },
        }
    }

    /// Keeps entries the live stream brought (the page shows them).
    pub async fn plus_audit_keep(&self, cache_root: &Path, id: &str, items: Vec<PlusAuditEntry>) {
        let dir = plus_audit::audit_dir(cache_root);
        let account = id.to_owned();
        let today = plus_audit::plus_day(unix_now());
        let _ =
            tokio::task::spawn_blocking(move || plus_audit::keep(&dir, &account, &items, today))
                .await;
    }

    /// What this computer kept of an account's audit log, newest first.
    pub async fn plus_audit_kept(&self, cache_root: &Path, id: &str) -> Vec<PlusAuditEntry> {
        let dir = plus_audit::audit_dir(cache_root);
        let account = id.to_owned();
        let today = plus_audit::plus_day(unix_now());
        tokio::task::spawn_blocking(move || plus_audit::read(&dir, &account, today))
            .await
            .unwrap_or_default()
    }

    /// Drops what this computer kept of the audit logs of accounts that are not AFK+ any more (or
    /// of every account: a sign-out).
    pub async fn plus_audit_prune(&self, cache_root: &Path, keep: Vec<String>) {
        let dir = plus_audit::audit_dir(cache_root);
        let _ = tokio::task::spawn_blocking(move || {
            let names: Vec<&str> = keep.iter().map(String::as_str).collect();
            plus_audit::forget(&dir, &names);
        })
        .await;
    }

    /// The game's item data for a Minecraft version (the account's; None: the newest), from our
    /// CDN with its SHA-256 checked, cached under `cache_root`.
    pub async fn plus_item_data(
        &self,
        cache_root: &Path,
        minecraft: Option<&str>,
    ) -> Option<PlusItemData> {
        let client = self.inner.client.clone();
        let get = move |url: String| {
            let client = client.clone();
            async move {
                client
                    .fetch_public(&url, pl_items::data::MAX_DATA_BYTES)
                    .await
            }
        };
        self.cache()
            .items
            .load(&item_data_dir(cache_root), minecraft, get)
            .await
    }

    /// An item's names (Turkish, English) without asking anything: a custom item by the account's
    /// settings as read, a game item by the item data read this session.
    pub fn plus_item_names(&self, acct: Option<&str>, item: &str) -> Option<(String, String)> {
        if item.starts_with("custom:") {
            let settings = self.cache().settings(acct?)?;
            let custom = settings.config.custom_item(item)?;
            return Some((custom.name.clone(), custom.name.clone()));
        }
        self.cache().items.item_names(item)
    }

    /// The Minecraft version of an account as last read (its item data's version).
    pub fn plus_minecraft(&self, acct: &str) -> Option<String> {
        self.cache().settings(acct)?.server.mc
    }

    pub async fn plus_receipts(
        &self,
        token: &Bearer,
        id: &str,
        before: Option<&str>,
    ) -> Result<PlusReceiptPage> {
        self.inner.client.plus_receipts(token, id, before).await
    }

    pub async fn plus_approvals(&self, token: &Bearer, id: &str) -> Result<Vec<PlusApproval>> {
        Ok(self.inner.client.plus_approvals(token, id).await?.approvals)
    }

    /// "Onayla", "Reddet", "Fiyatı düşür" (`approve` with a lower `max_price`), "Takas oldu" /
    /// "Olmadı", "Evet, bu" / "Hayır".
    pub async fn plus_answer(
        &self,
        token: &Bearer,
        id: &str,
        approval: &str,
        approve: bool,
        max_price: Option<u64>,
    ) -> Result<PlusApproval> {
        self.inner
            .client
            .answer_approval(token, id, approval, approve, max_price)
            .await
    }

    pub async fn plus_quota(&self, token: &Bearer, id: &str) -> Result<PlusQuota> {
        self.inner.client.plus_quota(token, id).await
    }

    pub async fn plus_inbox(
        &self,
        token: &Bearer,
        id: &str,
        after: Option<&str>,
    ) -> Result<PlusInboxPage> {
        self.inner.client.plus_inbox(token, id, after).await
    }

    pub async fn plus_thread(
        &self,
        token: &Bearer,
        id: &str,
        name: &str,
        after: Option<&str>,
    ) -> Result<PlusThreadPage> {
        self.inner.client.plus_thread(token, id, name, after).await
    }

    /// The owner's own words to a player in game ("Mesajlar"): checked by the service, never
    /// repeated.
    pub async fn plus_reply(
        &self,
        token: &Bearer,
        id: &str,
        name: &str,
        text: &str,
    ) -> Result<PlusInboxLine> {
        let text = text.trim();
        if text.is_empty() || text.chars().count() > MAX_REPLY {
            return Err(CloudError::Invalid("reply"));
        }
        self.inner.client.plus_reply(token, id, name, text).await
    }

    pub async fn plus_read(&self, token: &Bearer, id: &str, name: &str, up_to: i64) -> Result<()> {
        self.inner.client.plus_read(token, id, name, up_to).await
    }

    pub async fn plus_hand_back(&self, token: &Bearer, id: &str, name: &str) -> Result<()> {
        self.inner.client.plus_hand_back(token, id, name).await
    }

    pub async fn plus_history(&self, token: &Bearer, id: &str) -> Result<PlusHistory> {
        self.inner.client.plus_history(token, id).await
    }

    pub async fn plus_receipt_summary(
        &self,
        token: &Bearer,
        id: &str,
        from: Option<i64>,
        to: Option<i64>,
    ) -> Result<PlusReceiptSummary> {
        self.inner
            .client
            .plus_receipt_summary(token, id, from, to)
            .await
    }

    pub async fn plus_receipts_csv(
        &self,
        token: &Bearer,
        id: &str,
        from: Option<i64>,
        to: Option<i64>,
        lang: &str,
    ) -> Result<Vec<u8>> {
        self.inner
            .client
            .plus_receipts_csv(token, id, from, to, lang)
            .await
    }

    pub async fn plus_weekly_report(
        &self,
        token: &Bearer,
        week: Option<&str>,
    ) -> Result<PlusWeeklyReport> {
        self.inner.client.plus_weekly_report(token, week).await
    }

    pub async fn notify_prefs(&self, token: &Bearer) -> Result<CloudNotifyPrefs> {
        self.inner.client.notify_prefs(token).await
    }

    pub async fn set_notify_prefs(
        &self,
        token: &Bearer,
        prefs: &CloudNotifyPrefs,
    ) -> Result<CloudNotifyPrefs> {
        self.inner.client.put_notify_prefs(token, prefs).await
    }

    /// The AFK+ panel is on screen: the account's AI state, audit log entries, orders' progress
    /// and approvals stream as live events (and its console with them, the same subscription).
    pub async fn watch_ai(&self, token: &Bearer, id: &str) -> Result<()> {
        let account = self.account(token, id).await?;
        let Some(host) = account.host.clone() else {
            return Err(CloudError::Invalid("no machine yet"));
        };
        let url = self.inner.client.live_url(&host);
        self.inner.live.watch_ai(url, token.clone(), &host, id);
        Ok(())
    }

    pub fn unwatch_ai(&self, id: &str) {
        self.inner.live.unwatch_ai(id);
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use serde_json::json;
    use tokio::runtime::Handle;

    use super::*;
    use crate::client::{CloudConfig, api_url};
    use crate::copies::Copies;
    use crate::error::from_response;
    use crate::live::LiveSink;
    use crate::plus::{PlusReq, PlusServer};

    fn offline_service(dir: &Path) -> CloudService {
        let store = Arc::new(pl_afk::MemorySecretStore::new());
        let copies = Copies::new(store, dir.join("copies.json"));
        let config = CloudConfig::new(api_url(Some("http://127.0.0.1:9")), "test");
        let sink: LiveSink = Arc::new(|_| {});
        CloudService::new(
            reqwest::Client::new(),
            config,
            copies,
            Handle::current(),
            sink,
        )
    }

    const NOW: i64 = 1_792_900_000;

    fn settings() -> PlusSettings {
        let mut config = PlusConfig {
            mode: "projects-smp".into(),
            ..PlusConfig::default()
        };
        config.limits.approval_above = Some(50_000);
        config.orders.push(PlusOrder {
            id: "o1".into(),
            item: "minecraft:elytra".into(),
            target: 32_000,
            limit: 40_000,
            until: NOW + 14 * 86_400,
            at: NOW - 100,
            ..PlusOrder::default()
        });
        PlusSettings {
            config,
            ai_rev: 3,
            server: PlusServer {
                kind: "verified".into(),
                mode: "projects-smp".into(),
                trade: "bot".into(),
                money: crate::plus::PlusServerMoney {
                    label: None,
                    fixed: false,
                    max_deal: 10_000_000,
                    currency: "smp".into(),
                },
                on: true,
                ..PlusServer::default()
            },
            ..PlusSettings::default()
        }
    }

    fn new_order() -> PlusNewOrder {
        PlusNewOrder {
            id: None,
            at: None,
            on: true,
            side: "buy".into(),
            item: "minecraft:diamond".into(),
            qty: 64,
            req: Some(PlusReq {
                dur: 0,
                ..PlusReq::default()
            }),
            pay: "money".into(),
            unit: "total".into(),
            target: 7_000,
            limit: 8_000,
            style: "balanced".into(),
            trade: "auto".into(),
            until: NOW + 14 * 86_400,
            note: "64 elmas alırım".into(),
            src: "text".into(),
        }
    }

    #[test]
    fn settings_are_checked_as_the_service_checks_them() {
        let settings = settings();
        let mut config = settings.config.clone();
        config.pause_until = Some(NOW - 5);
        config.persona.intro = "  Ahmet'in asistanı  ".into();
        let sent = checked_settings(&config, &settings, NOW).unwrap();
        assert_eq!(sent.pause_until, None, "a break already over is no break");
        assert_eq!(sent.persona.intro, "Ahmet'in asistanı");
        assert_eq!(
            sent.orders, settings.config.orders,
            "the orders stay as stored"
        );
        config.limits.per_deal_max = Some(10_000);
        config.limits.approval_above = Some(20_000);
        assert_eq!(
            checked_settings(&config, &settings, NOW),
            Err(vec![PlusIssue::new("limits.approvalAbove", "limits")])
        );
        let mut config = settings.config.clone();
        config.who.mode = "friends".into();
        config.who.friends = vec!["Kaan_07".into()];
        config.blocked = vec!["kaan_07".into()];
        assert_eq!(
            checked_settings(&config, &settings, NOW),
            Err(vec![PlusIssue::new("who.friends[0]", "blocked")])
        );
    }

    #[test]
    fn the_money_name_goes_as_the_player_left_it() {
        let mut settings = settings();
        settings.rules.features.currency = true;
        settings.config.trade.currency = Some(Some("TL".into()));
        let sent = |config: &PlusConfig| {
            let checked = checked_settings(config, &settings, NOW).unwrap();
            checked.trade.currency
        };
        // Left out (a screen that does not show it): left out of the write, the stored one stays.
        let mut config = settings.config.clone();
        config.trade.currency = None;
        assert_eq!(sent(&config), None);
        config.trade.currency = Some(Some("  Altın  Para ".into()));
        assert_eq!(sent(&config), Some(Some("Altın Para".into())));
        config.trade.currency = Some(None);
        assert_eq!(sent(&config), Some(None), "a cleared name goes as null");
        config.trade.currency = Some(Some("TL2".into()));
        assert_eq!(
            checked_settings(&config, &settings, NOW),
            Err(vec![PlusIssue::text("trade.currency", "form")])
        );
        // Studio's switch off (an older service reads so too): the stored name may go back, a new
        // one is refused.
        let mut off = settings.clone();
        off.rules.features.currency = false;
        assert_eq!(
            checked_settings(&off.config, &off, NOW).map(|checked| checked.trade.currency),
            Ok(Some(Some("TL".into())))
        );
        let mut config = off.config.clone();
        config.trade.currency = Some(Some("coin".into()));
        assert_eq!(
            checked_settings(&config, &off, NOW),
            Err(vec![PlusIssue::new("trade.currency", "feature")])
        );
    }

    #[test]
    fn a_save_keeps_the_ready_mark_while_the_assistant_waits() {
        let mut settings = settings();
        settings.config.enabled = false;
        settings.config.ready = Some(true);
        let sent = checked_settings(&settings.config, &settings, NOW).unwrap();
        assert_eq!(sent.ready, Some(true));
        // A save that turns the assistant on starts it: the mark ends.
        let mut on = settings.config.clone();
        on.enabled = true;
        assert_eq!(checked_settings(&on, &settings, NOW).unwrap().ready, None);
    }

    #[test]
    fn orders_are_checked_on_the_forms_fields() {
        let settings = settings();
        let none = PlusOrderSettings::default();
        assert_eq!(
            checked_new_order(&settings, &new_order(), &[], &none, NOW),
            Ok(())
        );
        let wrong = PlusNewOrder {
            target: 9_000,
            ..new_order()
        };
        assert_eq!(
            checked_new_order(&settings, &wrong, &[], &none, NOW),
            Err(vec![PlusIssue::new("order.limit", "order")])
        );
        // A swap while the setting is money only, unless the chip turns it on in the same write.
        let swap = PlusNewOrder {
            pay: "minecraft:diamond_block".into(),
            target: 2,
            limit: 3,
            ..new_order()
        };
        // Studio's swaps switch off: no swap at all.
        assert_eq!(
            checked_new_order(&settings, &swap, &[], &none, NOW),
            Err(vec![PlusIssue::new("order.pay", "feature")])
        );
        let mut open = settings.clone();
        open.rules.features.item_pay = true;
        assert_eq!(
            checked_new_order(&open, &swap, &[], &none, NOW),
            Err(vec![PlusIssue::new("order.pay", "pay")])
        );
        let turn_on = PlusOrderSettings {
            pay: Some("both".into()),
            per_deal_max: None,
        };
        assert_eq!(checked_new_order(&open, &swap, &[], &turn_on, NOW), Ok(()));
        // A typed custom item without its fingerprint, and a name the rule refuses.
        let custom = PlusNewOrder {
            item: "custom:c1".into(),
            ..new_order()
        };
        let ruby = PlusNewCustomItem {
            k: "c1".into(),
            name: "Yakut".into(),
            base: None,
            fp: None,
            look: None,
            from: "text".into(),
        };
        assert_eq!(
            checked_new_order(&settings, &custom, std::slice::from_ref(&ruby), &none, NOW),
            Ok(())
        );
        let marked = PlusNewCustomItem {
            name: "[AFK+] Yakut".into(),
            ..ruby
        };
        assert_eq!(
            checked_new_order(&settings, &custom, &[marked], &none, NOW),
            Err(vec![PlusIssue::text("customItem.name", "marker")])
        );
        // Ten active orders already.
        let mut full = settings.clone();
        for n in 2..=10 {
            let mut order = full.config.orders[0].clone();
            order.id = format!("o{n}");
            full.config.orders.push(order);
        }
        let issues = checked_new_order(&full, &new_order(), &[], &none, NOW).unwrap_err();
        assert_eq!(issues[0].reason, "limit_orders");
        // A change: a buy's target past its limit.
        let patch = PlusOrderPatch {
            target: Some(50_000),
            ..PlusOrderPatch::default()
        };
        assert_eq!(
            checked_patch(&settings, "o1", &patch, NOW),
            Err(vec![PlusIssue::new("order.limit", "order")])
        );
        let pause = PlusOrderPatch {
            on: Some(false),
            ..PlusOrderPatch::default()
        };
        assert_eq!(checked_patch(&settings, "o1", &pause, NOW), Ok(()));
    }

    fn refusal<T>(status: u16, body: serde_json::Value) -> Result<T> {
        Err(from_response(status, None, body.to_string().as_bytes()))
    }

    #[test]
    fn answers_become_outcomes() {
        assert_eq!(
            check_outcome(Ok("{\"draft\":{}}".into())).unwrap(),
            PlusCheckOutcome::Draft {
                answer: "{\"draft\":{}}".into()
            }
        );
        assert_eq!(
            check_outcome(refusal(
                422,
                json!({"error": {"code": "order_rejected", "reason": "loosen", "rule": "prepay"}})
            ))
            .unwrap(),
            PlusCheckOutcome::Rejected {
                reason: "loosen".into(),
                rule: Some("prepay".into())
            }
        );
        // Money that may be the game's is asked (28 Oct 2026): the panel offers "Bu oyun parası".
        assert_eq!(
            check_outcome(refusal(
                422,
                json!({"error": {"code": "order_rejected", "reason": "question", "rule": "rmt"}})
            ))
            .unwrap(),
            PlusCheckOutcome::Rejected {
                reason: "question".into(),
                rule: Some("rmt".into())
            }
        );
        assert_eq!(
            check_outcome(refusal(
                429,
                json!({"error": {"code": "rate_limited", "reason": "daily", "retryAfter": 600}})
            ))
            .unwrap(),
            PlusCheckOutcome::Limited {
                reason: "daily".into(),
                retry_after: Some(600)
            }
        );
        assert_eq!(
            check_outcome(refusal(
                503,
                json!({"error": {"code": "ai_unavailable", "reason": "budget"}})
            ))
            .unwrap(),
            PlusCheckOutcome::Unavailable {
                reason: "budget".into()
            }
        );
        assert_eq!(
            check_outcome(refusal(503, json!({"error": {"code": "host_offline"}}))).unwrap(),
            PlusCheckOutcome::Offline
        );
        assert_eq!(
            check_outcome(refusal(
                409,
                json!({"error": {"code": "feature_off", "feature": "orders"}})
            ))
            .unwrap(),
            PlusCheckOutcome::Off
        );
        assert!(check_outcome(refusal(409, json!({"error": {"code": "plus_off"}}))).is_err());
        assert_eq!(
            pick_outcome(refusal(
                422,
                json!({"error": {"code": "pick_failed", "reason": "stale"}})
            ))
            .unwrap(),
            PlusPickOutcome::Failed {
                reason: "stale".into()
            }
        );
        assert_eq!(
            on_the_form(
                PlusIssue::new("orders[12].req.ench.minecraft:sharpness", "range"),
                Some(12),
                3
            )
            .field,
            "order.req.ench.minecraft:sharpness"
        );
        // Another order's refusal (a stored one) is not the written order's.
        assert_eq!(
            on_the_form(PlusIssue::new("orders[2].at", "range"), Some(12), 3).field,
            "orders[2].at"
        );
        assert_eq!(
            on_the_form(PlusIssue::text("customItems[3].name", "link"), Some(12), 3).field,
            "customItem.name"
        );
        assert_eq!(
            on_the_form(PlusIssue::text("customItems[0].name", "link"), Some(12), 3).field,
            "customItems[0].name"
        );
        assert_eq!(
            on_the_form(PlusIssue::new("pay", "pay"), Some(12), 3).field,
            "pay"
        );
        // The service's answer to a write: any order's refusal is the written one's.
        assert_eq!(
            on_the_form(PlusIssue::new("orders[4].limit", "order"), None, 0).field,
            "order.limit"
        );
    }

    #[test]
    fn a_clock_behind_the_service_still_writes_orders() {
        // The stored order was stamped by the service an hour ahead of this computer's clock.
        let mut settings = settings();
        settings.config.orders[0].at = NOW + 3_600;
        assert_eq!(checks_now(&settings, NOW), NOW + 3_600);
        let none = PlusOrderSettings::default();
        assert_eq!(
            checked_new_order(&settings, &new_order(), &[], &none, NOW),
            Ok(())
        );
    }

    #[tokio::test]
    async fn texts_out_of_bounds_never_leave() {
        let dir = tempfile::tempdir().unwrap();
        let service = offline_service(dir.path());
        let token = Bearer::new("t");
        let long = "a".repeat(MAX_REPLY + 1);
        assert!(matches!(
            service.plus_reply(&token, "a_1", "Kaan_07", &long).await,
            Err(CloudError::Invalid("reply"))
        ));
        assert!(matches!(
            service.plus_reply(&token, "a_1", "Kaan_07", "   ").await,
            Err(CloudError::Invalid("reply"))
        ));
        for text in ["  a ", &"x".repeat(MAX_ORDER_TEXT + 1)] {
            assert_eq!(
                service
                    .plus_check(&token, "a_1", text, "tr", false)
                    .await
                    .unwrap(),
                PlusCheckOutcome::Rejected {
                    reason: "length".into(),
                    rule: None
                }
            );
        }
    }

    #[tokio::test]
    async fn names_custom_items_by_the_settings_read() {
        let dir = tempfile::tempdir().unwrap();
        let service = offline_service(dir.path());
        let mut known = settings();
        known.config.custom_items.push(crate::plus::PlusCustomItem {
            k: "c1".into(),
            name: "Yakut".into(),
            from: "text".into(),
            ..Default::default()
        });
        known.server.mc = Some("26.3".into());
        service.inner.plus.remember("a_1", &known);
        assert_eq!(
            service.plus_item_names(Some("a_1"), "custom:c1"),
            Some(("Yakut".into(), "Yakut".into()))
        );
        assert_eq!(service.plus_item_names(Some("a_2"), "custom:c1"), None);
        assert_eq!(service.plus_item_names(None, "minecraft:elytra"), None);
        assert_eq!(service.plus_minecraft("a_1").as_deref(), Some("26.3"));
        service.forget().await;
        assert_eq!(service.plus_minecraft("a_1"), None);
    }

    #[tokio::test]
    async fn a_save_shows_the_server_view_it_answers_with() {
        let dir = tempfile::tempdir().unwrap();
        let service = offline_service(dir.path());
        let token = Bearer::new("t");
        let read = settings();
        service.inner.plus.remember("a_1", &read);
        let label = |settings: &PlusSettings| settings.server.money.label.clone().map(|l| l.tr);
        // An older service answers without it: the view read stays.
        let answer = json!({"config": read.config, "aiRev": 4});
        let written: PlusWritten = serde_json::from_value(answer).unwrap();
        assert_eq!(written.server, None);
        let outcome = service.settings_written(&token, "a_1", Ok(written)).await;
        let Ok(PlusSaveOutcome::Saved { settings }) = outcome else {
            panic!("not saved");
        };
        assert_eq!(settings.server, read.server);
        // A save's answer names the money as the service resolved it ("Para birimi"): it shows at
        // once, never worked out here.
        let mut server = serde_json::to_value(&read.server).unwrap();
        server["money"]["label"] = json!({"tr": "TL", "en": "TL"});
        let answer = json!({"config": read.config, "aiRev": 5, "server": server});
        let written: PlusWritten = serde_json::from_value(answer).unwrap();
        let outcome = service.settings_written(&token, "a_1", Ok(written)).await;
        let Ok(PlusSaveOutcome::Saved { settings }) = outcome else {
            panic!("not saved");
        };
        assert_eq!(label(&settings).as_deref(), Some("TL"));
        assert_eq!(settings.rules, read.rules, "Studio's rules stay as read");
        // An order write's answer has none: the view known stays.
        let answer = json!({"config": read.config, "aiRev": 6});
        let written: PlusOrderWritten = serde_json::from_value(answer).unwrap();
        let outcome = service.order_written("a_1", Ok(written));
        let Ok(PlusOrderOutcome::Saved { settings, .. }) = outcome else {
            panic!("not saved");
        };
        assert_eq!(label(&settings).as_deref(), Some("TL"));
    }
}
