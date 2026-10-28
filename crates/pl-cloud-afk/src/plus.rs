//! AFK+, the AI tier of a cloud account (`services/cloud-afk/README.md` in the Studio repository,
//! "AFK+": the tier, the player's settings `AfkPlusConfig` v2 with their trade orders
//! ("Siparişler") and custom items, the player API, the live socket's AI messages, quotas and
//! notifications; the catalog-free design's reasons are in the maintainers' notes).
//!
//! The launcher shows and edits; the Worker and the account's machine decide. Nothing of AFK+ shows
//! while the service's `plusMode` is `off`: `Offer.plus` is null then and every AFK+ write answers
//! `409 plus_off`. Purchases, upgrades and plan changes happen on the site.
//!
//! - Objects: [`PlusOffer`], [`PlusAi`] (an account's AI state), [`PlusConfig`] with its
//!   [`PlusOrder`]s and [`PlusCustomItem`]s, [`PlusSettings`] (the settings view: the server's way
//!   of working, Studio's rules, each order's [`PlusOrderView`]), [`PlusQuota`], [`PlusApproval`]
//!   (a deal to OK, "Takas sende", a first sight), the audit log's [`PlusAuditEntry`], receipts,
//!   the QoL v1 objects and [`CloudNotifyPrefs`].
//! - The checks before a write are in `plus_check.rs` (the contract's reference, run on the
//!   settings vectors).
//! - The AFK+ routes of [`CloudClient`] (this module's `impl`).
//!
//! As everywhere in this crate, answers may gain fields (they are ignored) and values the launcher
//! does not know yet are kept as text. Amounts are whole numbers of the game mode's money (or a
//! paying item's count); times are Unix seconds unless a field says milliseconds.

use std::collections::BTreeMap;

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
    /// Studio's switches: a feature that is off is not shown.
    pub features: PlusFeatures,
    /// Where AFK+ runs: `any` (Studio's `features.anyServer`), `verified` the server keys where the
    /// bot trades itself (elsewhere it agrees and the player makes the trade).
    pub servers: PlusOfferServers,
    /// AFK+ extra credits ("Ek kontör"): the packs on sale; None while Studio has them off. The
    /// launcher only shows that they exist: buying happens on afk.projects.gg.
    pub topup: Option<PlusOfferTopup>,
}

/// `Offer.plus.topup` (cloud-afk README "AFK+ extra credits").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusOfferTopup {
    /// Packs can be bought now.
    pub sales: bool,
    pub packs: Vec<PlusTopupPack>,
    pub refund_window_hours: u32,
    pub max_balance: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusTopupPack {
    pub price_tl: String,
    pub credits: u64,
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

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusOfferServers {
    pub any: bool,
    pub verified: Vec<String>,
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

/// `Offer.plus.features` (README "AFK+ player API"): what to show. A switch an older service
/// does not name reads as off.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusFeatures {
    /// "Mesajlar" (Q1).
    pub inbox: bool,
    /// Typed orders ("Kontrol et").
    pub orders: bool,
    /// The audit log ("Akış").
    pub audit: bool,
    /// Custom items: "Envanterden seç", fingerprints, first sight.
    pub custom_items: bool,
    /// Paying with items (swaps).
    pub item_pay: bool,
    /// AFK+ on servers other than Projects.
    pub any_server: bool,
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
    /// What the assistant spent: only with a live machine answer.
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
/// `"profile"` (the mode's profile is not enabled); `by` who makes the trades of orders with
/// `trade: "auto"` (`bot` or `owner`); `buy: "notify"` without spending authority.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusTradeState {
    pub on: bool,
    pub off: Option<String>,
    pub by: String,
    pub buy: String,
}

/// What the assistant spent on its own buys in the game's money: `total` since the player last
/// reset it (against `limits.totalBudget`), `today` this AFK+ day (04:00 to 04:00 Turkey time,
/// against `limits.dailySpendMax`). A trade the owner made is the owner's money and never counts.
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
    /// Bought credits left (AFK+ extra credits); 0 from an older service.
    pub bought_left: u64,
}

/// `GET /accounts/:id/plus/quota` (`QuotaView`). Players see the unit as "kontör" ("AI credits").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusQuota {
    pub w5h: PlusQuotaWindow,
    pub week: PlusQuotaWindow,
    /// In spending order.
    pub gifts: Vec<PlusGift>,
    /// Bought packs with credits left, oldest first (AFK+ extra credits): spent after the windows
    /// and every gift.
    pub bought: PlusBought,
    pub promo: Option<PlusPromo>,
    pub frozen: bool,
    /// "Yaklaşık N yanıt kaldı" (gifts and bought credits included).
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

/// The bought packs with credits left and their sum (cloud-afk README "AFK+ extra credits").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusBought {
    pub remaining: u64,
    pub packs: Vec<PlusPack>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusPack {
    pub id: String,
    pub remaining: u64,
}

/// A promotion's multiplier (100-500 %), its words and its end.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusPromo {
    pub pct: u32,
    pub label: CloudLocalized,
    pub until: i64,
}

// --- the player's settings (`AfkPlusConfig` v2) ----------------------------------------------------

/// The player's AFK+ settings of one account. `PUT /plus` writes everything but the orders and
/// custom items (a settings screen never sends them back); orders go one at a time with the order
/// routes. Values with a closed set (`lang`, `authority`, an order's `style`, …) are kept as text,
/// so a value a newer service adds round-trips unchanged.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusConfig {
    pub v: u32,
    /// The player's AI switch ("Yapay zekâyı durdur"); false: plain AFK and templates only.
    pub enabled: bool,
    /// A server profile open for the account's server (`projects-smp`), or `generic`.
    pub mode: String,
    /// `auto` (English to players who write English, else Turkish), `tr`, `en`.
    pub lang: String,
    pub persona: PlusPersona,
    pub replies: PlusReplies,
    /// "Asistan ne yapsın?": `reply` (only answers and passes offers on), `ask` (bargains, then
    /// waits for the owner's OK; the default), `deal` (closes within the limits).
    pub authority: String,
    /// "Alış", "Satış": an order of a side switched off waits.
    pub allow: PlusAllow,
    /// What deals may be paid with: `money`, `items`, `both`.
    pub pay: String,
    /// "Kimlerle".
    pub who: PlusWho,
    /// At most `limits.orderStore` kept, `limits.orderMax` active.
    pub orders: Vec<PlusOrder>,
    pub custom_items: Vec<PlusCustomItem>,
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
            v: 2,
            enabled: true,
            mode: String::new(),
            lang: "auto".into(),
            persona: PlusPersona::default(),
            replies: PlusReplies::default(),
            authority: "ask".into(),
            allow: PlusAllow::default(),
            pay: "money".into(),
            who: PlusWho::default(),
            orders: Vec::new(),
            custom_items: Vec::new(),
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
    /// Whether the bot may spend money itself: without both limits a buy for money it would trade
    /// itself only reports what it finds (the owner's own trades need none).
    pub fn can_spend(&self) -> bool {
        self.limits.total_budget.is_some() && self.limits.per_deal_max.is_some()
    }

    /// The custom item a `custom:<k>` reference names.
    pub fn custom_item(&self, reference: &str) -> Option<&PlusCustomItem> {
        let key = reference.strip_prefix("custom:")?;
        self.custom_items.iter().find(|item| item.k == key)
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
    /// Small talk; "Konu dışına çıkmasın" is its opposite (off by default since 27 Oct 2026).
    pub small_talk: bool,
    /// "Oyuncu gibi konuşsun": chats like a player instead of in formal language.
    pub player_talk: bool,
    /// The tag before the assistant's lines: the player's choice, off by default.
    pub prefix: PlusPrefix,
}

impl Default for PlusPersona {
    fn default() -> Self {
        Self {
            preset: "friendly".into(),
            intro: String::new(),
            status: String::new(),
            status_preset: None,
            small_talk: false,
            player_talk: false,
            prefix: PlusPrefix::default(),
        }
    }
}

/// The tag before the assistant's lines (`persona.prefix`, the owner's decision of 27 Oct 2026):
/// off, with `[AFK+]` as its text, until the player sets one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusPrefix {
    pub on: bool,
    /// 1-16 code points: Latin letters, digits, spaces and a few marks; never staff or a player.
    pub text: String,
}

impl Default for PlusPrefix {
    fn default() -> Self {
        Self {
            on: false,
            text: crate::plus_check::PREFIX_DEFAULT_TEXT.into(),
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

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusAllow {
    pub buy: bool,
    pub sell: bool,
}

impl Default for PlusAllow {
    fn default() -> Self {
        Self {
            buy: true,
            sell: true,
        }
    }
}

/// "Kimlerle": `all`, or `friends` (only the in-game names in `friends`, at most 50).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusWho {
    pub mode: String,
    pub friends: Vec<String>,
}

impl Default for PlusWho {
    fn default() -> Self {
        Self {
            mode: "all".into(),
            friends: Vec::new(),
        }
    }
}

/// A trade order ("sipariş"): one kind of item, a quantity in all, a term. Its status and
/// progress are never stored ([`PlusOrderView`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusOrder {
    /// `^[a-z0-9]{1,8}$`, unique in the document (the Worker makes it).
    pub id: String,
    /// False: paused ("Durdur").
    pub on: bool,
    /// `buy` (the bot gets the item) or `sell` (the bot gives it).
    pub side: String,
    /// What changes hands: `minecraft:<path>` or `custom:<k>`.
    pub item: String,
    /// In all, 1-9999: the order is done when its progress reaches it.
    pub qty: u32,
    /// Conditions of what the bot receives (the item when buying, the paying item when selling for
    /// one); None exactly when selling for money.
    pub req: Option<PlusReq>,
    /// `money`, or the paying item's reference (a swap).
    pub pay: String,
    /// `each` or `total`: whether target and limit are per item or for the whole quantity.
    pub unit: String,
    /// The hoped price (money, or the paying item's count).
    pub target: u64,
    /// Buying the most, selling the least; never passed.
    pub limit: u64,
    /// `patient`, `balanced`, `quick`.
    pub style: String,
    /// `auto` (the assistant trades where the server's profile lets it) or `owner`.
    pub trade: String,
    /// Unix seconds: the order expires then.
    pub until: i64,
    /// A typed order's words, cleaned and masked: shown to the player only.
    pub note: String,
    /// `text`, `form`, `inv`.
    pub src: String,
    /// Created (Unix seconds).
    pub at: i64,
}

impl Default for PlusOrder {
    fn default() -> Self {
        Self {
            id: String::new(),
            on: true,
            side: "buy".into(),
            item: String::new(),
            qty: 1,
            req: Some(PlusReq::default()),
            pay: "money".into(),
            unit: "total".into(),
            target: 1,
            limit: 1,
            style: "balanced".into(),
            trade: "auto".into(),
            until: 0,
            note: String::new(),
            src: "form".into(),
            at: 0,
        }
    }
}

/// What the bot must receive: enchantments (id → lowest level), durability, an anvil name, curses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusReq {
    pub ench: BTreeMap<String, u8>,
    /// Lowest durability %, 0-100 (0 for an item that does not wear out).
    pub dur: u8,
    /// An anvil name is fine.
    pub named: bool,
    /// A curse is fine.
    pub cursed: bool,
}

impl Default for PlusReq {
    fn default() -> Self {
        Self {
            ench: BTreeMap::new(),
            dur: 90,
            named: false,
            cursed: false,
        }
    }
}

/// An item a server made (ItemsAdder, …), known only by its fingerprint.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusCustomItem {
    /// `^c[0-9a-z]{1,4}$`, chosen by the client that adds it.
    pub k: String,
    /// The player's name for it (1-32 code points, the persona text rule).
    pub name: String,
    /// Its material once seen (`minecraft:paper`).
    pub base: Option<String>,
    /// The identity fingerprint (32 hex); None: waits for its first sight.
    pub fp: Option<String>,
    /// How ItemSprite draws it (IA-ART's overlay).
    pub look: Option<PlusLook>,
    /// `inv`, `text`, `seen`.
    pub from: String,
    /// Added (Unix seconds).
    pub at: i64,
}

/// A custom item's picture keys: at least one of the two.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusLook {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cmd: Option<i64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub model: Option<String>,
}

/// Spending limits ("Harcama"). None: no limit, or for `per_deal_max` and `total_budget` no
/// spending authority at all.
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
    /// "Para birimi" (28 Oct 2026): the money's name the player gave, `Some(None)` cleared (written
    /// as null), None not in the document (a write leaves the stored one, and an older service
    /// knows no such key). It goes back as it came.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    #[specta(type = Option<String>)]
    pub currency: Option<Option<String>>,
}

/// A text that may be missing (None, the field's default) or null (`Some(None)`). Any other value
/// reads as missing, so one odd field never hides the settings.
fn present<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> std::result::Result<Option<Option<String>>, D::Error> {
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Some(None),
        serde_json::Value::String(text) => Some(Some(text)),
        _ => None,
    })
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

// --- the settings view ---------------------------------------------------------------------------

/// `GET /accounts/:id/plus`: the settings, how AFK+ works on the account's server, Studio's limits
/// and each order's state.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusSettings {
    pub config: PlusConfig,
    /// The revision (`If-Match` of the next write).
    pub ai_rev: u64,
    pub judge: PlusJudge,
    pub server: PlusServer,
    pub rules: PlusRules,
    /// Each order's progress and live state, by order id.
    pub orders: BTreeMap<String, PlusOrderView>,
    /// What the assistant spent; None when the machine cannot say now.
    pub spent: Option<PlusSpent>,
    /// The orders' states come from the machine's last snapshot of then.
    pub as_of: Option<i64>,
}

/// How AFK+ works on the account's server (`PlusServer`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusServer {
    /// `verified` (a Studio profile with a trade part: the bot trades), `profile` (a Studio profile
    /// without one), `generic` (the built-in one).
    pub kind: String,
    /// The profile in use (`projects-smp`, `generic`).
    pub mode: String,
    /// The mode's name; None for the generic profile.
    pub name: Option<CloudLocalized>,
    /// Who makes the trades of orders with `trade: "auto"`: `bot` or `owner`.
    pub trade: String,
    pub money: PlusServerMoney,
    /// The account's Minecraft version (its item data); None: the item data's default.
    pub mc: Option<String>,
    /// The mode's ad templates (none for the generic profile).
    pub ads: Vec<PlusAdTemplate>,
    /// False: AFK+ does not run here (`ai.reason` `server`).
    pub on: bool,
}

/// The game mode's money: `label` the name to write after amounts ("Coin"), the profile's or else
/// the player's own (`trade.currency`, the same in both languages); None: bare amounts and once
/// "Oyundaki parayla".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusServerMoney {
    pub label: Option<CloudLocalized>,
    /// The name is the profile's (Projects Survival and SkyBlock: ripple): "Para birimi" shows it
    /// read-only. False from an older service.
    #[serde(default)]
    pub fixed: bool,
    pub max_deal: u64,
    /// The money's key (receipts carry it).
    pub currency: String,
}

impl Default for PlusServerMoney {
    fn default() -> Self {
        Self {
            label: None,
            fixed: false,
            max_deal: GENERIC_MAX_DEAL,
            currency: String::new(),
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

/// The Studio limits a client checks by before it sends (`PlusRules`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusRules {
    pub order_max: u32,
    pub order_store: u32,
    pub custom_items_max: u32,
    pub order_days_default: u32,
    pub order_days_max: u32,
    /// A buy order's most a day, in stacks of its item.
    pub buy_stacks_per_day: u32,
    pub checks_per_day: u32,
    /// The typed checks left today, when the machine said.
    pub checks_left: Option<u32>,
    pub friends_max: u32,
    pub note_max: u32,
    pub text_max: u32,
    /// `[lowest, highest]` minutes.
    pub approval_timeout_min: Vec<u32>,
    pub features: PlusRulesFeatures,
}

impl Default for PlusRules {
    fn default() -> Self {
        Self {
            order_max: 10,
            order_store: 20,
            custom_items_max: 20,
            order_days_default: 14,
            order_days_max: 30,
            buy_stacks_per_day: 1,
            checks_per_day: 20,
            checks_left: None,
            friends_max: MAX_FRIENDS as u32,
            note_max: MAX_NOTE as u32,
            text_max: MAX_ORDER_TEXT as u32,
            approval_timeout_min: vec![1, 60],
            features: PlusRulesFeatures::default(),
        }
    }
}

/// `rules.features`: the order switches (a missing one reads as on, the service refuses in words)
/// and "Para birimi".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusRulesFeatures {
    pub orders: bool,
    pub audit: bool,
    pub custom_items: bool,
    pub item_pay: bool,
    /// "Para birimi" (`trade.currency`), off by default: a missing one (an older service) reads
    /// as off, so the field stays hidden.
    #[serde(default)]
    pub currency: bool,
}

impl Default for PlusRulesFeatures {
    fn default() -> Self {
        Self {
            orders: true,
            audit: true,
            custom_items: true,
            item_pay: false,
            currency: false,
        }
    }
}

/// An order's state: `got` its progress, `at` when it last grew (0: never), `live` the machine's
/// state of it (`waiting_ok`, `your_trade`, `trading`, `haggling`) and `status` the first of those,
/// `done`, `expired`, `paused`, `side_off` that holds, else `looking`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusOrderView {
    pub status: String,
    pub got: u32,
    pub at: i64,
    pub live: Option<String>,
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
    /// The server view as `GET` resolves it for the stored settings (28 Oct 2026: its money name
    /// follows `trade.currency`); None from an older service.
    pub server: Option<PlusServer>,
}

/// An order route's answer: the order (none for a delete), the stored settings and their
/// revision; `deferred` for a delete while the order's trade runs ("Takas bitince silinir.").
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusOrderWritten {
    pub order: Option<PlusOrder>,
    pub config: PlusConfig,
    pub ai_rev: u64,
    pub deferred: bool,
}

/// A new order as `POST …/plus/orders` takes it: without `id` and `at` (the Worker makes them),
/// or with both to put a deleted order back ("Geri al").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlusNewOrder {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<i64>,
    pub on: bool,
    pub side: String,
    pub item: String,
    pub qty: u32,
    pub req: Option<PlusReq>,
    pub pay: String,
    pub unit: String,
    pub target: u64,
    pub limit: u64,
    pub style: String,
    pub trade: String,
    pub until: i64,
    pub note: String,
    pub src: String,
}

impl PlusNewOrder {
    /// The order as stored, with the id and time it would get.
    pub fn stored(&self, id: &str, now: i64) -> PlusOrder {
        PlusOrder {
            id: self.id.clone().unwrap_or_else(|| id.to_owned()),
            on: self.on,
            side: self.side.clone(),
            item: self.item.clone(),
            qty: self.qty,
            req: self.req.clone(),
            pay: self.pay.clone(),
            unit: self.unit.clone(),
            target: self.target,
            limit: self.limit,
            style: self.style.clone(),
            trade: self.trade.clone(),
            until: self.until,
            note: self.note.clone(),
            src: self.src.clone(),
            at: self.at.unwrap_or(now),
        }
    }
}

/// A new custom item an order names, as the POST takes it (without `at`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PlusNewCustomItem {
    pub k: String,
    pub name: String,
    pub base: Option<String>,
    pub fp: Option<String>,
    pub look: Option<PlusLook>,
    pub from: String,
}

/// A chip's settings change saved with the order in the same write (`pay`; the single-buy cap).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlusOrderSettings {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub pay: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub per_deal_max: Option<u64>,
}

impl PlusOrderSettings {
    fn is_empty(&self) -> bool {
        self.pay.is_none() && self.per_deal_max.is_none()
    }

    fn body(&self) -> serde_json::Value {
        let mut out = serde_json::Map::new();
        if let Some(pay) = &self.pay {
            out.insert("pay".into(), serde_json::Value::String(pay.clone()));
        }
        if let Some(cap) = self.per_deal_max {
            out.insert("limits".into(), serde_json::json!({ "perDealMax": cap }));
        }
        serde_json::Value::Object(out)
    }
}

/// `PATCH …/plus/orders/:oid`: the fields that change ("Süreyi uzat" is `extend`); a field left
/// out stays as stored. `no_req` sends `req: null` (an order now sold for money has no conditions).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusOrderPatch {
    pub on: Option<bool>,
    pub qty: Option<u32>,
    pub req: Option<PlusReq>,
    pub no_req: bool,
    pub pay: Option<String>,
    pub unit: Option<String>,
    pub target: Option<u64>,
    pub limit: Option<u64>,
    pub style: Option<String>,
    pub trade: Option<String>,
    pub until: Option<i64>,
    pub extend: bool,
}

impl PlusOrderPatch {
    /// The PATCH body: only the fields that change.
    pub fn body(&self) -> Result<serde_json::Value> {
        let mut out = serde_json::Map::new();
        let mut put = |key: &str, value: serde_json::Value| {
            out.insert(key.to_owned(), value);
        };
        if let Some(on) = self.on {
            put("on", on.into());
        }
        if let Some(qty) = self.qty {
            put("qty", qty.into());
        }
        if self.no_req {
            put("req", serde_json::Value::Null);
        } else if let Some(req) = &self.req {
            put("req", to_body(req)?);
        }
        if let Some(pay) = &self.pay {
            put("pay", pay.clone().into());
        }
        if let Some(unit) = &self.unit {
            put("unit", unit.clone().into());
        }
        if let Some(target) = self.target {
            put("target", target.into());
        }
        if let Some(limit) = self.limit {
            put("limit", limit.into());
        }
        if let Some(style) = &self.style {
            put("style", style.clone().into());
        }
        if let Some(trade) = &self.trade {
            put("trade", trade.clone().into());
        }
        if let Some(until) = self.until {
            put("until", until.into());
        }
        if self.extend {
            put("extend", true.into());
        }
        Ok(serde_json::Value::Object(out))
    }

    /// The order as the patch leaves it (what the launcher checks before sending).
    pub fn applied(&self, order: &PlusOrder, now: i64, extend_days: u32) -> PlusOrder {
        let mut next = order.clone();
        if let Some(on) = self.on {
            next.on = on;
        }
        if let Some(qty) = self.qty {
            next.qty = qty;
        }
        if self.no_req {
            next.req = None;
        } else if let Some(req) = &self.req {
            next.req = Some(req.clone());
        }
        if let Some(pay) = &self.pay {
            next.pay = pay.clone();
        }
        if let Some(unit) = &self.unit {
            next.unit = unit.clone();
        }
        if let Some(target) = self.target {
            next.target = target;
        }
        if let Some(limit) = self.limit {
            next.limit = limit;
        }
        if let Some(style) = &self.style {
            next.style = style.clone();
        }
        if let Some(trade) = &self.trade {
            next.trade = trade.clone();
        }
        if let Some(until) = self.until {
            next.until = until;
        }
        if self.extend {
            next.until = now + i64::from(extend_days) * 86_400;
        }
        next
    }
}

/// `POST …/plus/items/pick`: a plain vanilla item gives its id and its enchantments and
/// durability as suggested conditions (`reference` + `req`); any other a custom item.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusPick {
    #[serde(rename = "ref")]
    pub reference: Option<String>,
    pub req: Option<PlusReq>,
    pub custom: Option<PlusPickedCustom>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusPickedCustom {
    /// The item's shown name, cleaned; None for an item without one ("Adsız eşya").
    pub name: Option<String>,
    pub base: String,
    pub fp: String,
    pub look: Option<PlusLook>,
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

// --- approvals, the audit log, receipts -----------------------------------------------------------------

/// Something waiting for the player (`state` `pending`), or one of the last 24 hours' answers.
/// `kind`: `deal` (OK the bot's deal), `handoff` ("Takas sende": the player makes the trade, open
/// 24 hours) or `confirm_item` ("Bu eşya aradığın … mi?").
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PlusApproval {
    pub id: String,
    pub acct: String,
    pub kind: String,
    /// The order it is for.
    pub order: Option<String>,
    /// `confirm_item`: the custom item asked about, as the order's item or its payment.
    pub custom: Option<PlusApprovalCustom>,
    pub deal: PlusApprovalDeal,
    /// In-game name.
    pub counterpart: String,
    /// `deal`: `above`, `new_counterpart`, `authority`, `price_unusual`, `budget`, `daily`.
    pub why: Vec<String>,
    /// `confirm_item`: the item in the window (plain text: the counterpart's words).
    pub seen: Option<PlusSeenItem>,
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
    /// Money, or the paying item's count.
    pub price: u64,
    /// `money` or the paying item.
    pub pay: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusApprovalCustom {
    pub k: String,
    /// `item` or `pay`.
    pub field: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusSeenItem {
    pub id: String,
    pub cmd: Option<i64>,
    pub model: Option<String>,
    pub name: Option<String>,
    /// Up to 4 lines of 80, plain and masked.
    pub lore: Vec<String>,
}

/// `GET /accounts/:id/plus/approvals`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusApprovalList {
    pub approvals: Vec<PlusApproval>,
}

/// A step of the audit log ("Akış"): every step the assistant took and how it ended, from the
/// machine (7 days), never from D1. Clients write the lines. `k`: `pm_in`, `reply`, `owner_msg`,
/// `offer`, `counter`, `agree`, `ask_owner`, `owner_ok`, `owner_no`, `owner_timeout`,
/// `trade_start`, `trade_done`, `trade_fail`, `handoff`, `handoff_done`, `handoff_no`, `cancel`,
/// `refuse`, `found`, `confirm_item`, `order_done`, `order_expired`, `paused`, `resumed`, `quota`,
/// `gate` (kept as text).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusAuditEntry {
    /// The account's sequence: the AFK+ day number × 1,000,000 + a counter.
    pub s: i64,
    /// Unix milliseconds.
    pub t: i64,
    pub k: String,
    /// The counterpart's in-game name.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub w: Option<String>,
    /// The order.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub o: Option<String>,
    /// The item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub i: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub q: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub p: Option<u64>,
    /// What the price counts: `money` or the paying item.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub u: Option<String>,
    /// Why it was refused, cancelled, failed or skipped.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub r: Option<String>,
    /// Text, masked (`pm_in`, `reply`, `owner_msg` only).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub x: Option<String>,
    /// Unsuitable: shown folded ("Göster").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub h: Option<bool>,
    /// The approval it opened or closed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub a: Option<String>,
}

impl PlusAuditEntry {
    /// The AFK+ day the entry belongs to (its sequence's day number).
    pub fn day(&self) -> i64 {
        self.s.div_euclid(AUDIT_DAY)
    }
}

/// The sequence's day factor.
pub const AUDIT_DAY: i64 = 1_000_000;

/// `GET …/plus/audit`: newest first; `more` when older ones are left; `from` the oldest AFK+ day
/// the machine keeps.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusAuditPage {
    pub items: Vec<PlusAuditEntry>,
    pub more: bool,
    pub from: Option<i64>,
}

/// A completed deal (and an unclear trade under review): D1, 90 days.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusReceipt {
    /// Unix milliseconds; with the account, its key.
    pub at: i64,
    /// None: a deal from before orders.
    pub order: Option<String>,
    pub side: String,
    pub item: String,
    pub qty: u32,
    /// Money, or the paying item's count.
    pub price: u64,
    /// `money` or the paying item.
    pub pay: String,
    /// The money's key (the label of `server.money` is written when they match); None for items.
    pub currency: Option<String>,
    pub counterpart: String,
    /// `auto`, `approved`, `owner` (the player reported the trade).
    pub approval: String,
    /// Waiting for staff: does not count as progress.
    pub review: bool,
    /// The order's bound for the deal's quantity.
    pub limit: Option<u64>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusReceiptPage {
    pub receipts: Vec<PlusReceipt>,
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

/// A saved revision of the settings (Q7). `by`: `site`, `launcher`, `phone`, `restore`, `studio`,
/// `confirm`, `player` (kept as text).
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

/// The trade summary of a range (Q4): receipts by side, item and what paid.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(default)]
pub struct PlusReceiptSummary {
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
    /// `money` or the paying item.
    pub pay: String,
    pub currency: Option<String>,
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
    pub currency: Option<String>,
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
    /// Buys: the limits minus what was paid.
    pub below_limit: u64,
    /// Sales: what came above the floors.
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
    /// All of them (the service's figures: never an overflow, however large).
    pub fn total(&self) -> u32 {
        self.scam
            .saturating_add(self.rmt)
            .saturating_add(self.unsafe_text)
            .saturating_add(self.fake)
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

// --- bounds -----------------------------------------------------------------------------------------

/// The generic profile's `money.maxDeal` (a Studio profile has its own).
pub const GENERIC_MAX_DEAL: u64 = 100_000_000;
pub const MAX_BLOCKED: usize = 100;
pub const MAX_FRIENDS: usize = 50;
pub const MAX_WINDOWS: usize = 7;
pub const MAX_INTRO: usize = 60;
pub const MAX_STATUS: usize = 80;
/// Quantities and a paying item's counts.
pub const MAX_COUNT: u32 = 9_999;
/// A break reaches at most this far ahead.
pub const MAX_PAUSE_SECONDS: i64 = 90 * 86_400;
/// The owner's reply in "Mesajlar" (code points).
pub const MAX_REPLY: usize = 200;
/// A typed order, after cleaning (code points).
pub const MAX_ORDER_TEXT: usize = 200;
/// The shortest typed order worth checking (code points).
pub const MIN_ORDER_TEXT: usize = 3;
/// An order's note (code points).
pub const MAX_NOTE: usize = 200;
/// A custom item's name (code points).
pub const CUSTOM_NAME_MAX: usize = 32;

/// One problem of a config, where the contract names it: `field` a path (`orders[2].limit`,
/// `limits.approvalAbove`, `persona.intro`, `schedule.windows[0].from`), `reason` the contract's
/// (`range`, `order`, `id`, `item`, `limits`, `name`, `text`, `limit_rows`, `limit_orders`,
/// `mode`, `row`, `missing`, `type`, `unknown`, `pay`, `same`, `req`, `feature`, `until`,
/// `blocked`, `fp`), `problem` the text rule's word with `text` (`length`, `marker`, `link`, …).
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

/// A refusal of a settings or order write about its contents, as issues.
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
                "orders".into()
            } else {
                field
            },
            "limit_rows",
        )]),
        // Too many active (`scope: active`) or kept (`stored`) orders.
        "limit_orders" => Some(vec![PlusIssue {
            field: "orders".into(),
            reason: "limit_orders".into(),
            problem: error.extra_str("scope").map(str::to_owned),
        }]),
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

/// `<id>` of an approval (a ULID), an order or a history revision, refusing anything that could
/// leave the route.
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

fn if_match(rev: u64) -> [(reqwest::header::HeaderName, String); 1] {
    [(IF_MATCH, format!("\"{rev}\""))]
}

fn to_body<T: Serialize>(value: &T) -> Result<serde_json::Value> {
    serde_json::to_value(value).map_err(|e| CloudError::Protocol(e.to_string()))
}

impl CloudClient {
    /// `GET /accounts/:id/plus`.
    pub async fn plus_settings(&self, token: &Bearer, id: &str) -> Result<PlusSettings> {
        self.get(Some(token), &account_path(id, "/plus")?).await
    }

    /// `PUT /accounts/:id/plus` with `If-Match: "<rev>"`: everything but the orders and custom
    /// items (the stored ones stay). A stale `rev` is `412 rev_conflict`.
    pub async fn put_plus(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        config: &PlusConfig,
    ) -> Result<PlusWritten> {
        let mut body = to_body(config)?;
        if let Some(object) = body.as_object_mut() {
            object.remove("orders");
            object.remove("customItems");
        }
        self.send(
            Method::PUT,
            &account_path(id, "/plus")?,
            Some(token),
            Some(body),
            &if_match(rev),
            Repeat::Never,
        )
        .await?
        .into_result()?
        .json()
    }

    /// "Kontrol et": `POST /accounts/:id/plus/orders/check {text, lang}`. The answer's JSON as it
    /// came (`{draft, questions, notes, left}`): the panel answers the questions on it locally, so
    /// it must reach the page exactly as the service wrote it. Counted per minute and per day, so
    /// never repeated.
    pub async fn check_order(
        &self,
        token: &Bearer,
        id: &str,
        text: &str,
        lang: &str,
    ) -> Result<String> {
        let lang = if lang == "en" { "en" } else { "tr" };
        let reply = self
            .send(
                Method::POST,
                &account_path(id, "/plus/orders/check")?,
                Some(token),
                Some(serde_json::json!({ "text": text, "lang": lang })),
                &[],
                Repeat::Never,
            )
            .await?
            .into_result()?;
        let answer: serde_json::Value = reply.json()?;
        if !answer
            .get("draft")
            .is_some_and(serde_json::Value::is_object)
        {
            return Err(CloudError::Protocol("a check without a draft".into()));
        }
        Ok(answer.to_string())
    }

    /// `POST /accounts/:id/plus/orders` + `If-Match`: a new order (or a deleted one put back), the
    /// new custom items it names and a chip's settings change, in one write.
    pub async fn create_order(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        order: &PlusNewOrder,
        custom_items: &[PlusNewCustomItem],
        settings: &PlusOrderSettings,
    ) -> Result<PlusOrderWritten> {
        let mut body = serde_json::json!({ "order": to_body(order)? });
        if !custom_items.is_empty() {
            body["customItems"] = to_body(&custom_items)?;
        }
        if !settings.is_empty() {
            body["settings"] = settings.body();
        }
        self.send(
            Method::POST,
            &account_path(id, "/plus/orders")?,
            Some(token),
            Some(body),
            &if_match(rev),
            Repeat::Never,
        )
        .await?
        .into_result()?
        .json()
    }

    /// `PATCH /accounts/:id/plus/orders/:oid` + `If-Match`.
    pub async fn patch_order(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        order: &str,
        patch: &PlusOrderPatch,
    ) -> Result<PlusOrderWritten> {
        path_part(order, "order")?;
        self.send(
            Method::PATCH,
            &account_path(id, &format!("/plus/orders/{order}"))?,
            Some(token),
            Some(patch.body()?),
            &if_match(rev),
            Repeat::Never,
        )
        .await?
        .into_result()?
        .json()
    }

    /// `DELETE /accounts/:id/plus/orders/:oid` + `If-Match`; `deferred` while its trade runs.
    pub async fn delete_order(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        order: &str,
    ) -> Result<PlusOrderWritten> {
        path_part(order, "order")?;
        self.send(
            Method::DELETE,
            &account_path(id, &format!("/plus/orders/{order}"))?,
            Some(token),
            None,
            &if_match(rev),
            Repeat::Never,
        )
        .await?
        .into_result()?
        .json()
    }

    /// "Envanterden seç": `POST /accounts/:id/plus/items/pick {slot, id, cmd?, model?}` (the slot
    /// as the live inventory shows it, the item seen there and its look).
    pub async fn pick_item(
        &self,
        token: &Bearer,
        id: &str,
        slot: u32,
        item: &str,
        cmd: Option<i64>,
        model: Option<&str>,
    ) -> Result<PlusPick> {
        if !(5..=45).contains(&slot) {
            return Err(CloudError::Invalid("inventory slot"));
        }
        let mut body = serde_json::json!({ "slot": slot, "id": item });
        if let Some(cmd) = cmd {
            body["cmd"] = serde_json::Value::from(cmd);
        }
        if let Some(model) = model {
            body["model"] = serde_json::Value::String(model.to_owned());
        }
        #[derive(Deserialize)]
        struct Answer {
            pick: PlusPick,
        }
        let answer: Answer = self
            .send(
                Method::POST,
                &account_path(id, "/plus/items/pick")?,
                Some(token),
                Some(body),
                &[],
                Repeat::Safe,
            )
            .await?
            .into_result()?
            .json()?;
        Ok(answer.pick)
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

    /// `GET /accounts/:id/plus/audit` (live from the machine; `503 host_offline`): newest first;
    /// `before` older than an entry's `s`, `after` the newest past one; `group` `deals`,
    /// `messages` or `warnings`.
    pub async fn plus_audit(
        &self,
        token: &Bearer,
        id: &str,
        before: Option<i64>,
        after: Option<i64>,
        limit: Option<u32>,
        group: Option<&str>,
    ) -> Result<PlusAuditPage> {
        let group = group.filter(|group| matches!(*group, "deals" | "messages" | "warnings"));
        let path = format!(
            "{}{}",
            account_path(id, "/plus/audit")?,
            query(&[
                ("before", before.map(|s| s.to_string())),
                ("after", after.map(|s| s.to_string())),
                ("limit", limit.map(|n| n.clamp(1, 200).to_string())),
                ("group", group.map(str::to_owned)),
            ])
        );
        self.get(Some(token), &path).await
    }

    /// `GET /accounts/:id/plus/receipts` (D1, 90 days), newest first.
    pub async fn plus_receipts(
        &self,
        token: &Bearer,
        id: &str,
        before: Option<&str>,
    ) -> Result<PlusReceiptPage> {
        let path = format!(
            "{}{}",
            account_path(id, "/plus/receipts")?,
            query(&[("before", before.map(str::to_owned))])
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

    /// "Ayar geçmişi" (Q7): newest first.
    pub async fn plus_history(&self, token: &Bearer, id: &str) -> Result<PlusHistory> {
        self.get(Some(token), &account_path(id, "/plus/history")?)
            .await
    }

    /// "Bu hale dön": the revision `rev` becomes the next one (`If-Match: "<current>"`). The
    /// settings come back; the orders and custom items stay as they are now.
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
            &if_match(current),
            Repeat::Never,
        )
        .await?
        .into_result()?
        .json()
    }

    /// The trade summary of `from..to` (Unix seconds; the service's default: the last 30 days).
    pub async fn plus_receipt_summary(
        &self,
        token: &Bearer,
        id: &str,
        from: Option<i64>,
        to: Option<i64>,
    ) -> Result<PlusReceiptSummary> {
        let path = format!(
            "{}{}",
            account_path(id, "/plus/receipts/summary")?,
            query(&[
                ("from", from.map(|at| at.to_string())),
                ("to", to.map(|at| at.to_string())),
            ])
        );
        self.get(Some(token), &path).await
    }

    /// "Tabloyu indir": the receipts of `from..to` as CSV (UTF-8 with a byte order mark; Turkish
    /// headers with `;`, or English with `,`).
    pub async fn plus_receipts_csv(
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
            account_path(id, "/plus/receipts.csv")?,
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
        self.send(
            Method::PUT,
            "me/notifications",
            Some(token),
            Some(to_body(prefs)?),
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
            "trade": {"on": true, "by": "owner", "buy": "notify"}, "pendingApprovals": 2,
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
        assert_eq!(
            (ai.trade.by.as_str(), ai.trade.buy.as_str()),
            ("owner", "notify")
        );
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
    fn reads_bought_credits_and_the_packs_on_sale() {
        // cloud-afk README "AFK+ extra credits": the view's packs, the brief's sum, the offer's packs.
        let quota: PlusQuota = serde_json::from_value(json!({
            "w5h": {"used": 5000, "cap": 5000, "resetAt": 1792418400},
            "week": {"used": 12800, "cap": 20000, "resetAt": 1792807200},
            "gifts": [], "promo": null, "frozen": false, "estReplies": 615,
            "bought": {"remaining": 12300, "packs": [{"id": "gr_0P8Y9A0BUY01", "remaining": 12300}]},
            "pct5": 100, "pctWeek": 64, "asOf": 1792400000
        }))
        .unwrap();
        assert_eq!(quota.bought.remaining, 12_300);
        assert_eq!(quota.bought.packs[0].id, "gr_0P8Y9A0BUY01");
        // An older service says nothing of them: none.
        let old: PlusQuotaBrief = serde_json::from_value(json!({
            "pct5": 100, "pctWeek": 41, "w5hResetAt": null, "weekResetAt": 1792807200,
            "exhaustedUntil": null, "giftsLeft": 0
        }))
        .unwrap();
        assert_eq!(old.bought_left, 0);
        let offer: Offer = serde_json::from_value(json!({
            "mode": "on",
            "plus": {"mode": "on", "sales": true, "prices": [], "features": {},
              "topup": {"sales": true, "packs": [{"priceTl": "10.00", "credits": 14500}],
                "refundWindowHours": 336, "maxBalance": 1000000}}
        }))
        .unwrap();
        let topup = offer.plus.unwrap().topup.unwrap();
        assert!(topup.sales);
        assert_eq!(topup.packs[0].credits, 14_500);
    }

    #[test]
    fn the_offer_says_whether_afk_plus_shows_and_where() {
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
              "features": {"inbox": true, "orders": true, "audit": true, "customItems": true,
                "itemPay": false, "anyServer": false, "schedule": true, "report": false,
                "history": true, "saver": true, "notifyDigest": false, "handoff": false, "watch": false},
              "servers": {"any": false, "verified": ["projects.gg"]}}
        }))
        .unwrap();
        let plus = offer.plus.unwrap();
        assert_eq!(plus.mode, "invite");
        assert_eq!(plus.month_price().unwrap().price_tl, "200.00");
        assert_eq!(plus.stock.remaining, Some(7));
        assert!(plus.features.inbox && plus.features.orders && !plus.features.report);
        assert!(!plus.features.item_pay && !plus.servers.any);
        assert_eq!(plus.servers.verified, ["projects.gg"]);
        let me: Me = serde_json::from_value(json!({
            "user": {"id": "1", "username": "ahmet"},
            "plus": {"canBuy": {"ok": false, "reason": "plus_invite_only"}, "restricted": null}
        }))
        .unwrap();
        let plus = me.plus.unwrap();
        assert!(!plus.can_buy.ok);
        assert_eq!(plus.can_buy.reason.as_deref(), Some("plus_invite_only"));
    }

    /// The contract's defaults (README "AfkPlusConfig v2"), with one order and one custom item.
    fn document() -> serde_json::Value {
        json!({"v": 2, "enabled": true, "mode": "projects-smp", "lang": "auto",
          "persona": {"preset": "friendly", "intro": "", "status": "", "statusPreset": null, "smallTalk": true,
            "playerTalk": false, "prefix": {"on": false, "text": "[AFK+]"}},
          "replies": {"scope": "trade", "away": true, "saveAt": 80},
          "authority": "ask", "allow": {"buy": true, "sell": true}, "pay": "money",
          "who": {"mode": "all", "friends": []},
          "orders": [{"id": "o1", "on": true, "side": "buy", "item": "minecraft:elytra", "qty": 1,
            "req": {"ench": {"minecraft:unbreaking": 3}, "dur": 90, "named": false, "cursed": false},
            "pay": "money", "unit": "total", "target": 32000, "limit": 40000, "style": "balanced",
            "trade": "auto", "until": 1794109600, "note": "Elytra alırım, en fazla 40 bin",
            "src": "text", "at": 1792900000},
            {"id": "o2", "on": true, "side": "sell", "item": "custom:c1", "qty": 5, "req": null,
            "pay": "money", "unit": "each", "target": 5000, "limit": 4500, "style": "patient",
            "trade": "owner", "until": 1794109600, "note": "", "src": "inv", "at": 1792900000}],
          "customItems": [{"k": "c1", "name": "Yakut", "base": "minecraft:paper",
            "fp": "0123456789abcdef0123456789abcdef", "look": {"cmd": 10001}, "from": "inv",
            "at": 1792900000}],
          "limits": {"perDealMax": null, "dailySpendMax": null, "totalBudget": null, "balanceFloor": 0,
            "approvalAbove": 50000, "newCounterpartApprovalAbove": null, "approvalTimeoutMin": 10},
          "discovery": {"scanChat": true, "proactivePm": true, "ads": false, "adTemplate": "buy-basic"},
          "trade": {"returnRow": null}, "summaryAt": "21:00", "blocked": [],
          "schedule": {"on": false, "windows": []}, "pauseUntil": null})
    }

    #[test]
    fn the_config_round_trips_in_the_contracts_form() {
        let config: PlusConfig = serde_json::from_value(document()).unwrap();
        assert_eq!(serde_json::to_value(&config).unwrap(), document());
        assert!(!config.can_spend());
        assert_eq!(config.orders[1].req, None);
        assert_eq!(config.custom_item("custom:c1").unwrap().name, "Yakut");
        assert_eq!(config.custom_item("minecraft:paper"), None);
        assert_eq!(
            config.custom_items[0].look,
            Some(PlusLook {
                cmd: Some(10001),
                model: None
            })
        );
        // A look writes only the keys it has (`model: null` would be refused).
        assert_eq!(
            serde_json::to_value(&config.custom_items[0].look).unwrap(),
            json!({"cmd": 10001})
        );
    }

    #[test]
    fn reads_the_settings_view() {
        let settings: PlusSettings = serde_json::from_value(json!({
            "config": document(), "aiRev": 4,
            "judge": {"intro": {"state": "ok", "at": 1}, "status": {"state": "ok", "at": 1}},
            "server": {"kind": "generic", "mode": "generic", "name": null, "trade": "owner",
              "money": {"label": null, "maxDeal": 100000000, "currency": "money"}, "mc": "26.3",
              "ads": [], "on": true},
            "rules": {"orderMax": 10, "orderStore": 20, "customItemsMax": 20, "orderDaysDefault": 14,
              "orderDaysMax": 30, "buyStacksPerDay": 1, "checksPerDay": 20, "checksLeft": 17,
              "friendsMax": 50, "noteMax": 200, "textMax": 200, "approvalTimeoutMin": [1, 60],
              "features": {"orders": true, "audit": true, "customItems": true, "itemPay": false}},
            "orders": {"o1": {"status": "haggling", "got": 0, "at": 0, "live": "haggling"},
              "o2": {"status": "your_trade", "got": 2, "at": 1792950000, "live": "your_trade"}},
            "spent": null, "asOf": 1792950100
        }))
        .unwrap();
        assert_eq!(settings.server.trade, "owner");
        assert_eq!(settings.server.money.label, None);
        assert_eq!(settings.server.mc.as_deref(), Some("26.3"));
        assert_eq!(settings.rules.checks_left, Some(17));
        assert!(!settings.rules.features.item_pay);
        assert_eq!(settings.orders["o2"].got, 2);
        assert_eq!(settings.orders["o1"].live.as_deref(), Some("haggling"));
        assert_eq!(settings.as_of, Some(1792950100));
        // Without the newer parts (an older service), the defaults hold.
        let settings: PlusSettings =
            serde_json::from_value(json!({"config": document(), "aiRev": 1})).unwrap();
        assert_eq!(settings.rules, PlusRules::default());
        assert_eq!(settings.server.money.max_deal, GENERIC_MAX_DEAL);
    }

    #[test]
    fn the_money_name_goes_back_as_it_came() {
        // Not in the document (no name stored, or an older service): a write leaves it out.
        let config: PlusConfig = serde_json::from_value(document()).unwrap();
        assert_eq!(config.trade.currency, None);
        let written = serde_json::to_value(&config).unwrap();
        assert_eq!(written["trade"], json!({"returnRow": null}));
        // A name, and a cleared one, are written as they came.
        for (sent, read) in [
            (json!("TL"), Some(Some("TL".to_owned()))),
            (serde_json::Value::Null, Some(None)),
        ] {
            let mut doc = document();
            doc["trade"]["currency"] = sent;
            let config: PlusConfig = serde_json::from_value(doc.clone()).unwrap();
            assert_eq!(config.trade.currency, read);
            assert_eq!(serde_json::to_value(&config).unwrap(), doc);
        }
        // An odd value reads as missing: nothing is written back.
        let mut doc = document();
        doc["trade"]["currency"] = json!(5);
        let config: PlusConfig = serde_json::from_value(doc).unwrap();
        assert_eq!(config.trade.currency, None);
    }

    #[test]
    fn reads_whose_money_name_shows() {
        let settings: PlusSettings = serde_json::from_value(json!({
            "config": document(), "aiRev": 4,
            "server": {"kind": "verified", "mode": "projects-survival",
              "name": {"tr": "Survival", "en": "Survival"}, "trade": "bot",
              "money": {"label": {"tr": "ripple", "en": "ripple"}, "fixed": true,
                "maxDeal": 1000000, "currency": "survival"},
              "ads": [], "on": true},
            "rules": {"features": {"orders": true, "audit": true, "customItems": true,
              "itemPay": false, "currency": true}}
        }))
        .unwrap();
        assert!(settings.server.money.fixed);
        assert!(settings.rules.features.currency);
        // An older service names neither: nothing is fixed and the field stays hidden, while the
        // order switches it leaves out still read as on.
        let settings: PlusSettings = serde_json::from_value(json!({
            "config": document(), "aiRev": 4,
            "server": {"money": {"label": {"tr": "ripple", "en": "ripple"}, "maxDeal": 1000000,
              "currency": "survival"}},
            "rules": {"features": {"itemPay": false}}
        }))
        .unwrap();
        assert!(!settings.server.money.fixed);
        assert!(!settings.rules.features.currency);
        assert!(settings.rules.features.orders && settings.rules.features.audit);
    }

    #[test]
    fn writes_orders_in_the_routes_form() {
        let order = PlusNewOrder {
            id: None,
            at: None,
            on: true,
            side: "sell".into(),
            item: "minecraft:diamond".into(),
            qty: 64,
            req: None,
            pay: "money".into(),
            unit: "each".into(),
            target: 120,
            limit: 100,
            style: "patient".into(),
            trade: "auto".into(),
            until: 1794109600,
            note: String::new(),
            src: "form".into(),
        };
        let body = to_body(&order).unwrap();
        assert!(body.get("id").is_none() && body.get("at").is_none());
        assert_eq!(body["req"], serde_json::Value::Null);
        assert_eq!(order.stored("n1", 5).id, "n1");
        assert_eq!(order.stored("n1", 5).at, 5);
        let back = PlusNewOrder {
            id: Some("o9".into()),
            at: Some(3),
            ..order
        };
        assert_eq!(to_body(&back).unwrap()["id"], json!("o9"));
        assert_eq!(back.stored("n1", 5).id, "o9");

        let patch = PlusOrderPatch {
            on: Some(false),
            ..PlusOrderPatch::default()
        };
        assert_eq!(patch.body().unwrap(), json!({"on": false}));
        let patch = PlusOrderPatch {
            pay: Some("money".into()),
            no_req: true,
            extend: true,
            ..PlusOrderPatch::default()
        };
        assert_eq!(
            patch.body().unwrap(),
            json!({"req": null, "pay": "money", "extend": true})
        );
        let stored: PlusConfig = serde_json::from_value(document()).unwrap();
        let next = patch.applied(&stored.orders[0], 100, 14);
        assert_eq!((next.req, next.until), (None, 100 + 14 * 86_400));
        assert_eq!(
            PlusOrderSettings {
                pay: Some("both".into()),
                per_deal_max: Some(60000)
            }
            .body(),
            json!({"pay": "both", "limits": {"perDealMax": 60000}})
        );
        assert!(PlusOrderSettings::default().is_empty());
    }

    #[test]
    fn reads_picks_approvals_audit_and_receipts() {
        let pick: PlusPick = serde_json::from_value(json!({"ref": "minecraft:diamond_sword",
          "req": {"ench": {"minecraft:sharpness": 5}, "dur": 80, "named": false, "cursed": false}}))
        .unwrap();
        assert_eq!(pick.reference.as_deref(), Some("minecraft:diamond_sword"));
        assert_eq!(pick.req.unwrap().dur, 80);
        let pick: PlusPick = serde_json::from_value(json!({"custom": {"name": null,
          "base": "minecraft:paper", "fp": "0123456789abcdef0123456789abcdef",
          "look": {"model": "projects:ruby"}}}))
        .unwrap();
        let custom = pick.custom.unwrap();
        assert_eq!(custom.name, None);
        assert_eq!(custom.look.unwrap().model.as_deref(), Some("projects:ruby"));

        let approval: PlusApproval = serde_json::from_value(json!({
            "id": "01JBXK3D2E", "acct": "a_1", "kind": "confirm_item", "order": "o2",
            "custom": {"k": "c1", "field": "item"},
            "deal": {"side": "buy", "item": "custom:c1", "qty": 2, "price": 9000, "pay": "money"},
            "counterpart": "Kaan_07", "why": [],
            "seen": {"id": "minecraft:paper", "cmd": 10001, "name": "Yakut", "lore": ["Nadir"]},
            "createdAt": 1, "expiresAt": 601, "state": "pending"
        }))
        .unwrap();
        assert!(approval.is_pending());
        assert_eq!(approval.kind, "confirm_item");
        assert_eq!(approval.custom.unwrap().field, "item");
        assert_eq!(approval.seen.unwrap().lore, ["Nadir"]);

        let page: PlusAuditPage = serde_json::from_value(json!({
            "items": [{"s": 20387000014i64, "t": 1792950000000i64, "k": "agree", "w": "Kaan_07",
              "o": "o1", "i": "minecraft:elytra", "q": 1, "p": 37500, "u": "money"},
              {"s": 20387000013i64, "t": 1792949000000i64, "k": "pm_in", "w": "Kaan_07",
              "x": "elytra var 45k", "h": true}],
            "more": true, "from": 20381
        }))
        .unwrap();
        assert_eq!(page.items[0].day(), 20387);
        assert_eq!(page.items[1].h, Some(true));
        // The cache writes only the keys an entry has.
        assert_eq!(
            serde_json::to_value(&page.items[1]).unwrap(),
            json!({"s": 20387000013i64, "t": 1792949000000i64, "k": "pm_in", "w": "Kaan_07",
              "x": "elytra var 45k", "h": true})
        );

        let receipts: PlusReceiptPage = serde_json::from_value(json!({
            "receipts": [{"at": 1792950000000i64, "order": "o2", "side": "sell", "item": "custom:c1",
              "qty": 2, "price": 9000, "pay": "money", "currency": "money", "counterpart": "Kaan_07",
              "approval": "owner", "review": false}],
            "next": null
        }))
        .unwrap();
        assert_eq!(receipts.receipts[0].approval, "owner");
        assert_eq!(receipts.receipts[0].limit, None);
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
        let refusal = |status: u16, body: serde_json::Value| {
            from_response(status, None, body.to_string().as_bytes())
        };
        assert_eq!(
            refusal_issues(&refusal(
                400,
                json!({"error": {"code": "invalid_plus_config", "field": "orders[0].limit",
                    "reason": "order"}})
            )),
            Some(vec![PlusIssue::new("orders[0].limit", "order")])
        );
        assert_eq!(
            refusal_issues(&refusal(
                400,
                json!({"error": {"code": "invalid_plus_config", "field": "customItems[0].name",
                    "reason": "text", "problem": "link"}})
            )),
            Some(vec![PlusIssue::text("customItems[0].name", "link")])
        );
        let issues = refusal_issues(&refusal(
            422,
            json!({"error": {"code": "limit_orders", "scope": "active", "max": 10}}),
        ))
        .unwrap();
        assert_eq!(
            (issues[0].reason.as_str(), issues[0].problem.as_deref()),
            ("limit_orders", Some("active"))
        );
        let issues = refusal_issues(&refusal(
            422,
            json!({"error": {"code": "plus_text_rejected", "field": "persona.status",
                "reason": "contact"}}),
        ))
        .unwrap();
        assert_eq!(issues[0].field, "persona.status");
        assert_eq!(issues[0].reason, "judged");
        assert_eq!(issues[0].problem.as_deref(), Some("contact"));
        assert_eq!(
            refusal_issues(&refusal(409, json!({"error": {"code": "plus_off"}}))),
            None
        );
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
