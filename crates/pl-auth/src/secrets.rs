//! Where the secrets of Microsoft accounts live: the OS keychain ([`KeyringStore`]) or memory
//! ([`MemoryStore`], tests). Nothing secret is ever written to `accounts.json`.

use std::collections::HashMap;
use std::fmt;

use parking_lot::Mutex;
use uuid::Uuid;

use crate::config::KEYRING_SERVICE;
use crate::error::{AuthError, Result};
use crate::model::AccountSecrets;

/// Storage for [`AccountSecrets`], keyed by the local account id. Implementations are blocking;
/// [`crate::AuthManager`] calls them on the blocking thread pool.
pub trait SecretStore: Send + Sync {
    /// The account's secrets; `None` when there are none (or they are damaged beyond use).
    fn load(&self, account_id: Uuid) -> Result<Option<AccountSecrets>>;
    fn save(&self, account_id: Uuid, secrets: &AccountSecrets) -> Result<()>;
    /// Deleting secrets that do not exist is not an error.
    fn delete(&self, account_id: Uuid) -> Result<()>;
}

/// In-memory secrets, for tests and for running without a keychain.
#[derive(Default)]
pub struct MemoryStore {
    entries: Mutex<HashMap<Uuid, AccountSecrets>>,
}

impl MemoryStore {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn len(&self) -> usize {
        self.entries.lock().len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.lock().is_empty()
    }
}

impl fmt::Debug for MemoryStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("MemoryStore")
            .field("entries", &self.len())
            .finish()
    }
}

impl SecretStore for MemoryStore {
    fn load(&self, account_id: Uuid) -> Result<Option<AccountSecrets>> {
        Ok(self.entries.lock().get(&account_id).cloned())
    }

    fn save(&self, account_id: Uuid, secrets: &AccountSecrets) -> Result<()> {
        self.entries.lock().insert(account_id, secrets.clone());
        Ok(())
    }

    fn delete(&self, account_id: Uuid) -> Result<()> {
        self.entries.lock().remove(&account_id);
        Ok(())
    }
}

/// Failure of a raw keychain operation.
#[derive(Debug)]
pub(crate) struct BackendError {
    /// The keychain is missing or locked (no Secret Service, locked keychain).
    pub unavailable: bool,
    pub message: String,
}

impl BackendError {
    fn into_auth(self, op: &'static str) -> AuthError {
        AuthError::keychain(op, self.unavailable, self.message)
    }
}

impl From<keyring::Error> for BackendError {
    fn from(error: keyring::Error) -> Self {
        let unavailable = matches!(
            error,
            keyring::Error::NoDefaultStore | keyring::Error::NoStorageAccess(_)
        );
        Self {
            unavailable,
            message: error.to_string(),
        }
    }
}

/// Raw byte entries under one service name.
pub(crate) trait KeyringBackend: Send + Sync {
    fn read(&self, user: &str) -> Result<Option<Vec<u8>>, BackendError>;
    fn write(&self, user: &str, data: &[u8]) -> Result<(), BackendError>;
    /// Removing a missing entry succeeds.
    fn remove(&self, user: &str) -> Result<(), BackendError>;
}

/// The platform keychain through the `keyring` crate: Windows Credential Manager, macOS
/// Keychain, Linux Secret Service.
struct OsKeyring {
    service: String,
}

impl OsKeyring {
    fn entry(&self, user: &str) -> Result<keyring::Entry, BackendError> {
        keyring::Entry::new(&self.service, user).map_err(BackendError::from)
    }
}

impl KeyringBackend for OsKeyring {
    fn read(&self, user: &str) -> Result<Option<Vec<u8>>, BackendError> {
        match self.entry(user)?.get_secret() {
            Ok(bytes) => Ok(Some(bytes)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(error) => Err(error.into()),
        }
    }

    fn write(&self, user: &str, data: &[u8]) -> Result<(), BackendError> {
        self.entry(user)?
            .set_secret(data)
            .map_err(BackendError::from)
    }

    fn remove(&self, user: &str) -> Result<(), BackendError> {
        match self.entry(user)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(error) => Err(error.into()),
        }
    }
}

/// Bytes per keychain entry. Windows Credential Manager caps a blob at 2560 bytes and Microsoft
/// refresh tokens have no documented maximum, so payloads are split into chunks.
const CHUNK_BYTES: usize = 1800;
const MAX_CHUNKS: usize = 16;
const HEADER_TAG: &str = "pl-secrets/1";

/// Secrets in the OS keychain under the service `gg.projects.launcher`.
///
/// Layout: entry `account:<id>` holds `pl-secrets/1 <n>\n` followed by the first chunk of the
/// JSON payload; entries `account:<id>:1` … `account:<id>:<n-1>` hold the rest. Chunks are
/// written before the head, so a reader never sees a head that points at missing chunks
/// (a damaged set reads as "no secrets", which asks the player to sign in again).
pub struct KeyringStore {
    backend: Box<dyn KeyringBackend>,
}

impl KeyringStore {
    pub fn new() -> Self {
        Self::with_service(KEYRING_SERVICE)
    }

    pub fn with_service(service: impl Into<String>) -> Self {
        Self {
            backend: Box::new(OsKeyring {
                service: service.into(),
            }),
        }
    }

    #[cfg(test)]
    pub(crate) fn with_backend(backend: impl KeyringBackend + 'static) -> Self {
        Self {
            backend: Box::new(backend),
        }
    }

    /// Whether the platform keychain could be initialised. False e.g. on Linux without a
    /// running Secret Service; the app may then fall back to [`MemoryStore`] for the session.
    pub fn is_available() -> bool {
        keyring::Entry::store_status().is_ok()
    }

    fn chunk_count(&self, account_id: Uuid) -> Option<usize> {
        let head = self.backend.read(&head_user(account_id)).ok()??;
        parse_head(&head).map(|(count, _)| count)
    }
}

impl Default for KeyringStore {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for KeyringStore {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("KeyringStore").finish_non_exhaustive()
    }
}

fn head_user(account_id: Uuid) -> String {
    format!("account:{account_id}")
}

fn chunk_user(account_id: Uuid, index: usize) -> String {
    format!("account:{account_id}:{index}")
}

fn parse_head(raw: &[u8]) -> Option<(usize, &[u8])> {
    let newline = raw.iter().position(|byte| *byte == b'\n')?;
    let header = std::str::from_utf8(&raw[..newline]).ok()?;
    let count: usize = header
        .strip_prefix(HEADER_TAG)?
        .strip_prefix(' ')?
        .parse()
        .ok()?;
    (1..=MAX_CHUNKS)
        .contains(&count)
        .then(|| (count, &raw[newline + 1..]))
}

impl SecretStore for KeyringStore {
    fn load(&self, account_id: Uuid) -> Result<Option<AccountSecrets>> {
        let Some(head) = self
            .backend
            .read(&head_user(account_id))
            .map_err(|error| error.into_auth("load"))?
        else {
            return Ok(None);
        };
        let Some((count, first)) = parse_head(&head) else {
            tracing::warn!(%account_id, "stored session has an unknown format, ignoring it");
            return Ok(None);
        };
        let mut payload = first.to_vec();
        for index in 1..count {
            match self
                .backend
                .read(&chunk_user(account_id, index))
                .map_err(|error| error.into_auth("load"))?
            {
                Some(part) => payload.extend_from_slice(&part),
                None => {
                    tracing::warn!(%account_id, "stored session is incomplete, ignoring it");
                    return Ok(None);
                }
            }
        }
        match serde_json::from_slice(&payload) {
            Ok(secrets) => Ok(Some(secrets)),
            Err(_) => {
                tracing::warn!(%account_id, "stored session is damaged, ignoring it");
                Ok(None)
            }
        }
    }

    fn save(&self, account_id: Uuid, secrets: &AccountSecrets) -> Result<()> {
        let payload = serde_json::to_vec(secrets)
            .map_err(|error| AuthError::keychain("save", false, error.to_string()))?;
        let parts: Vec<&[u8]> = payload.chunks(CHUNK_BYTES).collect();
        if parts.is_empty() || parts.len() > MAX_CHUNKS {
            return Err(AuthError::keychain(
                "save",
                false,
                format!(
                    "session data has an unsupported size ({} bytes)",
                    payload.len()
                ),
            ));
        }
        let previous = self.chunk_count(account_id);
        for (index, part) in parts.iter().enumerate().skip(1) {
            self.backend
                .write(&chunk_user(account_id, index), part)
                .map_err(|error| error.into_auth("save"))?;
        }
        let mut head = format!("{HEADER_TAG} {}\n", parts.len()).into_bytes();
        head.extend_from_slice(parts[0]);
        self.backend
            .write(&head_user(account_id), &head)
            .map_err(|error| error.into_auth("save"))?;
        for index in parts.len()..previous.unwrap_or(0) {
            if let Err(error) = self.backend.remove(&chunk_user(account_id, index)) {
                tracing::debug!(%account_id, index, error = %error.message, "stale chunk not removed");
            }
        }
        Ok(())
    }

    fn delete(&self, account_id: Uuid) -> Result<()> {
        let count = self.chunk_count(account_id);
        // Head first: from then on the account reads as "no secrets" even if a chunk survives.
        self.backend
            .remove(&head_user(account_id))
            .map_err(|error| error.into_auth("delete"))?;
        match count {
            Some(count) => {
                for index in 1..count {
                    self.backend
                        .remove(&chunk_user(account_id, index))
                        .map_err(|error| error.into_auth("delete"))?;
                }
            }
            None => {
                // No readable head: sweep chunks left behind by an interrupted save.
                for index in 1..MAX_CHUNKS {
                    let user = chunk_user(account_id, index);
                    match self.backend.read(&user) {
                        Ok(Some(_)) => self
                            .backend
                            .remove(&user)
                            .map_err(|error| error.into_auth("delete"))?,
                        _ => break,
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use jiff::Timestamp;

    use super::*;

    /// Windows' `CRED_MAX_CREDENTIAL_BLOB_SIZE`.
    const WINDOWS_BLOB_LIMIT: usize = 2560;

    #[derive(Clone, Default)]
    struct FakeKeyring {
        entries: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        unavailable: bool,
    }

    impl FakeKeyring {
        fn users(&self) -> Vec<String> {
            let mut users: Vec<String> = self.entries.lock().keys().cloned().collect();
            users.sort();
            users
        }

        fn check(&self) -> Result<(), BackendError> {
            if self.unavailable {
                Err(BackendError {
                    unavailable: true,
                    message: "no secret service".into(),
                })
            } else {
                Ok(())
            }
        }
    }

    impl KeyringBackend for FakeKeyring {
        fn read(&self, user: &str) -> Result<Option<Vec<u8>>, BackendError> {
            self.check()?;
            Ok(self.entries.lock().get(user).cloned())
        }

        fn write(&self, user: &str, data: &[u8]) -> Result<(), BackendError> {
            self.check()?;
            if data.len() > WINDOWS_BLOB_LIMIT {
                return Err(BackendError {
                    unavailable: false,
                    message: "too long".into(),
                });
            }
            self.entries.lock().insert(user.to_owned(), data.to_vec());
            Ok(())
        }

        fn remove(&self, user: &str) -> Result<(), BackendError> {
            self.check()?;
            self.entries.lock().remove(user);
            Ok(())
        }
    }

    fn secrets(refresh_len: usize) -> AccountSecrets {
        AccountSecrets {
            refresh_token: Some("r".repeat(refresh_len)),
            access_token: "dummy-access-token".into(),
            expires_at: Timestamp::from_second(1_900_000_000).unwrap(),
            xuid: Some("2535400000000000".into()),
        }
    }

    #[test]
    fn memory_store_round_trip() {
        let store = MemoryStore::new();
        let id = Uuid::new_v4();
        assert!(store.load(id).unwrap().is_none());
        store.save(id, &secrets(10)).unwrap();
        assert_eq!(store.load(id).unwrap(), Some(secrets(10)));
        assert_eq!(store.len(), 1);
        store.delete(id).unwrap();
        store.delete(id).unwrap();
        assert!(store.is_empty());
        assert!(!format!("{store:?}").contains("dummy"));
    }

    #[test]
    fn small_payload_uses_one_entry() {
        let fake = FakeKeyring::default();
        let store = KeyringStore::with_backend(fake.clone());
        let id = Uuid::new_v4();
        store.save(id, &secrets(100)).unwrap();
        assert_eq!(fake.users(), vec![head_user(id)]);
        assert_eq!(store.load(id).unwrap(), Some(secrets(100)));
    }

    #[test]
    fn large_payload_is_chunked_under_the_windows_limit() {
        let fake = FakeKeyring::default();
        let store = KeyringStore::with_backend(fake.clone());
        let id = Uuid::new_v4();
        let other = Uuid::new_v4();
        store.save(other, &secrets(50)).unwrap();

        store.save(id, &secrets(6000)).unwrap();
        assert_eq!(store.load(id).unwrap(), Some(secrets(6000)));
        let own: Vec<String> = fake
            .users()
            .into_iter()
            .filter(|user| user.contains(&id.to_string()))
            .collect();
        assert_eq!(own.len(), 4);
        assert!(
            fake.entries
                .lock()
                .values()
                .all(|blob| blob.len() <= WINDOWS_BLOB_LIMIT)
        );

        // Shrinking removes the stale chunks.
        store.save(id, &secrets(10)).unwrap();
        assert_eq!(store.load(id).unwrap(), Some(secrets(10)));
        assert_eq!(fake.users().len(), 2);

        store.save(id, &secrets(4000)).unwrap();
        store.delete(id).unwrap();
        assert!(store.load(id).unwrap().is_none());
        assert_eq!(fake.users(), vec![head_user(other)]);
        assert_eq!(store.load(other).unwrap(), Some(secrets(50)));
    }

    #[test]
    fn damaged_entries_read_as_missing() {
        let fake = FakeKeyring::default();
        let store = KeyringStore::with_backend(fake.clone());
        let id = Uuid::new_v4();
        fake.entries
            .lock()
            .insert(head_user(id), b"garbage without header".to_vec());
        assert!(store.load(id).unwrap().is_none());

        store.save(id, &secrets(5000)).unwrap();
        fake.entries.lock().remove(&chunk_user(id, 2));
        assert!(store.load(id).unwrap().is_none());

        // Orphan chunks without a head are swept by delete.
        fake.entries.lock().remove(&head_user(id));
        store.delete(id).unwrap();
        assert!(fake.users().is_empty());
    }

    #[test]
    fn unavailable_keychain_maps_to_kc_erisim() {
        let store = KeyringStore::with_backend(FakeKeyring {
            unavailable: true,
            ..FakeKeyring::default()
        });
        let error = store.save(Uuid::new_v4(), &secrets(1)).unwrap_err();
        assert_eq!(error.code(), "KC-ERISIM");
        assert!(store.load(Uuid::new_v4()).is_err());
    }

    /// Writes a dummy (non-secret) value to the real OS keychain and deletes it again.
    /// Run with: `cargo test -p pl-auth -- --ignored real_keychain_round_trip`
    #[test]
    #[ignore = "touches the real OS keychain (writes and deletes a dummy entry)"]
    fn real_keychain_round_trip() {
        struct Cleanup(Uuid);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = KeyringStore::new().delete(self.0);
            }
        }

        assert!(KeyringStore::is_available());
        let store = KeyringStore::new();
        let id = Uuid::new_v4();
        let _cleanup = Cleanup(id);
        let dummy = secrets(3000); // chunked: exercises more than one credential
        store.save(id, &dummy).unwrap();
        assert_eq!(store.load(id).unwrap(), Some(dummy));
        store.delete(id).unwrap();
        assert!(store.load(id).unwrap().is_none());
    }
}
