//! AFK+, the AI tier of a cloud account (`services/cloud-afk/README.md` in the Studio repository,
//! "AFK+": the tier, the player's settings `AfkPlusConfig` v1, the player API, the live socket's AI
//! messages, quotas and notifications; the reasons are in the maintainers' notes).
//!
//! The launcher shows and edits; the Worker and the account's machine decide. Nothing of AFK+ shows
//! while the service's `plusMode` is `off`: `Offer.plus` is null then and every AFK+ write answers
//! `409 plus_off`. Purchases, upgrades and plan changes happen on the site.
//!
//! - Objects: [`PlusOffer`], [`PlusAi`] (an account's AI state), [`PlusConfig`] and its rows,
//!   [`PlusSettings`], [`PlusQuota`], [`PlusApproval`], [`PlusFeedItem`], [`PlusDeal`], the QoL v1
//!   objects (messages, typed rules, the settings history, the trade summary and the weekly
//!   report) and [`CloudNotifyPrefs`].
//! - [`check_config`]: the rules the launcher checks before a `PUT /plus`, as issues on the
//!   contract's field paths (`buy[2].max`), the paths the Worker's refusals name too.
//! - The AFK+ routes of [`CloudClient`] (this module's `impl`).
//!
//! As everywhere in this crate, answers may gain fields (they are ignored) and values the launcher
//! does not know yet are kept as text. Amounts are whole numbers of the game mode's money; times
//! are Unix seconds.

use std::collections::{BTreeMap, HashSet};

use reqwest::Method;
use reqwest::header::IF_MATCH;
use serde::{Deserialize, Serialize};

use crate::client::{Bearer, CloudClient, Repeat, account_path};
use crate::error::{CloudError, Result};
use crate::model::{CloudLocalized, CloudPlanPrice, CloudStock};

// --- the tier and the AI's state ------------------------------------------------------------------

/// `Offer.plus`: AFK+ as sold now. Null (None) while `plusMode` is `off`: then nothing of AFK+ shows.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusOffer {
    /// `invite`, `on` or `paused`.
    pub mode: String,
    /// Purchases and upgrades open (an invite list may still apply: [`MePlus`]).
    pub sales: bool,
    pub prices: Vec<CloudPlanPrice>,
    pub stock: CloudStock,
    pub refund_window_hours: u32,
    pub terms_version: u32,
    pub allowance: PlusAllowance,
    /// Studio's QoL switches: a feature that is off is not shown.
    pub features: PlusFeatures,
}

impl PlusOffer {
    /// The monthly AFK+ price, the intro's "Aylık 200 TL".
    pub fn month_price(&self) -> Option<&CloudPlanPrice> {
        self.prices
            .iter()
            .find(|price| price.plan == "month")
            .or_else(|| self.prices.first())
    }
}

/// The two quota windows' caps (credits) per plan.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusAllowance {
    pub month: PlusWindowCaps,
    pub year: PlusWindowCaps,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusWindowCaps {
    pub w5h: u64,
    pub week: u64,
}

/// `Offer.plus.features` (README "AFK+ player API" → QoL v1): what to show. A switch an older
/// service does not name reads as off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusFeatures {
    /// "Mesajlar" (Q1).
    pub inbox: bool,
    /// "Yazarak ekle" (Q2).
    pub parse: bool,
    /// "Çalışma saatleri", breaks and ready statuses (Q3).
    pub schedule: bool,
    /// The weekly report and the trade history (Q4).
    pub report: bool,
    /// "Ayar geçmişi" (Q7).
    pub history: bool,
    /// "Hakkı koru" (Q9).
    pub saver: bool,
    pub notify_digest: bool,
    pub handoff: bool,
    pub watch: bool,
}

/// `Me.plus`: whether this player may buy AFK+ now, and AFK+'s own restriction of them.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct MePlus {
    pub can_buy: PlusCanBuy,
    pub restricted: Option<PlusRestriction>,
}

/// `{ok: true}` or `{ok: false, reason, until?}` (`plus_off`, `sales_closed`, `plus_invite_only`,
/// `plus_restricted`, `plus_account_limit`, `plus_sold_out`; kept as text).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusCanBuy {
    pub ok: bool,
    pub reason: Option<String>,
    pub until: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusRestriction {
    pub until: Option<i64>,
    /// A reason key (README "Reasons and notes").
    pub reason: String,
}

/// `Account.ai` (`AiView`): the assistant's state now. `state` is `on`, `paused` (comes back by
/// itself), `off` (someone has to act) or `unknown`; `reason` with `paused` and `off`
/// (`restricted`, `staff`, `service`, `server`, `player`, `break`, `schedule`, `gate`, `muted`,
/// `host`, `budget`, `outage`, `quota`). Both kept as text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusAi {
    pub state: String,
    pub reason: Option<String>,
    /// With `staff`: a reason key.
    pub staff_reason: Option<String>,
    /// `paused`: when it comes back by itself, if known; `off`: a restriction's end.
    pub until: Option<i64>,
    pub trade: PlusTradeState,
    pub pending_approvals: u32,
    pub quota: Option<PlusQuotaBrief>,
    /// Set when this is the machine's last snapshot, not a live answer.
    pub as_of: Option<i64>,
    /// What the assistant spent (CAFK-W5): only with a live machine answer.
    pub spent: Option<PlusSpent>,
    /// "Mesajlar" (QoL Q1): the latest message the assistant left to the owner since they last
    /// opened the conversations; only in the player's own views.
    pub new_messages: Option<PlusNewMessages>,
}

/// `AiView.newMessages`: that conversation's unread lines, and when the message came.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusNewMessages {
    pub count: u32,
    pub at: i64,
}

impl PlusAi {
    pub fn is_on(&self) -> bool {
        self.state == "on"
    }
}

/// `AiView.trade`: `on: false` with `off: "mode"` (the bot is not in the profile's game mode) or
/// `"profile"` (the mode's profile is not enabled); `buy: "notify"` without spending authority.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusTradeState {
    pub on: bool,
    pub off: Option<String>,
    pub buy: String,
}

/// What the assistant spent in the game's money: `total` since the player last reset it (against
/// `limits.totalBudget`), `today` this AFK+ day (04:00 to 04:00 Turkey time, against
/// `limits.dailySpendMax`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusSpent {
    pub total: u64,
    pub today: u64,
}

/// `AiView.quota` (`QuotaBrief`): the two bars.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusQuotaBrief {
    pub pct5: u32,
    pub pct_week: u32,
    pub w5h_reset_at: Option<i64>,
    pub week_reset_at: i64,
    /// When used up: when room comes back.
    pub exhausted_until: Option<i64>,
    /// Credits of unexpired gifts.
    pub gifts_left: u64,
}

/// `GET /accounts/:id/plus/quota` (`QuotaView`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusQuota {
    pub w5h: PlusQuotaWindow,
    pub week: PlusQuotaWindow,
    /// In spending order.
    pub gifts: Vec<PlusGift>,
    pub promo: Option<PlusPromo>,
    pub frozen: bool,
    /// "Yaklaşık N yanıt kaldı".
    pub est_replies: u64,
    pub pct5: u32,
    pub pct_week: u32,
    pub as_of: i64,
}

/// One window: `reset_at` is None for a 5-hour window that is not open yet ("İlk kullanımda başlar").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusQuotaWindow {
    pub used: u64,
    pub cap: u64,
    pub reset_at: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusGift {
    pub id: String,
    pub remaining: u64,
    pub expires_at: Option<i64>,
}

/// A promotion's multiplier (100-500 %), its words and its end.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusPromo {
    pub pct: u32,
    pub label: CloudLocalized,
    pub until: i64,
}

// --- the catalog ----------------------------------------------------------------------------------

/// `GET /plus/catalog?mode=`: what a game mode trades, and its money's limits.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusCatalog {
    pub mode: String,
    pub name: CloudLocalized,
    pub items: Vec<PlusCatalogItem>,
    pub enchantments: Vec<PlusEnchantment>,
    pub money: PlusMoney,
    pub limits: PlusCatalogLimits,
    pub ad_templates: Vec<PlusAdTemplate>,
}

impl PlusCatalog {
    pub fn item(&self, id: &str) -> Option<&PlusCatalogItem> {
        self.items.iter().find(|item| item.id == id)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusCatalogItem {
    /// `minecraft:elytra`, or a profile's own item (`custom:spawner:zombie`).
    pub id: String,
    pub tr: String,
    pub en: String,
    /// The picture's key (the item's `minecraft:` path).
    pub sprite: String,
    pub stack: u32,
    pub damageable: bool,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusEnchantment {
    pub id: String,
    pub tr: String,
    pub en: String,
    pub max: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusMoney {
    pub currency: String,
    pub max_deal: u64,
}

impl Default for PlusMoney {
    fn default() -> Self {
        Self {
            currency: String::new(),
            max_deal: DEFAULT_MAX_DEAL,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusCatalogLimits {
    pub list_max: u32,
    /// `[lowest, highest]` minutes.
    pub approval_timeout_min: Vec<u32>,
}

impl Default for PlusCatalogLimits {
    fn default() -> Self {
        Self {
            list_max: LIST_MAX,
            approval_timeout_min: vec![1, 60],
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusAdTemplate {
    pub id: String,
    pub tr: String,
    pub en: String,
}

// --- the player's settings (`AfkPlusConfig` v1) ----------------------------------------------------

/// The player's AFK+ settings of one account, written whole with `PUT /plus` and `If-Match`.
/// Values with a closed set (`lang`, `persona.preset`, a row's `unit`, `style`, `approval`, …) are
/// kept as text, so a value a newer service adds round-trips unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusConfig {
    pub v: u32,
    /// The player's AI switch ("Yapay zekâyı durdur"); false: plain AFK and templates only.
    pub enabled: bool,
    /// A server profile open for the account's server (`projects-smp`).
    pub mode: String,
    /// `auto` (English to players who write English, else Turkish), `tr`, `en`.
    pub lang: String,
    pub persona: PlusPersona,
    pub replies: PlusReplies,
    pub buy: Vec<PlusBuyRow>,
    pub sell: Vec<PlusSellRow>,
    pub limits: PlusLimits,
    pub discovery: PlusDiscovery,
    pub trade: PlusTradeSettings,
    /// "HH:MM" Turkey time: the daily summary.
    pub summary_at: String,
    /// In-game names the bot never answers (at most 100).
    pub blocked: Vec<String>,
    /// Working hours, Turkey time (QoL Q3).
    pub schedule: PlusSchedule,
    /// A break until then (QoL Q3).
    pub pause_until: Option<i64>,
}

impl Default for PlusConfig {
    /// The contract's defaults for a new AFK+ account (the mode as the service fills it).
    fn default() -> Self {
        Self {
            v: 1,
            enabled: true,
            mode: String::new(),
            lang: "auto".into(),
            persona: PlusPersona::default(),
            replies: PlusReplies::default(),
            buy: Vec::new(),
            sell: Vec::new(),
            limits: PlusLimits::default(),
            discovery: PlusDiscovery::default(),
            trade: PlusTradeSettings::default(),
            summary_at: "21:00".into(),
            blocked: Vec::new(),
            schedule: PlusSchedule::default(),
            pause_until: None,
        }
    }
}

impl PlusConfig {
    /// Whether the bot may buy at all: without both limits buy rows only report what they find.
    pub fn can_spend(&self) -> bool {
        self.limits.total_budget.is_some() && self.limits.per_deal_max.is_some()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusPersona {
    /// `friendly`, `brief`, `polite`, `playful`.
    pub preset: String,
    /// "" or 1-60 code points.
    pub intro: String,
    /// "" or 1-80 code points; "" while `status_preset` is set.
    pub status: String,
    /// `back_soon`, `back_evening`, `back_tomorrow`, `weekend`, `busy` (QoL Q3).
    pub status_preset: Option<String>,
    pub small_talk: bool,
}

impl Default for PlusPersona {
    fn default() -> Self {
        Self {
            preset: "friendly".into(),
            intro: String::new(),
            status: String::new(),
            status_preset: None,
            small_talk: true,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusReplies {
    /// `all`, `trade`, `off`.
    pub scope: String,
    /// The away template for the other messages.
    pub away: bool,
    /// "Hakkı koru" (QoL Q9): 50, 80 or None (off).
    pub save_at: Option<u32>,
}

impl Default for PlusReplies {
    fn default() -> Self {
        Self {
            scope: "trade".into(),
            away: true,
            save_at: Some(80),
        }
    }
}

/// A buy row ("Alınacak").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusBuyRow {
    /// `^[a-z0-9]{1,8}$`, unique across buy and sell.
    pub id: String,
    /// A catalog id of the mode's profile.
    pub item: String,
    pub req: PlusRequirements,
    pub qty: u32,
    /// `each` or `total`: whether target and max are per item or for `qty` items.
    pub unit: String,
    pub target: u64,
    /// Never exceeded.
    pub max: u64,
    /// `patient`, `balanced`, `quick`.
    pub style: String,
    /// `auto`, `above` (over `limits.approvalAbove`), `always`.
    pub approval: String,
    pub daily_qty: u32,
    pub notify: bool,
    pub enabled: bool,
}

impl Default for PlusBuyRow {
    fn default() -> Self {
        Self {
            id: String::new(),
            item: String::new(),
            req: PlusRequirements::default(),
            qty: 1,
            unit: "each".into(),
            target: 1,
            max: 1,
            style: "balanced".into(),
            approval: "above".into(),
            daily_qty: 1,
            notify: true,
            enabled: true,
        }
    }
}

/// What a bought item must be: enchantments (id → lowest level), durability, an anvil name, curses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusRequirements {
    pub ench: BTreeMap<String, u8>,
    pub min_durability_pct: u8,
    pub allow_custom_name: bool,
    pub allow_curses: bool,
}

impl Default for PlusRequirements {
    fn default() -> Self {
        Self {
            ench: BTreeMap::new(),
            min_durability_pct: 90,
            allow_custom_name: false,
            allow_curses: false,
        }
    }
}

/// A sell row ("Satılacak").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusSellRow {
    pub id: String,
    pub item: String,
    pub qty: u32,
    pub unit: String,
    pub target: u64,
    /// Never undercut.
    pub min: u64,
    pub style: String,
    pub approval: String,
    pub daily_qty: u32,
    pub enabled: bool,
}

impl Default for PlusSellRow {
    fn default() -> Self {
        Self {
            id: String::new(),
            item: String::new(),
            qty: 1,
            unit: "each".into(),
            target: 1,
            min: 1,
            style: "balanced".into(),
            approval: "above".into(),
            daily_qty: 1,
            enabled: true,
        }
    }
}

/// Spending limits ("Harcama"). None: no limit, or for `per_deal_max` and `total_budget` no
/// spending authority at all (buy rows then only notify).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusLimits {
    pub per_deal_max: Option<u64>,
    pub daily_spend_max: Option<u64>,
    pub total_budget: Option<u64>,
    pub balance_floor: u64,
    pub approval_above: Option<u64>,
    pub new_counterpart_approval_above: Option<u64>,
    /// 1-60: an unanswered approval is a no.
    pub approval_timeout_min: u32,
}

impl Default for PlusLimits {
    fn default() -> Self {
        Self {
            per_deal_max: None,
            daily_spend_max: None,
            total_budget: None,
            balance_floor: 0,
            approval_above: None,
            new_counterpart_approval_above: None,
            approval_timeout_min: 10,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusDiscovery {
    pub scan_chat: bool,
    pub proactive_pm: bool,
    pub ads: bool,
    pub ad_template: String,
}

impl Default for PlusDiscovery {
    fn default() -> Self {
        Self {
            scan_chat: true,
            proactive_pm: true,
            ads: false,
            ad_template: "buy-basic".into(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusTradeSettings {
    /// A plain command row of the account's config to run after a trade, or None (stay at spawn).
    pub return_row: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusSchedule {
    pub on: bool,
    pub windows: Vec<PlusWindow>,
}

/// A working-hours window: `days` 1 Monday … 7 Sunday; `from` and `to` "HH:MM" Turkey time; it
/// belongs to the day it starts and may run past midnight; `from == to` is the whole day.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusWindow {
    pub days: Vec<u8>,
    pub from: String,
    pub to: String,
}

/// `GET /accounts/:id/plus`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusSettings {
    pub config: PlusConfig,
    /// The revision (`If-Match` of the next write).
    pub ai_rev: u64,
    pub judge: PlusJudge,
    /// The mode's profile: its name and whether it trades now. None for an unknown mode.
    pub profile: Option<PlusProfile>,
    /// What the assistant spent (CAFK-W5); None when the machine cannot say now.
    pub spent: Option<PlusSpent>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusProfile {
    pub id: String,
    pub name: CloudLocalized,
    pub trade: bool,
}

/// How the persona texts were judged: `ok`, `pending` (not used until judged) or `rejected`
/// (`reason`: `inappropriate`, `impersonation`, `contact`, `redirect`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusJudge {
    pub intro: PlusJudgeState,
    pub status: PlusJudgeState,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusJudgeState {
    pub state: String,
    pub reason: Option<String>,
    pub at: i64,
}

/// `PUT /plus` and a restore: the stored settings and their revision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusWritten {
    pub config: PlusConfig,
    pub ai_rev: u64,
    pub judge: PlusJudge,
}

/// `POST /plus/control`: the AI's state after it, and the settings' revision.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusControlAnswer {
    pub ai: Option<PlusAi>,
    pub ai_rev: u64,
}

/// What "control" does: `pause`/`resume` (the AI switch), `block`/`unblock` and `forget` (a
/// player, by in-game name), `reset_spent` ("Harcananı sıfırla").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum PlusAction {
    Pause,
    Resume,
    Block,
    Unblock,
    Forget,
    ResetSpent,
}

impl PlusAction {
    fn needs_name(self) -> bool {
        matches!(self, Self::Block | Self::Unblock | Self::Forget)
    }
}

// --- approvals, the feed, deals ---------------------------------------------------------------------

/// A deal waiting for the player's OK (`state` `pending`), or one of the last 24 hours' answers.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusApproval {
    pub id: String,
    pub acct: String,
    pub deal: PlusApprovalDeal,
    /// In-game name.
    pub counterpart: String,
    /// `above`, `new_counterpart`, `always`, `price_unusual`, `budget`, `daily`.
    pub why: Vec<String>,
    pub created_at: i64,
    pub expires_at: i64,
    /// `pending`, `approved`, `rejected`, `expired`, `closed`.
    pub state: String,
    /// Approved with a lower ceiling ("Fiyatı düşür").
    pub max_price: Option<u64>,
    /// `deal_gone`, `ai_stopped`.
    pub closed: Option<String>,
}

impl PlusApproval {
    pub fn is_pending(&self) -> bool {
        self.state == "pending"
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusApprovalDeal {
    /// `buy` or `sell`.
    pub side: String,
    pub item: String,
    pub qty: u32,
    pub price: u64,
    pub currency: String,
}

/// `GET /accounts/:id/plus/approvals`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusApprovalList {
    pub approvals: Vec<PlusApproval>,
}

/// One line of the live feed. `kind`: `pm_in`, `pm_out`, `offer`, `counter`, `deal`, `approval`,
/// `approval_answer`, `trade_started`, `trade_done`, `trade_failed`, `trade_cancelled`, `found`,
/// `suspicious`, `ai_stopped`, `ai_resumed`, `summary`, `review` (kept as text).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusFeedItem {
    /// A ULID: time order.
    pub id: String,
    pub at: i64,
    pub kind: String,
    /// In-game name (the owner's own bot's messages show real names).
    pub counterpart: Option<String>,
    /// `pm_in`, `pm_out`: the masked text.
    pub text: Option<String>,
    pub item: Option<String>,
    pub qty: Option<u32>,
    pub price: Option<u64>,
    pub currency: Option<String>,
    /// A code the launcher words in one short sentence.
    pub reason: Option<String>,
}

/// A page of the feed, newest first; `next` continues further back.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusFeedPage {
    pub items: Vec<PlusFeedItem>,
    pub next: Option<String>,
}

/// A finished deal (D1, 90 days).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusDeal {
    pub id: String,
    pub side: String,
    pub item: String,
    pub qty: u32,
    pub price: u64,
    pub currency: String,
    pub counterpart: String,
    /// `completed`, `failed`, `cancelled`, `review`.
    pub status: String,
    pub reason: Option<String>,
    /// `auto` or `approved`.
    pub approval: String,
    pub at: i64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusDealPage {
    pub deals: Vec<PlusDeal>,
    pub next: Option<String>,
}

// --- QoL v1 -----------------------------------------------------------------------------------------

/// A conversation of "Mesajlar" (Q1): a player who wrote to the bot in the last 7 days.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusInboxThread {
    pub name: String,
    pub last_at: i64,
    pub last: PlusInboxLast,
    pub unread: u32,
    /// `active`, `handoff` (the owner holds it until `handoff_until`), `blocked`.
    pub state: String,
    pub handoff_until: Option<i64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusInboxLast {
    /// `them`, `bot`, `you`.
    pub from: String,
    pub text: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusInboxPage {
    pub threads: Vec<PlusInboxThread>,
    pub next: Option<String>,
}

/// A line of a conversation: `hidden: "unsafe"` comes folded ("Uygunsuz mesaj gizlendi. Göster").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusInboxLine {
    pub at: i64,
    pub from: String,
    pub text: String,
    pub hidden: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusThreadPage {
    pub lines: Vec<PlusInboxLine>,
    pub handoff_until: Option<i64>,
    /// The player wrote within 24 hours: the owner may answer.
    pub can_reply: bool,
    pub next: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub(crate) struct PlusReplyAnswer {
    pub(crate) line: PlusInboxLine,
}

/// "Yazarak ekle" (Q2): a rule's draft, never stored by itself. `item` None with 2-5 `candidates`
/// when the item is unclear.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusParseDraft {
    pub side: String,
    pub item: Option<String>,
    pub candidates: Vec<String>,
    pub qty: u32,
    pub unit: String,
    pub target: Option<u64>,
    /// Buy rows.
    pub max: Option<u64>,
    /// Sell rows.
    pub min: Option<u64>,
    pub style: String,
    pub approval: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusParseAnswer {
    pub draft: PlusParseDraft,
    pub issues: Vec<String>,
}

/// A saved revision of the settings (Q7). `by`: `site`, `launcher`, `phone`, `restore`, `studio`,
/// `player` (kept as text).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusHistoryEntry {
    pub ai_rev: u64,
    pub at: i64,
    pub by: String,
    pub config: PlusConfig,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusHistory {
    /// Newest first.
    pub entries: Vec<PlusHistoryEntry>,
}

/// The trade summary of a range (Q4): completed deals by side, item and currency.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusDealSummary {
    pub from: i64,
    pub to: i64,
    pub items: Vec<PlusItemSummary>,
    pub totals: Vec<PlusTotals>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusItemSummary {
    pub side: String,
    pub item: String,
    pub currency: String,
    pub deals: u32,
    pub qty: u64,
    pub total: u64,
    pub avg_unit: u64,
    pub min_unit: u64,
    pub max_unit: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusTotals {
    pub currency: String,
    pub buys: u32,
    pub sells: u32,
    pub spent: u64,
    pub earned: u64,
}

/// `GET /me/plus/report`: an AFK+ week (Monday 04:00 Turkey time), each of the player's accounts.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusWeeklyReport {
    /// `2026-W43`.
    pub week: String,
    pub from: i64,
    pub to: i64,
    pub accounts: Vec<PlusWeeklyAccount>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusWeeklyAccount {
    pub id: String,
    pub name: String,
    /// Answers the assistant wrote.
    pub pms: u32,
    pub bought: u32,
    pub sold: u32,
    pub spent: u64,
    pub earned: u64,
    /// Buys: the rows' limits minus what was paid ("Alımlarda tasarruf").
    pub below_limit: u64,
    /// Sales: what came above the rows' floors ("Satışta fazladan").
    pub above_floor: u64,
    pub blocked: PlusBlockedCounts,
    pub allowance_pct: u32,
    pub currency: Option<String>,
}

/// Messages held back last week, by kind (no content).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusBlockedCounts {
    pub scam: u32,
    pub rmt: u32,
    #[serde(rename = "unsafe")]
    pub unsafe_text: u32,
    pub fake: u32,
}

impl PlusBlockedCounts {
    pub fn total(&self) -> u32 {
        self.scam + self.rmt + self.unsafe_text + self.fake
    }
}

/// `GET`/`PUT /me/notifications`: the player's switches per event (all their cloud accounts) and
/// quiet hours ("HH:MM" Turkey time). Events: `stopped`, `renewal`, `expired`, `found`,
/// `approval`, `deal_done`, `deal_failed`, `quota_full`, `quota_80`, `gift_ending`, `summary`,
/// `paused`, `incident`, `message`, `weekly`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudNotifyPrefs {
    pub events: BTreeMap<String, bool>,
    pub quiet: CloudQuietHours,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct CloudQuietHours {
    pub enabled: bool,
    pub from: String,
    pub to: String,
}

impl Default for CloudQuietHours {
    fn default() -> Self {
        Self {
            enabled: true,
            from: "23:00".into(),
            to: "08:00".into(),
        }
    }
}

// --- the checks before a write ---------------------------------------------------------------------

/// The contract's fallback for a profile's `money.maxDeal`.
pub const DEFAULT_MAX_DEAL: u64 = 10_000_000;
/// Rows per list (Studio may set fewer: the catalog's `limits.listMax`).
pub const LIST_MAX: u32 = 10;
pub const MAX_BLOCKED: usize = 100;
pub const MAX_WINDOWS: usize = 7;
pub const MAX_INTRO: usize = 60;
pub const MAX_STATUS: usize = 80;
pub const MAX_QTY: u32 = 9_999;
/// A break reaches at most this far ahead.
pub const MAX_PAUSE_SECONDS: i64 = 90 * 86_400;
/// The owner's reply in "Mesajlar" (code points).
pub const MAX_REPLY: usize = 200;
/// A typed rule ("Yazarak ekle"), after cleaning.
pub const MAX_PARSE: usize = 200;

const PRESETS: [&str; 4] = ["friendly", "brief", "polite", "playful"];
const STATUS_PRESETS: [&str; 5] = [
    "back_soon",
    "back_evening",
    "back_tomorrow",
    "weekend",
    "busy",
];
const SCOPES: [&str; 3] = ["all", "trade", "off"];
const LANGS: [&str; 3] = ["auto", "tr", "en"];
const UNITS: [&str; 2] = ["each", "total"];
const STYLES: [&str; 3] = ["patient", "balanced", "quick"];
const APPROVALS: [&str; 3] = ["auto", "above", "always"];

/// One problem of a config, where the contract names it: `field` a path (`buy[2].max`,
/// `limits.approvalAbove`, `persona.intro`, `schedule.windows[0].from`), `reason` the contract's
/// (`range`, `order`, `id`, `item`, `limits`, `name`, `text`, `limit_rows`, `mode`, `row`,
/// `missing`, `type`, `unknown`), `problem` the text rule's word with `text` (`length`, `marker`,
/// `link`, `phone`, …).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PlusIssue {
    pub field: String,
    pub reason: String,
    pub problem: Option<String>,
}

impl PlusIssue {
    pub fn new(field: impl Into<String>, reason: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            reason: reason.into(),
            problem: None,
        }
    }

    pub fn text(field: impl Into<String>, problem: impl Into<String>) -> Self {
        Self {
            field: field.into(),
            reason: "text".into(),
            problem: Some(problem.into()),
        }
    }
}

/// "HH:MM", 00:00-23:59.
pub fn is_time(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 5
        && bytes[2] == b':'
        && bytes[..2].iter().chain(&bytes[3..]).all(u8::is_ascii_digit)
        && (bytes[0] < b'2' || (bytes[0] == b'2' && bytes[1] <= b'3'))
        && bytes[3] <= b'5'
}

/// An in-game name: `^[A-Za-z0-9_]{3,16}$`.
pub fn is_player_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

fn is_row_id(id: &str) -> bool {
    (1..=8).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit())
}

fn is_template_id(id: &str) -> bool {
    (1..=32).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
}

fn one_of(value: &str, values: &[&str]) -> bool {
    values.contains(&value)
}

/// A persona text's quick look: the length and the `[AFK+]` marker the bot's own lines carry. The
/// rest of the text rule (links, numbers, listed words) and the judge are the service's.
fn persona_text(field: &str, text: &str, max: usize, out: &mut Vec<PlusIssue>) {
    if text.chars().count() > max {
        out.push(PlusIssue::text(field, "length"));
    } else if text.to_ascii_lowercase().contains("[afk+]") {
        out.push(PlusIssue::text(field, "marker"));
    }
}

/// The rules the launcher checks before `PUT /plus` (README "AfkPlusConfig v1" → Rules, in the
/// Worker's terms), all of them at once so each field can say its own. `catalog`: the mode's
/// (items, enchantments, `money.maxDeal`, `limits.listMax`); without it the catalog rules wait for
/// the service. `now`: Unix seconds (a break's reach). The trade's return row and persona texts'
/// full rule are the service's.
pub fn check_config(
    config: &PlusConfig,
    catalog: Option<&PlusCatalog>,
    now: i64,
) -> Vec<PlusIssue> {
    let mut out = Vec::new();
    let max_deal = catalog.map_or(DEFAULT_MAX_DEAL, |c| c.money.max_deal.max(1));
    let list_max = catalog.map_or(LIST_MAX, |c| c.limits.list_max.clamp(1, LIST_MAX)) as usize;
    if config.v != 1 {
        out.push(PlusIssue::new("v", "range"));
    }
    if !one_of(&config.lang, &LANGS) {
        out.push(PlusIssue::new("lang", "range"));
    }
    let persona = &config.persona;
    if !one_of(&persona.preset, &PRESETS) {
        out.push(PlusIssue::new("persona.preset", "range"));
    }
    if persona
        .status_preset
        .as_deref()
        .is_some_and(|preset| !one_of(preset, &STATUS_PRESETS))
    {
        out.push(PlusIssue::new("persona.statusPreset", "range"));
    }
    persona_text("persona.intro", &persona.intro, MAX_INTRO, &mut out);
    // A ready status wins: the player's own text is not kept (nothing to check).
    if persona.status_preset.is_none() {
        persona_text("persona.status", &persona.status, MAX_STATUS, &mut out);
    }
    if !one_of(&config.replies.scope, &SCOPES) {
        out.push(PlusIssue::new("replies.scope", "range"));
    }
    if config
        .replies
        .save_at
        .is_some_and(|at| at != 50 && at != 80)
    {
        out.push(PlusIssue::new("replies.saveAt", "range"));
    }

    if config.buy.len() > list_max {
        out.push(PlusIssue::new("buy", "limit_rows"));
    }
    if config.sell.len() > list_max {
        out.push(PlusIssue::new("sell", "limit_rows"));
    }
    let mut ids = HashSet::new();
    let ranged = |field: &str, value: u64, low: u64, high: u64, out: &mut Vec<PlusIssue>| {
        if value < low || value > high {
            out.push(PlusIssue::new(field, "range"));
            false
        } else {
            true
        }
    };
    let common = |field: &str,
                  id: &str,
                  item: &str,
                  unit: &str,
                  style: &str,
                  approval: &str,
                  ids: &mut HashSet<String>,
                  out: &mut Vec<PlusIssue>| {
        if !is_row_id(id) || !ids.insert(id.to_owned()) {
            out.push(PlusIssue::new(format!("{field}.id"), "id"));
        }
        if catalog.is_some_and(|catalog| catalog.item(item).is_none()) || item.is_empty() {
            out.push(PlusIssue::new(format!("{field}.item"), "item"));
        }
        if !one_of(unit, &UNITS) {
            out.push(PlusIssue::new(format!("{field}.unit"), "range"));
        }
        if !one_of(style, &STYLES) {
            out.push(PlusIssue::new(format!("{field}.style"), "range"));
        }
        if !one_of(approval, &APPROVALS) {
            out.push(PlusIssue::new(format!("{field}.approval"), "range"));
        }
    };
    for (i, row) in config.buy.iter().enumerate() {
        let field = format!("buy[{i}]");
        common(
            &field,
            &row.id,
            &row.item,
            &row.unit,
            &row.style,
            &row.approval,
            &mut ids,
            &mut out,
        );
        if row.req.ench.len() > 8 {
            out.push(PlusIssue::new(format!("{field}.req.ench"), "range"));
        }
        for (key, level) in &row.req.ench {
            let at = format!("{field}.req.ench.{key}");
            match catalog.map(|c| c.enchantments.iter().find(|e| &e.id == key)) {
                Some(None) => out.push(PlusIssue::new(at, "item")),
                Some(Some(known)) => {
                    ranged(
                        &at,
                        u64::from(*level),
                        1,
                        u64::from(known.max.clamp(1, 10)),
                        &mut out,
                    );
                }
                None => {
                    ranged(&at, u64::from(*level), 1, 10, &mut out);
                }
            }
        }
        ranged(
            &format!("{field}.req.minDurabilityPct"),
            u64::from(row.req.min_durability_pct),
            0,
            100,
            &mut out,
        );
        ranged(
            &format!("{field}.qty"),
            u64::from(row.qty),
            1,
            u64::from(MAX_QTY),
            &mut out,
        );
        let target = ranged(
            &format!("{field}.target"),
            row.target,
            1,
            max_deal,
            &mut out,
        );
        let max = ranged(&format!("{field}.max"), row.max, 1, max_deal, &mut out);
        if target && max && row.target > row.max {
            out.push(PlusIssue::new(format!("{field}.max"), "order"));
        }
        ranged(
            &format!("{field}.dailyQty"),
            u64::from(row.daily_qty),
            1,
            u64::from(MAX_QTY),
            &mut out,
        );
    }
    for (i, row) in config.sell.iter().enumerate() {
        let field = format!("sell[{i}]");
        common(
            &field,
            &row.id,
            &row.item,
            &row.unit,
            &row.style,
            &row.approval,
            &mut ids,
            &mut out,
        );
        ranged(
            &format!("{field}.qty"),
            u64::from(row.qty),
            1,
            u64::from(MAX_QTY),
            &mut out,
        );
        let target = ranged(
            &format!("{field}.target"),
            row.target,
            1,
            max_deal,
            &mut out,
        );
        let min = ranged(&format!("{field}.min"), row.min, 1, max_deal, &mut out);
        if target && min && row.min > row.target {
            out.push(PlusIssue::new(format!("{field}.min"), "order"));
        }
        ranged(
            &format!("{field}.dailyQty"),
            u64::from(row.daily_qty),
            1,
            u64::from(MAX_QTY),
            &mut out,
        );
    }

    let limits = &config.limits;
    let amount = |field: &str, value: Option<u64>, low: u64, out: &mut Vec<PlusIssue>| match value {
        Some(n) if n < low => {
            out.push(PlusIssue::new(field, "range"));
            false
        }
        Some(n) if n > max_deal => {
            out.push(PlusIssue::new(field, "limits"));
            false
        }
        _ => true,
    };
    let per_deal = amount("limits.perDealMax", limits.per_deal_max, 1, &mut out);
    amount("limits.dailySpendMax", limits.daily_spend_max, 1, &mut out);
    amount("limits.totalBudget", limits.total_budget, 1, &mut out);
    amount(
        "limits.balanceFloor",
        Some(limits.balance_floor),
        0,
        &mut out,
    );
    let above = amount("limits.approvalAbove", limits.approval_above, 1, &mut out);
    if per_deal
        && above
        && let (Some(ask_above), Some(deal_max)) = (limits.approval_above, limits.per_deal_max)
        && ask_above > deal_max
    {
        out.push(PlusIssue::new("limits.approvalAbove", "limits"));
    }
    amount(
        "limits.newCounterpartApprovalAbove",
        limits.new_counterpart_approval_above,
        1,
        &mut out,
    );
    if !(1..=60).contains(&limits.approval_timeout_min) {
        out.push(PlusIssue::new("limits.approvalTimeoutMin", "limits"));
    }

    if !is_template_id(&config.discovery.ad_template)
        || (config.discovery.ads
            && catalog.is_some_and(|c| {
                !c.ad_templates
                    .iter()
                    .any(|t| t.id == config.discovery.ad_template)
            }))
    {
        out.push(PlusIssue::new("discovery.adTemplate", "range"));
    }
    if config
        .trade
        .return_row
        .as_deref()
        .is_some_and(|row| row.is_empty() || row.len() > 16)
    {
        out.push(PlusIssue::new("trade.returnRow", "row"));
    }
    if !is_time(&config.summary_at) {
        out.push(PlusIssue::new("summaryAt", "range"));
    }

    if config.blocked.len() > MAX_BLOCKED {
        out.push(PlusIssue::new("blocked", "limit_rows"));
    }
    let mut names = HashSet::new();
    for (i, name) in config.blocked.iter().enumerate() {
        if !is_player_name(name) || !names.insert(name.to_ascii_lowercase()) {
            out.push(PlusIssue::new(format!("blocked[{i}]"), "name"));
        }
    }

    let schedule = &config.schedule;
    if schedule.windows.len() > MAX_WINDOWS || (schedule.on && schedule.windows.is_empty()) {
        out.push(PlusIssue::new("schedule.windows", "range"));
    }
    for (i, window) in schedule.windows.iter().enumerate() {
        let field = format!("schedule.windows[{i}]");
        let distinct: HashSet<u8> = window.days.iter().copied().collect();
        if window.days.is_empty()
            || window.days.len() > 7
            || distinct.len() != window.days.len()
            || window.days.iter().any(|day| !(1..=7).contains(day))
        {
            out.push(PlusIssue::new(format!("{field}.days"), "range"));
        }
        if !is_time(&window.from) {
            out.push(PlusIssue::new(format!("{field}.from"), "range"));
        }
        if !is_time(&window.to) {
            out.push(PlusIssue::new(format!("{field}.to"), "range"));
        }
    }
    if config
        .pause_until
        .is_some_and(|until| until > now + MAX_PAUSE_SECONDS)
    {
        out.push(PlusIssue::new("pauseUntil", "range"));
    }
    out
}

/// The config as it is sent: a ready status clears the own text (the service stores it so), a
/// break already over is no break, and a schedule's days are in order.
pub fn normalized(mut config: PlusConfig, now: i64) -> PlusConfig {
    if config.persona.status_preset.is_some() {
        config.persona.status.clear();
    }
    config.persona.intro = config.persona.intro.trim().to_owned();
    config.persona.status = config.persona.status.trim().to_owned();
    if config.pause_until.is_some_and(|until| until <= now) {
        config.pause_until = None;
    }
    for window in &mut config.schedule.windows {
        window.days.sort_unstable();
        window.days.dedup();
    }
    config
}

/// A refusal of `PUT /plus` (or a restore) about the settings' contents, as issues.
pub fn refusal_issues(error: &CloudError) -> Option<Vec<PlusIssue>> {
    let field = error.extra_str("field").unwrap_or("").to_owned();
    match error.code() {
        "invalid_plus_config" => Some(vec![PlusIssue {
            field: if field.is_empty() {
                "config".into()
            } else {
                field
            },
            reason: error.extra_str("reason").unwrap_or("range").to_owned(),
            problem: error.extra_str("problem").map(str::to_owned),
        }]),
        "limit_rows" => Some(vec![PlusIssue::new(
            if field.is_empty() {
                "buy".into()
            } else {
                field
            },
            "limit_rows",
        )]),
        // A persona text the judge found unsuitable: `reason` names why.
        "plus_text_rejected" => Some(vec![PlusIssue {
            field: if field.is_empty() {
                "persona.intro".into()
            } else {
                field
            },
            reason: "judged".into(),
            problem: error.extra_str("reason").map(str::to_owned),
        }]),
        _ => None,
    }
}

// --- "Tabloyu indir" ------------------------------------------------------------------------------------

/// The start of every table "Tabloyu indir" saves (and the only files the launcher shows there).
pub const TABLE_PREFIX: &str = "afkplus-";

/// A file name in `dir` that no file has yet for an account's table of `day` (`YYYY-MM-DD`):
/// `afkplus-Ahmet_AFK-2026-10-24.csv`, then ` (2)`, ` (3)`… The bot's name keeps only
/// `[A-Za-z0-9_]` (in-game names are made of them).
pub fn table_file_name(dir: &std::path::Path, bot_name: &str, day: &str) -> String {
    let name: String = bot_name
        .chars()
        .take(16)
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect();
    let day: String = day
        .chars()
        .filter(|c| c.is_ascii_digit() || *c == '-')
        .take(10)
        .collect();
    let stem = format!("{TABLE_PREFIX}{name}-{day}");
    let first = format!("{stem}.csv");
    if !dir.join(&first).exists() {
        return first;
    }
    (2..1000)
        .map(|n| format!("{stem} ({n}).csv"))
        .find(|candidate| !dir.join(candidate).exists())
        .unwrap_or(first)
}

/// Whether a file name is one [`table_file_name`] made (nothing else is revealed for the page).
pub fn is_table_file_name(name: &str) -> bool {
    name.starts_with(TABLE_PREFIX)
        && name.ends_with(".csv")
        && name.len() <= 64
        && !name.contains(['/', '\\', ':'])
        && !name.contains("..")
}

// --- the routes ---------------------------------------------------------------------------------------

/// `<id>` of an approval (a ULID) or a history revision, refusing anything that could leave the
/// route.
fn path_part(text: &str, what: &'static str) -> Result<()> {
    let safe = !text.is_empty()
        && text.len() <= 40
        && text
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-');
    if safe {
        Ok(())
    } else {
        Err(CloudError::Invalid(what))
    }
}

fn query(pairs: &[(&str, Option<String>)]) -> String {
    let mut params = url::form_urlencoded::Serializer::new(String::new());
    for (name, value) in pairs {
        if let Some(value) = value {
            params.append_pair(name, value);
        }
    }
    let text = params.finish();
    if text.is_empty() {
        text
    } else {
        format!("?{text}")
    }
}

/// `YYYY-Www`.
fn is_week(text: &str) -> bool {
    let bytes = text.as_bytes();
    bytes.len() == 8
        && bytes[..4].iter().all(u8::is_ascii_digit)
        && &bytes[4..6] == b"-W"
        && bytes[6..].iter().all(u8::is_ascii_digit)
}

impl CloudClient {
    /// `GET /plus/catalog?mode=` (no session; caches keep it an hour).
    pub async fn plus_catalog(&self, mode: &str) -> Result<PlusCatalog> {
        path_part(mode, "game mode")?;
        self.get(
            None,
            &format!("plus/catalog{}", query(&[("mode", Some(mode.into()))])),
        )
        .await
    }

    /// `GET /accounts/:id/plus`.
    pub async fn plus_settings(&self, token: &Bearer, id: &str) -> Result<PlusSettings> {
        self.get(Some(token), &account_path(id, "/plus")?).await
    }

    /// `PUT /accounts/:id/plus` with `If-Match: "<rev>"`. A stale `rev` is `412 rev_conflict`.
    pub async fn put_plus(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        config: &PlusConfig,
    ) -> Result<PlusWritten> {
        let body = serde_json::to_value(config).map_err(|e| CloudError::Protocol(e.to_string()))?;
        self.send(
            Method::PUT,
            &account_path(id, "/plus")?,
            Some(token),
            Some(body),
            &[(IF_MATCH, format!("\"{rev}\""))],
            Repeat::Never,
        )
        .await?
        .into_result()?
        .json()
    }

    /// `POST /accounts/:id/plus/control {action, name?}`.
    pub async fn plus_control(
        &self,
        token: &Bearer,
        id: &str,
        action: PlusAction,
        name: Option<&str>,
    ) -> Result<PlusControlAnswer> {
        let mut body = serde_json::json!({ "action": action });
        if action.needs_name() {
            let name = name.filter(|name| is_player_name(name));
            let Some(name) = name else {
                return Err(CloudError::Invalid("player name"));
            };
            body["name"] = serde_json::Value::String(name.to_owned());
        }
        self.send(
            Method::POST,
            &account_path(id, "/plus/control")?,
            Some(token),
            Some(body),
            &[],
            Repeat::Safe,
        )
        .await?
        .into_result()?
        .json()
    }

    /// `GET /accounts/:id/plus/feed` (live from the machine; `503 host_offline`).
    pub async fn plus_feed(
        &self,
        token: &Bearer,
        id: &str,
        after: Option<&str>,
        limit: Option<u32>,
    ) -> Result<PlusFeedPage> {
        let path = format!(
            "{}{}",
            account_path(id, "/plus/feed")?,
            query(&[
                ("after", after.map(str::to_owned)),
                ("limit", limit.map(|n| n.clamp(1, 100).to_string())),
            ])
        );
        self.get(Some(token), &path).await
    }

    /// `GET /accounts/:id/plus/deals` (D1, 90 days).
    pub async fn plus_deals(
        &self,
        token: &Bearer,
        id: &str,
        after: Option<&str>,
    ) -> Result<PlusDealPage> {
        let path = format!(
            "{}{}",
            account_path(id, "/plus/deals")?,
            query(&[("after", after.map(str::to_owned))])
        );
        self.get(Some(token), &path).await
    }

    /// `GET /accounts/:id/plus/approvals`: pending ones and the last 24 hours' answers.
    pub async fn plus_approvals(&self, token: &Bearer, id: &str) -> Result<PlusApprovalList> {
        self.get(Some(token), &account_path(id, "/plus/approvals")?)
            .await
    }

    /// `POST /accounts/:id/plus/approvals/:aid`: `approve` (with `max_price`: "Fiyatı düşür") or
    /// `reject`. Never repeated: an answer that arrived twice would read as closed.
    pub async fn answer_approval(
        &self,
        token: &Bearer,
        id: &str,
        approval: &str,
        approve: bool,
        max_price: Option<u64>,
    ) -> Result<PlusApproval> {
        path_part(approval, "approval")?;
        let mut body =
            serde_json::json!({ "decision": if approve { "approve" } else { "reject" } });
        if approve && let Some(max_price) = max_price {
            body["maxPrice"] = serde_json::Value::from(max_price);
        }
        #[derive(Deserialize)]
        struct Answer {
            approval: PlusApproval,
        }
        let answer: Answer = self
            .send(
                Method::POST,
                &account_path(id, &format!("/plus/approvals/{approval}"))?,
                Some(token),
                Some(body),
                &[],
                Repeat::Never,
            )
            .await?
            .into_result()?
            .json()?;
        Ok(answer.approval)
    }

    /// `GET /accounts/:id/plus/quota` (live, or the machine's last snapshot with `asOf`).
    pub async fn plus_quota(&self, token: &Bearer, id: &str) -> Result<PlusQuota> {
        self.get(Some(token), &account_path(id, "/plus/quota")?)
            .await
    }

    /// `GET /accounts/:id/plus/inbox` ("Mesajlar", Q1; live).
    pub async fn plus_inbox(
        &self,
        token: &Bearer,
        id: &str,
        after: Option<&str>,
    ) -> Result<PlusInboxPage> {
        let path = format!(
            "{}{}",
            account_path(id, "/plus/inbox")?,
            query(&[("after", after.map(str::to_owned))])
        );
        self.get(Some(token), &path).await
    }

    fn thread_path(id: &str, name: &str, rest: &str) -> Result<String> {
        if !is_player_name(name) {
            return Err(CloudError::Invalid("player name"));
        }
        account_path(id, &format!("/plus/inbox/{name}{rest}"))
    }

    /// `GET /accounts/:id/plus/inbox/:name` (one conversation; live).
    pub async fn plus_thread(
        &self,
        token: &Bearer,
        id: &str,
        name: &str,
        after: Option<&str>,
    ) -> Result<PlusThreadPage> {
        let path = format!(
            "{}{}",
            Self::thread_path(id, name, "")?,
            query(&[("after", after.map(str::to_owned))])
        );
        self.get(Some(token), &path).await
    }

    /// `POST /accounts/:id/plus/inbox/:name {text}`: the owner's own words in game. Never repeated
    /// (it may have reached the game).
    pub async fn plus_reply(
        &self,
        token: &Bearer,
        id: &str,
        name: &str,
        text: &str,
    ) -> Result<PlusInboxLine> {
        let answer: PlusReplyAnswer = self
            .send(
                Method::POST,
                &Self::thread_path(id, name, "")?,
                Some(token),
                Some(serde_json::json!({ "text": text })),
                &[],
                Repeat::Never,
            )
            .await?
            .into_result()?
            .json()?;
        Ok(answer.line)
    }

    /// `POST /accounts/:id/plus/inbox/:name/read {upTo}`.
    pub async fn plus_read(&self, token: &Bearer, id: &str, name: &str, up_to: i64) -> Result<()> {
        self.send(
            Method::POST,
            &Self::thread_path(id, name, "/read")?,
            Some(token),
            Some(serde_json::json!({ "upTo": up_to })),
            &[],
            Repeat::Safe,
        )
        .await?
        .into_result()
        .map(|_| ())
    }

    /// "Şimdi devret": `POST /accounts/:id/plus/inbox/:name/handoff {action: "end"}`.
    pub async fn plus_hand_back(&self, token: &Bearer, id: &str, name: &str) -> Result<()> {
        self.send(
            Method::POST,
            &Self::thread_path(id, name, "/handoff")?,
            Some(token),
            Some(serde_json::json!({ "action": "end" })),
            &[],
            Repeat::Safe,
        )
        .await?
        .into_result()
        .map(|_| ())
    }

    /// "Yazarak ekle" (Q2): `POST /accounts/:id/plus/parse {text, lang}`. Counted per day, so never
    /// repeated.
    pub async fn plus_parse(
        &self,
        token: &Bearer,
        id: &str,
        text: &str,
        lang: &str,
    ) -> Result<PlusParseAnswer> {
        let lang = if lang == "en" { "en" } else { "tr" };
        self.send(
            Method::POST,
            &account_path(id, "/plus/parse")?,
            Some(token),
            Some(serde_json::json!({ "text": text, "lang": lang })),
            &[],
            Repeat::Never,
        )
        .await?
        .into_result()?
        .json()
    }

    /// "Ayar geçmişi" (Q7): newest first.
    pub async fn plus_history(&self, token: &Bearer, id: &str) -> Result<PlusHistory> {
        self.get(Some(token), &account_path(id, "/plus/history")?)
            .await
    }

    /// "Bu hale dön": the revision `rev` becomes the next one (`If-Match: "<current>"`).
    pub async fn plus_restore(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        current: u64,
    ) -> Result<PlusWritten> {
        self.send(
            Method::POST,
            &account_path(id, &format!("/plus/history/{rev}/restore"))?,
            Some(token),
            None,
            &[(IF_MATCH, format!("\"{current}\""))],
            Repeat::Never,
        )
        .await?
        .into_result()?
        .json()
    }

    /// The trade summary of `from..to` (Unix seconds; the service's default: the last 30 days).
    pub async fn plus_deal_summary(
        &self,
        token: &Bearer,
        id: &str,
        from: Option<i64>,
        to: Option<i64>,
    ) -> Result<PlusDealSummary> {
        let path = format!(
            "{}{}",
            account_path(id, "/plus/deals/summary")?,
            query(&[
                ("from", from.map(|at| at.to_string())),
                ("to", to.map(|at| at.to_string())),
            ])
        );
        self.get(Some(token), &path).await
    }

    /// "Tabloyu indir": the deals of `from..to` as CSV (UTF-8 with a byte order mark; Turkish
    /// headers with `;`, or English with `,`).
    pub async fn plus_deals_csv(
        &self,
        token: &Bearer,
        id: &str,
        from: Option<i64>,
        to: Option<i64>,
        lang: &str,
    ) -> Result<Vec<u8>> {
        let lang = if lang == "en" { "en" } else { "tr" };
        let path = format!(
            "{}{}",
            account_path(id, "/plus/deals.csv")?,
            query(&[
                ("from", from.map(|at| at.to_string())),
                ("to", to.map(|at| at.to_string())),
                ("lang", Some(lang.to_owned())),
            ])
        );
        let reply = self
            .send(Method::GET, &path, Some(token), None, &[], Repeat::Safe)
            .await?
            .into_result()?;
        Ok(reply.body)
    }

    /// `GET /me/plus/report?week=YYYY-Www` (None: the last whole week).
    pub async fn plus_weekly_report(
        &self,
        token: &Bearer,
        week: Option<&str>,
    ) -> Result<PlusWeeklyReport> {
        if week.is_some_and(|week| !is_week(week)) {
            return Err(CloudError::Invalid("week"));
        }
        let path = format!(
            "me/plus/report{}",
            query(&[("week", week.map(str::to_owned))])
        );
        self.get(Some(token), &path).await
    }

    /// `GET /me/notifications`.
    pub async fn notify_prefs(&self, token: &Bearer) -> Result<CloudNotifyPrefs> {
        self.get(Some(token), "me/notifications").await
    }

    /// `PUT /me/notifications` (events left out keep their value).
    pub async fn put_notify_prefs(
        &self,
        token: &Bearer,
        prefs: &CloudNotifyPrefs,
    ) -> Result<CloudNotifyPrefs> {
        let body = serde_json::to_value(prefs).map_err(|e| CloudError::Protocol(e.to_string()))?;
        self.send(
            Method::PUT,
            "me/notifications",
            Some(token),
            Some(body),
            &[],
            Repeat::Safe,
        )
        .await?
        .into_result()?
        .json()
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::error::from_response;
    use crate::model::{CloudAccount, Me, Offer};

    /// The contract's account with an AFK+ `ai` (README "AFK+ tier").
    fn plus_account() -> serde_json::Value {
        json!({
          "id": "a_7KQ2MZ3P9TW4XB1C", "name": "Ahmet_AFK",
          "server": {"host": "play.projects.gg", "port": 0}, "version": "26.3", "lang": "tr",
          "host": "kz1", "createdAt": 1792000000, "expiresAt": 1795000000, "expired": false,
          "graceUntil": 1797592000, "renewal": {"open": false, "opensAt": 1794395200},
          "desired": "run", "hold": null, "live": {"state": "running", "since": 1792400000},
          "everJoined": true,
          "config": {"v": 1, "owner": "", "antiAfk": {"enabled": true, "delayMin": 60, "delayMax": 90},
            "commands": [], "rules": []},
          "rev": 7, "serverChanges": {"used": 0, "limit": 6, "resetAt": null},
          "tier": "plus", "plusUntil": 1795000000,
          "ai": {"state": "paused", "reason": "quota", "until": 1792418400,
            "trade": {"on": true, "buy": "notify"}, "pendingApprovals": 2,
            "quota": {"pct5": 100, "pctWeek": 41, "w5hResetAt": 1792418400,
              "weekResetAt": 1792807200, "exhaustedUntil": 1792418400, "giftsLeft": 0},
            "spent": {"total": 37500, "today": 0}, "newMessages": {"count": 2, "at": 1792400100},
            "later": true}
        })
    }

    #[test]
    fn reads_the_tier_and_the_ai_of_an_account() {
        let account: CloudAccount = serde_json::from_value(plus_account()).unwrap();
        assert_eq!(account.tier, "plus");
        assert_eq!(account.plus_until, Some(1795000000));
        let ai = account.ai.clone().unwrap();
        assert_eq!(ai.state, "paused");
        assert_eq!(ai.reason.as_deref(), Some("quota"));
        assert!(!ai.is_on());
        assert_eq!(ai.trade.buy, "notify");
        assert_eq!(ai.pending_approvals, 2);
        assert_eq!(ai.quota.unwrap().w5h_reset_at, Some(1792418400));
        assert_eq!(
            ai.spent,
            Some(PlusSpent {
                total: 37500,
                today: 0
            })
        );
        assert_eq!(
            ai.new_messages,
            Some(PlusNewMessages {
                count: 2,
                at: 1792400100
            })
        );
        // A plain account of an older service: no tier, no AI.
        let mut plain = plus_account();
        let object = plain.as_object_mut().unwrap();
        object.remove("tier");
        object.remove("plusUntil");
        object.remove("ai");
        let account: CloudAccount = serde_json::from_value(plain).unwrap();
        assert_eq!(
            (account.tier.as_str(), account.plus_until, account.ai),
            ("", None, None)
        );
    }

    #[test]
    fn the_offer_says_whether_afk_plus_shows() {
        let offer: Offer = serde_json::from_value(json!({
            "mode": "on", "plus": null
        }))
        .unwrap();
        assert_eq!(offer.plus, None, "plusMode off: nothing of AFK+");
        let offer: Offer = serde_json::from_value(json!({
            "mode": "on",
            "plus": {"mode": "invite", "sales": true,
              "prices": [{"plan": "month", "priceTl": "200.00", "days": 30},
                {"plan": "year", "priceTl": "2000.00", "days": 365}],
              "stock": {"soldOut": false, "remaining": 7}, "refundWindowHours": 24,
              "termsVersion": 1,
              "allowance": {"month": {"w5h": 5000, "week": 20000}, "year": {"w5h": 5000, "week": 20000}},
              "features": {"inbox": true, "parse": true, "schedule": true, "report": false,
                "history": true, "saver": true, "notifyDigest": false, "handoff": false, "watch": false}}
        }))
        .unwrap();
        let plus = offer.plus.unwrap();
        assert_eq!(plus.mode, "invite");
        assert_eq!(plus.month_price().unwrap().price_tl, "200.00");
        assert_eq!(plus.stock.remaining, Some(7));
        assert!(plus.features.inbox && !plus.features.report);
        let me: Me = serde_json::from_value(json!({
            "user": {"id": "1", "username": "ahmet"},
            "plus": {"canBuy": {"ok": false, "reason": "plus_invite_only"}, "restricted": null}
        }))
        .unwrap();
        let plus = me.plus.unwrap();
        assert!(!plus.can_buy.ok);
        assert_eq!(plus.can_buy.reason.as_deref(), Some("plus_invite_only"));
    }

    /// The contract's defaults (README "AfkPlusConfig v1").
    fn defaults() -> serde_json::Value {
        json!({"v": 1, "enabled": true, "mode": "projects-smp", "lang": "auto",
          "persona": {"preset": "friendly", "intro": "", "status": "", "statusPreset": null, "smallTalk": true},
          "replies": {"scope": "trade", "away": true, "saveAt": 80}, "buy": [], "sell": [],
          "limits": {"perDealMax": null, "dailySpendMax": null, "totalBudget": null, "balanceFloor": 0,
            "approvalAbove": 50000, "newCounterpartApprovalAbove": null, "approvalTimeoutMin": 10},
          "discovery": {"scanChat": true, "proactivePm": true, "ads": false, "adTemplate": "buy-basic"},
          "trade": {"returnRow": null}, "summaryAt": "21:00", "blocked": [],
          "schedule": {"on": false, "windows": []}, "pauseUntil": null})
    }

    #[test]
    fn the_config_round_trips_in_the_contracts_form() {
        let config: PlusConfig = serde_json::from_value(defaults()).unwrap();
        assert_eq!(serde_json::to_value(&config).unwrap(), defaults());
        assert!(!config.can_spend());
        // A stored document from before the QoL fields reads with their defaults.
        let mut old = defaults();
        for key in ["schedule", "pauseUntil"] {
            old.as_object_mut().unwrap().remove(key);
        }
        old["persona"]
            .as_object_mut()
            .unwrap()
            .remove("statusPreset");
        old["replies"].as_object_mut().unwrap().remove("saveAt");
        let config: PlusConfig = serde_json::from_value(old).unwrap();
        assert_eq!(config.replies.save_at, Some(80));
        assert_eq!(config.schedule, PlusSchedule::default());
        let row: PlusBuyRow = serde_json::from_value(json!({
            "id": "b1", "item": "minecraft:elytra",
            "req": {"ench": {"minecraft:unbreaking": 3}, "minDurabilityPct": 90,
              "allowCustomName": false, "allowCurses": false},
            "qty": 1, "unit": "total", "target": 32000, "max": 40000, "style": "balanced",
            "approval": "above", "dailyQty": 1, "notify": true, "enabled": true}))
        .unwrap();
        assert_eq!(row.req.ench.get("minecraft:unbreaking"), Some(&3));
        assert_eq!(
            serde_json::to_value(&row).unwrap()["req"]["minDurabilityPct"],
            json!(90)
        );
    }

    fn catalog() -> PlusCatalog {
        serde_json::from_value(json!({
            "mode": "projects-smp", "name": {"tr": "SMP", "en": "SMP"},
            "items": [
              {"id": "minecraft:elytra", "tr": "Elytra", "en": "Elytra", "sprite": "elytra", "stack": 1, "damageable": true},
              {"id": "minecraft:diamond", "tr": "Elmas", "en": "Diamond", "sprite": "diamond", "stack": 64, "damageable": false}
            ],
            "enchantments": [{"id": "minecraft:unbreaking", "tr": "Kırılmazlık", "en": "Unbreaking", "max": 3}],
            "money": {"currency": "smp", "maxDeal": 10000000},
            "limits": {"listMax": 10, "approvalTimeoutMin": [1, 60]},
            "adTemplates": [{"id": "buy-basic", "tr": "Alıyorum", "en": "Buying"}]
        }))
        .unwrap()
    }

    fn fields(issues: &[PlusIssue]) -> Vec<(String, String)> {
        issues
            .iter()
            .map(|issue| (issue.field.clone(), issue.reason.clone()))
            .collect()
    }

    #[test]
    fn a_good_config_passes() {
        let mut config: PlusConfig = serde_json::from_value(defaults()).unwrap();
        config.buy.push(PlusBuyRow {
            id: "b1".into(),
            item: "minecraft:elytra".into(),
            target: 32_000,
            max: 40_000,
            ..PlusBuyRow::default()
        });
        config.sell.push(PlusSellRow {
            id: "s1".into(),
            item: "minecraft:diamond".into(),
            qty: 64,
            target: 120,
            min: 100,
            ..PlusSellRow::default()
        });
        config.limits.per_deal_max = Some(50_000);
        config.limits.total_budget = Some(300_000);
        config.schedule = PlusSchedule {
            on: true,
            windows: vec![PlusWindow {
                days: vec![1, 2, 3, 4, 5],
                from: "18:00".into(),
                to: "02:00".into(),
            }],
        };
        assert_eq!(
            check_config(&config, Some(&catalog()), 1_792_000_000),
            vec![]
        );
        assert!(config.can_spend());
    }

    #[test]
    fn every_broken_rule_is_named_where_the_service_names_it() {
        let mut config: PlusConfig = serde_json::from_value(defaults()).unwrap();
        config.persona.intro = "x".repeat(61);
        config.persona.status = "[AFK+] dönerim".into();
        config.buy = vec![
            PlusBuyRow {
                id: "b1".into(),
                item: "minecraft:elytra".into(),
                target: 40_000,
                max: 30_000,
                ..PlusBuyRow::default()
            },
            PlusBuyRow {
                id: "b1".into(),
                item: "minecraft:netherite_sword".into(),
                qty: 0,
                target: 5,
                max: 20_000_000,
                ..PlusBuyRow::default()
            },
        ];
        config.buy[0]
            .req
            .ench
            .insert("minecraft:sharpness".into(), 5);
        config.sell = vec![PlusSellRow {
            id: "S1".into(),
            item: "minecraft:diamond".into(),
            target: 100,
            min: 120,
            ..PlusSellRow::default()
        }];
        config.limits.per_deal_max = Some(10_000);
        config.limits.approval_above = Some(20_000);
        config.limits.approval_timeout_min = 0;
        config.blocked = vec!["Kaan_07".into(), "kaan_07".into(), "a!".into()];
        config.schedule = PlusSchedule {
            on: true,
            windows: vec![PlusWindow {
                days: vec![1, 1, 8],
                from: "24:00".into(),
                to: "7:00".into(),
            }],
        };
        config.summary_at = "9:00".into();
        config.pause_until = Some(1_792_000_000 + MAX_PAUSE_SECONDS + 1);
        let issues = check_config(&config, Some(&catalog()), 1_792_000_000);
        let want = [
            ("persona.intro", "text"),
            ("persona.status", "text"),
            ("buy[0].req.ench.minecraft:sharpness", "item"),
            ("buy[0].max", "order"),
            ("buy[1].id", "id"),
            ("buy[1].item", "item"),
            ("buy[1].qty", "range"),
            ("buy[1].max", "range"),
            ("sell[0].id", "id"),
            ("sell[0].min", "order"),
            ("limits.approvalAbove", "limits"),
            ("limits.approvalTimeoutMin", "limits"),
            ("summaryAt", "range"),
            ("blocked[1]", "name"),
            ("blocked[2]", "name"),
            ("schedule.windows[0].days", "range"),
            ("schedule.windows[0].from", "range"),
            ("schedule.windows[0].to", "range"),
            ("pauseUntil", "range"),
        ];
        let found = fields(&issues);
        for (field, reason) in want {
            assert!(
                found.contains(&(field.to_owned(), reason.to_owned())),
                "{field} {reason} missing in {found:?}"
            );
        }
        assert_eq!(found.len(), want.len(), "{found:?}");
        assert_eq!(issues[0].problem.as_deref(), Some("length"));
        assert_eq!(issues[1].problem.as_deref(), Some("marker"));
    }

    #[test]
    fn a_ready_status_wins_and_an_old_break_is_no_break() {
        let mut config = PlusConfig::default();
        config.persona.status_preset = Some("back_evening".into());
        config.persona.status = "x".repeat(200);
        config.persona.intro = "  Ahmet'in asistanı  ".into();
        config.pause_until = Some(10);
        config.schedule.windows.push(PlusWindow {
            days: vec![5, 1, 5],
            from: "08:00".into(),
            to: "08:00".into(),
        });
        assert!(
            !fields(&check_config(&config, None, 100))
                .iter()
                .any(|(field, _)| field == "persona.status"),
            "a ready status is not the player's text"
        );
        let sent = normalized(config, 100);
        assert_eq!(sent.persona.status, "");
        assert_eq!(sent.persona.intro, "Ahmet'in asistanı");
        assert_eq!(sent.pause_until, None);
        assert_eq!(sent.schedule.windows[0].days, vec![1, 5]);
    }

    #[test]
    fn times_names_and_weeks() {
        for good in ["00:00", "09:30", "23:59", "21:00"] {
            assert!(is_time(good), "{good}");
        }
        for bad in ["24:00", "9:30", "12:60", "12-30", "", "123:00"] {
            assert!(!is_time(bad), "{bad}");
        }
        assert!(is_player_name("Kaan_07"));
        assert!(!is_player_name("Ka"));
        assert!(!is_player_name("Kaan.07"));
        assert!(!is_player_name(&"a".repeat(17)));
        assert!(is_week("2026-W43"));
        assert!(!is_week("2026-43"));
        assert_eq!(query(&[("a", None)]), "");
        assert_eq!(
            query(&[("after", Some("01J X".into())), ("limit", Some("5".into()))]),
            "?after=01J+X&limit=5"
        );
    }

    #[test]
    fn refusals_become_issues() {
        let error = from_response(
            400,
            None,
            json!({"error": {"code": "invalid_plus_config", "field": "persona.intro",
                "reason": "text", "problem": "phone"}})
            .to_string()
            .as_bytes(),
        );
        assert_eq!(
            refusal_issues(&error),
            Some(vec![PlusIssue::text("persona.intro", "phone")])
        );
        let error = from_response(
            422,
            None,
            json!({"error": {"code": "plus_text_rejected", "field": "persona.status",
                "reason": "contact"}})
            .to_string()
            .as_bytes(),
        );
        let issues = refusal_issues(&error).unwrap();
        assert_eq!(issues[0].field, "persona.status");
        assert_eq!(issues[0].reason, "judged");
        assert_eq!(issues[0].problem.as_deref(), Some("contact"));
        let error = from_response(
            409,
            None,
            json!({"error": {"code": "plus_off"}})
                .to_string()
                .as_bytes(),
        );
        assert_eq!(refusal_issues(&error), None);
    }

    #[test]
    fn reads_the_qol_objects() {
        let page: PlusThreadPage = serde_json::from_value(json!({
            "lines": [{"at": 1, "from": "them", "text": "elytra var 45k"},
                      {"at": 2, "from": "them", "text": "…", "hidden": "unsafe"}],
            "handoffUntil": null, "canReply": true, "next": null
        }))
        .unwrap();
        assert!(page.can_reply);
        assert_eq!(page.lines[1].hidden.as_deref(), Some("unsafe"));
        let report: PlusWeeklyReport = serde_json::from_value(json!({
            "week": "2026-W43", "from": 1, "to": 2,
            "accounts": [{"id": "a_1", "name": "Ahmet_AFK", "pms": 14, "bought": 2, "sold": 1,
              "spent": 75000, "earned": 7200, "belowLimit": 2500, "aboveFloor": 400,
              "blocked": {"scam": 1, "rmt": 0, "unsafe": 2, "fake": 1}, "allowancePct": 38,
              "currency": "smp"}]
        }))
        .unwrap();
        assert_eq!(report.accounts[0].blocked.total(), 4);
        assert_eq!(report.accounts[0].below_limit, 2500);
        let draft: PlusParseAnswer = serde_json::from_value(json!({
            "draft": {"side": "buy", "item": null, "candidates": ["minecraft:elytra", "minecraft:diamond"],
              "qty": 1, "unit": "total", "target": 32000, "max": 40000, "style": "balanced",
              "approval": "above"},
            "issues": []
        }))
        .unwrap();
        assert_eq!(draft.draft.item, None);
        assert_eq!(draft.draft.candidates.len(), 2);
        assert_eq!(draft.draft.min, None);
        let prefs: CloudNotifyPrefs = serde_json::from_value(json!({
            "events": {"approval": true, "quota_80": false},
            "quiet": {"enabled": true, "from": "23:00", "to": "08:00"}
        }))
        .unwrap();
        assert_eq!(prefs.events.get("quota_80"), Some(&false));
        let approval: PlusApproval = serde_json::from_value(json!({
            "id": "01JBXK3D2E", "acct": "a_1",
            "deal": {"side": "buy", "item": "minecraft:elytra", "qty": 1, "price": 37500, "currency": "smp"},
            "counterpart": "Kaan_07", "why": ["above"], "createdAt": 1, "expiresAt": 601,
            "state": "pending"
        }))
        .unwrap();
        assert!(approval.is_pending());
        assert_eq!(approval.deal.price, 37500);
    }

    #[test]
    fn tables_get_plain_names_and_never_overwrite() {
        let dir = tempfile::tempdir().unwrap();
        let first = table_file_name(dir.path(), "Ahmet_AFK", "2026-10-24");
        assert_eq!(first, "afkplus-Ahmet_AFK-2026-10-24.csv");
        assert!(is_table_file_name(&first));
        std::fs::write(dir.path().join(&first), b"x").unwrap();
        let second = table_file_name(dir.path(), "Ahmet_AFK", "2026-10-24");
        assert_eq!(second, "afkplus-Ahmet_AFK-2026-10-24 (2).csv");
        assert!(is_table_file_name(&second));
        assert_eq!(
            table_file_name(dir.path(), "../x y", "2026/10/24"),
            "afkplus-___x_y-20261024.csv"
        );
        for bad in [
            "../afkplus-a.csv",
            "afkplus-a.exe",
            "afkplus-..csv",
            "x.csv",
            "afkplus-a\\b.csv",
        ] {
            assert!(!is_table_file_name(bad), "{bad}");
        }
    }

    #[test]
    fn paths_stay_in_their_routes() {
        assert!(CloudClient::thread_path("a_1", "Kaan_07", "/read").is_ok());
        assert!(CloudClient::thread_path("a_1", "../me", "").is_err());
        assert!(path_part("01JBXK3D2E", "approval").is_ok());
        assert!(path_part("../x", "approval").is_err());
        assert!(path_part("", "approval").is_err());
    }
}
