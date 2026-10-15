//! Where login commands live: the OS keychain in production, memory in tests.
//!
//! One entry per account (`afk-login-commands/<account id>`) holding the command list as a JSON
//! array. Implementations must never log values.

use std::collections::HashMap;
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;

use crate::error::{AfkError, Result};

/// Keychain service name shared by the launcher (see `docs/architecture.md`).
pub const KEYRING_SERVICE: &str = "gg.projects.launcher";

/// Secret storage keyed by short ASCII names. Calls may block (D-Bus, Keychain prompts): callers
/// on async paths use `spawn_blocking`.
pub trait SecretStore: Send + Sync {
    fn get(&self, key: &str) -> Result<Option<String>>;
    fn set(&self, key: &str, value: &str) -> Result<()>;
    /// Deleting a missing entry succeeds.
    fn delete(&self, key: &str) -> Result<()>;
}

/// Keychain key of an account's login commands.
pub fn login_commands_key(account_id: &str) -> String {
    format!("afk-login-commands/{account_id}")
}

/// OS keychain via `keyring`: Windows Credential Manager, macOS Keychain, Linux Secret Service.
#[derive(Debug, Clone)]
pub struct KeyringSecretStore {
    service: String,
}

impl Default for KeyringSecretStore {
    fn default() -> Self {
        Self::new()
    }
}

impl KeyringSecretStore {
    pub fn new() -> Self {
        Self::with_service(KEYRING_SERVICE)
    }

    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            service: service.into(),
        }
    }

    /// False when the platform credential store could not be initialized (for example no Secret
    /// Service on a minimal Linux desktop).
    pub fn is_available() -> bool {
        keyring::Entry::store_status().is_ok()
    }

    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, key).map_err(secret_error)
    }
}

fn secret_error(error: keyring::Error) -> AfkError {
    // keyring errors describe the store failure, never the secret itself.
    AfkError::Secret(error.to_string())
}

impl SecretStore for KeyringSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(secret_error(e)),
        }
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.entry(key)?.set_password(value).map_err(secret_error)
    }

    fn delete(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(secret_error(e)),
        }
    }
}

/// In-memory store for tests. [`MemorySecretStore::set_failing`] simulates an unavailable keychain.
#[derive(Debug, Default)]
pub struct MemorySecretStore {
    entries: Mutex<HashMap<String, String>>,
    failing: AtomicBool,
}

impl MemorySecretStore {
    pub fn new() -> Self {
        Self::default()
    }

    /// While true every call fails like a locked or missing keychain.
    pub fn set_failing(&self, failing: bool) {
        self.failing.store(failing, Ordering::SeqCst);
    }

    /// Snapshot of all entries (tests only inspect what was stored).
    pub fn entries(&self) -> HashMap<String, String> {
        self.entries.lock().clone()
    }

    fn check(&self) -> Result<()> {
        if self.failing.load(Ordering::SeqCst) {
            Err(AfkError::Secret("keychain unavailable".into()))
        } else {
            Ok(())
        }
    }
}

impl SecretStore for MemorySecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        self.check()?;
        Ok(self.entries.lock().get(key).cloned())
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        self.check()?;
        self.entries.lock().insert(key.to_owned(), value.to_owned());
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        self.check()?;
        self.entries.lock().remove(key);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn memory_store_round_trip_and_failure_mode() {
        let store = MemorySecretStore::new();
        let key = login_commands_key("abc");
        assert_eq!(key, "afk-login-commands/abc");
        assert_eq!(store.get(&key).unwrap(), None);
        store.set(&key, "[\"/login pw\"]").unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some("[\"/login pw\"]"));
        store.set_failing(true);
        assert!(matches!(store.get(&key), Err(AfkError::Secret(_))));
        store.set_failing(false);
        store.delete(&key).unwrap();
        store.delete(&key).unwrap();
        assert!(store.entries().is_empty());
    }

    /// Touches the real OS keychain: `cargo test -p pl-afk -- --ignored keyring_round_trip`.
    #[test]
    #[ignore = "uses the real OS keychain"]
    fn keyring_round_trip() {
        let store = KeyringSecretStore::with_service("gg.projects.launcher.test");
        let key = login_commands_key("pl-afk-selftest");
        store.set(&key, "[\"/kit\"]").unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some("[\"/kit\"]"));
        store.delete(&key).unwrap();
        assert_eq!(store.get(&key).unwrap(), None);
    }
}
