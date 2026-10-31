//! What the launcher runs: the client, what it read last (the gate, the player's accounts, the
//! vault key), the keychain copies and the live sockets, behind the few calls the AFK page makes.
//!
//! Nothing here polls. The offer is read when the AFK page opens (at most once a minute; after a
//! failure, once in two minutes), the accounts with it and after each action, the vault key when a
//! secret is sealed (at most five minutes old), and the live socket runs only while a console is
//! watched.

use std::collections::HashMap;
use std::sync::Arc;
use std::time::{Duration, Instant};

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use tokio::runtime::Handle;
use zeroize::Zeroizing;

use crate::client::{Bearer, CloudClient, CloudConfig, LogQuery};
use crate::config::{
    BuildError, CloudEditConfig, CloudIssue, SealContext, build, copies_after, edit_config,
    fingerprints,
};
use crate::copies::{Copies, RowCopies};
use crate::error::{CloudError, Result};
use crate::feat::{CloudFeatures, PlaceholderRules};
use crate::live::{LiveHub, LiveSink};
use crate::migrate::{CloudMovePlan, MoveContext, plan};
use crate::model::{
    AccountPatch, CloudAccount, CloudAccountCount, CloudCanBuy, CloudLimits, CloudLocalized,
    CloudLogPage, CloudServer, CloudVersions, Me, Offer, ServiceMode, VaultKey,
};
use crate::pattern::PatternSuggestion;
use crate::plus::{MePlus, PlusOffer};
use crate::seal::{CONSOLE_ROW, RevealKey, RevealTarget, SealTarget, seal};
use crate::server::{CloudAddressProblem, normalize, same_server};
use crate::text::{check_text, looks_sensitive};

const OFFER_FRESH: Duration = Duration::from_secs(60);
const OFFER_FAILED: Duration = Duration::from_secs(120);
const VERSIONS_FRESH: Duration = Duration::from_secs(300);
const VAULT_FRESH: Duration = Duration::from_secs(300);

/// Whether the account parts can show.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CloudSession {
    /// No friends session in the launcher (or it is no longer valid).
    SignedOut,
    SignedIn,
    /// Signed in, but the accounts could not be read now.
    Failed,
}

/// The AFK page's look at 7/24 Bulut.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudOverview {
    /// False: the service did not answer, or it is off. Then nothing of 7/24 Bulut shows.
    pub available: bool,
    /// `on` or `paused` (maintenance: purchases closed, accounts held).
    pub mode: String,
    pub maintenance: Option<CloudLocalized>,
    /// The monthly price, a decimal string in TL (`"60.00"`).
    pub month_price: Option<String>,
    pub sold_out: bool,
    /// Shown only below Studio's "Son N yer" threshold.
    pub remaining: Option<u32>,
    pub sales_open: bool,
    pub renewal_window_days: u32,
    pub limits: CloudLimits,
    pub session: CloudSession,
    pub accounts: Vec<CloudAccount>,
    pub count: Option<CloudAccountCount>,
    pub can_buy: Option<CloudCanBuy>,
    /// The service's clock when the accounts were read (Unix seconds): the UI's skew.
    pub now: i64,
    /// AFK+ as sold now: None while its `plusMode` is `off` (then nothing of AFK+ shows).
    pub plus: Option<PlusOffer>,
    /// With a session: whether this player may buy AFK+ (an invite list, a restriction).
    pub plus_me: Option<MePlus>,
    /// Studio's switches of the cloud features (CAFK-FEAT); None from a service before them, which
    /// shows none of them (bulk management included).
    pub features: Option<CloudFeatures>,
    /// "ProBot AI ile kalıp kur" can answer now (`/offer` → `ai.patterns`); false hides the helper
    /// and a rule's pattern is written by hand, as before it.
    pub ai_patterns: bool,
}

impl CloudOverview {
    fn hidden() -> Self {
        Self {
            available: false,
            mode: ServiceMode::Off.as_str().to_owned(),
            maintenance: None,
            month_price: None,
            sold_out: false,
            remaining: None,
            sales_open: false,
            renewal_window_days: 0,
            limits: CloudLimits::default(),
            session: CloudSession::SignedOut,
            accounts: Vec::new(),
            count: None,
            can_buy: None,
            now: 0,
            plus: None,
            plus_me: None,
            features: None,
            ai_patterns: false,
        }
    }
}

/// The editor's look at one account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudEditView {
    pub account: CloudAccount,
    pub config: CloudEditConfig,
    pub versions: CloudVersions,
    pub limits: CloudLimits,
}

/// A change the editor saves: what changed besides the commands and replies is optional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CloudEdit {
    /// The revision the editor started from (`If-Match`).
    pub rev: u64,
    pub name: Option<String>,
    pub server: Option<CloudServer>,
    pub version: Option<String>,
    pub lang: Option<String>,
    pub config: Option<CloudEditConfig>,
}

/// How "Yanlışsa bildir" ended (README "Saving" → Refusal reports).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CloudReportOutcome {
    /// Staff will look at it (also when the same text was reported before).
    Sent,
    /// The text saves now (or nothing is refused any more): nothing to report.
    NotRefused,
    /// Three reports today already.
    Limited,
}

/// The service's answer to a refusal report, as the editor says it.
fn report_outcome(result: Result<()>) -> Result<CloudReportOutcome> {
    match result {
        Ok(()) => Ok(CloudReportOutcome::Sent),
        Err(error) => match error.code() {
            "not_refused" => Ok(CloudReportOutcome::NotRefused),
            "rate_limited" => Ok(CloudReportOutcome::Limited),
            _ => Err(error),
        },
    }
}

/// How a save ended. Problems are worded by the UI where they are.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum CloudSaveOutcome {
    Saved {
        account: CloudAccount,
    },
    /// Something the service would refuse, found here or said by it.
    Invalid {
        issues: Vec<CloudIssue>,
    },
    /// The account changed elsewhere since the editor read it: its newer state.
    Conflict {
        account: CloudAccount,
    },
}

struct Cached<T> {
    value: Option<T>,
    at: Option<Instant>,
    failed: bool,
}

impl<T> Default for Cached<T> {
    fn default() -> Self {
        Self {
            value: None,
            at: None,
            failed: false,
        }
    }
}

impl<T: Clone> Cached<T> {
    /// The cached answer while it is fresh: `Some(None)` for a fresh failure.
    fn fresh(&self, ok: Duration, failed: Duration) -> Option<Option<T>> {
        let at = self.at?;
        let age = at.elapsed();
        if self.failed {
            (age < failed).then_some(None)
        } else {
            (age < ok).then(|| self.value.clone())
        }
    }

    fn set(&mut self, value: Option<T>) {
        self.failed = value.is_none();
        self.value = value;
        self.at = Some(Instant::now());
    }
}

#[derive(Default)]
pub(crate) struct Known {
    /// The signed-in player's Discord id (from `/me`).
    owner: Option<String>,
    pub(crate) accounts: HashMap<String, CloudAccount>,
}

pub(crate) struct Inner {
    pub(crate) client: CloudClient,
    offer: Mutex<Cached<Offer>>,
    versions: Mutex<Cached<CloudVersions>>,
    vault: Mutex<Cached<VaultKey>>,
    pub(crate) known: Mutex<Known>,
    copies: Arc<Copies>,
    pub(crate) live: LiveHub,
    /// AFK+: the item data and the settings read this session.
    pub(crate) plus: crate::service_plus::PlusCache,
}

/// Cheap to clone.
#[derive(Clone)]
pub struct CloudService {
    pub(crate) inner: Arc<Inner>,
}

impl std::fmt::Debug for CloudService {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CloudService")
            .field("client", &self.inner.client)
            .finish_non_exhaustive()
    }
}

/// Runs a keychain call off the async runtime (it may block).
async fn blocking<T: Send + 'static>(work: impl FnOnce() -> T + Send + 'static) -> Result<T> {
    tokio::task::spawn_blocking(work)
        .await
        .map_err(|error| CloudError::Keychain(error.to_string()))
}

impl CloudService {
    /// `copies` keeps the words of the rows this launcher seals; `sink` gets the live sockets'
    /// events.
    pub fn new(
        http: reqwest::Client,
        config: CloudConfig,
        copies: Copies,
        runtime: Handle,
        sink: LiveSink,
    ) -> Self {
        let client = CloudClient::with_http_client(http, config);
        let live = LiveHub::new(runtime, client.user_agent().to_owned(), sink);
        Self {
            inner: Arc::new(Inner {
                client,
                offer: Mutex::new(Cached::default()),
                versions: Mutex::new(Cached::default()),
                vault: Mutex::new(Cached::default()),
                known: Mutex::new(Known::default()),
                copies: Arc::new(copies),
                live,
                plus: crate::service_plus::PlusCache::default(),
            }),
        }
    }

    pub fn client(&self) -> &CloudClient {
        &self.inner.client
    }

    /// Studio's console switches as the offer says them, `(looks, dialog cards)`, for the desktop
    /// console (AFK-UX); both off from a service before them. None while the service does not
    /// answer (the last known ones stand). Reads the cached offer when it is fresh.
    pub async fn console_switches(&self) -> Option<(bool, bool)> {
        let offer = self.offer(false).await?;
        Some(
            offer
                .features
                .as_ref()
                .map_or((false, false), |f| (f.console_looks, f.dialog_cards)),
        )
    }

    /// The offer, cached (`force`: read it again now, "Tekrar dene").
    pub(crate) async fn offer(&self, force: bool) -> Option<Offer> {
        if !force && let Some(cached) = self.inner.offer.lock().fresh(OFFER_FRESH, OFFER_FAILED) {
            return cached;
        }
        let offer = match self.inner.client.offer().await {
            Ok(offer) => Some(offer),
            Err(error) => {
                tracing::debug!(%error, "7/24 Bulut is not reachable");
                None
            }
        };
        self.inner.offer.lock().set(offer.clone());
        offer
    }

    async fn versions(&self) -> CloudVersions {
        if let Some(Some(versions)) = self
            .inner
            .versions
            .lock()
            .fresh(VERSIONS_FRESH, Duration::ZERO)
        {
            return versions;
        }
        match self.inner.client.versions().await {
            Ok(versions) => {
                self.inner.versions.lock().set(Some(versions.clone()));
                versions
            }
            Err(error) => {
                tracing::debug!(%error, "the cloud's game versions are not readable now");
                CloudVersions::default()
            }
        }
    }

    pub(crate) async fn vault_key(&self) -> Result<VaultKey> {
        if let Some(Some(key)) = self.inner.vault.lock().fresh(VAULT_FRESH, Duration::ZERO) {
            return Ok(key);
        }
        let key = self.inner.client.vault_key().await?;
        self.inner.vault.lock().set(Some(key.clone()));
        Ok(key)
    }

    pub(crate) async fn owner(&self, token: &Bearer) -> Result<String> {
        if let Some(owner) = self.inner.known.lock().owner.clone() {
            return Ok(owner);
        }
        let me = self.inner.client.me(token).await?;
        self.inner.known.lock().owner = Some(me.user.id.clone());
        Ok(me.user.id)
    }

    pub(crate) fn remember(&self, account: &CloudAccount) {
        self.inner
            .known
            .lock()
            .accounts
            .insert(account.id.clone(), account.clone());
    }

    /// The account as last read, or read now.
    pub(crate) async fn account(&self, token: &Bearer, id: &str) -> Result<CloudAccount> {
        if let Some(account) = self.inner.known.lock().accounts.get(id).cloned() {
            return Ok(account);
        }
        self.fresh_account(token, id).await
    }

    pub(crate) async fn fresh_account(&self, token: &Bearer, id: &str) -> Result<CloudAccount> {
        let account = self.inner.client.account(token, id).await?;
        self.remember(&account);
        Ok(account)
    }

    /// The AFK page's look: the gate, and with a session the player's accounts. Never fails: a
    /// service that does not answer is `available: false`, accounts that could not be read are
    /// `session: Failed`.
    pub async fn overview(&self, token: Option<&Bearer>, force: bool) -> CloudOverview {
        let Some(offer) = self.offer(force).await else {
            self.inner.live.stop_all();
            return CloudOverview::hidden();
        };
        if offer.mode == ServiceMode::Off {
            self.inner.live.stop_all();
            return CloudOverview::hidden();
        }
        let mut overview = CloudOverview {
            available: true,
            mode: offer.mode.as_str().to_owned(),
            maintenance: offer.maintenance_message.clone(),
            month_price: offer.month_price().map(|price| price.price_tl.clone()),
            sold_out: offer.stock.sold_out,
            remaining: offer.stock.remaining,
            sales_open: offer.sales.new_accounts,
            renewal_window_days: offer.renewal_window_days,
            limits: offer.limits.clone(),
            plus: offer.plus.clone(),
            features: offer.features.as_ref().map(CloudFeatures::from),
            ai_patterns: offer.ai.as_ref().is_some_and(|ai| ai.patterns),
            ..CloudOverview::hidden()
        };
        let Some(token) = token else {
            return overview;
        };
        let client = &self.inner.client;
        let (me, list) = tokio::join!(client.me(token), client.accounts(token));
        match (me, list) {
            (Ok(me), Ok(list)) => {
                self.adopt(&me, &list.accounts).await;
                overview.session = CloudSession::SignedIn;
                overview.count = Some(me.accounts);
                overview.can_buy = Some(me.can_buy);
                overview.plus_me = me.plus;
                overview.now = list.now;
                overview.accounts = list.accounts;
            }
            (Err(CloudError::Unauthorized), _) | (_, Err(CloudError::Unauthorized)) => {
                overview.session = CloudSession::SignedOut;
            }
            (Err(error), _) | (_, Err(error)) => {
                tracing::info!(%error, "cloud AFK accounts are not readable now");
                overview.session = CloudSession::Failed;
            }
        }
        overview
    }

    /// The signed-in player and their accounts: copies of gone accounts or of another player go.
    async fn adopt(&self, me: &Me, accounts: &[CloudAccount]) {
        {
            let mut known = self.inner.known.lock();
            if known.owner.as_deref() != Some(me.user.id.as_str()) {
                known.accounts.clear();
                self.inner.live.stop_all();
            }
            known.owner = Some(me.user.id.clone());
            known.accounts = accounts
                .iter()
                .map(|account| (account.id.clone(), account.clone()))
                .collect();
        }
        let copies = self.inner.copies.clone();
        let owner = me.user.id.clone();
        let ids: Vec<String> = accounts.iter().map(|account| account.id.clone()).collect();
        let _ = blocking(move || copies.prune(&owner, &ids)).await;
    }

    /// Signed out of the friends service: the keychain copies, the sockets and what was read about
    /// the player go (the contract: copies are deleted with a sign-out).
    pub async fn forget(&self) {
        self.inner.live.stop_all();
        *self.inner.known.lock() = Known::default();
        self.inner.plus.forget();
        let copies = self.inner.copies.clone();
        let _ = blocking(move || copies.forget_all()).await;
    }

    pub async fn start(&self, token: &Bearer, id: &str) -> Result<CloudAccount> {
        let account = self.inner.client.start(token, id).await?;
        self.remember(&account);
        Ok(account)
    }

    pub async fn stop(&self, token: &Bearer, id: &str) -> Result<CloudAccount> {
        let account = self.inner.client.stop(token, id).await?;
        self.remember(&account);
        Ok(account)
    }

    pub async fn restart(&self, token: &Bearer, id: &str) -> Result<CloudAccount> {
        let account = self.inner.client.restart(token, id).await?;
        self.remember(&account);
        Ok(account)
    }

    /// "Yanlışsa bildir" under a save refused for its content: the row's plain text goes (a secret
    /// row is never read for content), and the service reads it again with its rules.
    pub async fn report_refusal(
        &self,
        token: &Bearer,
        id: &str,
        text: &str,
    ) -> Result<CloudReportOutcome> {
        let text = text.trim();
        if text.is_empty() || text.encode_utf16().count() > 256 {
            return Err(CloudError::Invalid("refused text"));
        }
        report_outcome(self.inner.client.report_refusal(token, id, text).await)
    }

    /// "ProBot AI ile kalıp kur": a rule's trigger from the player's own words, its sentence in
    /// `lang` (`tr` or `en`). `fresh`: "Tekrar dene", a new answer instead of the kept one.
    pub async fn suggest_pattern(
        &self,
        token: &Bearer,
        id: &str,
        text: &str,
        lang: &str,
        fresh: bool,
    ) -> Result<PatternSuggestion> {
        self.inner
            .client
            .suggest_pattern(token, id, text, lang, fresh)
            .await
    }

    /// Sends a console line; a password command is sealed here first. The console never echoes
    /// it: the server's answer shows.
    pub async fn send(&self, token: &Bearer, id: &str, line: &str) -> Result<()> {
        let line = line.trim();
        check_text(line).map_err(|_| CloudError::Invalid("console line"))?;
        if !looks_sensitive(line) {
            return self.inner.client.send_line(token, id, line).await;
        }
        let account = self.account(token, id).await?;
        let owner = self.owner(token).await?;
        let key = self.vault_key().await?;
        let target = SealTarget {
            discord_id: &owner,
            account_id: id,
            row_id: CONSOLE_ROW,
            server: &account.server,
        };
        let sealed = seal(&key, line, &target).map_err(|e| CloudError::Seal(e.to_string()))?;
        self.inner.client.send_sealed(token, id, &sealed).await
    }

    /// A press on the card of the server's dialog (DIALOG-CARD), as the view built it (JSON text:
    /// `{rev, i, inputs, cmd?}` or `{rev, cancel: true}`); the service, the machine and the client
    /// check it again, and a command runs only as the player confirmed it.
    pub async fn press_dialog(&self, token: &Bearer, id: &str, press: &str) -> Result<()> {
        if press.len() > 4 * 1024 {
            return Err(CloudError::Invalid("dialog press"));
        }
        let press: serde_json::Value =
            serde_json::from_str(press).map_err(|_| CloudError::Invalid("dialog press"))?;
        if !press.is_object() {
            return Err(CloudError::Invalid("dialog press"));
        }
        self.inner.client.press_dialog(token, id, press).await
    }

    /// The machine's masked log before `before` (Unix seconds; none: the last hour), 200 lines.
    pub async fn logs(
        &self,
        token: &Bearer,
        id: &str,
        before: Option<i64>,
    ) -> Result<CloudLogPage> {
        let query = LogQuery {
            to: before.map(|at| at - 1),
            limit: Some(200),
            ..LogQuery::default()
        };
        self.inner.client.logs(token, id, &query).await
    }

    /// Streams the account's console while it is on screen.
    pub async fn watch(&self, token: &Bearer, id: &str) -> Result<()> {
        let account = self.account(token, id).await?;
        let Some(host) = account.host.clone() else {
            return Err(CloudError::Invalid("no machine yet"));
        };
        let url = self.inner.client.live_url(&host);
        self.inner.live.watch(url, token.clone(), &host, id);
        Ok(())
    }

    pub fn unwatch(&self, id: &str) {
        self.inner.live.unwatch(id);
    }

    /// The account for the editor: read now, secret rows with this computer's copies.
    pub async fn edit_view(&self, token: &Bearer, id: &str) -> Result<CloudEditView> {
        let account = self.fresh_account(token, id).await?;
        let owner = self.owner(token).await?;
        let copies = self.load_copies(&owner, id).await?;
        let versions = self.versions().await;
        let limits = self
            .offer(false)
            .await
            .map(|offer| offer.limits)
            .unwrap_or_default();
        Ok(CloudEditView {
            config: edit_config(&account.config, &copies),
            account,
            versions,
            limits,
        })
    }

    pub(crate) async fn load_copies(&self, owner: &str, id: &str) -> Result<RowCopies> {
        let copies = self.inner.copies.clone();
        let (owner, id) = (owner.to_owned(), id.to_owned());
        blocking(move || copies.load(&owner, &id)).await?
    }

    pub(crate) async fn save_copies(&self, owner: &str, id: &str, rows: RowCopies) {
        let copies = self.inner.copies.clone();
        let (owner, id) = (owner.to_owned(), id.to_owned());
        match blocking(move || copies.save(&owner, &id, &rows)).await {
            Ok(Ok(())) => {}
            Ok(Err(error)) | Err(error) => {
                tracing::warn!(%error, "the words of the sealed rows are not kept on this computer");
            }
        }
    }

    /// Saves an edit: secrets sealed here (their words kept in the keychain after), stored seals
    /// kept, a new server sealed again for. A change made elsewhere meanwhile comes back as
    /// `Conflict`.
    pub async fn save(
        &self,
        token: &Bearer,
        id: &str,
        edit: CloudEdit,
    ) -> Result<CloudSaveOutcome> {
        let mut edit = edit;
        // The service never rewrites a host (it is part of every seal's AAD): sent normalized.
        if let Some(server) = &edit.server {
            match normalize(&server.host, server.port) {
                Ok(normal) => edit.server = Some(normal),
                Err(problem) => {
                    let problem = match problem {
                        CloudAddressProblem::Empty | CloudAddressProblem::Format => "format",
                        CloudAddressProblem::Dot => "dot",
                        CloudAddressProblem::Private => "private",
                        CloudAddressProblem::Port => "port",
                    };
                    return Ok(CloudSaveOutcome::Invalid {
                        issues: vec![CloudIssue::from_path("server", problem)],
                    });
                }
            }
        }
        let current = self.fresh_account(token, id).await?;
        if current.rev != edit.rev {
            return Ok(CloudSaveOutcome::Conflict { account: current });
        }
        let owner = self.owner(token).await?;
        let offer = self.offer(false).await;
        let placeholders = PlaceholderRules::of(offer.as_ref().and_then(|o| o.features.as_ref()));
        let limits = offer.map(|offer| offer.limits).unwrap_or_default();
        let server = edit
            .server
            .clone()
            .unwrap_or_else(|| current.server.clone());
        let server_changed = edit.server.as_ref().is_some_and(|s| *s != current.server);
        let before = self.load_copies(&owner, id).await?;
        let mut patch = AccountPatch {
            name: edit.name.filter(|name| *name != current.name),
            server: edit.server.filter(|_| server_changed),
            version: edit.version.filter(|version| *version != current.version),
            lang: edit.lang.filter(|lang| *lang != current.lang),
            config: None,
        };
        let mut built = None;
        // A new server needs every secret sealed again, so the config goes with it.
        let config = match edit.config {
            Some(config) => Some(config),
            None if server_changed => Some(edit_config(&current.config, &before)),
            None => None,
        };
        if let Some(mut config) = config {
            // A service before movement refuses the field: it goes only where the account has it.
            if current
                .features
                .as_ref()
                .and_then(|features| features.movement.as_ref())
                .is_none()
            {
                config.movement = None;
            }
            let key = self.vault_key().await?;
            let ctx = SealContext {
                key: &key,
                discord_id: &owner,
                account_id: id,
                server: &server,
                server_changed,
            };
            match build(
                &config,
                &limits,
                &placeholders,
                &ctx,
                &before,
                &fingerprints(&current.config),
            ) {
                Ok(done) => {
                    patch.config = Some(done.config.clone());
                    built = Some(done);
                }
                Err(BuildError::Invalid(issues)) => {
                    return Ok(CloudSaveOutcome::Invalid { issues });
                }
                Err(BuildError::Seal(error)) => return Err(CloudError::Seal(error.to_string())),
            }
        }
        if patch.is_empty() {
            return Ok(CloudSaveOutcome::Saved { account: current });
        }
        match self
            .inner
            .client
            .patch_account(token, id, edit.rev, &patch)
            .await
        {
            Ok(account) => {
                self.remember(&account);
                if let Some(built) = built {
                    let after = copies_after(built, &before, &account.config);
                    self.save_copies(&owner, id, after).await;
                }
                Ok(CloudSaveOutcome::Saved { account })
            }
            Err(error) => match refusal_issues(&error) {
                Some(issues) => Ok(CloudSaveOutcome::Invalid { issues }),
                None if error.code() == "rev_conflict" => Ok(CloudSaveOutcome::Conflict {
                    account: self.fresh_account(token, id).await?,
                }),
                None => Err(error),
            },
        }
    }

    /// "Göster": one stored secret row's words, from the account's machine, sealed to a key made
    /// for this request. They are handed on and never kept.
    pub async fn reveal(
        &self,
        token: &Bearer,
        id: &str,
        row_id: &str,
    ) -> Result<Zeroizing<String>> {
        let owner = self.owner(token).await?;
        let key = RevealKey::generate();
        let answer = self
            .inner
            .client
            .reveal(token, id, row_id, key.public())
            .await?;
        let target = RevealTarget {
            discord_id: &owner,
            account_id: id,
            row_id,
            nonce: &answer.nonce,
        };
        key.open(&answer.revealed, &target)
            .map_err(|error| CloudError::Seal(error.to_string()))
    }

    /// "Buluta taşı": the desktop account in the cloud's form (`owner`: the launcher's active game
    /// account).
    pub async fn move_plan(
        &self,
        account: &pl_afk::AfkAccount,
        login_commands: &[String],
        owner: &str,
    ) -> CloudMovePlan {
        let versions = self.versions().await;
        let limits = self
            .offer(false)
            .await
            .map(|offer| offer.limits)
            .unwrap_or_default();
        plan(
            account,
            login_commands,
            MoveContext {
                owner,
                limits: &limits,
                versions: &versions,
            },
        )
    }

    /// Writes the desktop account's plan over the cloud account `id` (its `rev` as the player saw
    /// it). The name, server, version and every command and reply are replaced.
    pub async fn move_to(
        &self,
        token: &Bearer,
        id: &str,
        rev: u64,
        plan: &CloudMovePlan,
    ) -> Result<CloudSaveOutcome> {
        let Some(server) = plan.server.clone() else {
            return Err(CloudError::Invalid("server"));
        };
        let edit = CloudEdit {
            rev,
            name: Some(plan.name.clone()),
            server: Some(server),
            version: Some(plan.version.clone()),
            lang: None,
            config: Some(plan.config.clone()),
        };
        self.save(token, id, edit).await
    }

    /// The player's cloud account with this name on this server, if any (the same account in two
    /// places keeps kicking itself).
    pub fn twin(&self, name: &str, server: &CloudServer) -> Option<CloudAccount> {
        self.inner
            .known
            .lock()
            .accounts
            .values()
            .find(|account| {
                account.name.eq_ignore_ascii_case(name) && same_server(&account.server, server)
            })
            .cloned()
    }
}

/// A refusal about the change's contents, as issues the editor shows where they are.
fn refusal_issues(error: &CloudError) -> Option<Vec<CloudIssue>> {
    let field = error.extra_str("field").unwrap_or("");
    let reason = error.extra_str("reason").unwrap_or("");
    let issue = match error.code() {
        "invalid_name" => CloudIssue::from_path("name", "format"),
        "invalid_server" => {
            CloudIssue::from_path("server", if reason.is_empty() { "format" } else { reason })
        }
        "invalid_version" => CloudIssue::from_path("version", "format"),
        "invalid_config" => CloudIssue::from_path(
            if field.is_empty() { "config" } else { field },
            if reason.is_empty() { "format" } else { reason },
        ),
        // README "Saving": a clear hit in an enforced category; the problem carries the category
        // ("content:advertising") for the editor's words.
        "invalid_command" if reason == "content" => CloudIssue::from_path(
            field,
            &format!("content:{}", error.extra_str("category").unwrap_or("other")),
        ),
        "invalid_command" => {
            CloudIssue::from_path(field, if reason.is_empty() { "format" } else { reason })
        }
        "invalid_pattern" => {
            CloudIssue::from_path(field, error.extra_str("problem").unwrap_or("invalid"))
        }
        "limit_rows" => {
            CloudIssue::from_path(if field.is_empty() { "commands" } else { field }, "limit")
        }
        "duplicate_account" => CloudIssue::from_path("name", "duplicate"),
        // README "Protected names": the lobby keeps the name for staff (projects.gg only).
        "name_protected" => CloudIssue::from_path("name", "protected"),
        "server_blocked" => CloudIssue::from_path("server", "blocked"),
        "server_full" => CloudIssue::from_path("server", "full"),
        "server_change_limit" => CloudIssue::from_path("server", "changeLimit"),
        // No movement slot: another of the player's accounts has it (`player`), or the machine's
        // slots are taken (`host`).
        "limit_movement" => {
            let scope = error.extra_str("scope").unwrap_or("host");
            CloudIssue {
                max: error
                    .extra("max")
                    .and_then(serde_json::Value::as_u64)
                    .and_then(|max| u32::try_from(max).ok()),
                ..CloudIssue::from_path(
                    "movement",
                    if scope == "player" { "player" } else { "host" },
                )
            }
        }
        _ => return None,
    };
    Some(vec![issue])
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::client::api_url;
    use crate::error::from_response;

    #[test]
    fn a_refusal_report_answers_in_three_ways() {
        // README "Saving" → Refusal reports: 201, the text saves now, or three today already.
        let refused = |status: u16, code: &str| {
            Err(from_response(
                status,
                None,
                json!({"error": {"code": code}}).to_string().as_bytes(),
            ))
        };
        assert_eq!(report_outcome(Ok(())).unwrap(), CloudReportOutcome::Sent);
        assert_eq!(
            report_outcome(refused(409, "not_refused")).unwrap(),
            CloudReportOutcome::NotRefused
        );
        assert_eq!(
            report_outcome(refused(429, "rate_limited")).unwrap(),
            CloudReportOutcome::Limited
        );
        // Anything else is the error it is (the editor says it as any other).
        let other = report_outcome(refused(503, "unavailable")).unwrap_err();
        assert_eq!(other.code(), "unavailable");
        let words = serde_json::to_value(CloudReportOutcome::NotRefused).unwrap();
        assert_eq!(words, json!({"kind": "notRefused"}));
    }

    #[tokio::test]
    async fn a_refusal_report_takes_a_rows_text_only() {
        let dir = tempfile::tempdir().unwrap();
        let service = offline_service(dir.path());
        let long = "x".repeat(257);
        for text in ["", "   ", long.as_str()] {
            let error = service
                .report_refusal(&Bearer::new("pls_test"), "a_1", text)
                .await
                .unwrap_err();
            assert!(matches!(error, CloudError::Invalid(_)), "{text:?}");
        }
    }

    #[test]
    fn a_protected_name_is_the_names_own_problem() {
        // README "Protected names": the lobby keeps a staff name off the machines, so the editor
        // says so under the name instead of "something went wrong".
        let error = from_response(
            409,
            None,
            json!({"error": {"code": "name_protected", "field": "name"}})
                .to_string()
                .as_bytes(),
        );
        let issues = refusal_issues(&error).unwrap();
        assert_eq!(issues.len(), 1);
        assert_eq!(
            (
                issues[0].place.as_str(),
                issues[0].field.as_str(),
                issues[0].problem.as_str()
            ),
            ("general", "name", "protected")
        );
    }

    #[test]
    fn a_content_refusal_names_its_category_and_moderation_reads() {
        use crate::model::CloudHold;
        // README "Saving": no punishment, the editor says which kind of content (MOD-B).
        let error = from_response(
            400,
            None,
            json!({"error": {"code": "invalid_command", "field": "rules[0].reply.text",
                             "reason": "content", "category": "advertising"}})
            .to_string()
            .as_bytes(),
        );
        let issues = refusal_issues(&error).unwrap();
        assert_eq!(
            (
                issues[0].place.as_str(),
                issues[0].index,
                issues[0].problem.as_str()
            ),
            ("rules", Some(0), "content:advertising")
        );
        // The account's step in force, a moderation suspension and a paused row (README "Moderation
        // player API"); an older service says none of it.
        let account: CloudAccount = serde_json::from_value(json!({
            "id": "a_1", "name": "Ahmet_AFK", "server": {"host": "play.projects.gg", "port": 0},
            "version": "26.3", "lang": "tr", "host": null, "createdAt": 1, "expiresAt": 2,
            "expired": false, "graceUntil": 3, "renewal": {"open": false, "opensAt": 0},
            "desired": "run",
            "hold": {"kind": "suspended", "until": null, "reason": "abuse",
                     "case": "mc_01JBQ7XK2M9RPV4TNW8HD3CFGZ", "category": "scam", "auto": true},
            "live": {"state": "stopped"}, "everJoined": true,
            "config": {"v": 1, "owner": "", "antiAfk": {"enabled": true, "delayMin": 60, "delayMax": 90},
                       "commands": [{"id": "c1", "enabled": true, "on": "join", "delay": 2,
                                     "secret": false, "text": "gel", "preview": null, "fp": null,
                                     "paused": {"case": "mc_1", "category": "advertising"}}],
                       "rules": []},
            "rev": 4, "serverChanges": {"used": 0, "limit": 6, "resetAt": null},
            "moderation": {"case": "mc_01JBQ7XK2M9RPV4TNW8HD3CFGZ", "step": "suspend",
                           "category": "scam", "at": 10, "until": null,
                           "appeal": {"can": true, "until": 20, "state": "none"}}
        }))
        .unwrap();
        assert!(matches!(
            &account.hold,
            Some(CloudHold::Suspended { case: Some(case), auto: true, .. }) if case.starts_with("mc_")
        ));
        assert_eq!(
            account.config.commands[0]
                .paused
                .as_ref()
                .map(|p| p.category.as_str()),
            Some("advertising")
        );
        let moderation = account.moderation.unwrap();
        assert_eq!(
            (moderation.step.as_str(), moderation.appeal.can),
            ("suspend", true)
        );
    }

    #[test]
    fn refusals_point_at_the_field() {
        let error = from_response(
            400,
            None,
            json!({"error": {"code": "invalid_command", "field": "commands[1].text", "reason": "variable"}})
                .to_string()
                .as_bytes(),
        );
        assert_eq!(
            refusal_issues(&error),
            Some(vec![CloudIssue {
                place: "commands".into(),
                index: Some(1),
                field: "text".into(),
                problem: "variable".into(),
                max: None,
            }])
        );
        let error = from_response(
            409,
            None,
            json!({"error": {"code": "server_change_limit", "retryAfter": 120}})
                .to_string()
                .as_bytes(),
        );
        assert_eq!(refusal_issues(&error).unwrap()[0].field, "server");
        let error = from_response(
            409,
            None,
            json!({"error": {"code": "service_paused", "mode": "off"}})
                .to_string()
                .as_bytes(),
        );
        assert_eq!(refusal_issues(&error), None);
        // Movement: no slot (with the player's number), or switched off meanwhile.
        let error = from_response(
            422,
            None,
            json!({"error": {"code": "limit_movement", "scope": "player", "max": 2}})
                .to_string()
                .as_bytes(),
        );
        assert_eq!(
            refusal_issues(&error),
            Some(vec![CloudIssue {
                place: "general".into(),
                index: None,
                field: "movement".into(),
                problem: "player".into(),
                max: Some(2),
            }])
        );
        let error = from_response(
            422,
            None,
            json!({"error": {"code": "limit_movement", "scope": "host", "max": 10}})
                .to_string()
                .as_bytes(),
        );
        assert_eq!(refusal_issues(&error).unwrap()[0].problem, "host");
        let error = from_response(
            400,
            None,
            json!({"error": {"code": "invalid_config", "field": "movement.enabled", "reason": "off"}})
                .to_string()
                .as_bytes(),
        );
        let issue = &refusal_issues(&error).unwrap()[0];
        assert_eq!(
            (issue.field.as_str(), issue.problem.as_str()),
            ("movement", "off")
        );
    }

    #[test]
    fn the_cache_keeps_answers_and_failures_apart() {
        let mut cached: Cached<u32> = Cached::default();
        assert_eq!(cached.fresh(OFFER_FRESH, OFFER_FAILED), None);
        cached.set(Some(3));
        assert_eq!(cached.fresh(OFFER_FRESH, OFFER_FAILED), Some(Some(3)));
        cached.set(None);
        assert_eq!(cached.fresh(OFFER_FRESH, OFFER_FAILED), Some(None));
        assert_eq!(
            cached.fresh(OFFER_FRESH, Duration::ZERO),
            None,
            "a stale failure asks again"
        );
    }

    /// A service whose address answers nothing: these tests must not reach it.
    fn offline_service(dir: &std::path::Path) -> CloudService {
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

    #[tokio::test]
    async fn a_new_address_is_checked_before_anything_is_sent() {
        let dir = tempfile::tempdir().unwrap();
        let service = offline_service(dir.path());
        let edit = |host: &str, port: u16| CloudEdit {
            rev: 3,
            name: None,
            server: Some(CloudServer {
                host: host.into(),
                port,
            }),
            version: None,
            lang: None,
            config: None,
        };
        for (host, port, problem) in [
            ("192.168.1.5", 25565, "private"),
            ("localhost", 25565, "private"),
            ("sunucu", 25565, "dot"),
            ("mc.ornek.net", 22, "port"),
            ("", 25565, "format"),
        ] {
            let outcome = service
                .save(&Bearer::new("t"), "a_1", edit(host, port))
                .await
                .unwrap();
            let CloudSaveOutcome::Invalid { issues } = outcome else {
                panic!("{host}:{port} was sent");
            };
            assert_eq!(
                issues,
                vec![CloudIssue::from_path("server", problem)],
                "{host}:{port}"
            );
        }
    }
}
