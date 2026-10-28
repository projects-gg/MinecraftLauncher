//! Client for 7/24 Bulut: paid cloud AFK accounts that run on our machines while the player's
//! computer is off, bought and managed on `https://afk.projects.gg` (the Worker `cloud-afk` in the
//! Studio repository; its `services/cloud-afk/README.md` is the normative contract this crate
//! follows: player API `api: 1`, seal format `pl1`, reveal format `rv1`, account config `v: 1`).
//!
//! The launcher only shows and manages: prices, stock and every rule are the service's. Purchase,
//! renewal and linking mc.projects.gg always happen on the site.
//!
//! - [`CloudService`]: what the launcher runs. The AFK page's [`CloudOverview`] (the gate: nothing
//!   of 7/24 Bulut shows unless the service answers and is not off), actions, the console, the
//!   editor ([`CloudEditView`], [`SaveOutcome`]), "Göster" and "Buluta taşı" ([`MovePlan`]).
//! - [`CloudClient`]: the HTTP API, stateless besides configuration; the Bearer token is the
//!   player's friends session (`pls_…`).
//! - [`LiveHub`]: the live console socket, open only while a console is watched.
//! - [`seal`] / [`RevealKey`]: HPKE (RFC 9180) sealing of secret rows to the machines' key, and
//!   opening a revealed row with a one-time key; checked byte for byte against the contract's test
//!   vectors.
//! - [`Copies`]: the words of the rows this launcher sealed, in the OS keychain.
//! - [`plan`]: a desktop AFK account in the cloud's form, with what stays on the desktop.
//! - AFK+ (the AI tier, README "AFK+"): its objects ([`PlusConfig`] with its orders, [`PlusAi`],
//!   …), the checks before a write ([`check_config_value`], the contract's reference run on its
//!   vectors), the game's item data from our CDN, the audit log this computer keeps, the routes,
//!   and the panel's calls ([`PlusPanelView`]).
//! - The cloud features (README "Features", CAFK-FEAT phase 1): Studio's switches
//!   ([`CloudFeatures`], [`CloudAccountFeatures`]), the live inventory ([`CloudInvItem`],
//!   [`check_inv_op`]), the console's kinds, strip and download ([`CloudBars`], [`console_text`]),
//!   and the placeholders of auto-replies ([`placeholder_problem`], [`strict_value`], [`try_reply`]).
//! - Movement (README "Movement"): what an account may do ([`CloudMovementFeature`]), its switch,
//!   the live frames ([`CloudMovementState`], [`CloudPos`]) and the shape of an operation
//!   ([`check_move_op`]).
//!
//! Nothing polls: the gate is read when the AFK page opens (cached a minute), the accounts with it
//! and after an action, and the socket runs only while a console is on screen.

mod client;
mod config;
mod copies;
mod error;
mod feat;
mod live;
mod migrate;
mod model;
mod movement;
mod plus;
mod plus_audit;
mod plus_check;
mod plus_items;
mod seal;
mod server;
mod service;
mod service_feat;
mod service_plus;
mod text;
mod tls;

pub use client::{Bearer, CloudClient, CloudConfig, LogQuery, PRODUCTION_URL, api_url};
pub use config::{
    BuildError, Built, CloudEditCommand, CloudEditConfig, CloudEditRule, CloudIssue, CloudRowText,
    SealContext, build, copies_after, edit_config, fingerprints, validate, validate_with,
};
pub use copies::{Copies, RowCopies, RowCopy};
pub use error::{CloudError, Result};
pub use feat::{
    CloudAccountFeatures, CloudBarText, CloudBars, CloudBossBar, CloudFeatures, CloudInvDurability,
    CloudInvEnchant, CloudInvError, CloudInvInside, CloudInvInsideStack, CloudInvItem,
    CloudInventoryFeature, CloudInventorySetting, CloudLogRange, CloudReplyTry, CloudTitleBar,
    CloudWinOpen, DEFAULT_PLACEHOLDER_COMMANDS, OfferFeatures, PLACEHOLDER_VALUE_MAX,
    PlaceholderProblem, PlaceholderRow, PlaceholderRules, RuleLine, RuleRun, check_inv_op,
    console_file_name, console_text, has_placeholder, is_console_file_name, is_inv_op_id,
    line_kind, placeholder_names, placeholder_problem, run_rule, strict_value, try_reply,
};
pub use live::{CloudLiveEvent, CloudLiveStatus, LiveHub, LiveSink, PlusOrderLive};
pub use migrate::{
    CloudAdjusted, CloudDropped, CloudDroppedKind, CloudMovePlan, MoveContext, plan,
};
pub use model::{
    AccountList, AccountPatch, CLOUD_CAUSES, CloudAccount, CloudAccountCount, CloudAntiAfk,
    CloudCanBuy, CloudCommandView, CloudConfigView, CloudConsoleLine, CloudEvery, CloudHold,
    CloudLimits, CloudLiveState, CloudLocalized, CloudLogPage, CloudModAppeal, CloudModPaused,
    CloudModeration, CloudPayloadView, CloudPlanPrice, CloudRenewal, CloudRestriction,
    CloudRuleView, CloudSales, CloudServer, CloudServerChanges, CloudStock, CloudVersionOption,
    CloudVersions, CommandInput, ConfigInput, Me, MeUser, Offer, PayloadInput, RevealAnswer,
    RuleInput, ServiceMode, VaultKey,
};
pub use movement::{
    CloudBlock, CloudMovementError, CloudMovementFeature, CloudMovementFollow,
    CloudMovementSetting, CloudMovementState, CloudPos, MOVE_GOTO_MAX_BLOCKS, MOVE_STEP_MAX,
    check_move_op,
};
pub use plus::{
    AUDIT_DAY, CUSTOM_NAME_MAX, CloudNotifyPrefs, CloudQuietHours, GENERIC_MAX_DEAL, MAX_BLOCKED,
    MAX_COUNT, MAX_FRIENDS, MAX_INTRO, MAX_NOTE, MAX_ORDER_TEXT, MAX_PAUSE_SECONDS, MAX_REPLY,
    MAX_STATUS, MAX_WINDOWS, MIN_ORDER_TEXT, MePlus, PlusAction, PlusAdTemplate, PlusAi, PlusAllow,
    PlusAllowance, PlusApproval, PlusApprovalCustom, PlusApprovalDeal, PlusApprovalList,
    PlusAuditEntry, PlusAuditPage, PlusBlockedCounts, PlusCanBuy, PlusConfig, PlusControlAnswer,
    PlusCustomItem, PlusDiscovery, PlusFeatures, PlusGift, PlusHistory, PlusHistoryEntry,
    PlusInboxLast, PlusInboxLine, PlusInboxPage, PlusInboxThread, PlusIssue, PlusItemSummary,
    PlusJudge, PlusJudgeState, PlusLimits, PlusLook, PlusNewCustomItem, PlusNewMessages,
    PlusNewOrder, PlusOffer, PlusOfferServers, PlusOrder, PlusOrderPatch, PlusOrderSettings,
    PlusOrderView, PlusOrderWritten, PlusPersona, PlusPick, PlusPickedCustom, PlusPrefix,
    PlusPromo, PlusQuota, PlusQuotaBrief, PlusQuotaWindow, PlusReceipt, PlusReceiptPage,
    PlusReceiptSummary, PlusReplies, PlusReq, PlusRestriction, PlusRules, PlusRulesFeatures,
    PlusSchedule, PlusSeenItem, PlusServer, PlusServerMoney, PlusSettings, PlusSpent,
    PlusThreadPage, PlusTotals, PlusTradeSettings, PlusTradeState, PlusWeeklyAccount,
    PlusWeeklyReport, PlusWho, PlusWindow, PlusWindowCaps, PlusWritten, TABLE_PREFIX,
    is_player_name, is_table_file_name, is_time, refusal_issues as plus_refusal_issues,
    table_file_name,
};
pub use plus_audit::{AUDIT_KEEP_DAYS, AUDIT_MAX_BYTES};
pub use plus_check::{
    PREFIX_DEFAULT_TEXT, PREFIX_MAX, PlusCheckContext, PlusCheckFeatures, PlusCheckLimits,
    PlusModeFacts, PlusProgress, PlusRefusal, check_config_value, check_persona, check_prefix,
    clean_text, is_custom_key, is_order_id, is_vanilla_id,
};
pub use plus_items::PlusItemData;
pub use seal::{
    CONSOLE_ROW, RevealKey, RevealTarget, SEAL_ALG, SealError, SealTarget, fingerprint, seal,
};
pub use server::{CloudAddressProblem, check_port, normalize, normalize_host, same_server};
pub use service::{
    CloudEdit, CloudEditView, CloudOverview, CloudSaveOutcome, CloudService, CloudSession,
};
pub use service_feat::{CloudCopyOutcome, CloudCopyRow, CloudCopySecret, CloudCopySource};
pub use service_plus::{
    PlusAuditOutcome, PlusCheckOutcome, PlusOrderOutcome, PlusPanelView, PlusPickOutcome,
    PlusSaveOutcome, check_context, check_outcome, checked_new_order, checked_patch,
    checked_settings, pick_outcome,
};
pub use text::{
    CloudTextProblem, MAX_TEXT_UNITS, check_text, check_text_utf16, is_leading_ignorable,
    is_sensitive, looks_sensitive, preview, sensitive_command_names, without_leading_ignorable,
};
