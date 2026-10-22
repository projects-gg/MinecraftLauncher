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
//!
//! Nothing polls: the gate is read when the AFK page opens (cached a minute), the accounts with it
//! and after an action, and the socket runs only while a console is on screen.

mod client;
mod config;
mod copies;
mod error;
mod live;
mod migrate;
mod model;
mod seal;
mod server;
mod service;
mod text;
mod tls;

pub use client::{Bearer, CloudClient, CloudConfig, LogQuery, PRODUCTION_URL, api_url};
pub use config::{
    BuildError, Built, CloudEditCommand, CloudEditConfig, CloudEditRule, CloudIssue, CloudRowText,
    SealContext, build, copies_after, edit_config, fingerprints, validate,
};
pub use copies::{Copies, RowCopies, RowCopy};
pub use error::{CloudError, Result};
pub use live::{CloudLiveEvent, CloudLiveStatus, LiveHub, LiveSink};
pub use migrate::{
    CloudAdjusted, CloudDropped, CloudDroppedKind, CloudMovePlan, MoveContext, plan,
};
pub use model::{
    AccountList, AccountPatch, CloudAccount, CloudAccountCount, CloudAntiAfk, CloudCanBuy,
    CloudCommandView, CloudConfigView, CloudConsoleLine, CloudEvery, CloudHold, CloudLimits,
    CloudLiveState, CloudLocalized, CloudLogPage, CloudPayloadView, CloudPlanPrice, CloudRenewal,
    CloudRestriction, CloudRuleView, CloudSales, CloudServer, CloudServerChanges, CloudStock,
    CloudVersionOption, CloudVersions, CommandInput, ConfigInput, Me, MeUser, Offer, PayloadInput,
    RevealAnswer, RuleInput, ServiceMode, VaultKey,
};
pub use seal::{
    CONSOLE_ROW, RevealKey, RevealTarget, SEAL_ALG, SealError, SealTarget, fingerprint, seal,
};
pub use server::{CloudAddressProblem, check_port, normalize, normalize_host, same_server};
pub use service::{
    CloudEdit, CloudEditView, CloudOverview, CloudSaveOutcome, CloudService, CloudSession,
};
pub use text::{
    CloudTextProblem, MAX_TEXT_UNITS, check_text, is_sensitive, looks_sensitive, preview,
    sensitive_command_names,
};
