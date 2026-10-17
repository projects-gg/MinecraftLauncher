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
/// The first layout: one set of chunk names, overwritten in place. Still read.
const HEADER_TAG_V1: &str = "pl-secrets/1";
/// Two sets of chunk names used in turn.
const HEADER_TAG: &str = "pl-secrets/2";
/// How often a read starts over when a save switched chunk sets under it.
const READ_ATTEMPTS: usize = 3;

/// Secrets in the OS keychain under the service `gg.projects.launcher`.
///
/// Layout: entry `account:<id>` (the head) holds `pl-secrets/2 <set> <n>\n` followed by the first
/// chunk of the JSON payload; entries `account:<id>:<set>:1` … `account:<id>:<set>:<n-1>` hold
/// the rest, `<set>` being `a` or `b`. A save writes its chunks under the set the head does not
/// point at, then rewrites the head (one entry: the switch is all or nothing), then removes the
/// old set's chunks. So a save that fails part way leaves the old session whole and readable (the
/// new chunks it wrote are taken back), never a mix of old and new parts that reads as damaged
/// and asks the player to sign in again. A read that finds a chunk gone because a save switched
/// sets meanwhile starts over. Sessions of the first layout (`pl-secrets/1 <n>`, chunks
/// `account:<id>:<i>`) are still read and move to this one on their next save. Saves of one
/// account must not run at the same time ([`crate::AuthManager`] serializes them).
pub struct KeyringStore {
    backend: Box<dyn KeyringBackend>,
}

/// Where the parts of a payload after the first one live.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ChunkSet {
    /// `account:<id>:<i>`: the first layout.
    Legacy,
    A,
    B,
}

impl ChunkSet {
    fn letter(self) -> Option<char> {
        match self {
            Self::Legacy => None,
            Self::A => Some('a'),
            Self::B => Some('b'),
        }
    }

    /// The set a save uses when the head points at this one.
    fn next(self) -> Self {
        match self {
            Self::A => Self::B,
            Self::Legacy | Self::B => Self::A,
        }
    }
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

    /// The set and part count the head points at, when there is a readable head.
    fn current(&self, account_id: Uuid) -> Option<(ChunkSet, usize)> {
        let head = self.backend.read(&head_user(account_id)).ok()??;
        parse_head(&head).map(|(set, count, _)| (set, count))
    }

    /// The payload a head points at; `None` when one of its chunks is missing.
    fn payload(
        &self,
        account_id: Uuid,
        set: ChunkSet,
        count: usize,
        first: &[u8],
    ) -> Result<Option<Vec<u8>>> {
        let mut payload = first.to_vec();
        for index in 1..count {
            match self
                .backend
                .read(&chunk_user(account_id, set, index))
                .map_err(|error| error.into_auth("load"))?
            {
                Some(part) => payload.extend_from_slice(&part),
                None => return Ok(None),
            }
        }
        Ok(Some(payload))
    }

    /// Removes chunks `indices` of a set, as far as the keychain lets it (they are no longer
    /// read; leftovers go with the account).
    fn remove_chunks(&self, account_id: Uuid, set: ChunkSet, indices: std::ops::Range<usize>) {
        for index in indices {
            if let Err(error) = self.backend.remove(&chunk_user(account_id, set, index)) {
                tracing::debug!(%account_id, index, error = %error.message, "stale chunk not removed");
            }
        }
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

fn chunk_user(account_id: Uuid, set: ChunkSet, index: usize) -> String {
    match set.letter() {
        None => format!("account:{account_id}:{index}"),
        Some(letter) => format!("account:{account_id}:{letter}:{index}"),
    }
}

/// A head: the set its chunks are in, the number of parts in all, and the first part.
fn parse_head(raw: &[u8]) -> Option<(ChunkSet, usize, &[u8])> {
    let newline = raw.iter().position(|byte| *byte == b'\n')?;
    let header = std::str::from_utf8(&raw[..newline]).ok()?;
    let (set, count) = match header
        .strip_prefix(HEADER_TAG)
        .and_then(|rest| rest.strip_prefix(' '))
    {
        Some(rest) => {
            let (letter, count) = rest.split_once(' ')?;
            let set = match letter {
                "a" => ChunkSet::A,
                "b" => ChunkSet::B,
                _ => return None,
            };
            (set, count)
        }
        None => (
            ChunkSet::Legacy,
            header.strip_prefix(HEADER_TAG_V1)?.strip_prefix(' ')?,
        ),
    };
    let count: usize = count.parse().ok()?;
    (1..=MAX_CHUNKS)
        .contains(&count)
        .then(|| (set, count, &raw[newline + 1..]))
}

impl SecretStore for KeyringStore {
    fn load(&self, account_id: Uuid) -> Result<Option<AccountSecrets>> {
        let read_head = || {
            self.backend
                .read(&head_user(account_id))
                .map_err(|error| error.into_auth("load"))
        };
        let mut head = read_head()?;
        for _ in 0..READ_ATTEMPTS {
            let Some(raw) = head else {
                return Ok(None);
            };
            let Some((set, count, first)) = parse_head(&raw) else {
                tracing::warn!(%account_id, "stored session has an unknown format, ignoring it");
                return Ok(None);
            };
            if let Some(payload) = self.payload(account_id, set, count, first)? {
                return match serde_json::from_slice(&payload) {
                    Ok(secrets) => Ok(Some(secrets)),
                    Err(_) => {
                        tracing::warn!(%account_id, "stored session is damaged, ignoring it");
                        Ok(None)
                    }
                };
            }
            // A chunk is gone: a save may have switched sets meanwhile. Start over when the head
            // changed; under the same head the session really is incomplete.
            let again = read_head()?;
            if again.as_deref() == Some(raw.as_slice()) {
                tracing::warn!(%account_id, "stored session is incomplete, ignoring it");
                return Ok(None);
            }
            head = again;
        }
        tracing::warn!(%account_id, "stored session kept changing while read, ignoring it");
        Ok(None)
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
        let previous = self.current(account_id);
        // The new parts go where the head does not point: until the head switches, the old
        // session stays whole.
        let set = previous.map_or(ChunkSet::A, |(set, _)| set.next());
        for (index, part) in parts.iter().enumerate().skip(1) {
            if let Err(error) = self
                .backend
                .write(&chunk_user(account_id, set, index), part)
            {
                self.remove_chunks(account_id, set, 1..index);
                return Err(error.into_auth("save"));
            }
        }
        let letter = set.letter().unwrap_or('a');
        let mut head = format!("{HEADER_TAG} {letter} {}\n", parts.len()).into_bytes();
        head.extend_from_slice(parts[0]);
        if let Err(error) = self.backend.write(&head_user(account_id), &head) {
            self.remove_chunks(account_id, set, 1..parts.len());
            return Err(error.into_auth("save"));
        }
        // Switched: the old parts are no longer read.
        if let Some((old, count)) = previous {
            self.remove_chunks(account_id, old, 1..count);
        }
        Ok(())
    }

    fn delete(&self, account_id: Uuid) -> Result<()> {
        let current = self.current(account_id);
        // Head first: from then on the account reads as "no secrets" even if a chunk survives.
        self.backend
            .remove(&head_user(account_id))
            .map_err(|error| error.into_auth("delete"))?;
        if let Some((set, count)) = current {
            for index in 1..count {
                self.backend
                    .remove(&chunk_user(account_id, set, index))
                    .map_err(|error| error.into_auth("delete"))?;
            }
        }
        // Chunks the head did not point at: left by an interrupted save or an earlier layout.
        for set in [ChunkSet::Legacy, ChunkSet::A, ChunkSet::B] {
            for index in 1..MAX_CHUNKS {
                let user = chunk_user(account_id, set, index);
                match self.backend.read(&user) {
                    Ok(Some(_)) => self
                        .backend
                        .remove(&user)
                        .map_err(|error| error.into_auth("delete"))?,
                    _ => break,
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

    type ReadHook = Box<dyn FnOnce() + Send>;

    #[derive(Clone, Default)]
    struct FakeKeyring {
        entries: Arc<Mutex<HashMap<String, Vec<u8>>>>,
        unavailable: bool,
        /// Writes still allowed before every further one fails (`None`: no limit).
        writes_left: Arc<Mutex<Option<usize>>>,
        /// Runs once, at the first read of a chunk entry (an entry other than a head).
        on_chunk_read: Arc<Mutex<Option<ReadHook>>>,
    }

    impl FakeKeyring {
        fn users(&self) -> Vec<String> {
            let mut users: Vec<String> = self.entries.lock().keys().cloned().collect();
            users.sort();
            users
        }

        fn fail_writes_after(&self, writes: usize) {
            *self.writes_left.lock() = Some(writes);
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
            if user.matches(':').count() > 1 {
                let hook = self.on_chunk_read.lock().take();
                if let Some(hook) = hook {
                    hook();
                }
            }
            Ok(self.entries.lock().get(user).cloned())
        }

        fn write(&self, user: &str, data: &[u8]) -> Result<(), BackendError> {
            self.check()?;
            if let Some(left) = self.writes_left.lock().as_mut() {
                if *left == 0 {
                    return Err(BackendError {
                        unavailable: false,
                        message: "write refused".into(),
                    });
                }
                *left -= 1;
            }
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
        fake.entries.lock().remove(&chunk_user(id, ChunkSet::A, 2));
        assert!(store.load(id).unwrap().is_none());

        // Orphan chunks without a head are swept by delete.
        fake.entries.lock().remove(&head_user(id));
        store.delete(id).unwrap();
        assert!(fake.users().is_empty());
    }

    /// A save that fails part way (the keychain refuses a write, the launcher is closed) leaves
    /// the old, working session readable instead of a mix of old and new parts that reads as
    /// damaged and asks the player to sign in again.
    #[test]
    fn a_save_that_fails_halfway_keeps_the_old_session() {
        let fake = FakeKeyring::default();
        let store = KeyringStore::with_backend(fake.clone());
        let id = Uuid::new_v4();
        store.save(id, &secrets(6000)).unwrap();
        let before = fake.users();

        // Every write but the first fails.
        fake.fail_writes_after(1);
        assert!(store.save(id, &secrets(5000)).is_err());
        assert_eq!(store.load(id).unwrap(), Some(secrets(6000)));
        assert_eq!(fake.users(), before, "the parts written are taken back");

        // The same when only the last step, the head, fails.
        let fresh = Uuid::new_v4();
        *fake.writes_left.lock() = None;
        store.save(fresh, &secrets(6000)).unwrap();
        fake.fail_writes_after(3);
        assert!(store.save(fresh, &secrets(6001)).is_err());
        assert_eq!(store.load(fresh).unwrap(), Some(secrets(6000)));

        // Once the keychain works again the next save goes through.
        *fake.writes_left.lock() = None;
        store.save(id, &secrets(5000)).unwrap();
        assert_eq!(store.load(id).unwrap(), Some(secrets(5000)));
    }

    /// A read that overlaps a save of the same session (the hub checks a session while a refresh
    /// writes it) still gets one whole session, never a mix.
    #[test]
    fn a_read_racing_a_save_still_gets_a_whole_session() {
        // Different letters, so a mix of the two shows.
        let session = |letter: &str, len: usize| AccountSecrets {
            refresh_token: Some(letter.repeat(len)),
            ..secrets(0)
        };
        let fake = FakeKeyring::default();
        let store = KeyringStore::with_backend(fake.clone());
        let id = Uuid::new_v4();
        store.save(id, &session("x", 6000)).unwrap();
        let writer = KeyringStore::with_backend(fake.clone());
        *fake.on_chunk_read.lock() = Some(Box::new(move || {
            writer.save(id, &session("y", 7000)).unwrap();
        }));
        let read = store.load(id).unwrap();
        assert!(
            read == Some(session("x", 6000)) || read == Some(session("y", 7000)),
            "a whole session, old or new: {:?}",
            read.map(|s| s.refresh_token.map(|t| t.len()))
        );
        assert_eq!(store.load(id).unwrap(), Some(session("y", 7000)));
    }

    /// Sessions written by the first layout (`pl-secrets/1`, chunks overwritten in place) still
    /// load, and the next save moves them over without leaving the old chunks behind.
    #[test]
    fn sessions_of_the_first_layout_still_load_and_move_over() {
        let fake = FakeKeyring::default();
        let store = KeyringStore::with_backend(fake.clone());
        let id = Uuid::new_v4();
        let payload = serde_json::to_vec(&secrets(3000)).unwrap();
        let (first, rest) = payload.split_at(CHUNK_BYTES);
        let mut head = b"pl-secrets/1 2\n".to_vec();
        head.extend_from_slice(first);
        fake.entries.lock().insert(format!("account:{id}"), head);
        fake.entries
            .lock()
            .insert(format!("account:{id}:1"), rest.to_vec());
        assert_eq!(store.load(id).unwrap(), Some(secrets(3000)));

        store.save(id, &secrets(3500)).unwrap();
        assert_eq!(store.load(id).unwrap(), Some(secrets(3500)));
        assert!(!fake.users().contains(&format!("account:{id}:1")));
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
