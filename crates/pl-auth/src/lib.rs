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
//! - [`HeadSource`] / [`HeadCache`]: player heads for the account list, cut from the profile's
//!   Mojang texture or from the default skin in a local client jar.
//! - [`SkinHistory`]: the skins each Microsoft profile wore, kept on this computer so the player
//!   can go back to an old one ([`AuthManager::wear_history_skin`]).

mod avatar;
mod config;
pub mod e2e;
mod error;
mod history;
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

pub use avatar::{
    DEFAULT_SKIN_NAMES, DefaultSkin, HeadCache, HeadError, HeadSource, client_jars, front_png,
    head_png, read_default_skin, texture_id,
};
pub use config::{
    AuthConfig, BROWSER_LOGIN_TIMEOUT, CLIENT_ID, Endpoints, KEYRING_SERVICE, REFRESH_MARGIN, SCOPE,
};
pub use error::{
    AuthError, AuthErrorInfo, AuthStep, FailureClass, OfflineNameProblem, Result, SkinFileProblem,
};
pub use history::{
    FORGOTTEN_PROFILE_KEPT, HISTORY_UNDO_WINDOW, HistoryEntry, HistoryTidy, MAX_KEPT, SkinHistory,
    SkinSource,
};
#[cfg(windows)]
pub use legacy::{LegacyImport, legacy_premium_path};
pub use manager::{
    Appearance, AuthManager, PROFILE_SYNC_EVERY, SignIn, SkinHistoryList, UNDO_WINDOW,
};
pub use microsoft::{
    DeviceCodeLogin, LoginProgressFn, MicrosoftAuth, MicrosoftLogin, OpenUrlFn, RefreshedSession,
};
pub use model::{
    Account, AccountKind, AccountSecrets, Cape, DeviceCodeInfo, LaunchIdentityData, LoginStep,
    MinecraftProfile, Ownership, ProfileSkin, SessionCheck, Skin, SkinVariant, UserType,
    launch_identity,
};
pub use offline::{
    OFFLINE_ACCESS_TOKEN, OFFLINE_NAME_MAX, OFFLINE_NAME_MIN, offline_uuid, validate_offline_name,
};
pub use secrets::{KeyringStore, MemoryStore, SecretStore};
pub use skins::{
    MAX_SKIN_BYTES, TEXTURE_HOST, is_mojang_texture_url, read_skin_file, validate_skin_png,
};
pub use store::{AccountStore, RemovedAccount};
