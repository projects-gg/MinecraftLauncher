//! Accounts: Microsoft (Xbox Live / Minecraft Services) and offline, with secure token storage.
//!
//! - [`AuthManager`]: what the app uses. Account list ([`AccountStore`], `accounts.json`, no
//!   secrets), secrets ([`SecretStore`]: [`KeyringStore`] for the OS keychain, [`MemoryStore`]
//!   for tests), browser and device-code sign-in, single-flight token refresh
//!   ([`AuthManager::ensure_fresh`]), launch identity, skins and capes, and on Windows the import
//!   of the legacy launcher's premium session.
//! - [`MicrosoftAuth`]: stateless client for Microsoft OAuth (PKCE + loopback redirect, or device
//!   code) → Xbox Live → XSTS → Minecraft Services, plus the skin and cape endpoints.
//! - [`offline_uuid`] / [`validate_offline_name`]: offline accounts compatible with offline-mode
//!   servers.
//! - [`AuthError`]: short codes for support, friendly Turkish messages
//!   ([`AuthError::user_message_tr`]) and copyable diagnostics ([`AuthError::details`]); tokens
//!   never appear in either.

mod config;
mod error;
mod http;
#[cfg(windows)]
mod legacy;
mod loopback;
mod manager;
mod microsoft;
mod model;
mod offline;
mod page;
mod pkce;
mod secrets;
mod skins;
mod store;
#[cfg(test)]
mod testutil;

pub use config::{
    AuthConfig, BROWSER_LOGIN_TIMEOUT, CLIENT_ID, Endpoints, KEYRING_SERVICE, REFRESH_MARGIN, SCOPE,
};
pub use error::{
    AuthError, AuthErrorInfo, AuthStep, FailureClass, OfflineNameProblem, Result, SkinFileProblem,
};
#[cfg(windows)]
pub use legacy::{LegacyImport, legacy_premium_path};
pub use manager::AuthManager;
pub use microsoft::{
    DeviceCodeLogin, LoginProgressFn, MicrosoftAuth, MicrosoftLogin, OpenUrlFn, RefreshedSession,
};
pub use model::{
    Account, AccountKind, AccountSecrets, Cape, DeviceCodeInfo, LaunchIdentityData, LoginStep,
    MinecraftProfile, Ownership, ProfileSkin, Skin, SkinVariant, UserType, launch_identity,
};
pub use offline::{
    OFFLINE_ACCESS_TOKEN, OFFLINE_NAME_MAX, OFFLINE_NAME_MIN, offline_uuid, validate_offline_name,
};
pub use secrets::{KeyringStore, MemoryStore, SecretStore};
pub use skins::{MAX_SKIN_BYTES, validate_skin_png};
pub use store::AccountStore;
