//! The player API's objects (`services/cloud-afk/README.md` in the Studio repository, "Player
//! API" → "Objects": the normative contract), as the launcher reads and writes them.
//!
//! Answers may gain fields at any time: unknown fields are ignored, and values the launcher may
//! not know yet (a new `live.state`, a new trigger, a new hold) are kept as text or fall back to the
//! most generic value of their kind, so one new value never hides every account. Times are Unix
//! seconds; money is a decimal string in Turkish lira (`"60.00"`).

use serde::{Deserialize, Serialize};

use crate::feat::{CloudAccountFeatures, CloudInventorySetting, OfferFeatures};
use crate::plus::{MePlus, PlusAi, PlusOffer};

/// Turkish and English words written in Studio (a maintenance note, a block list reason).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudLocalized {
    pub tr: String,
    pub en: String,
}

/// `Offer.mode`. Anything this launcher does not know reads as `Off` (nothing is shown).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ServiceMode {
    On,
    Paused,
    #[default]
    #[serde(other)]
    Off,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudSales {
    pub new_accounts: bool,
    pub renewals: bool,
}

/// One plan's price: `plan` is `month` or `year`, `price_tl` a decimal string (`"60.00"`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudPlanPrice {
    pub plan: String,
    pub price_tl: String,
    pub days: u32,
}

/// `remaining` only below Studio's "Son N yer" threshold.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudStock {
    pub sold_out: bool,
    pub remaining: Option<u32>,
}

/// The Studio numbers the launcher's editor checks against before the service does.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudLimits {
    pub per_player_cap: u32,
    pub server_changes_per_hour: u32,
    pub max_command_rows: u32,
    pub max_rules: u32,
    pub min_interval_sec: u32,
    pub min_rule_cooldown_sec: u32,
    pub console_lines_per_minute: u32,
}

impl ServiceMode {
    /// `on`, `paused` or `off`, as the UI reads it.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::On => "on",
            Self::Paused => "paused",
            Self::Off => "off",
        }
    }
}

impl Default for CloudLimits {
    /// The contract's defaults (README, "Settings").
    fn default() -> Self {
        Self {
            per_player_cap: 10,
            server_changes_per_hour: 6,
            max_command_rows: 20,
            max_rules: 10,
            min_interval_sec: 60,
            min_rule_cooldown_sec: 10,
            console_lines_per_minute: 20,
        }
    }
}

/// `GET /offer`: what the service sells now, readable without a session.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Offer {
    pub mode: ServiceMode,
    /// None when both languages are empty.
    pub maintenance_message: Option<CloudLocalized>,
    pub sales: CloudSales,
    /// Month first.
    pub prices: Vec<CloudPlanPrice>,
    pub stock: CloudStock,
    pub renewal_window_days: u32,
    pub grace_days: u32,
    pub recommended_version: String,
    pub limits: CloudLimits,
    /// AFK+ as sold now; None while the service's `plusMode` is `off` (nothing of AFK+ shows).
    pub plus: Option<PlusOffer>,
    /// Studio's switches of the cloud features (CAFK-FEAT); None from a service before them.
    pub features: Option<OfferFeatures>,
}

impl Offer {
    /// The monthly price, the intro's "Aylık 60 TL".
    pub fn month_price(&self) -> Option<&CloudPlanPrice> {
        self.prices
            .iter()
            .find(|price| price.plan == "month")
            .or_else(|| self.prices.first())
    }
}

/// `GET /me` → `user`: the player's Discord account (the friends session's).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct MeUser {
    /// The Discord id: part of every seal's AAD, never shown.
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudAccountCount {
    pub count: u32,
    pub cap: u32,
}

/// Whether the player may buy a new account now, and if not, why (a refusal key of the contract,
/// kept as text).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudCanBuy {
    pub ok: bool,
    pub reason: Option<String>,
    pub until: Option<i64>,
}

/// Cloud AFK's own ban of the player.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CloudRestriction {
    pub until: Option<i64>,
    pub reason: String,
}

/// `GET /me`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Me {
    pub user: MeUser,
    pub accounts: CloudAccountCount,
    pub can_buy: CloudCanBuy,
    pub restricted: Option<CloudRestriction>,
    pub now: i64,
    /// Whether this player may buy AFK+, and AFK+'s own restriction of them.
    pub plus: Option<MePlus>,
}

/// A server address as the service stores it: `host` normalized (lowercase ASCII, punycode),
/// `port` 0 for "the client finds it" (SRV record, then 25565).
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
pub struct CloudServer {
    pub host: String,
    pub port: u16,
}

impl CloudServer {
    pub fn new(host: impl Into<String>, port: u16) -> Self {
        Self {
            host: host.into(),
            port,
        }
    }

    /// `play.projects.gg` or `mc.example.net:25566` (the port only when one is set and not the
    /// default).
    pub fn label(&self) -> String {
        match self.port {
            0 | 25565 => self.host.clone(),
            port => format!("{}:{port}", self.host),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudRenewal {
    pub open: bool,
    pub opens_at: i64,
}

/// The contract's stop causes: pl-afk's names (`KickCause`, `FailureReason`) and the cloud's own.
/// `invalidCommand`: a command or rule-reply row that would run breaks the text rule's rule 4 (a
/// `\` after only leading spaces or format characters), so the client would take it for its own
/// command. Any other cause reads like a plain stop.
pub const CLOUD_CAUSES: &[&str] = &[
    "banned",
    "notWhitelisted",
    "wrongPassword",
    "premiumOnly",
    "loggedInElsewhere",
    "alreadyConnected",
    "loginTimeout",
    "unsupportedVersion",
    "secretUnreadable",
    "invalidCommand",
    "crashLoop",
    "forbiddenTarget",
];

/// Why an account does not run although it is wanted (in this order of precedence). A hold this
/// launcher does not know yet is `Other` (shown like a plain stop).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum CloudHold {
    /// Staff stopped it; `reason` is a reason key.
    Suspended {
        until: Option<i64>,
        reason: String,
    },
    /// The player is banned from Cloud AFK.
    Restricted {
        until: Option<i64>,
    },
    Expired {
        at: i64,
    },
    /// The service is not on (`paused` or `off`).
    Paused {
        mode: String,
    },
    /// No machine yet, or its machine is disabled.
    NoHost {
        since: i64,
    },
    ServerBlocked {
        reason: Option<CloudLocalized>,
    },
    /// The client stopped for good: `cause` is a stop cause of the contract (pl-afk's names plus
    /// the cloud's own, [`CLOUD_CAUSES`]), kept as text so a newer one still reads. `row`: with
    /// `secretUnreadable` and `invalidCommand`, the row to write again, when the machine named it.
    Cause {
        cause: String,
        at: i64,
        #[serde(default)]
        row: Option<String>,
    },
    #[serde(other)]
    Other,
}

/// What the machine says about the account now. `state` is one of `unknown`, `stopped`, `queued`,
/// `connecting`, `running`, `waiting`, kept as text (anything else reads as `unknown`).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudLiveState {
    pub state: String,
    pub since: Option<i64>,
    /// `queued`: why it waits (`start_interval`, `memory`, `target_cap`, `connect_interval`).
    pub reason: Option<String>,
    /// `connecting`, `waiting`: the reconnect attempt (1 = first).
    pub attempt: Option<u32>,
    /// `waiting`: the next attempt.
    pub retry_at: Option<i64>,
}

impl CloudLiveState {
    /// In the game, or on its way there (the client runs).
    pub fn is_active(&self) -> bool {
        matches!(
            self.state.as_str(),
            "running" | "connecting" | "waiting" | "queued"
        )
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudEvery {
    pub min: u32,
    pub max: u32,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudAntiAfk {
    pub enabled: bool,
    pub delay_min: u32,
    pub delay_max: u32,
}

/// A row's words as the account view shows them: plain (`text`) or secret (`preview`, and `fp`,
/// the stored seal's fingerprint; never the sealed value).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudPayloadView {
    pub secret: bool,
    pub text: Option<String>,
    pub preview: Option<String>,
    pub fp: Option<String>,
}

/// A command row of the view. `on` is `join`, `start`, `reconnect` (with `delay`), `interval`
/// (with `every`) or `daily` (with `at`), kept as text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudCommandView {
    pub id: String,
    pub enabled: bool,
    pub on: String,
    pub delay: Option<u32>,
    pub every: Option<CloudEvery>,
    pub at: Option<Vec<String>>,
    pub secret: bool,
    pub text: Option<String>,
    pub preview: Option<String>,
    pub fp: Option<String>,
}

impl CloudCommandView {
    pub fn payload(&self) -> CloudPayloadView {
        CloudPayloadView {
            secret: self.secret,
            text: self.text.clone(),
            preview: self.preview.clone(),
            fp: self.fp.clone(),
        }
    }
}

/// An auto-reply rule of the view. `from` is `any`, `player`, `private` or `server`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudRuleView {
    pub id: String,
    pub enabled: bool,
    #[serde(rename = "match")]
    pub pattern: String,
    pub regex: bool,
    pub from: String,
    pub owner_only: bool,
    pub cooldown: u32,
    pub reply: CloudPayloadView,
}

/// `Account.config`: the account's commands and auto-replies, secrets as their preview only.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudConfigView {
    pub v: u32,
    pub owner: String,
    pub anti_afk: CloudAntiAfk,
    pub commands: Vec<CloudCommandView>,
    pub rules: Vec<CloudRuleView>,
    /// The player's own inventory choice; None: never chosen (Studio's default applies).
    pub inventory: Option<CloudInventorySetting>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudServerChanges {
    pub used: u32,
    pub limit: u32,
    pub reset_at: Option<i64>,
}

/// A cloud account (`GET /accounts`, `GET /accounts/:id`, and the answer of every change).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudAccount {
    pub id: String,
    /// The in-game name.
    pub name: String,
    pub server: CloudServer,
    /// A value of `GET /versions`, or `auto`.
    pub version: String,
    /// The client's console language: `tr` or `en`.
    pub lang: String,
    /// The machine's id (for the live socket); None while it waits for a slot. Never shown.
    pub host: Option<String>,
    pub created_at: i64,
    /// Paid until.
    pub expires_at: i64,
    pub expired: bool,
    /// Reactivation possible until then; then the account is deleted.
    pub grace_until: i64,
    pub renewal: CloudRenewal,
    /// What the player (or staff) asked for: `run` or `stop` (anything else reads as `stop`).
    pub desired: String,
    pub hold: Option<CloudHold>,
    pub live: CloudLiveState,
    pub ever_joined: bool,
    pub config: CloudConfigView,
    /// Version of name, server, version, lang and config (`If-Match`).
    pub rev: u64,
    pub server_changes: CloudServerChanges,
    /// `plain` or `plus` ("plus" while `plus_until` is ahead); "" from a service before AFK+.
    #[serde(default)]
    pub tier: String,
    /// AFK+ until then; None: never AFK+. May lie in the past (its deals and settings stay readable).
    #[serde(default)]
    pub plus_until: Option<i64>,
    /// The assistant's state; None unless the tier is `plus`.
    #[serde(default)]
    pub ai: Option<PlusAi>,
    /// What this account may use of the cloud features (CAFK-FEAT); None from a service before
    /// them: then none shows.
    #[serde(default)]
    pub features: Option<CloudAccountFeatures>,
}

impl CloudAccount {
    /// Wanted, and nothing holds it: it runs or is on its way (the machine may not say yet).
    pub fn wants_to_run(&self) -> bool {
        self.desired == "run" && self.hold.is_none() && !self.expired
    }

    /// AFK+ now.
    pub fn is_plus(&self) -> bool {
        self.tier == "plus"
    }

    /// AFK+ now or before: its AFK+ settings, deals and history are still readable.
    pub fn was_plus(&self) -> bool {
        self.is_plus() || self.plus_until.is_some()
    }
}

/// `GET /accounts`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(default)]
pub struct AccountList {
    pub accounts: Vec<CloudAccount>,
    pub now: i64,
}

/// One masked console line: `k` is `n` (normal), `e` (a client error) or `s` (a command row).
/// With console kinds (CAFK-FEAT) `c` says what it is (`chat`, `pm`, `sys`, `title`, `cli`, `err`;
/// kept as text, an unknown one reads as `cli`), `who` the other player of a chat or private line,
/// `goto: "inventory"` a machine line the console may link to the inventory. Older lines have none.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudConsoleLine {
    pub at: i64,
    pub text: String,
    pub k: String,
    pub c: Option<String>,
    pub who: Option<String>,
    pub goto: Option<String>,
}

/// `GET /accounts/:id/logs`: oldest first; `next` continues.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudLogPage {
    pub lines: Vec<CloudConsoleLine>,
    pub next: Option<String>,
}

/// `GET /vault-key`: the machines' current public key (seal with this one only).
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VaultKey {
    pub kid: String,
    pub alg: String,
    /// base64url of the 65-byte uncompressed P-256 point.
    pub public_key: String,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudVersionOption {
    pub value: String,
    pub label: String,
}

/// `GET /versions`: what the machines' clients can connect with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct CloudVersions {
    pub versions: Vec<CloudVersionOption>,
    pub recommended: String,
}

impl CloudVersions {
    pub fn has(&self, value: &str) -> bool {
        self.versions.iter().any(|v| v.value == value)
    }
}

/// `POST /accounts/:id/reveal`: one secret row, sealed by the machine to a one-time key.
#[derive(Debug, Clone, Default, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct RevealAnswer {
    pub row_id: String,
    pub nonce: String,
    pub revealed: String,
}

// --- what the launcher sends ------------------------------------------------------------------

/// A row's words in a change (`PATCH`): plain, a new seal, or the stored seal kept.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PayloadInput {
    pub secret: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sealed: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub preview: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub keep: Option<bool>,
}

impl PayloadInput {
    pub fn plain(text: impl Into<String>) -> Self {
        Self {
            secret: false,
            text: Some(text.into()),
            sealed: None,
            preview: None,
            keep: None,
        }
    }

    pub fn sealed(sealed: impl Into<String>, preview: impl Into<String>) -> Self {
        Self {
            secret: true,
            text: None,
            sealed: Some(sealed.into()),
            preview: Some(preview.into()),
            keep: None,
        }
    }

    pub fn keep() -> Self {
        Self {
            secret: true,
            text: None,
            sealed: None,
            preview: None,
            keep: Some(true),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CommandInput {
    pub id: String,
    pub enabled: bool,
    pub on: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub delay: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub every: Option<CloudEvery>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub at: Option<Vec<String>>,
    #[serde(flatten)]
    pub payload: PayloadInput,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleInput {
    pub id: String,
    pub enabled: bool,
    #[serde(rename = "match")]
    pub pattern: String,
    pub regex: bool,
    pub from: String,
    pub owner_only: bool,
    pub cooldown: u32,
    pub reply: PayloadInput,
}

/// `CloudAccountConfig` v1 in its input form. The fields the features added (`inventory`, later
/// `sequences` and `movement`) are kept by the service when a change leaves them out, so a field
/// this launcher does not send is never erased.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ConfigInput {
    pub v: u32,
    pub owner: String,
    pub anti_afk: CloudAntiAfk,
    pub commands: Vec<CommandInput>,
    pub rules: Vec<RuleInput>,
    /// Sent only when the player changed it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub inventory: Option<CloudInventorySetting>,
}

/// `PATCH /accounts/:id`: at least one field. A new `server` needs `config` with every secret row
/// sealed again for it (or left out).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct AccountPatch {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub server: Option<CloudServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub lang: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub config: Option<ConfigInput>,
}

impl AccountPatch {
    pub fn is_empty(&self) -> bool {
        self.name.is_none()
            && self.server.is_none()
            && self.version.is_none()
            && self.lang.is_none()
            && self.config.is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The contract's `Account` example, with `desired`, `hold` and `live` as given.
    fn account_json(desired: &str, hold: &str, live: &str) -> String {
        format!(
            r#"{{
          "id": "a_7KQ2MZ3P9TW4XB1C", "name": "Ahmet_AFK",
          "server": {{"host": "play.projects.gg", "port": 0}}, "version": "26.3", "lang": "tr",
          "host": "kz1", "createdAt": 1792000000, "expiresAt": 1793000000, "expired": false,
          "graceUntil": 1795592000, "renewal": {{"open": false, "opensAt": 1792395200}},
          "desired": "{desired}", "hold": {hold}, "live": {live},
          "everJoined": true,
          "config": {{"v": 1, "owner": "", "antiAfk": {{"enabled": true, "delayMin": 60, "delayMax": 90}},
            "commands": [], "rules": []}},
          "rev": 7, "serverChanges": {{"used": 0, "limit": 6, "resetAt": null}},
          "refund": {{"eligible": false, "until": null, "orderId": null}}
        }}"#
        )
    }

    #[test]
    fn a_stop_cause_names_its_row_and_an_unknown_one_still_reads() {
        let hold: CloudHold = serde_json::from_str(
            r#"{"kind": "cause", "cause": "invalidCommand", "at": 1792400000, "row": "c2"}"#,
        )
        .unwrap();
        assert_eq!(
            hold,
            CloudHold::Cause {
                cause: "invalidCommand".into(),
                at: 1792400000,
                row: Some("c2".into()),
            }
        );
        assert!(CLOUD_CAUSES.contains(&"invalidCommand"));
        // An older machine names no row; a newer cause is kept as it came.
        let hold: CloudHold =
            serde_json::from_str(r#"{"kind": "cause", "cause": "somethingNew", "at": 5}"#).unwrap();
        assert_eq!(
            hold,
            CloudHold::Cause {
                cause: "somethingNew".into(),
                at: 5,
                row: None,
            }
        );
    }

    #[test]
    fn reads_an_account_of_the_contract() {
        let json = r#"{
          "id": "a_7KQ2MZ3P9TW4XB1C", "name": "Ahmet_AFK",
          "server": {"host": "play.projects.gg", "port": 0}, "version": "26.3", "lang": "tr",
          "host": "kz1", "createdAt": 1792000000, "expiresAt": 1793000000, "expired": false,
          "graceUntil": 1795592000, "renewal": {"open": false, "opensAt": 1792395200},
          "desired": "run", "hold": {"kind": "cause", "cause": "wrongPassword", "at": 1792400000},
          "live": {"state": "waiting", "since": 1792400000, "attempt": 2, "retryAt": 1792400020},
          "everJoined": true,
          "config": {"v": 1, "owner": "", "antiAfk": {"enabled": true, "delayMin": 60, "delayMax": 90},
            "commands": [
              {"id": "c1", "enabled": true, "on": "join", "delay": 3, "secret": true,
               "preview": "/login ******", "fp": "AAAAAAAAAAAAAAAA"},
              {"id": "c3", "enabled": true, "on": "interval", "every": {"min": 300, "max": 600},
               "secret": false, "text": "/spawn"},
              {"id": "c4", "enabled": true, "on": "daily", "at": ["08:00"], "secret": false, "text": "/günlük"}
            ],
            "rules": [{"id": "r1", "enabled": true, "match": "giriş yap", "regex": false,
              "from": "server", "ownerOnly": false, "cooldown": 30,
              "reply": {"secret": true, "preview": "/login ******", "fp": "BBBBBBBBBBBBBBBB"}}]},
          "rev": 7, "serverChanges": {"used": 2, "limit": 6, "resetAt": 1792403600},
          "refund": {"eligible": false, "until": null, "orderId": null},
          "somethingNew": {"later": true}
        }"#;
        let account: CloudAccount = serde_json::from_str(json).unwrap();
        assert_eq!(account.name, "Ahmet_AFK");
        assert_eq!(account.server, CloudServer::new("play.projects.gg", 0));
        assert_eq!(
            account.hold,
            Some(CloudHold::Cause {
                cause: "wrongPassword".into(),
                at: 1792400000,
                row: None,
            })
        );
        assert_eq!(account.live.retry_at, Some(1792400020));
        assert_eq!(
            account.config.commands[0].fp.as_deref(),
            Some("AAAAAAAAAAAAAAAA")
        );
        assert_eq!(
            account.config.commands[1].every,
            Some(CloudEvery { min: 300, max: 600 })
        );
        assert_eq!(account.config.rules[0].pattern, "giriş yap");
        assert_eq!(account.rev, 7);
        assert!(!account.wants_to_run());
    }

    #[test]
    fn unknown_values_fall_back_to_the_most_generic_kind() {
        let account: CloudAccount = serde_json::from_str(&account_json(
            "pause",
            r#"{"kind": "frozen", "x": 1}"#,
            r#"{"state": "teleporting"}"#,
        ))
        .unwrap();
        assert!(!account.wants_to_run());
        assert_eq!(account.hold, Some(CloudHold::Other));
        assert!(!account.live.is_active());
        let account: CloudAccount = serde_json::from_str(&account_json(
            "run",
            "null",
            r#"{"state": "running", "since": 1792400000}"#,
        ))
        .unwrap();
        assert!(account.wants_to_run());
        assert!(account.live.is_active());
        let offer: Offer = serde_json::from_str(r#"{"mode": "sleeping"}"#).unwrap();
        assert_eq!(offer.mode, ServiceMode::Off);
        let offer: Offer = serde_json::from_str(
            r#"{"api": 1, "mode": "on", "prices": [{"plan": "year", "priceTl": "600.00", "days": 365},
                {"plan": "month", "priceTl": "60.00", "days": 30}]}"#,
        )
        .unwrap();
        assert_eq!(offer.mode, ServiceMode::On);
        assert_eq!(offer.month_price().unwrap().price_tl, "60.00");
        assert_eq!(offer.limits, CloudLimits::default());
    }

    #[test]
    fn writes_the_input_forms_without_empty_fields() {
        let command = CommandInput {
            id: "c1".into(),
            enabled: true,
            on: "join".into(),
            delay: Some(2),
            every: None,
            at: None,
            payload: PayloadInput::sealed("pl1.k1.x", "/login ******"),
        };
        assert_eq!(
            serde_json::to_value(&command).unwrap(),
            serde_json::json!({"id": "c1", "enabled": true, "on": "join", "delay": 2,
                "secret": true, "sealed": "pl1.k1.x", "preview": "/login ******"})
        );
        let keep = serde_json::to_value(PayloadInput::keep()).unwrap();
        assert_eq!(keep, serde_json::json!({"secret": true, "keep": true}));
        let patch = AccountPatch {
            name: Some("Afk_1".into()),
            ..AccountPatch::default()
        };
        assert_eq!(
            serde_json::to_value(&patch).unwrap(),
            serde_json::json!({"name": "Afk_1"})
        );
        assert!(AccountPatch::default().is_empty());
    }

    #[test]
    fn reads_the_features_and_an_account_from_before_them() {
        let old: CloudAccount =
            serde_json::from_str(&account_json("run", "null", r#"{"state": "running"}"#)).unwrap();
        assert_eq!(old.features, None);
        assert_eq!(old.config.inventory, None);
        let json = account_json("run", "null", r#"{"state": "running"}"#)
            .replace(
                r#""commands": [], "rules": []"#,
                r#""commands": [], "rules": [], "inventory": {"enabled": true}"#,
            )
            .replace(
                r#""rev": 7"#,
                r#""features": {"inventory": {"mode": "full", "on": true}, "consoleKinds": true,
                  "placeholders": false, "legacyRules": true}, "rev": 7"#,
            );
        let account: CloudAccount = serde_json::from_str(&json).unwrap();
        let features = account.features.unwrap();
        assert_eq!(features.inventory.mode, "full");
        assert!(features.inventory.on && features.console_kinds && features.legacy_rules);
        assert_eq!(
            account.config.inventory,
            Some(CloudInventorySetting { enabled: true })
        );
        let line: CloudConsoleLine =
            serde_json::from_str(r#"{"at": 1, "text": "selam", "k": "n"}"#).unwrap();
        assert_eq!((line.c, line.who, line.goto), (None, None, None));
        let line: CloudConsoleLine = serde_json::from_str(
            r#"{"at": 1, "text": "✉ »» Kaan_07: selam", "k": "n", "c": "pm", "who": "Kaan_07"}"#,
        )
        .unwrap();
        assert_eq!(line.c.as_deref(), Some("pm"));
        let offer: Offer =
            serde_json::from_str(r#"{"mode": "on", "features": {"inventory": "view"}}"#).unwrap();
        assert_eq!(offer.features.unwrap().inventory, "view");
        // A change that leaves the inventory alone does not name it: the service keeps it.
        let config = ConfigInput {
            v: 1,
            owner: String::new(),
            anti_afk: CloudAntiAfk::default(),
            commands: vec![],
            rules: vec![],
            inventory: None,
        };
        assert!(
            serde_json::to_value(&config)
                .unwrap()
                .get("inventory")
                .is_none()
        );
    }

    #[test]
    fn server_labels_leave_out_the_default_port() {
        assert_eq!(
            CloudServer::new("play.projects.gg", 0).label(),
            "play.projects.gg"
        );
        assert_eq!(
            CloudServer::new("mc.example.net", 25565).label(),
            "mc.example.net"
        );
        assert_eq!(
            CloudServer::new("mc.example.net", 25566).label(),
            "mc.example.net:25566"
        );
    }
}
