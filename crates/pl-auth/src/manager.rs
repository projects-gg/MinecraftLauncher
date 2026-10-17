//! [`AuthManager`]: the stateful façade the app talks to. It ties the account list, the secret
//! store and the Microsoft client together, and serializes token refreshes per account.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::time::{Duration, Instant, SystemTime};

use jiff::Timestamp;
use parking_lot::Mutex;
use pl_net::CancellationToken;
use uuid::Uuid;

use crate::avatar::{HeadCache, HeadSource, head_png, read_default_skin, texture_id};
use crate::error::{AuthError, AuthStep, FailureClass, OfflineNameProblem, Result};
use crate::history::{HistoryEntry, HistoryTidy, SkinHistory, SkinSource};
use crate::microsoft::{
    DeviceCodeLogin, LoginProgressFn, MicrosoftAuth, MicrosoftLogin, OpenUrlFn,
};
use crate::model::{
    Account, AccountKind, AccountSecrets, LaunchIdentityData, MinecraftProfile, SessionCheck,
    SkinVariant, is_preset_avatar, launch_identity,
};
use crate::offline::{offline_uuid, validate_offline_name};
use crate::secrets::SecretStore;
use crate::skins::{read_skin_file, validate_skin_png};
use crate::store::{AccountStore, RemovedAccount};

/// How long a removed account can be brought back with [`AuthManager::undo_remove`]. The UI
/// offers undo for a few seconds and forgets the account sooner ([`AuthManager::forget_removed`]);
/// this only bounds how long a forgotten one stays in memory.
pub const UNDO_WINDOW: Duration = Duration::from_secs(5 * 60);

/// [`AuthManager::sync_profile`] reads a profile at most this often (any other read counts too).
pub const PROFILE_SYNC_EVERY: Duration = Duration::from_secs(15 * 60);

/// A saved Microsoft sign-in.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignIn {
    pub account: Account,
    /// A new account; `false` when the profile was already saved and got updated in place.
    pub created: bool,
}

/// An account removed moments ago, kept so the removal can be undone. Its secrets are already
/// gone from the keychain; this copy is the only one left.
struct Removed {
    entry: RemovedAccount,
    secrets: Option<AccountSecrets>,
    at: Instant,
}

/// What [`AuthManager::appearance`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Appearance {
    /// Name, skin and capes.
    pub account: Account,
    /// Minecraft Services could not be reached: `account` is the copy saved at the last sign-in
    /// or change, and may be out of date.
    pub stale: bool,
}

/// What [`AuthManager::skin_history_entries`] found.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SkinHistoryList {
    /// Favourites first, then the most recently worn.
    pub entries: Vec<HistoryEntry>,
    /// The texture id of the skin the account wore at the last profile read, unless it is one
    /// of Mojang's default skins.
    pub current: Option<String>,
}

pub struct AuthManager {
    auth: MicrosoftAuth,
    accounts: AccountStore,
    secrets: Arc<dyn SecretStore>,
    /// One lock per account: concurrent `ensure_fresh` calls share a single refresh.
    refresh_locks: Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
    /// Serializes adding and removing accounts (find-then-insert must not interleave).
    membership: tokio::sync::Mutex<()>,
    /// Recently removed accounts, for undo.
    removed: Mutex<HashMap<Uuid, Removed>>,
    /// Where skins read on profiles or put on them are recorded (see [`Self::with_skin_history`]).
    history: Option<SkinHistory>,
    /// When each account's profile was last read from Minecraft Services.
    profile_reads: Mutex<HashMap<Uuid, Instant>>,
}

impl AuthManager {
    pub fn new(auth: MicrosoftAuth, accounts: AccountStore, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            auth,
            accounts,
            secrets,
            refresh_locks: Mutex::new(HashMap::new()),
            membership: tokio::sync::Mutex::new(()),
            removed: Mutex::new(HashMap::new()),
            history: None,
            profile_reads: Mutex::new(HashMap::new()),
        }
    }

    /// Records the skins of Microsoft accounts in `history` (the appearance panel's "Önceki
    /// skinler") whenever a profile is read or changed: sign-in, session refresh, the panel,
    /// [`Self::sync_profile`], uploads and skins worn again. Without it nothing is recorded.
    pub fn with_skin_history(mut self, history: SkinHistory) -> Self {
        self.history = Some(history);
        self
    }

    pub fn skin_history(&self) -> Option<&SkinHistory> {
        self.history.as_ref()
    }

    pub fn microsoft(&self) -> &MicrosoftAuth {
        &self.auth
    }

    pub fn store(&self) -> &AccountStore {
        &self.accounts
    }

    // --- account list ------------------------------------------------------------------------

    pub fn accounts(&self) -> Vec<Account> {
        self.accounts.list()
    }

    pub fn account(&self, id: Uuid) -> Option<Account> {
        self.accounts.get(id)
    }

    pub fn active_account(&self) -> Option<Account> {
        self.accounts.active()
    }

    pub fn set_active(&self, id: Uuid) -> Result<()> {
        self.accounts.set_active(Some(id))
    }

    /// Pins or unpins an account (see [`AccountStore::set_pinned`]).
    pub fn set_pinned(&self, id: Uuid, pinned: bool) -> Result<Account> {
        self.accounts.set_pinned(id, pinned)
    }

    /// Rearranges the pinned accounts (see [`AccountStore::reorder_pinned`]).
    pub fn reorder_pinned(&self, ids: &[Uuid]) -> Result<()> {
        self.accounts.reorder_pinned(ids)
    }

    /// Shows a bundled picture (`preset:<slug>`) instead of the player head of an offline
    /// account; `None` goes back to the head. Microsoft accounts always show their own skin.
    pub fn set_avatar(&self, id: Uuid, avatar: Option<&str>) -> Result<Account> {
        let account = self
            .accounts
            .get(id)
            .ok_or(AuthError::AccountNotFound { id })?;
        if account.kind != AccountKind::Offline {
            return Err(AuthError::unexpected(
                AuthStep::Offline,
                "only offline accounts can choose a picture",
            ));
        }
        if let Some(avatar) = avatar
            && !is_preset_avatar(avatar)
        {
            return Err(AuthError::unexpected(
                AuthStep::Offline,
                format!("unknown picture {avatar:?}"),
            ));
        }
        if account.avatar.as_deref() == avatar {
            return Ok(account);
        }
        self.accounts
            .update(id, |account| account.avatar = avatar.map(str::to_owned))
    }

    // --- adding accounts -----------------------------------------------------------------------

    /// Browser sign-in, then saves the account (secrets to the keychain) and makes it active.
    pub async fn login_microsoft(
        &self,
        open_url: OpenUrlFn,
        progress: LoginProgressFn,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        Ok(self
            .sign_in_microsoft(open_url, progress, cancel, true)
            .await?
            .account)
    }

    /// Browser sign-in, then saves the account. `activate` makes it the active account; a sign-in
    /// that only renews the session of a saved account passes `false` to keep the selection.
    pub async fn sign_in_microsoft(
        &self,
        open_url: OpenUrlFn,
        progress: LoginProgressFn,
        cancel: &CancellationToken,
        activate: bool,
    ) -> Result<SignIn> {
        let login = self
            .auth
            .login_with_browser(open_url, progress, cancel)
            .await?;
        self.save_microsoft(login, activate).await
    }

    /// First half of the device-code sign-in; show [`DeviceCodeLogin::info`] to the player.
    pub async fn start_device_login(&self, cancel: &CancellationToken) -> Result<DeviceCodeLogin> {
        self.auth.start_device_code(cancel).await
    }

    /// Second half of the device-code sign-in: waits for approval, then saves the account.
    pub async fn complete_device_login(
        &self,
        login: DeviceCodeLogin,
        progress: LoginProgressFn,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        let login = login.complete(progress, cancel).await?;
        self.add_microsoft(login).await
    }

    /// Saves a signed-in Microsoft account and makes it active (see [`Self::save_microsoft`]).
    pub async fn add_microsoft(&self, login: MicrosoftLogin) -> Result<Account> {
        Ok(self.save_microsoft(login, true).await?.account)
    }

    /// Saves a signed-in Microsoft account: secrets first (so an account never exists without
    /// them), then the account itself. Signing in again with the same profile updates the saved
    /// account (and clears its expired-session flag) instead of adding a duplicate.
    pub async fn save_microsoft(&self, login: MicrosoftLogin, activate: bool) -> Result<SignIn> {
        let _membership = self.membership.lock().await;
        let MicrosoftLogin {
            profile, secrets, ..
        } = login;
        let existing = self
            .accounts
            .find_by_profile(AccountKind::Microsoft, profile.id);
        let created = existing.is_none();
        let mut account = existing
            .unwrap_or_else(|| Account::new(AccountKind::Microsoft, &profile.name, profile.id));
        account.apply_profile(&profile);
        account.needs_relogin = false;

        self.save_secrets(account.id, secrets).await?;
        let stored = self.accounts.put(account.clone()).and_then(|()| {
            if activate || self.accounts.active_id().is_none() {
                self.accounts.set_active(Some(account.id))
            } else {
                Ok(())
            }
        });
        if let Err(error) = stored {
            if created {
                // Do not leave a refresh token behind for an account that was never saved.
                let _ = self.delete_secrets(account.id).await;
            }
            return Err(error);
        }
        tracing::info!(account = %account.id, created, "Microsoft account saved");
        self.note_profile_read(account.id);
        self.record_skin(&account, SkinSource::Profile);
        Ok(SignIn { account, created })
    }

    /// Adds an offline account and makes it active. Surrounding whitespace is ignored; the name
    /// must then be 3–16 characters of `[A-Za-z0-9_]`. The exact name of a saved account just
    /// selects it again; the same name in other letter case is
    /// [`OfflineNameProblem::Taken`] (servers treat both as one player).
    pub async fn add_offline(&self, name: &str) -> Result<Account> {
        let _membership = self.membership.lock().await;
        let name = name.trim();
        validate_offline_name(name).map_err(|problem| AuthError::InvalidOfflineName { problem })?;
        if let Some(existing) = self.accounts.find_offline_name(name)
            && existing.username != name
        {
            return Err(AuthError::InvalidOfflineName {
                problem: OfflineNameProblem::Taken,
            });
        }
        let (account, _) =
            self.accounts
                .upsert(AccountKind::Offline, offline_uuid(name), |account| {
                    account.username = name.to_owned();
                })?;
        self.accounts.set_active(Some(account.id))?;
        Ok(account)
    }

    /// Renames an offline account. Its UUID follows the name (offline servers derive it from
    /// the name), so worlds and servers see a new player; the local id, position, selection and
    /// play history stay.
    pub async fn rename_offline(&self, id: Uuid, name: &str) -> Result<Account> {
        let _membership = self.membership.lock().await;
        let name = name.trim();
        validate_offline_name(name).map_err(|problem| AuthError::InvalidOfflineName { problem })?;
        let account = self
            .accounts
            .get(id)
            .ok_or(AuthError::AccountNotFound { id })?;
        if account.kind != AccountKind::Offline {
            return Err(AuthError::unexpected(
                AuthStep::Offline,
                "only offline accounts can be renamed",
            ));
        }
        if account.username == name {
            return Ok(account);
        }
        if self
            .accounts
            .find_offline_name(name)
            .is_some_and(|other| other.id != id)
        {
            return Err(AuthError::InvalidOfflineName {
                problem: OfflineNameProblem::Taken,
            });
        }
        let renamed = self.accounts.update(id, |account| {
            account.username = name.to_owned();
            account.uuid = offline_uuid(name);
        })?;
        tracing::info!(account = %id, "offline account renamed");
        Ok(renamed)
    }

    /// Removes an account and deletes its secrets from the keychain. If the keychain cannot
    /// delete them, the account stays so the player can retry (no refresh token is left behind
    /// silently). When the active account goes, the most recently played remaining one becomes
    /// active; the new active id comes back.
    ///
    /// The account and its secrets stay in memory for [`UNDO_WINDOW`] so
    /// [`Self::undo_remove`] can bring them back; [`Self::forget_removed`] drops them sooner.
    pub async fn remove(&self, id: Uuid) -> Result<Option<Uuid>> {
        let _membership = self.membership.lock().await;
        self.prune_removed();
        let account = self
            .accounts
            .get(id)
            .ok_or(AuthError::AccountNotFound { id })?;
        // A refresh in flight (a session check, a launch) would write its new session back after
        // the delete below, for an account that is gone: it finishes first, and its session is
        // the one kept for undo.
        let refresh = self.refresh_lock(id);
        let _refresh = refresh.lock().await;
        let secrets = if account.is_microsoft() {
            // Kept only for undo: a keychain that cannot be read costs the undo its session,
            // never the removal.
            let kept = self.load_secrets(id).await.unwrap_or_else(|error| {
                tracing::warn!(account = %id, code = %error.code(), "secrets not kept for undo");
                None
            });
            self.delete_secrets(id).await?;
            kept
        } else {
            None
        };
        let entry = match self.accounts.take(id) {
            Ok(Some(entry)) => entry,
            Ok(None) => return Err(AuthError::AccountNotFound { id }),
            Err(error) => {
                // The account is still listed: give it its session back.
                if let Some(secrets) = secrets {
                    let _ = self.save_secrets(id, secrets).await;
                }
                return Err(error);
            }
        };
        self.refresh_locks.lock().remove(&id);
        self.removed.lock().insert(
            id,
            Removed {
                entry,
                secrets,
                at: Instant::now(),
            },
        );
        tracing::info!(account = %id, "account removed");
        Ok(self.accounts.active_id())
    }

    /// Brings back an account removed within [`UNDO_WINDOW`]: same place in the list, active
    /// again if it was, session restored to the keychain. If the same profile was added again
    /// meanwhile, that account is kept and returned instead.
    pub async fn undo_remove(&self, id: Uuid) -> Result<Account> {
        let _membership = self.membership.lock().await;
        self.prune_removed();
        let Some(Removed { entry, secrets, .. }) = self.removed.lock().remove(&id) else {
            return Err(AuthError::AccountNotFound { id });
        };
        let (account, inserted) = match self.accounts.restore(&entry) {
            Ok(restored) => restored,
            Err(error) => {
                self.removed.lock().insert(
                    id,
                    Removed {
                        entry,
                        secrets,
                        at: Instant::now(),
                    },
                );
                return Err(error);
            }
        };
        if inserted && account.is_microsoft() {
            let session_back = match secrets {
                Some(secrets) => match self.save_secrets(id, secrets).await {
                    Ok(()) => true,
                    Err(error) => {
                        tracing::warn!(account = %id, code = %error.code(), "session not restored");
                        false
                    }
                },
                None => false,
            };
            if !session_back {
                // The account is back; only its sign-in is missing.
                self.note_session(id, true);
                return Ok(self.accounts.get(id).unwrap_or(account));
            }
        }
        tracing::info!(account = %account.id, inserted, "account removal undone");
        Ok(account)
    }

    /// Makes a removal final: the in-memory copy (and its secrets) is dropped. Returns whether
    /// there was one.
    pub fn forget_removed(&self, id: Uuid) -> bool {
        self.removed.lock().remove(&id).is_some()
    }

    fn prune_removed(&self) {
        self.removed
            .lock()
            .retain(|_, removed| removed.at.elapsed() < UNDO_WINDOW);
    }

    // --- sessions ------------------------------------------------------------------------------

    /// The stored secrets of a Microsoft account, as they are (possibly expired).
    pub async fn cached_secrets(&self, id: Uuid) -> Result<Option<AccountSecrets>> {
        self.require_microsoft(id)?;
        self.load_secrets(id).await
    }

    /// Returns usable secrets, refreshing first when the Minecraft token expires within the
    /// configured margin (10 minutes). Concurrent calls for one account share one refresh.
    ///
    /// On failure, [`AuthError::class`] says what to do: `NeedsRelogin` (invalid refresh token
    /// or missing secrets), `Offline` (network or outage: [`AuthManager::prepare_offline_launch`]
    /// still works with the cached token) or `Other`. The account's
    /// [`Account::needs_relogin`] flag follows: set on `NeedsRelogin`, cleared on success.
    pub async fn ensure_fresh(
        &self,
        id: Uuid,
        cancel: &CancellationToken,
    ) -> Result<AccountSecrets> {
        let result = self.ensure_fresh_inner(id, cancel).await;
        match &result {
            Ok(_) => self.note_session(id, false),
            Err(error) if error.class() == FailureClass::NeedsRelogin => {
                self.note_session(id, true);
            }
            Err(_) => {}
        }
        result
    }

    /// Whether the saved session of a Microsoft account still works. Talks to Microsoft only
    /// when the cached token is about to expire (a refresh, as before a launch).
    pub async fn check_session(
        &self,
        id: Uuid,
        cancel: &CancellationToken,
    ) -> Result<SessionCheck> {
        self.require_microsoft(id)?;
        match self.ensure_fresh(id, cancel).await {
            Ok(_) => Ok(SessionCheck::Valid),
            Err(error) if error.is_cancelled() => Err(error),
            Err(error) if error.class() == FailureClass::NeedsRelogin => Ok(SessionCheck::Expired),
            Err(error) => {
                tracing::debug!(account = %id, code = %error.code(), "session not checked");
                Ok(SessionCheck::Unknown)
            }
        }
    }

    /// Records whether an account needs a new sign-in (only writes when it changes).
    fn note_session(&self, id: Uuid, needs_relogin: bool) {
        let changed = self
            .accounts
            .get(id)
            .is_some_and(|account| account.needs_relogin != needs_relogin);
        if changed
            && let Err(error) = self
                .accounts
                .update(id, |account| account.needs_relogin = needs_relogin)
        {
            tracing::warn!(account = %id, code = %error.code(), "session state not saved");
        }
    }

    async fn ensure_fresh_inner(
        &self,
        id: Uuid,
        cancel: &CancellationToken,
    ) -> Result<AccountSecrets> {
        self.require_microsoft(id)?;
        let lock = self.refresh_lock(id);
        let _guard = tokio::select! {
            biased;
            () = cancel.cancelled() => return Err(AuthError::Cancelled),
            guard = lock.lock() => guard,
        };
        let stored = self
            .load_secrets(id)
            .await?
            .ok_or(AuthError::MissingSecrets { id })?;
        if !stored.expires_within(self.auth.config().refresh_margin, Timestamp::now()) {
            return Ok(stored);
        }
        let Some(refresh_token) = stored
            .refresh_token
            .clone()
            .filter(|token| !token.is_empty())
        else {
            return Err(AuthError::NoRefreshToken {
                at: Timestamp::now(),
            });
        };

        tracing::info!(account = %id, "refreshing the Minecraft session");
        let refreshed = match self.auth.refresh(&refresh_token, cancel).await {
            Ok(refreshed) => refreshed,
            Err(error) => {
                tracing::warn!(
                    account = %id,
                    code = %error.code(),
                    class = ?error.class(),
                    "session refresh failed"
                );
                return Err(error);
            }
        };
        let mut fresh = refreshed.secrets;
        if fresh.refresh_token.is_none() {
            fresh.refresh_token = stored.refresh_token;
        }
        if fresh.xuid.is_none() {
            fresh.xuid = stored.xuid;
        }
        if let Err(error) = self.save_secrets(id, fresh.clone()).await {
            // The keychain keeps the session it had (a failed save never leaves a mix, see
            // `KeyringStore`), and Microsoft does not revoke a refresh token for being used: the
            // next refresh still works. This launch goes on with the fresh session.
            tracing::warn!(
                account = %id,
                code = %error.code(),
                "the refreshed session was not saved; the stored one stays"
            );
        }
        if let Some(profile) = refreshed.profile
            && let Err(error) = self.apply_profile(id, &profile, SkinSource::Profile)
        {
            tracing::warn!(account = %id, code = %error.code(), "profile update not saved");
        }
        Ok(fresh)
    }

    /// Launch identity for online play: refreshes the Microsoft session when needed and records
    /// the account as used.
    pub async fn prepare_launch(
        &self,
        id: Uuid,
        cancel: &CancellationToken,
    ) -> Result<LaunchIdentityData> {
        let account = self
            .accounts
            .get(id)
            .ok_or(AuthError::AccountNotFound { id })?;
        let identity = match account.kind {
            AccountKind::Microsoft => {
                let secrets = self.ensure_fresh(id, cancel).await?;
                // The refresh may have updated the name.
                let account = self.accounts.get(id).unwrap_or(account);
                launch_identity(&account, Some(&secrets))?
            }
            AccountKind::Offline => launch_identity(&account, None)?,
        };
        self.mark_used(id);
        Ok(identity)
    }

    /// Launch identity from what is stored, without any network: offline play after
    /// `ensure_fresh` failed with [`crate::FailureClass::Offline`]. Online-mode servers will
    /// reject an expired token.
    pub async fn prepare_offline_launch(&self, id: Uuid) -> Result<LaunchIdentityData> {
        let account = self
            .accounts
            .get(id)
            .ok_or(AuthError::AccountNotFound { id })?;
        let secrets = if account.is_microsoft() {
            Some(
                self.load_secrets(id)
                    .await?
                    .ok_or(AuthError::MissingSecrets { id })?,
            )
        } else {
            None
        };
        let identity = launch_identity(&account, secrets.as_ref())?;
        self.mark_used(id);
        Ok(identity)
    }

    // --- profile, skins, capes -----------------------------------------------------------------

    /// Re-reads name, skin and capes from Minecraft Services.
    pub async fn refresh_profile(&self, id: Uuid, cancel: &CancellationToken) -> Result<Account> {
        let secrets = self.ensure_fresh(id, cancel).await?;
        let profile = self
            .auth
            .fetch_profile(&secrets.access_token, cancel)
            .await?;
        self.apply_profile(id, &profile, SkinSource::Profile)
    }

    /// Re-reads the profile of a Microsoft account unless it was read within
    /// [`PROFILE_SYNC_EVERY`], so a skin or name changed elsewhere reaches the account list and
    /// the skin history. Only a saved session that is still valid is used (this never refreshes
    /// or signs in). True when the name, skin or capes changed.
    pub async fn sync_profile(&self, id: Uuid, cancel: &CancellationToken) -> Result<bool> {
        let before = self.require_microsoft(id)?;
        if !self.claim_profile_read(id) {
            return Ok(false);
        }
        let Some(secrets) = self.load_secrets(id).await? else {
            return Ok(false);
        };
        if secrets.expires_within(self.auth.config().refresh_margin, Timestamp::now()) {
            return Ok(false);
        }
        let profile = self
            .auth
            .fetch_profile(&secrets.access_token, cancel)
            .await?;
        let after = self.apply_profile(id, &profile, SkinSource::Profile)?;
        Ok(after.username != before.username
            || after.skin != before.skin
            || after.capes != before.capes)
    }

    /// Uploads a skin PNG (64×64 or 64×32).
    pub async fn upload_skin(
        &self,
        id: Uuid,
        png: Vec<u8>,
        variant: SkinVariant,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        validate_skin_png(&png).map_err(|problem| AuthError::InvalidSkinFile { problem })?;
        let secrets = self.ensure_fresh(id, cancel).await?;
        let profile = self
            .auth
            .upload_skin(&secrets.access_token, png, variant, cancel)
            .await?;
        self.apply_profile(id, &profile, SkinSource::Launcher)
    }

    pub async fn set_skin_url(
        &self,
        id: Uuid,
        skin_url: &str,
        variant: SkinVariant,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        let secrets = self.ensure_fresh(id, cancel).await?;
        let profile = self
            .auth
            .set_skin_url(&secrets.access_token, skin_url, variant, cancel)
            .await?;
        self.apply_profile(id, &profile, SkinSource::Launcher)
    }

    pub async fn reset_skin(&self, id: Uuid, cancel: &CancellationToken) -> Result<Account> {
        let secrets = self.ensure_fresh(id, cancel).await?;
        let profile = self.auth.reset_skin(&secrets.access_token, cancel).await?;
        self.apply_profile(id, &profile, SkinSource::Launcher)
    }

    pub async fn show_cape(
        &self,
        id: Uuid,
        cape_id: &str,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        let secrets = self.ensure_fresh(id, cancel).await?;
        let profile = self
            .auth
            .show_cape(&secrets.access_token, cape_id, cancel)
            .await?;
        self.apply_profile(id, &profile, SkinSource::Profile)
    }

    pub async fn hide_cape(&self, id: Uuid, cancel: &CancellationToken) -> Result<Account> {
        let secrets = self.ensure_fresh(id, cancel).await?;
        let profile = self.auth.hide_cape(&secrets.access_token, cancel).await?;
        self.apply_profile(id, &profile, SkinSource::Profile)
    }

    /// PNG bytes of a skin or cape texture, for the 3D preview.
    pub async fn download_texture(&self, url: &str, cancel: &CancellationToken) -> Result<Vec<u8>> {
        self.auth.download_texture(url, cancel).await
    }

    /// The head PNG of an account (see [`HeadSource`]): from `cache`, or cut once from the
    /// profile's Mojang texture (Microsoft) or from a default skin in one of `jars` (offline),
    /// then cached. `None` when there is nothing to cut it from: a Microsoft profile that was
    /// never read, or no 1.19.3+ client on this computer.
    pub async fn head(
        &self,
        id: Uuid,
        cache: &HeadCache,
        jars: &[PathBuf],
        cancel: &CancellationToken,
    ) -> Result<Option<Vec<u8>>> {
        let account = self
            .accounts
            .get(id)
            .ok_or(AuthError::AccountNotFound { id })?;
        let Some(source) = HeadSource::for_account(&account) else {
            return Ok(None);
        };
        self.head_from(&source, cache, jars, cancel).await
    }

    async fn head_from(
        &self,
        source: &HeadSource,
        cache: &HeadCache,
        jars: &[PathBuf],
        cancel: &CancellationToken,
    ) -> Result<Option<Vec<u8>>> {
        if let Some(png) = cache.get(source) {
            return Ok(Some(png));
        }
        let texture = match source {
            // A texture the skin history keeps is not downloaded again.
            HeadSource::Texture { url, id } => match self.history_texture(id) {
                Some(texture) => texture,
                None => self.auth.download_texture(url, cancel).await?,
            },
            HeadSource::Default(skin) => {
                let (jars, skin) = (jars.to_vec(), *skin);
                match tokio::task::spawn_blocking(move || read_default_skin(&jars, skin)).await {
                    Ok(Some(texture)) => texture,
                    Ok(None) => return Ok(None),
                    Err(error) => {
                        return Err(AuthError::unexpected(AuthStep::Texture, error.to_string()));
                    }
                }
            }
        };
        let png = head_png(&texture)
            .map_err(|error| AuthError::unexpected(AuthStep::Texture, error.to_string()))?;
        if let Err(error) = cache.put(source, &png) {
            tracing::warn!(%error, "player head not cached");
        }
        Ok(Some(png))
    }

    /// Name, skin and capes for the appearance panel, re-read from Minecraft Services. When the
    /// services cannot be reached ([`FailureClass::Offline`]) the saved copy comes back marked
    /// `stale`; other failures (an expired session, no profile) are errors the player can act on.
    pub async fn appearance(&self, id: Uuid, cancel: &CancellationToken) -> Result<Appearance> {
        let saved = self.require_microsoft(id)?;
        match self.refresh_profile(id, cancel).await {
            Ok(account) => Ok(Appearance {
                account,
                stale: false,
            }),
            Err(error) if error.class() == FailureClass::Offline => {
                tracing::info!(account = %id, code = %error.code(), "showing the saved appearance");
                Ok(Appearance {
                    account: saved,
                    stale: true,
                })
            }
            Err(error) => Err(error),
        }
    }

    /// Uploads the skin PNG at `path` (read here, at most [`crate::MAX_SKIN_BYTES`]).
    pub async fn upload_skin_file(
        &self,
        id: Uuid,
        path: &Path,
        variant: SkinVariant,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        self.require_microsoft(id)?;
        let png = read_skin_file(path).await?;
        self.upload_skin(id, png, variant, cancel).await
    }

    /// Switches the arm model (classic or slim) of the current skin, keeping its texture.
    pub async fn set_skin_variant(
        &self,
        id: Uuid,
        variant: SkinVariant,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        let mut account = self.require_microsoft(id)?;
        if account.skin.is_none() {
            account = self.refresh_profile(id, cancel).await?;
        }
        let Some(skin) = account.skin.clone() else {
            return Err(AuthError::unexpected(
                AuthStep::Skin,
                "the profile has no active skin",
            ));
        };
        if skin.variant == variant {
            return Ok(account);
        }
        // Mojang copies the texture from its own URL; no need to download and upload it.
        self.set_skin_url(id, &skin.url, variant, cancel).await
    }

    /// Shows the owned cape `cape_id`, or hides the cape when `None`.
    pub async fn set_cape(
        &self,
        id: Uuid,
        cape_id: Option<&str>,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        match cape_id {
            Some(cape_id) => self.show_cape(id, cape_id, cancel).await,
            None => self.hide_cape(id, cancel).await,
        }
    }

    // --- skin history --------------------------------------------------------------------------

    /// The skin history of a Microsoft account and the entry it wears now. Empty without a
    /// history (see [`Self::with_skin_history`]).
    pub fn skin_history_entries(&self, id: Uuid) -> Result<SkinHistoryList> {
        let account = self.require_microsoft(id)?;
        let entries = self
            .history
            .as_ref()
            .map(|history| history.entries(account.uuid))
            .unwrap_or_default();
        Ok(SkinHistoryList {
            entries,
            current: current_texture(&account),
        })
    }

    /// Downloads the missing textures of an account's history from Mojang's public texture host
    /// (no credentials) and draws the missing thumbnails. Returns how many thumbnails were made;
    /// an entry whose texture cannot be had now is skipped and tried again next time.
    pub async fn prepare_skin_history(&self, id: Uuid, cancel: &CancellationToken) -> Result<u32> {
        let account = self.require_microsoft(id)?;
        let Some(history) = &self.history else {
            return Ok(0);
        };
        let mut made = 0;
        for entry in history.entries(account.uuid) {
            if history
                .thumbnail(&entry.texture_id, entry.variant)
                .is_some()
            {
                continue;
            }
            if history.texture(&entry.texture_id).is_none() {
                match self
                    .fetch_history_texture(history, &entry.texture_id, cancel)
                    .await
                {
                    Ok(_) => {}
                    Err(error) if error.is_cancelled() => return Err(error),
                    Err(error) => {
                        tracing::debug!(code = %error.code(), "skin history texture not downloaded");
                        continue;
                    }
                }
            }
            if history
                .make_thumbnail(&entry.texture_id, entry.variant)
                .is_some()
            {
                made += 1;
            }
        }
        Ok(made)
    }

    /// Puts a skin of the history back on, with the arm model it was last worn with. Its texture
    /// is uploaded like a skin file (the cached copy, else downloaded from Mojang's texture host
    /// first); only when no copy can be had is Mojang asked to take it from its own address. The
    /// history records it as worn through the launcher; when Mojang files it under a new texture
    /// id, the entry follows. A skin the account wears already is not sent again.
    pub async fn wear_history_skin(
        &self,
        id: Uuid,
        texture_id: &str,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        let account = self.require_microsoft(id)?;
        let history = self.history.as_ref().ok_or(AuthError::NotInSkinHistory)?;
        let entry = history
            .entry(account.uuid, texture_id)
            .ok_or(AuthError::NotInSkinHistory)?;
        let before = current_texture(&account);
        let same_variant = account
            .skin
            .as_ref()
            .is_some_and(|skin| skin.variant == entry.variant);
        if before.as_deref() == Some(entry.texture_id.as_str()) && same_variant {
            return Ok(account);
        }
        let secrets = self.ensure_fresh(id, cancel).await?;
        let texture = match history.texture(&entry.texture_id) {
            Some(texture) => Some(texture),
            None => match self
                .fetch_history_texture(history, &entry.texture_id, cancel)
                .await
            {
                Ok(texture) => Some(texture),
                Err(error) if error.is_cancelled() => return Err(error),
                Err(error) => {
                    tracing::debug!(code = %error.code(), "skin texture not downloaded, wearing by URL");
                    None
                }
            },
        };
        let profile = match texture {
            Some(png) => {
                self.auth
                    .upload_skin(&secrets.access_token, png, entry.variant, cancel)
                    .await?
            }
            None => {
                let url = self.auth.config().endpoints.texture(&entry.texture_id);
                self.auth
                    .set_skin_url(&secrets.access_token, &url, entry.variant, cancel)
                    .await?
            }
        };
        let worn = self.apply_profile(id, &profile, SkinSource::Launcher)?;
        // A profile that still shows the previous skin did not give this one a new id.
        if let Some(filed_as) = current_texture(&worn)
            .filter(|filed_as| *filed_as != entry.texture_id && Some(filed_as) != before.as_ref())
            && let Err(error) = history.retarget(worn.uuid, &entry.texture_id, &filed_as)
        {
            tracing::warn!(account = %id, code = %error.code(), "skin history entry not moved");
        }
        tracing::info!(account = %id, "skin from the history worn");
        Ok(worn)
    }

    /// Marks or unmarks a skin of an account's history as a favourite.
    pub fn set_skin_favorite(&self, id: Uuid, texture_id: &str, favorite: bool) -> Result<()> {
        let account = self.require_microsoft(id)?;
        let history = self.history.as_ref().ok_or(AuthError::NotInSkinHistory)?;
        if history.set_favorite(account.uuid, texture_id, favorite)? {
            Ok(())
        } else {
            Err(AuthError::NotInSkinHistory)
        }
    }

    /// Removes a skin from an account's history (nothing to do when it is gone already);
    /// [`Self::restore_skin`] can bring it back for a few minutes.
    pub fn forget_skin(&self, id: Uuid, texture_id: &str) -> Result<()> {
        let account = self.require_microsoft(id)?;
        if let Some(history) = &self.history {
            history.remove(account.uuid, texture_id)?;
        }
        Ok(())
    }

    /// Brings back a skin removed from an account's history moments ago.
    pub fn restore_skin(&self, id: Uuid, texture_id: &str) -> Result<()> {
        let account = self.require_microsoft(id)?;
        let history = self.history.as_ref().ok_or(AuthError::NotInSkinHistory)?;
        if history.restore(account.uuid, texture_id)? {
            Ok(())
        } else {
            Err(AuthError::NotInSkinHistory)
        }
    }

    /// The skin history's part of the daily tidy-up ([`SkinHistory::tidy`]): the profiles of the
    /// saved Microsoft accounts keep theirs. `None` without a history.
    pub fn tidy_skin_history(&self) -> Option<HistoryTidy> {
        let history = self.history.as_ref()?;
        let signed_in: Vec<Uuid> = self
            .accounts
            .list()
            .iter()
            .filter(|account| account.is_microsoft())
            .map(|account| account.uuid)
            .collect();
        Some(history.tidy(&signed_in, SystemTime::now()))
    }

    fn history_texture(&self, texture_id: &str) -> Option<Vec<u8>> {
        self.history.as_ref()?.texture(texture_id)
    }

    /// Downloads a texture of the history from Mojang's public texture host and caches it.
    async fn fetch_history_texture(
        &self,
        history: &SkinHistory,
        texture_id: &str,
        cancel: &CancellationToken,
    ) -> Result<Vec<u8>> {
        let url = self.auth.config().endpoints.texture(texture_id);
        let png = self.auth.download_texture(&url, cancel).await?;
        validate_skin_png(&png).map_err(|problem| {
            AuthError::unexpected(
                AuthStep::Texture,
                format!("not a skin texture: {problem:?}"),
            )
        })?;
        if let Err(error) = history.store_texture(texture_id, &png) {
            tracing::warn!(code = %error.code(), "skin texture not cached");
        }
        Ok(png)
    }

    /// Saves a profile just read from (or returned by) Minecraft Services into the account and
    /// records its skin in the history.
    fn apply_profile(
        &self,
        id: Uuid,
        profile: &MinecraftProfile,
        source: SkinSource,
    ) -> Result<Account> {
        let account = self
            .accounts
            .update(id, |account| account.apply_profile(profile))?;
        self.note_profile_read(id);
        self.record_skin(&account, source);
        Ok(account)
    }

    /// Records the account's skin in the history; a failure only costs the record.
    fn record_skin(&self, account: &Account, source: SkinSource) {
        let (Some(history), Some(skin)) = (&self.history, &account.skin) else {
            return;
        };
        if !account.is_microsoft() {
            return;
        }
        if let Err(error) = history.record(account.uuid, skin, source, Timestamp::now()) {
            tracing::warn!(account = %account.id, code = %error.code(), "skin history not saved");
        }
    }

    fn note_profile_read(&self, id: Uuid) {
        self.profile_reads.lock().insert(id, Instant::now());
    }

    /// True, and noted as a read now, when the profile was not read within
    /// [`PROFILE_SYNC_EVERY`].
    fn claim_profile_read(&self, id: Uuid) -> bool {
        let mut reads = self.profile_reads.lock();
        let due = reads
            .get(&id)
            .is_none_or(|at| at.elapsed() >= PROFILE_SYNC_EVERY);
        if due {
            reads.insert(id, Instant::now());
        }
        due
    }

    // --- helpers -------------------------------------------------------------------------------

    fn require_microsoft(&self, id: Uuid) -> Result<Account> {
        let account = self
            .accounts
            .get(id)
            .ok_or(AuthError::AccountNotFound { id })?;
        if account.is_microsoft() {
            Ok(account)
        } else {
            Err(AuthError::NotMicrosoftAccount { id })
        }
    }

    fn refresh_lock(&self, id: Uuid) -> Arc<tokio::sync::Mutex<()>> {
        Arc::clone(self.refresh_locks.lock().entry(id).or_default())
    }

    fn mark_used(&self, id: Uuid) {
        if let Err(error) = self.accounts.mark_used(id) {
            tracing::warn!(account = %id, code = %error.code(), "could not record account use");
        }
    }

    pub(crate) async fn load_secrets(&self, id: Uuid) -> Result<Option<AccountSecrets>> {
        let store = Arc::clone(&self.secrets);
        tokio::task::spawn_blocking(move || store.load(id))
            .await
            .map_err(|error| AuthError::keychain("load", false, error.to_string()))?
    }

    pub(crate) async fn save_secrets(&self, id: Uuid, secrets: AccountSecrets) -> Result<()> {
        let store = Arc::clone(&self.secrets);
        tokio::task::spawn_blocking(move || store.save(id, &secrets))
            .await
            .map_err(|error| AuthError::keychain("save", false, error.to_string()))?
    }

    pub(crate) async fn delete_secrets(&self, id: Uuid) -> Result<()> {
        let store = Arc::clone(&self.secrets);
        tokio::task::spawn_blocking(move || store.delete(id))
            .await
            .map_err(|error| AuthError::keychain("delete", false, error.to_string()))?
    }

    /// Account list mutations for the legacy import (same rules as [`Self::add_microsoft`]).
    pub(crate) async fn membership_lock(&self) -> tokio::sync::MutexGuard<'_, ()> {
        self.membership.lock().await
    }
}

/// The texture id of the skin an account wears, unless it is one of Mojang's default skins.
fn current_texture(account: &Account) -> Option<String> {
    account
        .skin
        .as_ref()
        .filter(|skin| !skin.is_default())
        .and_then(|skin| texture_id(&skin.url))
}

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use jiff::SignedDuration;
    use serde_json::json;

    use super::*;
    use crate::model::{Ownership, ProfileSkin, UserType};
    use crate::offline::OFFLINE_ACCESS_TOKEN;
    use crate::secrets::MemoryStore;
    use crate::testutil::{self, MockReply, MockServer, Recorded};

    struct Fixture {
        _dir: tempfile::TempDir,
        manager: AuthManager,
        secrets: Arc<MemoryStore>,
    }

    fn fixture(base: &str) -> Fixture {
        let dir = tempfile::tempdir().unwrap();
        let mut config = testutil::config_for_base(base);
        config.connect_retries = 0;
        let auth = MicrosoftAuth::with_config(testutil::http(), config);
        let accounts = AccountStore::open(dir.path().join("accounts.json")).unwrap();
        let secrets = Arc::new(MemoryStore::new());
        let manager = AuthManager::new(auth, accounts, secrets.clone());
        Fixture {
            _dir: dir,
            manager,
            secrets,
        }
    }

    fn in_minutes(minutes: i64) -> Timestamp {
        Timestamp::now()
            .checked_add(SignedDuration::from_mins(minutes))
            .unwrap()
    }

    fn stored_secrets(expires_at: Timestamp) -> AccountSecrets {
        AccountSecrets {
            refresh_token: Some("old-refresh".into()),
            access_token: "old-access".into(),
            expires_at,
            xuid: Some("2535400000000009".into()),
        }
    }

    async fn seed(manager: &AuthManager, expires_at: Timestamp) -> Account {
        let profile = MinecraftProfile {
            id: testutil::profile_uuid(),
            name: "OldName".into(),
            skins: Vec::new(),
            capes: Vec::new(),
        };
        manager
            .add_microsoft(MicrosoftLogin {
                profile,
                secrets: stored_secrets(expires_at),
                ownership: Ownership::default(),
            })
            .await
            .unwrap()
    }

    #[tokio::test]
    async fn accounts_persist_without_secrets_on_disk() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let offline = f.manager.add_offline("  Notch ").await.unwrap();
        assert_eq!(offline.username, "Notch");
        assert_eq!(
            offline.uuid.to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        let microsoft = seed(&f.manager, in_minutes(60)).await;
        assert_eq!(f.manager.active_account().unwrap().id, microsoft.id);
        // Signing in again updates the same account.
        let again = seed(&f.manager, in_minutes(90)).await;
        assert_eq!(again.id, microsoft.id);
        assert_eq!(f.manager.accounts().len(), 2);

        let reopened = AccountStore::open(f.manager.store().path()).unwrap();
        assert_eq!(reopened.list().len(), 2);
        assert_eq!(reopened.active_id(), Some(microsoft.id));
        let raw = std::fs::read_to_string(f.manager.store().path()).unwrap();
        assert!(!raw.contains("old-refresh"));
        assert!(!raw.contains("old-access"));
        assert_eq!(f.secrets.len(), 1);
        assert!(f.secrets.load(microsoft.id).unwrap().is_some());
        assert!(f.secrets.load(offline.id).unwrap().is_none());

        assert!(matches!(
            f.manager.add_offline("bad name").await,
            Err(AuthError::InvalidOfflineName { .. })
        ));
        // Removing the active account selects the one that is left.
        let active = f.manager.remove(microsoft.id).await.unwrap();
        assert_eq!(active, Some(offline.id));
        assert!(f.secrets.is_empty());
        assert_eq!(f.manager.accounts().len(), 1);
        assert_eq!(f.manager.active_account().unwrap().id, offline.id);
    }

    #[tokio::test]
    async fn removals_can_be_undone_with_their_session() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let offline = f.manager.add_offline("Alex").await.unwrap();
        let microsoft = seed(&f.manager, in_minutes(60)).await;
        assert_eq!(f.manager.active_account().unwrap().id, microsoft.id);

        assert_eq!(
            f.manager.remove(microsoft.id).await.unwrap(),
            Some(offline.id)
        );
        // Gone from the keychain and the file at once.
        assert!(f.secrets.is_empty());
        let reopened = AccountStore::open(f.manager.store().path()).unwrap();
        assert_eq!(reopened.list().len(), 1);

        let back = f.manager.undo_remove(microsoft.id).await.unwrap();
        assert_eq!(back.id, microsoft.id);
        let order: Vec<Uuid> = f.manager.accounts().iter().map(|a| a.id).collect();
        assert_eq!(order, vec![offline.id, microsoft.id]);
        assert_eq!(f.manager.active_account().unwrap().id, microsoft.id);
        assert_eq!(
            f.secrets.load(microsoft.id).unwrap().unwrap().access_token,
            "old-access"
        );
        // Nothing left to undo.
        assert!(f.manager.undo_remove(microsoft.id).await.is_err());

        // Forgotten removals are final.
        f.manager.remove(offline.id).await.unwrap();
        assert!(f.manager.forget_removed(offline.id));
        assert!(!f.manager.forget_removed(offline.id));
        assert!(matches!(
            f.manager.undo_remove(offline.id).await,
            Err(AuthError::AccountNotFound { .. })
        ));
        assert_eq!(f.manager.accounts().len(), 1);
    }

    #[tokio::test]
    async fn signing_in_again_during_the_undo_window_keeps_one_account() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let first = seed(&f.manager, in_minutes(60)).await;
        f.manager.remove(first.id).await.unwrap();
        let second = seed(&f.manager, in_minutes(90)).await;
        assert_ne!(second.id, first.id);

        let kept = f.manager.undo_remove(first.id).await.unwrap();
        assert_eq!(kept.id, second.id);
        assert_eq!(f.manager.accounts().len(), 1);
        // The old session was not written back under the dropped id.
        assert_eq!(f.secrets.len(), 1);
        assert!(f.secrets.load(first.id).unwrap().is_none());
    }

    #[tokio::test]
    async fn offline_accounts_can_be_renamed() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let steve = f.manager.add_offline("Steve").await.unwrap();
        let alex = f.manager.add_offline("Alex").await.unwrap();
        f.manager.set_active(steve.id).unwrap();

        let renamed = f
            .manager
            .rename_offline(steve.id, " Herobrine ")
            .await
            .unwrap();
        assert_eq!(renamed.id, steve.id);
        assert_eq!(renamed.username, "Herobrine");
        assert_eq!(renamed.uuid, offline_uuid("Herobrine"));
        assert_eq!(renamed.added_at, steve.added_at);
        assert_eq!(f.manager.active_account().unwrap().id, steve.id);
        assert_eq!(f.manager.accounts()[0].id, steve.id);

        // Case changes are allowed for the same account, not for another one.
        let cased = f
            .manager
            .rename_offline(steve.id, "HEROBRINE")
            .await
            .unwrap();
        assert_eq!(cased.uuid, offline_uuid("HEROBRINE"));
        let taken = f
            .manager
            .rename_offline(steve.id, "alex")
            .await
            .unwrap_err();
        assert_eq!(taken.code(), "AD-VAR");
        assert!(matches!(
            f.manager.rename_offline(alex.id, "x y").await,
            Err(AuthError::InvalidOfflineName {
                problem: OfflineNameProblem::InvalidCharacters
            })
        ));
        let same = f.manager.rename_offline(alex.id, "Alex").await.unwrap();
        assert_eq!(same, f.manager.account(alex.id).unwrap());

        let microsoft = seed(&f.manager, in_minutes(60)).await;
        assert!(
            f.manager
                .rename_offline(microsoft.id, "Other")
                .await
                .is_err()
        );
        assert_eq!(f.manager.account(microsoft.id).unwrap().username, "OldName");
    }

    #[tokio::test]
    async fn pins_and_pictures_survive_renames_and_undo() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let steve = f.manager.add_offline("Steve").await.unwrap();
        let alex = f.manager.add_offline("Alex").await.unwrap();
        let microsoft = seed(&f.manager, in_minutes(60)).await;

        // Offline accounts pick a bundled picture; anything else is refused.
        let fox = f.manager.set_avatar(alex.id, Some("preset:fox")).unwrap();
        assert_eq!(fox.avatar.as_deref(), Some("preset:fox"));
        assert!(f.manager.set_avatar(alex.id, Some("fox")).is_err());
        assert!(
            f.manager
                .set_avatar(alex.id, Some("https://example.com/a.png"))
                .is_err()
        );
        assert!(
            f.manager
                .set_avatar(microsoft.id, Some("preset:fox"))
                .is_err()
        );
        assert_eq!(f.manager.account(microsoft.id).unwrap().avatar, None);
        assert!(matches!(
            f.manager.set_avatar(Uuid::new_v4(), None),
            Err(AuthError::AccountNotFound { .. })
        ));

        // Pins, pinned order.
        f.manager.set_pinned(microsoft.id, true).unwrap();
        f.manager.set_pinned(alex.id, true).unwrap();
        let order: Vec<Uuid> = f.manager.accounts().iter().map(|a| a.id).collect();
        assert_eq!(order, vec![microsoft.id, alex.id, steve.id]);
        f.manager
            .reorder_pinned(&[alex.id, steve.id, microsoft.id])
            .unwrap();
        let order: Vec<Uuid> = f.manager.accounts().iter().map(|a| a.id).collect();
        assert_eq!(order, vec![alex.id, microsoft.id, steve.id]);

        // A rename keeps both, and so does an undone removal (session included).
        let renamed = f.manager.rename_offline(alex.id, "Alexa").await.unwrap();
        assert!(renamed.pinned);
        assert_eq!(renamed.avatar.as_deref(), Some("preset:fox"));
        f.manager.remove(alex.id).await.unwrap();
        let back = f.manager.undo_remove(alex.id).await.unwrap();
        assert!(back.pinned);
        assert_eq!(back.avatar.as_deref(), Some("preset:fox"));
        assert_eq!(f.manager.accounts()[0].id, alex.id);

        // Persisted; `None` goes back to the head.
        let reopened = AccountStore::open(f.manager.store().path()).unwrap();
        let saved = reopened.get(alex.id).unwrap();
        assert!(saved.pinned);
        assert_eq!(saved.avatar.as_deref(), Some("preset:fox"));
        assert_eq!(f.manager.set_avatar(alex.id, None).unwrap().avatar, None);
        assert!(!f.manager.set_pinned(alex.id, false).unwrap().pinned);
    }

    #[tokio::test]
    async fn offline_names_are_unique_ignoring_case() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let steve = f.manager.add_offline("Steve").await.unwrap();
        f.manager.add_offline("Alex").await.unwrap();
        let error = f.manager.add_offline("steve").await.unwrap_err();
        assert_eq!(error.code(), "AD-VAR");
        assert!(error.user_message_tr().contains("zaten var"));
        // The exact name selects the saved account again.
        let again = f.manager.add_offline("Steve").await.unwrap();
        assert_eq!(again.id, steve.id);
        assert_eq!(f.manager.active_account().unwrap().id, steve.id);
        assert_eq!(f.manager.accounts().len(), 2);
    }

    #[tokio::test]
    async fn a_renewed_sign_in_can_keep_the_selection() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let alex = f.manager.add_offline("Alex").await.unwrap();
        let profile = MinecraftProfile {
            id: testutil::profile_uuid(),
            name: "Steve".into(),
            skins: Vec::new(),
            capes: Vec::new(),
        };
        let login = || MicrosoftLogin {
            profile: profile.clone(),
            secrets: stored_secrets(in_minutes(60)),
            ownership: Ownership::default(),
        };
        let added = f.manager.save_microsoft(login(), false).await.unwrap();
        assert!(added.created);
        assert_eq!(f.manager.active_account().unwrap().id, alex.id);
        let again = f.manager.save_microsoft(login(), true).await.unwrap();
        assert!(!again.created);
        assert_eq!(again.account.id, added.account.id);
        assert_eq!(f.manager.active_account().unwrap().id, added.account.id);

        // With nothing selected, a sign-in is selected even when asked not to.
        f.manager.remove(alex.id).await.unwrap();
        f.manager.remove(added.account.id).await.unwrap();
        let alone = f.manager.save_microsoft(login(), false).await.unwrap();
        assert_eq!(f.manager.active_account().unwrap().id, alone.account.id);
    }

    #[tokio::test]
    async fn expired_sessions_are_flagged_until_the_next_sign_in() {
        let server = MockServer::start(|request: &Recorded| {
            if request.path == "/consumers/oauth2/v2.0/token" {
                MockReply::json(400, json!({"error": "invalid_grant"}))
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        let f = fixture(server.base());
        let cancel = CancellationToken::new();
        let account = seed(&f.manager, in_minutes(1)).await;
        assert!(!account.needs_relogin);
        assert_eq!(
            f.manager.check_session(account.id, &cancel).await.unwrap(),
            SessionCheck::Expired
        );
        assert!(f.manager.account(account.id).unwrap().needs_relogin);
        // Persisted, so the list shows it after a restart.
        let reopened = AccountStore::open(f.manager.store().path()).unwrap();
        assert!(reopened.get(account.id).unwrap().needs_relogin);

        // Signing in again clears it; a fresh token needs no network to check.
        let again = seed(&f.manager, in_minutes(60)).await;
        assert_eq!(again.id, account.id);
        assert!(!f.manager.account(account.id).unwrap().needs_relogin);
        assert_eq!(
            f.manager.check_session(account.id, &cancel).await.unwrap(),
            SessionCheck::Valid
        );

        let offline = f.manager.add_offline("Alex").await.unwrap();
        assert!(matches!(
            f.manager.check_session(offline.id, &cancel).await,
            Err(AuthError::NotMicrosoftAccount { .. })
        ));
    }

    #[tokio::test]
    async fn unreachable_services_leave_the_session_unknown() {
        let closed = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let base = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        let f = fixture(&base);
        let account = seed(&f.manager, in_minutes(-60)).await;
        let state = f
            .manager
            .check_session(account.id, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(state, SessionCheck::Unknown);
        assert!(!f.manager.account(account.id).unwrap().needs_relogin);

        // A lost keychain entry needs a new sign-in.
        f.secrets.delete(account.id).unwrap();
        let state = f
            .manager
            .check_session(account.id, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(state, SessionCheck::Expired);
    }

    #[tokio::test]
    async fn heads_are_cut_once_and_cached() {
        use crate::avatar::tests::texture;
        use crate::avatar::{DefaultSkin, HeadCache, HeadSource};

        let skin = texture(64, 64, |x, y| {
            if (8..16).contains(&x) && (8..16).contains(&y) {
                [1, 2, 3, 255]
            } else {
                [0, 0, 0, 0]
            }
        });
        let served = skin.clone();
        let server = MockServer::start(move |request: &Recorded| {
            if request.path == "/texture/abc" {
                MockReply::bytes(200, "image/png", served.clone())
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        let f = fixture(server.base());
        let dir = tempfile::tempdir().unwrap();
        let cache = HeadCache::new(dir.path().join("heads"));
        let cancel = CancellationToken::new();

        let source = HeadSource::Texture {
            url: format!("{}/texture/abc", server.base()),
            id: "abc".into(),
        };
        let head = f
            .manager
            .head_from(&source, &cache, &[], &cancel)
            .await
            .unwrap()
            .unwrap();
        assert!(head.starts_with(b"\x89PNG"));
        let again = f
            .manager
            .head_from(&source, &cache, &[], &cancel)
            .await
            .unwrap();
        assert_eq!(again, Some(head));
        assert_eq!(server.hits("/texture/abc"), 1);

        // Offline accounts: the default skin from a local client jar, or nothing.
        let offline = f.manager.add_offline("Steve").await.unwrap();
        assert_eq!(
            f.manager
                .head(offline.id, &cache, &[], &cancel)
                .await
                .unwrap(),
            None
        );
        let jar = dir.path().join("26.3.jar");
        {
            use std::io::Write as _;
            let mut writer = zip::ZipWriter::new(std::fs::File::create(&jar).unwrap());
            writer
                .start_file(
                    DefaultSkin::for_profile(offline.uuid).jar_entry(),
                    zip::write::SimpleFileOptions::default(),
                )
                .unwrap();
            writer.write_all(&skin).unwrap();
            writer.finish().unwrap();
        }
        let head = f
            .manager
            .head(offline.id, &cache, std::slice::from_ref(&jar), &cancel)
            .await
            .unwrap()
            .unwrap();
        let account = f.manager.account(offline.id).unwrap();
        assert_eq!(cache.for_account(&account), Some(head));

        // A Microsoft profile that was never read has no head to show.
        let microsoft = seed(&f.manager, in_minutes(60)).await;
        assert!(microsoft.skin.is_none());
        assert_eq!(
            f.manager
                .head(microsoft.id, &cache, &[], &cancel)
                .await
                .unwrap(),
            None
        );
    }

    #[tokio::test]
    async fn valid_tokens_are_not_refreshed() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let account = seed(&f.manager, in_minutes(30)).await;
        let secrets = f
            .manager
            .ensure_fresh(account.id, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(secrets.access_token, "old-access");
        assert!(server.requests().is_empty());
    }

    #[tokio::test]
    async fn expiring_tokens_are_refreshed_keeping_the_refresh_token() {
        let server = MockServer::start(|request: &Recorded| {
            if request.path == "/consumers/oauth2/v2.0/token" {
                assert_eq!(request.form()["grant_type"], "refresh_token");
                assert_eq!(request.form()["refresh_token"], "old-refresh");
                // Microsoft does not always rotate the refresh token.
                MockReply::json(
                    200,
                    json!({"access_token": "ms-access", "expires_in": 3600}),
                )
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        let f = fixture(server.base());
        let account = seed(&f.manager, in_minutes(5)).await;
        let fresh = f
            .manager
            .ensure_fresh(account.id, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(fresh.access_token, testutil::mc_token());
        assert_eq!(fresh.refresh_token.as_deref(), Some("old-refresh"));
        assert!(fresh.expires_at > in_minutes(60));
        assert_eq!(f.secrets.load(account.id).unwrap(), Some(fresh));
        // The best-effort profile update renamed the account.
        assert_eq!(f.manager.account(account.id).unwrap().username, "Steve");
    }

    /// A keychain that refuses writes on demand.
    #[derive(Default)]
    struct RefusingStore {
        inner: MemoryStore,
        refuse: std::sync::atomic::AtomicBool,
    }

    impl SecretStore for RefusingStore {
        fn load(&self, account_id: Uuid) -> Result<Option<AccountSecrets>> {
            self.inner.load(account_id)
        }

        fn save(&self, account_id: Uuid, secrets: &AccountSecrets) -> Result<()> {
            if self.refuse.load(std::sync::atomic::Ordering::SeqCst) {
                return Err(AuthError::keychain("save", false, "refused"));
            }
            self.inner.save(account_id, secrets)
        }

        fn delete(&self, account_id: Uuid) -> Result<()> {
            self.inner.delete(account_id)
        }
    }

    /// The keychain refuses to store a refreshed session: the game still gets the fresh one, and
    /// the stored session (never half overwritten, see `KeyringStore`) stays for the next refresh.
    #[tokio::test]
    async fn a_refreshed_session_the_keychain_cannot_store_still_plays() {
        let server = MockServer::start(testutil::happy_chain).await;
        let dir = tempfile::tempdir().unwrap();
        let mut config = testutil::config_for_base(server.base());
        config.connect_retries = 0;
        let auth = MicrosoftAuth::with_config(testutil::http(), config);
        let accounts = AccountStore::open(dir.path().join("accounts.json")).unwrap();
        let store = Arc::new(RefusingStore::default());
        let manager = AuthManager::new(auth, accounts, store.clone());
        let expires = in_minutes(5);
        let account = seed(&manager, expires).await;

        store
            .refuse
            .store(true, std::sync::atomic::Ordering::SeqCst);
        let fresh = manager
            .ensure_fresh(account.id, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(fresh.access_token, testutil::mc_token());
        assert_eq!(
            store.load(account.id).unwrap(),
            Some(stored_secrets(expires))
        );
        assert!(!manager.account(account.id).unwrap().needs_relogin);
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn concurrent_refreshes_share_one_request() {
        let server = MockServer::start(|request: &Recorded| {
            if request.path == "/consumers/oauth2/v2.0/token" {
                std::thread::sleep(std::time::Duration::from_millis(300));
            }
            testutil::happy_chain(request)
        })
        .await;
        let f = fixture(server.base());
        let account = seed(&f.manager, in_minutes(-5)).await;
        let cancel = CancellationToken::new();
        let (a, b, c) = tokio::join!(
            f.manager.ensure_fresh(account.id, &cancel),
            f.manager.ensure_fresh(account.id, &cancel),
            f.manager.ensure_fresh(account.id, &cancel),
        );
        let (a, b, c) = (a.unwrap(), b.unwrap(), c.unwrap());
        assert_eq!(a, b);
        assert_eq!(b, c);
        assert_eq!(server.hits("/consumers/oauth2/v2.0/token"), 1);
        assert_eq!(a.refresh_token.as_deref(), Some("ms-refresh-new"));
    }

    #[tokio::test(flavor = "multi_thread", worker_threads = 4)]
    async fn removing_an_account_while_its_session_refreshes_leaves_no_session_behind() {
        // Microsoft answers slowly; the player removes the account while the refresh (started
        // by the account center's session check) is still waiting.
        let server = MockServer::start(|request: &Recorded| {
            if request.path == "/consumers/oauth2/v2.0/token" {
                std::thread::sleep(std::time::Duration::from_millis(500));
            }
            testutil::happy_chain(request)
        })
        .await;
        let f = fixture(server.base());
        let account = seed(&f.manager, in_minutes(-5)).await;
        let cancel = CancellationToken::new();
        let (refreshed, removed) =
            tokio::join!(f.manager.ensure_fresh(account.id, &cancel), async {
                tokio::time::sleep(std::time::Duration::from_millis(100)).await;
                f.manager.remove(account.id).await
            });
        removed.unwrap();
        assert!(refreshed.is_ok(), "{refreshed:?}");
        assert!(f.manager.account(account.id).is_none());
        assert!(
            f.secrets.is_empty(),
            "the refreshed session outlived its account in the keychain"
        );

        // Undo brings back the session the refresh produced.
        let back = f.manager.undo_remove(account.id).await.unwrap();
        let session = f.secrets.load(back.id).unwrap().unwrap();
        assert_eq!(session.refresh_token.as_deref(), Some("ms-refresh-new"));
        f.manager.remove(back.id).await.unwrap();
        assert!(f.manager.forget_removed(back.id));
        assert!(f.secrets.is_empty());
    }

    async fn refresh_error(handler: fn(&Recorded) -> MockReply) -> AuthError {
        let server = MockServer::start(handler).await;
        let f = fixture(server.base());
        let account = seed(&f.manager, in_minutes(1)).await;
        f.manager
            .ensure_fresh(account.id, &CancellationToken::new())
            .await
            .unwrap_err()
    }

    #[tokio::test]
    async fn refresh_failures_are_classified() {
        let invalid_grant = refresh_error(|request| {
            if request.path == "/consumers/oauth2/v2.0/token" {
                MockReply::json(
                    400,
                    json!({"error": "invalid_grant",
                           "error_description": "AADSTS70000: The provided grant has expired.",
                           "error_codes": [70000],
                           "trace_id": "0f3e6f43-56d1-4c1d-9a6b-8b1a7c11e900"}),
                )
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        assert_eq!(invalid_grant.class(), FailureClass::NeedsRelogin);
        assert_eq!(invalid_grant.code(), "MSA-invalid_grant");
        assert!(invalid_grant.details().contains("70000"));
        assert!(invalid_grant.details().contains("Adım: microsoft-refresh"));
        assert!(
            invalid_grant
                .user_message_tr()
                .contains("Kayıtlı oturumunuzun süresi dolmuş")
        );

        let xbox_down = refresh_error(|request| {
            if request.path == "/user/authenticate" {
                MockReply::json(503, json!({}))
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        assert_eq!(xbox_down.class(), FailureClass::Offline);
        assert_eq!(xbox_down.code(), "XBL-503");

        let child = refresh_error(|request| {
            if request.path == "/xsts/authorize" {
                MockReply::json(401, json!({"XErr": 2_148_916_238_u64}))
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        assert_eq!(child.class(), FailureClass::Other);

        let rate_limited = refresh_error(|request| {
            if request.path == "/authentication/login_with_xbox" {
                MockReply::json(429, json!({}))
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        assert_eq!(rate_limited.code(), "MC-429");
        assert_eq!(rate_limited.class(), FailureClass::Offline);
    }

    #[tokio::test]
    async fn unreachable_services_mean_offline_and_the_cache_still_launches() {
        let closed = std::net::TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).unwrap();
        let base = format!("http://{}", closed.local_addr().unwrap());
        drop(closed);
        let f = fixture(&base);
        let account = seed(&f.manager, in_minutes(-60)).await;
        let cancel = CancellationToken::new();
        let error = f
            .manager
            .ensure_fresh(account.id, &cancel)
            .await
            .unwrap_err();
        assert_eq!(error.class(), FailureClass::Offline, "{error:?}");
        assert_eq!(error.code(), "AG-BAGLANTI");
        let error = f
            .manager
            .prepare_launch(account.id, &cancel)
            .await
            .unwrap_err();
        assert_eq!(error.class(), FailureClass::Offline);

        let identity = f.manager.prepare_offline_launch(account.id).await.unwrap();
        assert_eq!(identity.access_token, "old-access");
        assert_eq!(identity.user_type, UserType::Msa);
        assert_eq!(identity.xuid.as_deref(), Some("2535400000000009"));
        assert!(
            f.manager
                .account(account.id)
                .unwrap()
                .last_used_at
                .is_some()
        );
    }

    #[tokio::test]
    async fn missing_secrets_need_a_new_sign_in() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let account = seed(&f.manager, in_minutes(60)).await;
        f.secrets.delete(account.id).unwrap();
        let error = f
            .manager
            .ensure_fresh(account.id, &CancellationToken::new())
            .await
            .unwrap_err();
        assert_eq!(error.class(), FailureClass::NeedsRelogin);
        assert_eq!(error.code(), "KC-YOK");

        let offline = f.manager.add_offline("Alex").await.unwrap();
        assert!(matches!(
            f.manager
                .ensure_fresh(offline.id, &CancellationToken::new())
                .await,
            Err(AuthError::NotMicrosoftAccount { .. })
        ));
        let identity = f
            .manager
            .prepare_launch(offline.id, &CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(identity.access_token, OFFLINE_ACCESS_TOKEN);
        assert_eq!(identity.user_type.as_str(), "legacy");
    }

    #[tokio::test]
    async fn skin_changes_update_the_saved_account() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let account = seed(&f.manager, in_minutes(60)).await;
        assert!(account.skin.is_none());
        let cancel = CancellationToken::new();
        let updated = f
            .manager
            .upload_skin(
                account.id,
                testutil::png(64, 64),
                SkinVariant::Classic,
                &cancel,
            )
            .await
            .unwrap();
        assert_eq!(updated.username, "Steve");
        assert!(updated.skin.is_some());
        assert_eq!(updated.capes.len(), 1);
        assert_eq!(
            server
                .only("/minecraft/profile/skins")
                .header("authorization"),
            Some("Bearer old-access")
        );
        let invalid = f
            .manager
            .upload_skin(account.id, b"nope".to_vec(), SkinVariant::Classic, &cancel)
            .await
            .unwrap_err();
        assert_eq!(invalid.code(), "SKIN-DOSYA");
        let offline = f.manager.add_offline("Alex").await.unwrap();
        assert!(f.manager.hide_cape(offline.id, &cancel).await.is_err());
    }

    fn requests_to(server: &MockServer, path: &str) -> Vec<Recorded> {
        server
            .requests()
            .into_iter()
            .filter(|request| request.path == path)
            .collect()
    }

    #[tokio::test]
    async fn appearance_is_fresh_online_and_the_saved_copy_offline() {
        use std::sync::atomic::{AtomicBool, Ordering};

        let down = Arc::new(AtomicBool::new(false));
        let flag = Arc::clone(&down);
        let server = MockServer::start(move |request: &Recorded| {
            if flag.load(Ordering::SeqCst) && request.path == "/minecraft/profile" {
                MockReply::json(503, json!({}))
            } else {
                testutil::happy_chain(request)
            }
        })
        .await;
        let f = fixture(server.base());
        let account = seed(&f.manager, in_minutes(60)).await;
        let cancel = CancellationToken::new();

        let fresh = f.manager.appearance(account.id, &cancel).await.unwrap();
        assert!(!fresh.stale);
        assert_eq!(fresh.account.username, "Steve");
        let skin = fresh.account.skin.clone().unwrap();
        assert_eq!(skin.url, "https://textures.minecraft.net/texture/abc");
        assert_eq!(skin.variant, SkinVariant::Classic);
        assert!(!skin.is_default());
        assert_eq!(fresh.account.capes.len(), 1);
        assert_eq!(fresh.account.capes[0].name, "Migrator");
        assert!(fresh.account.capes[0].active);
        // The refreshed profile was saved.
        assert_eq!(f.manager.account(account.id).unwrap(), fresh.account);

        down.store(true, Ordering::SeqCst);
        let saved = f.manager.appearance(account.id, &cancel).await.unwrap();
        assert!(saved.stale);
        assert_eq!(saved.account, fresh.account);

        let offline = f.manager.add_offline("Alex").await.unwrap();
        assert!(matches!(
            f.manager.appearance(offline.id, &cancel).await,
            Err(AuthError::NotMicrosoftAccount { .. })
        ));
    }

    #[tokio::test]
    async fn appearance_reports_an_expired_session() {
        let error = {
            let server = MockServer::start(|request: &Recorded| {
                if request.path == "/consumers/oauth2/v2.0/token" {
                    MockReply::json(400, json!({"error": "invalid_grant"}))
                } else {
                    testutil::happy_chain(request)
                }
            })
            .await;
            let f = fixture(server.base());
            let account = seed(&f.manager, in_minutes(1)).await;
            f.manager
                .appearance(account.id, &CancellationToken::new())
                .await
                .unwrap_err()
        };
        assert_eq!(error.class(), FailureClass::NeedsRelogin);
    }

    #[tokio::test]
    async fn skin_files_variants_and_capes() {
        let server = MockServer::start(testutil::happy_chain).await;
        let f = fixture(server.base());
        let account = seed(&f.manager, in_minutes(60)).await;
        let cancel = CancellationToken::new();

        // No saved skin yet: the profile is read first, then the texture is re-set by URL.
        f.manager
            .set_skin_variant(account.id, SkinVariant::Slim, &cancel)
            .await
            .unwrap();
        assert_eq!(server.hits("/minecraft/profile"), 1);
        let by_url = server.only("/minecraft/profile/skins");
        assert_eq!(
            by_url.json()["url"],
            "https://textures.minecraft.net/texture/abc"
        );
        assert_eq!(by_url.json()["variant"], "slim");
        // The mock answers with a classic skin: asking for classic sends nothing.
        f.manager
            .set_skin_variant(account.id, SkinVariant::Classic, &cancel)
            .await
            .unwrap();
        assert_eq!(server.hits("/minecraft/profile/skins"), 1);

        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("skin.png");
        std::fs::write(&file, testutil::png(64, 64)).unwrap();
        f.manager
            .upload_skin_file(account.id, &file, SkinVariant::Slim, &cancel)
            .await
            .unwrap();
        let skins = requests_to(&server, "/minecraft/profile/skins");
        assert_eq!(skins.len(), 2);
        let multipart = String::from_utf8_lossy(&skins[1].body);
        assert!(multipart.contains("name=\"file\"; filename=\"skin.png\""));
        assert!(multipart.contains("slim"));

        // Bad files are rejected before anything is sent.
        let missing = f
            .manager
            .upload_skin_file(
                account.id,
                &dir.path().join("gone.png"),
                SkinVariant::Classic,
                &cancel,
            )
            .await
            .unwrap_err();
        assert_eq!(missing.code(), "SKIN-DOSYA");
        assert_eq!(server.hits("/minecraft/profile/skins"), 2);

        f.manager
            .set_cape(account.id, Some("cape-1"), &cancel)
            .await
            .unwrap();
        f.manager.set_cape(account.id, None, &cancel).await.unwrap();
        let capes = requests_to(&server, "/minecraft/profile/capes/active");
        assert_eq!(capes.len(), 2);
        assert_eq!(capes[0].method, "PUT");
        assert_eq!(capes[0].json()["capeId"], "cape-1");
        assert_eq!(capes[1].method, "DELETE");

        let offline = f.manager.add_offline("Alex").await.unwrap();
        assert!(matches!(
            f.manager
                .upload_skin_file(offline.id, &file, SkinVariant::Classic, &cancel)
                .await,
            Err(AuthError::NotMicrosoftAccount { .. })
        ));
        assert!(matches!(
            f.manager
                .set_skin_variant(offline.id, SkinVariant::Slim, &cancel)
                .await,
            Err(AuthError::NotMicrosoftAccount { .. })
        ));
    }

    // --- skin history ------------------------------------------------------------------------

    /// Three different skin textures and their texture ids.
    fn history_textures() -> Vec<(&'static str, Vec<u8>)> {
        use crate::avatar::tests::texture;
        vec![
            ("a1", texture(64, 64, |x, y| [x as u8, y as u8, 1, 255])),
            ("b2", texture(64, 64, |x, y| [y as u8, x as u8, 2, 255])),
            ("c3", texture(64, 64, |x, _| [x as u8, 3, 3, 255])),
        ]
    }

    type Worn = Arc<Mutex<Option<(String, &'static str)>>>;

    /// Minecraft Services with a memory: the profile wears what was last uploaded (recognised
    /// by its bytes) or set by URL, and Mojang's texture host serves `/texture/<id>`.
    struct SkinWorld {
        server: MockServer,
        worn: Worn,
        /// Every skin change answers with this status instead.
        refuse: Arc<Mutex<Option<u16>>>,
        /// Textures the host does not have.
        missing: Arc<Mutex<Vec<String>>>,
        /// Mojang files a change to the first id under the second one.
        refile: Arc<Mutex<Option<(String, String)>>>,
    }

    fn profile_wearing(worn: &Option<(String, &str)>) -> serde_json::Value {
        let skins = match worn {
            Some((id, variant)) => json!([{
                "id": "s",
                "state": "ACTIVE",
                "url": format!("http://textures.minecraft.net/texture/{id}"),
                "variant": variant
            }]),
            None => json!([{
                "id": "d",
                "state": "ACTIVE",
                "url": "http://textures.minecraft.net/texture/5e7e",
                "variant": "CLASSIC",
                "alias": "STEVE"
            }]),
        };
        json!({
            "id": "069a79f444e94726a5befca90e38aaf5",
            "name": "Steve",
            "skins": skins,
            "capes": []
        })
    }

    async fn skin_world(wearing: Option<(&str, &'static str)>) -> SkinWorld {
        let worn: Worn = Arc::new(Mutex::new(wearing.map(|(id, v)| (id.to_owned(), v))));
        let refuse = Arc::new(Mutex::new(None::<u16>));
        let missing = Arc::new(Mutex::new(Vec::<String>::new()));
        let refile = Arc::new(Mutex::new(None::<(String, String)>));
        let (w, r, m, f) = (
            Arc::clone(&worn),
            Arc::clone(&refuse),
            Arc::clone(&missing),
            Arc::clone(&refile),
        );
        let textures = history_textures();
        let server = MockServer::start(move |request: &Recorded| {
            match (request.method.as_str(), request.path.as_str()) {
                ("GET", "/minecraft/profile") => MockReply::json(200, profile_wearing(&w.lock())),
                ("POST", "/minecraft/profile/skins") => {
                    if let Some(status) = *r.lock() {
                        return MockReply::json(status, json!({"errorMessage": "refused"}));
                    }
                    let by_url = request
                        .header("content-type")
                        .is_some_and(|kind| kind.starts_with("application/json"));
                    let (id, variant) = if by_url {
                        let body = request.json();
                        let url = body["url"].as_str().unwrap_or_default();
                        let id = url.rsplit('/').next().unwrap_or_default().to_owned();
                        (
                            id,
                            if body["variant"] == "slim" {
                                "SLIM"
                            } else {
                                "CLASSIC"
                            },
                        )
                    } else {
                        let id = textures
                            .iter()
                            .find(|(_, png)| {
                                request
                                    .body
                                    .windows(png.len())
                                    .any(|window| window == png.as_slice())
                            })
                            .map_or("f00d", |(id, _)| *id)
                            .to_owned();
                        let slim =
                            String::from_utf8_lossy(&request.body).contains("\r\n\r\nslim\r\n");
                        (id, if slim { "SLIM" } else { "CLASSIC" })
                    };
                    let id = match &*f.lock() {
                        Some((from, to)) if *from == id => to.clone(),
                        _ => id,
                    };
                    *w.lock() = Some((id, variant));
                    MockReply::json(200, profile_wearing(&w.lock()))
                }
                ("DELETE", "/minecraft/profile/skins/active") => {
                    *w.lock() = None;
                    MockReply::json(200, profile_wearing(&None))
                }
                ("GET", path) if path.starts_with("/texture/") => {
                    let id = &path["/texture/".len()..];
                    let gone = m.lock().iter().any(|missing| missing == id);
                    match textures.iter().find(|(known, _)| *known == id) {
                        Some((_, png)) if !gone => MockReply::bytes(200, "image/png", png.clone()),
                        _ => MockReply::json(404, json!({})),
                    }
                }
                _ => testutil::happy_chain(request),
            }
        })
        .await;
        SkinWorld {
            server,
            worn,
            refuse,
            missing,
            refile,
        }
    }

    fn history_fixture(base: &str) -> Fixture {
        let Fixture {
            _dir: dir,
            manager,
            secrets,
        } = fixture(base);
        let manager = manager.with_skin_history(SkinHistory::new(dir.path().join("skins")));
        Fixture {
            _dir: dir,
            manager,
            secrets,
        }
    }

    /// A signed-in Microsoft account wearing the texture `id` (classic arms).
    async fn seed_wearing(manager: &AuthManager, id: &str) -> Account {
        let profile = MinecraftProfile {
            id: testutil::profile_uuid(),
            name: "Steve".into(),
            skins: vec![ProfileSkin {
                id: "s".into(),
                url: format!("https://textures.minecraft.net/texture/{id}"),
                variant: SkinVariant::Classic,
                active: true,
                alias: None,
            }],
            capes: Vec::new(),
        };
        manager
            .add_microsoft(MicrosoftLogin {
                profile,
                secrets: stored_secrets(in_minutes(60)),
                ownership: Ownership::default(),
            })
            .await
            .unwrap()
    }

    fn history_ids(manager: &AuthManager, id: Uuid) -> Vec<String> {
        manager
            .skin_history_entries(id)
            .unwrap()
            .entries
            .into_iter()
            .map(|entry| entry.texture_id)
            .collect()
    }

    #[tokio::test]
    async fn skins_are_recorded_wherever_profiles_are_read() {
        let world = skin_world(Some(("b2", "SLIM"))).await;
        let f = history_fixture(world.server.base());
        let cancel = CancellationToken::new();

        // The sign-in records the skin of the profile.
        let account = seed_wearing(&f.manager, "a1").await;
        let list = f.manager.skin_history_entries(account.id).unwrap();
        assert_eq!(list.current.as_deref(), Some("a1"));
        assert_eq!(history_ids(&f.manager, account.id), ["a1"]);
        assert_eq!(list.entries[0].source, SkinSource::Profile);

        // The appearance panel reads the profile: changed elsewhere meanwhile.
        f.manager.appearance(account.id, &cancel).await.unwrap();
        let list = f.manager.skin_history_entries(account.id).unwrap();
        assert_eq!(list.current.as_deref(), Some("b2"));
        assert_eq!(history_ids(&f.manager, account.id), ["b2", "a1"]);
        assert_eq!(list.entries[0].variant, SkinVariant::Slim);

        // An upload through the launcher.
        let c3 = history_textures()[2].1.clone();
        f.manager
            .upload_skin(account.id, c3, SkinVariant::Classic, &cancel)
            .await
            .unwrap();
        let list = f.manager.skin_history_entries(account.id).unwrap();
        assert_eq!(history_ids(&f.manager, account.id), ["c3", "b2", "a1"]);
        assert_eq!(list.entries[0].source, SkinSource::Launcher);

        // Back to the default skin: nothing new, the old skins stay.
        f.manager.reset_skin(account.id, &cancel).await.unwrap();
        let list = f.manager.skin_history_entries(account.id).unwrap();
        assert_eq!(list.current, None);
        assert_eq!(list.entries.len(), 3);

        let offline = f.manager.add_offline("Alex").await.unwrap();
        assert!(matches!(
            f.manager.skin_history_entries(offline.id),
            Err(AuthError::NotMicrosoftAccount { .. })
        ));
    }

    #[tokio::test]
    async fn refreshes_and_profile_syncs_record_skins_too() {
        let world = skin_world(Some(("b2", "CLASSIC"))).await;
        let f = history_fixture(world.server.base());
        let cancel = CancellationToken::new();

        // A token about to expire: the refresh reads the profile.
        let account = seed(&f.manager, in_minutes(1)).await;
        assert!(history_ids(&f.manager, account.id).is_empty());
        f.manager.ensure_fresh(account.id, &cancel).await.unwrap();
        assert_eq!(history_ids(&f.manager, account.id), ["b2"]);

        // The account center's sync reads only when no read happened lately.
        *world.worn.lock() = Some(("c3".into(), "CLASSIC"));
        let reads = world.server.hits("/minecraft/profile");
        assert!(!f.manager.sync_profile(account.id, &cancel).await.unwrap());
        assert_eq!(world.server.hits("/minecraft/profile"), reads);
        // As if a quarter of an hour had passed.
        f.manager.profile_reads.lock().clear();
        assert!(f.manager.sync_profile(account.id, &cancel).await.unwrap());
        assert_eq!(history_ids(&f.manager, account.id), ["c3", "b2"]);
        assert_eq!(
            world
                .server
                .requests()
                .iter()
                .rfind(|request| request.path == "/minecraft/profile")
                .and_then(|request| request.header("authorization").map(str::to_owned)),
            Some(format!("Bearer {}", testutil::mc_token()))
        );
        // No change, no news.
        f.manager.profile_reads.lock().clear();
        assert!(!f.manager.sync_profile(account.id, &cancel).await.unwrap());

        // An expired session is left to the session check: no read.
        let stale = seed(&f.manager, in_minutes(-5)).await;
        f.manager.profile_reads.lock().clear();
        let reads = world.server.hits("/minecraft/profile");
        assert!(!f.manager.sync_profile(stale.id, &cancel).await.unwrap());
        assert_eq!(world.server.hits("/minecraft/profile"), reads);
    }

    #[tokio::test]
    async fn history_pictures_come_from_the_public_texture_host() {
        let world = skin_world(Some(("b2", "SLIM"))).await;
        let f = history_fixture(world.server.base());
        let cancel = CancellationToken::new();
        let account = seed_wearing(&f.manager, "a1").await;
        f.manager.appearance(account.id, &cancel).await.unwrap();

        world.missing.lock().push("a1".into());
        assert_eq!(
            f.manager
                .prepare_skin_history(account.id, &cancel)
                .await
                .unwrap(),
            1
        );
        let history = f.manager.skin_history().unwrap();
        assert!(history.thumbnail("b2", SkinVariant::Slim).is_some());
        assert!(history.thumbnail("a1", SkinVariant::Classic).is_none());
        // Textures are public: no credentials go with them.
        for request in world.server.requests() {
            if request.path.starts_with("/texture/") {
                assert_eq!(request.header("authorization"), None);
            }
        }

        // Drawn once; the next round only fetches what is still missing.
        world.missing.lock().clear();
        assert_eq!(
            f.manager
                .prepare_skin_history(account.id, &cancel)
                .await
                .unwrap(),
            1
        );
        assert_eq!(
            f.manager
                .prepare_skin_history(account.id, &cancel)
                .await
                .unwrap(),
            0
        );
        assert_eq!(world.server.hits("/texture/b2"), 1);
        assert_eq!(world.server.hits("/texture/a1"), 2);

        // The account list's head comes from the cached texture.
        let heads = HeadCache::new(f._dir.path().join("heads"));
        let worn = f.manager.account(account.id).unwrap();
        assert!(
            f.manager
                .head(worn.id, &heads, &[], &cancel)
                .await
                .unwrap()
                .is_some()
        );
        assert_eq!(world.server.hits("/texture/b2"), 1);
    }

    #[tokio::test]
    async fn old_skins_are_worn_again_by_uploading_their_texture() {
        let world = skin_world(Some(("b2", "SLIM"))).await;
        let f = history_fixture(world.server.base());
        let cancel = CancellationToken::new();
        let account = seed_wearing(&f.manager, "a1").await;
        f.manager.appearance(account.id, &cancel).await.unwrap();
        f.manager.set_skin_favorite(account.id, "a1", true).unwrap();

        // Nothing cached yet: the texture is downloaded first, then uploaded.
        let worn = f
            .manager
            .wear_history_skin(account.id, "a1", &cancel)
            .await
            .unwrap();
        assert_eq!(current_texture(&worn).as_deref(), Some("a1"));
        assert_eq!(worn.skin.as_ref().unwrap().variant, SkinVariant::Classic);
        let upload = world.server.only("/minecraft/profile/skins");
        assert!(
            upload
                .header("content-type")
                .unwrap()
                .starts_with("multipart/form-data")
        );
        assert_eq!(upload.header("authorization"), Some("Bearer old-access"));
        assert_eq!(world.server.hits("/texture/a1"), 1);
        let list = f.manager.skin_history_entries(account.id).unwrap();
        assert_eq!(list.current.as_deref(), Some("a1"));
        let a1 = list
            .entries
            .iter()
            .find(|entry| entry.texture_id == "a1")
            .unwrap();
        assert!(a1.favorite);
        assert_eq!(a1.source, SkinSource::Profile);
        assert_eq!(f.manager.account(account.id).unwrap(), worn);

        // Wearing what it wears already sends nothing.
        f.manager
            .wear_history_skin(account.id, "a1", &cancel)
            .await
            .unwrap();
        assert_eq!(world.server.hits("/minecraft/profile/skins"), 1);
        assert!(matches!(
            f.manager
                .wear_history_skin(account.id, "ffff", &cancel)
                .await,
            Err(AuthError::NotInSkinHistory)
        ));

        // From the cache: no download, slim arms as it was last worn.
        f.manager
            .prepare_skin_history(account.id, &cancel)
            .await
            .unwrap();
        let downloads = world.server.hits("/texture/b2");
        let worn = f
            .manager
            .wear_history_skin(account.id, "b2", &cancel)
            .await
            .unwrap();
        assert_eq!(current_texture(&worn).as_deref(), Some("b2"));
        assert_eq!(worn.skin.as_ref().unwrap().variant, SkinVariant::Slim);
        assert_eq!(world.server.hits("/texture/b2"), downloads);
        assert_eq!(world.server.hits("/minecraft/profile/skins"), 2);
    }

    #[tokio::test]
    async fn a_skin_without_any_copy_is_worn_by_its_address() {
        let world = skin_world(Some(("b2", "CLASSIC"))).await;
        let f = history_fixture(world.server.base());
        let cancel = CancellationToken::new();
        let account = seed_wearing(&f.manager, "a1").await;
        f.manager.appearance(account.id, &cancel).await.unwrap();

        world.missing.lock().push("a1".into());
        let worn = f
            .manager
            .wear_history_skin(account.id, "a1", &cancel)
            .await
            .unwrap();
        assert_eq!(current_texture(&worn).as_deref(), Some("a1"));
        let set = world.server.only("/minecraft/profile/skins");
        assert_eq!(
            set.json()["url"],
            format!("{}/texture/a1", world.server.base())
        );
        assert_eq!(set.json()["variant"], "classic");
    }

    #[tokio::test]
    async fn rate_limited_skin_changes_are_not_retried() {
        let world = skin_world(Some(("b2", "CLASSIC"))).await;
        let f = history_fixture(world.server.base());
        let cancel = CancellationToken::new();
        let account = seed_wearing(&f.manager, "a1").await;
        f.manager.appearance(account.id, &cancel).await.unwrap();
        f.manager
            .prepare_skin_history(account.id, &cancel)
            .await
            .unwrap();

        *world.refuse.lock() = Some(429);
        let error = f
            .manager
            .wear_history_skin(account.id, "a1", &cancel)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "MC-SKIN-429");
        assert!(error.user_message_tr().contains("çok fazla"));
        // One request: the address is not tried after a refusal.
        assert_eq!(world.server.hits("/minecraft/profile/skins"), 1);
        let list = f.manager.skin_history_entries(account.id).unwrap();
        assert_eq!(list.current.as_deref(), Some("b2"));
        assert_eq!(
            f.manager.account(account.id).unwrap().skin.unwrap().url,
            "https://textures.minecraft.net/texture/b2"
        );

        // Other refusals keep the history as it was too.
        *world.refuse.lock() = Some(400);
        let error = f
            .manager
            .wear_history_skin(account.id, "a1", &cancel)
            .await
            .unwrap_err();
        assert_eq!(error.code(), "MC-SKIN-400");
        assert_eq!(history_ids(&f.manager, account.id), ["b2", "a1"]);
    }

    #[tokio::test]
    async fn a_skin_mojang_files_under_a_new_id_keeps_its_entry() {
        let world = skin_world(Some(("b2", "CLASSIC"))).await;
        let f = history_fixture(world.server.base());
        let cancel = CancellationToken::new();
        let account = seed_wearing(&f.manager, "a1").await;
        f.manager.appearance(account.id, &cancel).await.unwrap();
        f.manager.set_skin_favorite(account.id, "a1", true).unwrap();
        let first_seen = f.manager.skin_history_entries(account.id).unwrap().entries[0].first_seen;

        *world.refile.lock() = Some(("a1".into(), "abcd".into()));
        let worn = f
            .manager
            .wear_history_skin(account.id, "a1", &cancel)
            .await
            .unwrap();
        assert_eq!(current_texture(&worn).as_deref(), Some("abcd"));
        let list = f.manager.skin_history_entries(account.id).unwrap();
        assert_eq!(history_ids(&f.manager, account.id), ["abcd", "b2"]);
        assert!(list.entries[0].favorite);
        assert_eq!(list.entries[0].first_seen, first_seen);
        assert_eq!(list.current.as_deref(), Some("abcd"));
    }

    #[tokio::test]
    async fn history_entries_can_be_removed_and_brought_back() {
        let world = skin_world(Some(("b2", "CLASSIC"))).await;
        let f = history_fixture(world.server.base());
        let cancel = CancellationToken::new();
        let account = seed_wearing(&f.manager, "a1").await;
        f.manager.appearance(account.id, &cancel).await.unwrap();

        f.manager.forget_skin(account.id, "a1").unwrap();
        assert_eq!(history_ids(&f.manager, account.id), ["b2"]);
        // Gone already: nothing to do.
        f.manager.forget_skin(account.id, "a1").unwrap();
        f.manager.restore_skin(account.id, "a1").unwrap();
        assert_eq!(history_ids(&f.manager, account.id), ["b2", "a1"]);
        assert!(matches!(
            f.manager.restore_skin(account.id, "a1"),
            Err(AuthError::NotInSkinHistory)
        ));
        let error = f
            .manager
            .set_skin_favorite(account.id, "ffff", true)
            .unwrap_err();
        assert_eq!(error.code(), "SKIN-GECMIS");
        assert!(!error.user_message_tr().contains("SKIN"));

        // The daily tidy-up keeps what signed-in profiles use.
        let report = f.manager.tidy_skin_history().unwrap();
        assert_eq!(report.histories, 0);
        assert_eq!(history_ids(&f.manager, account.id), ["b2", "a1"]);

        // Without a history nothing is recorded and nothing breaks.
        let plain = fixture(world.server.base());
        let other = seed_wearing(&plain.manager, "a1").await;
        assert!(history_ids(&plain.manager, other.id).is_empty());
        assert!(plain.manager.tidy_skin_history().is_none());
        assert_eq!(
            plain
                .manager
                .prepare_skin_history(other.id, &cancel)
                .await
                .unwrap(),
            0
        );
    }
}
