//! Where the session token lives: the OS keychain, or memory when the computer has none.
//! The token is never written to a file or a log.

use std::sync::Mutex;

use crate::auth::SessionToken;

/// Keychain service of the launcher (shared with Microsoft and AFK secrets).
pub const KEYRING_SERVICE: &str = "gg.projects.launcher";
/// Keychain entry of the friends session.
const KEYRING_USER: &str = "social:session";

pub trait TokenStore: Send + Sync {
    /// The saved token, if any. Anything that does not look like a session token reads as none.
    fn load(&self) -> Option<SessionToken>;
    fn save(&self, token: &SessionToken) -> Result<(), String>;
    /// Forgets the token; forgetting nothing is fine.
    fn clear(&self);
}

/// The OS keychain (Windows Credential Manager, macOS Keychain, Linux Secret Service).
#[derive(Debug, Clone)]
pub struct KeyringTokenStore {
    service: String,
}

impl Default for KeyringTokenStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyringTokenStore {
    pub fn new() -> Self {
        Self {
            service: KEYRING_SERVICE.to_owned(),
        }
    }

    /// False when the platform keychain could not be initialised (e.g. no Secret Service).
    pub fn is_available() -> bool {
        keyring::Entry::store_status().is_ok()
    }

    fn entry(&self) -> Result<keyring::Entry, String> {
        // keyring errors describe the store, never the secret.
        keyring::Entry::new(&self.service, KEYRING_USER).map_err(|error| error.to_string())
    }
}

impl TokenStore for KeyringTokenStore {
    fn load(&self) -> Option<SessionToken> {
        let value = self.entry().ok()?.get_password().ok()?;
        let token = SessionToken::new(value);
        token.is_well_formed().then_some(token)
    }

    fn save(&self, token: &SessionToken) -> Result<(), String> {
        self.entry()?
            .set_password(token.expose())
            .map_err(|error| error.to_string())
    }

    fn clear(&self) {
        if let Ok(entry) = self.entry()
            && let Err(error) = entry.delete_credential()
            && !matches!(error, keyring::Error::NoEntry)
        {
            tracing::warn!(%error, "could not remove the friends session from the keychain");
        }
    }
}

/// Keeps the token for this run only (no keychain, and tests).
#[derive(Debug, Default)]
pub struct MemoryTokenStore {
    token: Mutex<Option<SessionToken>>,
}

impl MemoryTokenStore {
    pub fn new() -> Self {
        Self::default()
    }
}

impl TokenStore for MemoryTokenStore {
    fn load(&self) -> Option<SessionToken> {
        self.token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner())
            .clone()
    }

    fn save(&self, token: &SessionToken) -> Result<(), String> {
        *self
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = Some(token.clone());
        Ok(())
    }

    fn clear(&self) {
        *self
            .token
            .lock()
            .unwrap_or_else(|poisoned| poisoned.into_inner()) = None;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trip() {
        let store = MemoryTokenStore::new();
        assert!(store.load().is_none());
        let token = SessionToken::new(format!("pls_{}", "A".repeat(43)));
        store.save(&token).unwrap();
        assert_eq!(store.load(), Some(token));
        store.clear();
        assert!(store.load().is_none());
    }

    /// Touches the real OS keychain: `cargo test -p pl-social -- --ignored keyring_round_trip`.
    #[test]
    #[ignore = "uses the OS keychain"]
    fn keyring_round_trip() {
        let store = KeyringTokenStore {
            service: "gg.projects.launcher.test".into(),
        };
        let token = SessionToken::new(format!("pls_{}", "Z".repeat(43)));
        store.save(&token).unwrap();
        assert_eq!(store.load(), Some(token));
        store.clear();
        assert!(store.load().is_none());
    }
}
