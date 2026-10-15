//! [`AuthManager`]: the stateful façade the app talks to. It ties the account list, the secret
//! store and the Microsoft client together, and serializes token refreshes per account.

use std::collections::HashMap;
use std::sync::Arc;

use jiff::Timestamp;
use parking_lot::Mutex;
use pl_net::CancellationToken;
use uuid::Uuid;

use crate::error::{AuthError, Result};
use crate::microsoft::{
    DeviceCodeLogin, LoginProgressFn, MicrosoftAuth, MicrosoftLogin, OpenUrlFn,
};
use crate::model::{
    Account, AccountKind, AccountSecrets, LaunchIdentityData, SkinVariant, launch_identity,
};
use crate::offline::{offline_uuid, validate_offline_name};
use crate::secrets::SecretStore;
use crate::skins::validate_skin_png;
use crate::store::AccountStore;

pub struct AuthManager {
    auth: MicrosoftAuth,
    accounts: AccountStore,
    secrets: Arc<dyn SecretStore>,
    /// One lock per account: concurrent `ensure_fresh` calls share a single refresh.
    refresh_locks: Mutex<HashMap<Uuid, Arc<tokio::sync::Mutex<()>>>>,
    /// Serializes adding and removing accounts (find-then-insert must not interleave).
    membership: tokio::sync::Mutex<()>,
}

impl AuthManager {
    pub fn new(auth: MicrosoftAuth, accounts: AccountStore, secrets: Arc<dyn SecretStore>) -> Self {
        Self {
            auth,
            accounts,
            secrets,
            refresh_locks: Mutex::new(HashMap::new()),
            membership: tokio::sync::Mutex::new(()),
        }
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

    // --- adding accounts -----------------------------------------------------------------------

    /// Browser sign-in, then saves the account (secrets to the keychain) and makes it active.
    pub async fn login_microsoft(
        &self,
        open_url: OpenUrlFn,
        progress: LoginProgressFn,
        cancel: &CancellationToken,
    ) -> Result<Account> {
        let login = self
            .auth
            .login_with_browser(open_url, progress, cancel)
            .await?;
        self.add_microsoft(login).await
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

    /// Saves a signed-in Microsoft account: secrets first (so an account never exists without
    /// them), then the account itself, which becomes active. Signing in again with the same
    /// profile updates the existing account instead of adding a duplicate.
    pub async fn add_microsoft(&self, login: MicrosoftLogin) -> Result<Account> {
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

        self.save_secrets(account.id, secrets).await?;
        let stored = self
            .accounts
            .put(account.clone())
            .and_then(|()| self.accounts.set_active(Some(account.id)));
        if let Err(error) = stored {
            if created {
                // Do not leave a refresh token behind for an account that was never saved.
                let _ = self.delete_secrets(account.id).await;
            }
            return Err(error);
        }
        tracing::info!(account = %account.id, created, "Microsoft account saved");
        Ok(account)
    }

    /// Adds (or reselects) an offline account and makes it active. Surrounding whitespace is
    /// ignored; the name must then be 3–16 characters of `[A-Za-z0-9_]`.
    pub async fn add_offline(&self, name: &str) -> Result<Account> {
        let _membership = self.membership.lock().await;
        let name = name.trim();
        validate_offline_name(name).map_err(|problem| AuthError::InvalidOfflineName { problem })?;
        let (account, _) =
            self.accounts
                .upsert(AccountKind::Offline, offline_uuid(name), |account| {
                    account.username = name.to_owned();
                })?;
        self.accounts.set_active(Some(account.id))?;
        Ok(account)
    }

    /// Removes an account and deletes its secrets. If the keychain cannot delete them, the
    /// account stays so the player can retry (no refresh token is left behind silently).
    pub async fn remove(&self, id: Uuid) -> Result<()> {
        let _membership = self.membership.lock().await;
        let account = self
            .accounts
            .get(id)
            .ok_or(AuthError::AccountNotFound { id })?;
        if account.is_microsoft() {
            self.delete_secrets(id).await?;
        }
        self.accounts.remove(id)?;
        self.refresh_locks.lock().remove(&id);
        Ok(())
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
    /// still works with the cached token) or `Other`.
    pub async fn ensure_fresh(
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
        self.save_secrets(id, fresh.clone()).await?;
        if let Some(profile) = refreshed.profile {
            if let Err(error) = self
                .accounts
                .update(id, |account| account.apply_profile(&profile))
            {
                tracing::warn!(account = %id, code = %error.code(), "profile update not saved");
            }
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
        self.accounts
            .update(id, |account| account.apply_profile(&profile))
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
        self.accounts
            .update(id, |account| account.apply_profile(&profile))
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
        self.accounts
            .update(id, |account| account.apply_profile(&profile))
    }

    pub async fn reset_skin(&self, id: Uuid, cancel: &CancellationToken) -> Result<Account> {
        let secrets = self.ensure_fresh(id, cancel).await?;
        let profile = self.auth.reset_skin(&secrets.access_token, cancel).await?;
        self.accounts
            .update(id, |account| account.apply_profile(&profile))
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
        self.accounts
            .update(id, |account| account.apply_profile(&profile))
    }

    pub async fn hide_cape(&self, id: Uuid, cancel: &CancellationToken) -> Result<Account> {
        let secrets = self.ensure_fresh(id, cancel).await?;
        let profile = self.auth.hide_cape(&secrets.access_token, cancel).await?;
        self.accounts
            .update(id, |account| account.apply_profile(&profile))
    }

    /// PNG bytes of a skin or cape texture, for the 3D preview.
    pub async fn download_texture(&self, url: &str, cancel: &CancellationToken) -> Result<Vec<u8>> {
        self.auth.download_texture(url, cancel).await
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

#[cfg(test)]
mod tests {
    use std::net::Ipv4Addr;

    use jiff::SignedDuration;
    use serde_json::json;

    use super::*;
    use crate::error::FailureClass;
    use crate::model::{MinecraftProfile, Ownership, UserType};
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
        f.manager.remove(microsoft.id).await.unwrap();
        assert!(f.secrets.is_empty());
        assert_eq!(f.manager.accounts().len(), 1);
        assert_eq!(f.manager.active_account(), None);
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
}
