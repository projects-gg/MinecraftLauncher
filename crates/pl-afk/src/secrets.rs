//! Where login commands live: the OS keychain in production, memory in tests.
//!
//! One entry per account (`afk-login-commands/<account id>`) holding the command list as a JSON
//! array. A Windows keychain entry holds at most 2560 bytes and `keyring` stores the value as
//! UTF-16, so a longer value goes into numbered parts (see [`KeyringSecretStore`]).
//! Implementations must never log values.

use std::collections::HashMap;
use std::sync::Arc;
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

/// Raw entries of one keychain service: the OS keychain, or a fake in tests.
trait Entries: Send + Sync {
    fn read(&self, key: &str) -> Result<Option<String>>;
    fn write(&self, key: &str, value: &str) -> Result<()>;
    /// Removing a missing entry succeeds.
    fn remove(&self, key: &str) -> Result<()>;
}

struct OsEntries {
    service: String,
}

impl OsEntries {
    fn entry(&self, key: &str) -> Result<keyring::Entry> {
        keyring::Entry::new(&self.service, key).map_err(secret_error)
    }
}

impl Entries for OsEntries {
    fn read(&self, key: &str) -> Result<Option<String>> {
        match self.entry(key)?.get_password() {
            Ok(value) => Ok(Some(value)),
            Err(keyring::Error::NoEntry) => Ok(None),
            Err(e) => Err(secret_error(e)),
        }
    }

    fn write(&self, key: &str, value: &str) -> Result<()> {
        self.entry(key)?.set_password(value).map_err(secret_error)
    }

    fn remove(&self, key: &str) -> Result<()> {
        match self.entry(key)?.delete_credential() {
            Ok(()) | Err(keyring::Error::NoEntry) => Ok(()),
            Err(e) => Err(secret_error(e)),
        }
    }
}

/// UTF-16 units in one entry's value: Windows keeps at most 2560 bytes per entry and `keyring`
/// stores a password there as UTF-16 (1280 units); this leaves room for the parts header.
const PART_UNITS: usize = 1000;
/// Most entries one value may take (32 000 characters of login commands).
const MAX_PARTS: usize = 32;
/// First line of an entry whose value goes on in parts: `#pl-parts/1 <generation> <count>`. A
/// command list (a JSON array) never starts with `#`.
const PARTS_TAG: &str = "#pl-parts/1";

/// OS keychain via `keyring`: Windows Credential Manager, macOS Keychain, Linux Secret Service.
///
/// A value of up to [`PART_UNITS`] UTF-16 units is one entry, as it is. A longer one is split: the
/// entry itself holds the parts header and the first part, entries `<key>#<generation>.<n>` the
/// others. A new value's parts get a new generation and are written before the entry that
/// points at them, so a write that fails halfway leaves the old value whole.
#[derive(Clone)]
pub struct KeyringSecretStore {
    service: String,
    entries: Arc<dyn Entries>,
}

impl std::fmt::Debug for KeyringSecretStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("KeyringSecretStore")
            .field("service", &self.service)
            .finish_non_exhaustive()
    }
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
        let service = service.into();
        Self {
            entries: Arc::new(OsEntries {
                service: service.clone(),
            }),
            service,
        }
    }

    #[cfg(test)]
    fn with_entries(entries: Arc<dyn Entries>) -> Self {
        Self {
            service: "test".to_owned(),
            entries,
        }
    }

    /// False when the platform credential store could not be initialized (for example no Secret
    /// Service on a minimal Linux desktop).
    pub fn is_available() -> bool {
        keyring::Entry::store_status().is_ok()
    }

    /// Best effort: parts `1..count` of `generation` (a failed removal only leaves a fragment in
    /// the keychain, under a name nothing reads).
    fn remove_parts(&self, key: &str, generation: u32, count: usize) {
        for index in 1..count {
            if let Err(error) = self.entries.remove(&part_key(key, generation, index)) {
                tracing::debug!(%error, "stale keychain part not removed");
            }
        }
    }
}

fn secret_error(error: keyring::Error) -> AfkError {
    // keyring errors describe the store failure, never the secret itself.
    AfkError::Secret(error.to_string())
}

fn part_key(key: &str, generation: u32, index: usize) -> String {
    format!("{key}#{generation}.{index}")
}

/// `(generation, count, first part)` of an entry that holds a parts header.
fn parse_parts_head(value: &str) -> Option<(u32, usize, &str)> {
    let rest = value.strip_prefix(PARTS_TAG)?.strip_prefix(' ')?;
    let (header, first) = rest.split_once('\n')?;
    let (generation, count) = header.split_once(' ')?;
    let generation = generation.parse().ok()?;
    let count = count.parse().ok()?;
    (1..=MAX_PARTS)
        .contains(&count)
        .then_some((generation, count, first))
}

/// `value` cut on character boundaries into pieces of at most `units` UTF-16 units (at least one
/// piece, maybe empty).
fn split_units(value: &str, units: usize) -> Vec<&str> {
    let mut pieces = Vec::new();
    let (mut start, mut used) = (0, 0);
    for (at, c) in value.char_indices() {
        if used + c.len_utf16() > units {
            pieces.push(&value[start..at]);
            (start, used) = (at, 0);
        }
        used += c.len_utf16();
    }
    pieces.push(&value[start..]);
    pieces
}

impl SecretStore for KeyringSecretStore {
    fn get(&self, key: &str) -> Result<Option<String>> {
        let Some(head) = self.entries.read(key)? else {
            return Ok(None);
        };
        if !head.starts_with(PARTS_TAG) {
            return Ok(Some(head));
        }
        let Some((generation, count, first)) = parse_parts_head(&head) else {
            tracing::warn!(key, "keychain entry has an unknown format, ignoring it");
            return Ok(None);
        };
        let mut value = first.to_owned();
        for index in 1..count {
            match self.entries.read(&part_key(key, generation, index))? {
                Some(part) => value.push_str(&part),
                None => {
                    tracing::warn!(key, "keychain entry is incomplete, ignoring it");
                    return Ok(None);
                }
            }
        }
        Ok(Some(value))
    }

    fn set(&self, key: &str, value: &str) -> Result<()> {
        // An unreadable old entry is simply replaced (its parts, if any, stay unused).
        let old = self.entries.read(key).ok().flatten().and_then(|head| {
            parse_parts_head(&head).map(|(generation, count, _)| (generation, count))
        });
        let pieces = split_units(value, PART_UNITS);
        if pieces.len() == 1 && !value.starts_with(PARTS_TAG) {
            self.entries.write(key, value)?;
        } else {
            if pieces.len() > MAX_PARTS {
                return Err(AfkError::Secret(format!(
                    "value too long for the keychain ({} UTF-16 units)",
                    value.encode_utf16().count()
                )));
            }
            let generation = old.map_or(1, |(generation, _)| generation.wrapping_add(1));
            for (index, piece) in pieces.iter().enumerate().skip(1) {
                if let Err(error) = self.entries.write(&part_key(key, generation, index), piece) {
                    self.remove_parts(key, generation, index);
                    return Err(error);
                }
            }
            let head = format!("{PARTS_TAG} {generation} {}\n{}", pieces.len(), pieces[0]);
            if let Err(error) = self.entries.write(key, &head) {
                self.remove_parts(key, generation, pieces.len());
                return Err(error);
            }
        }
        if let Some((generation, count)) = old {
            self.remove_parts(key, generation, count);
        }
        Ok(())
    }

    fn delete(&self, key: &str) -> Result<()> {
        let old = self.entries.read(key).ok().flatten();
        // The entry first: from then on nothing reads the parts.
        self.entries.remove(key)?;
        if let Some((generation, count, _)) = old.as_deref().and_then(parse_parts_head) {
            self.remove_parts(key, generation, count);
        }
        Ok(())
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

    /// Entries in memory with Windows' limit (2560 bytes of UTF-16 per entry); writes can be made
    /// to fail after a number of them.
    #[derive(Default)]
    struct FakeEntries {
        map: Mutex<HashMap<String, String>>,
        writes_left: Mutex<Option<usize>>,
    }

    impl Entries for FakeEntries {
        fn read(&self, key: &str) -> Result<Option<String>> {
            Ok(self.map.lock().get(key).cloned())
        }

        fn write(&self, key: &str, value: &str) -> Result<()> {
            if value.encode_utf16().count() * 2 > 2560 {
                return Err(AfkError::Secret("longer than the platform limit".into()));
            }
            if let Some(left) = self.writes_left.lock().as_mut() {
                if *left == 0 {
                    return Err(AfkError::Secret("keychain unavailable".into()));
                }
                *left -= 1;
            }
            self.map.lock().insert(key.to_owned(), value.to_owned());
            Ok(())
        }

        fn remove(&self, key: &str) -> Result<()> {
            self.map.lock().remove(key);
            Ok(())
        }
    }

    /// A JSON command list of `lines` long lines with Turkish letters.
    fn long_list(lines: usize) -> String {
        let commands: Vec<String> = (0..lines)
            .map(|i| format!("/msg yardımcı {i} {}", "ğ".repeat(40)))
            .collect();
        serde_json::to_string(&commands).unwrap()
    }

    #[test]
    fn long_values_are_split_under_the_windows_limit() {
        let fake = Arc::new(FakeEntries::default());
        let store = KeyringSecretStore::with_entries(fake.clone());
        let key = login_commands_key("abc");

        // A short value stays one entry, exactly as before.
        store.set(&key, "[\"/login pw\"]").unwrap();
        assert_eq!(
            fake.map.lock().clone(),
            HashMap::from([(key.clone(), "[\"/login pw\"]".to_owned())])
        );

        let long = long_list(40);
        assert!(long.encode_utf16().count() > 2 * PART_UNITS);
        store.set(&key, &long).unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some(long.as_str()));
        assert_eq!(fake.map.lock().len(), split_units(&long, PART_UNITS).len());

        // Shrinking, then a short value again: no stale part stays behind.
        let shorter = long_list(20);
        store.set(&key, &shorter).unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some(shorter.as_str()));
        assert_eq!(
            fake.map.lock().len(),
            split_units(&shorter, PART_UNITS).len()
        );
        store.set(&key, "[\"/kit\"]").unwrap();
        assert_eq!(fake.map.lock().len(), 1);
        assert_eq!(store.get(&key).unwrap().as_deref(), Some("[\"/kit\"]"));

        store.set(&key, &long).unwrap();
        store.delete(&key).unwrap();
        assert!(fake.map.lock().is_empty());
        assert_eq!(store.get(&key).unwrap(), None);

        // A lost part reads as no value, never as a cut list.
        store.set(&key, &long).unwrap();
        let part = fake
            .map
            .lock()
            .keys()
            .find(|k| k.contains('#'))
            .cloned()
            .unwrap();
        fake.map.lock().remove(&part);
        assert_eq!(store.get(&key).unwrap(), None);
    }

    #[test]
    fn a_write_that_fails_halfway_keeps_the_old_value() {
        let fake = Arc::new(FakeEntries::default());
        let store = KeyringSecretStore::with_entries(fake.clone());
        let key = login_commands_key("abc");
        let old = long_list(40);
        store.set(&key, &old).unwrap();
        let before = fake.map.lock().clone();

        let new = long_list(45);
        for writes in 0..split_units(&new, PART_UNITS).len() {
            *fake.writes_left.lock() = Some(writes);
            assert!(store.set(&key, &new).is_err());
            *fake.writes_left.lock() = None;
            assert_eq!(
                store.get(&key).unwrap().as_deref(),
                Some(old.as_str()),
                "{writes}"
            );
            assert_eq!(fake.map.lock().clone(), before, "nothing left behind");
        }
        *fake.writes_left.lock() = Some(0);
        assert!(store.set(&key, "[\"/kit\"]").is_err());
        *fake.writes_left.lock() = None;
        assert_eq!(store.get(&key).unwrap().as_deref(), Some(old.as_str()));

        store.set(&key, &new).unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some(new.as_str()));
        assert!(
            store
                .set(&key, &"x".repeat(MAX_PARTS * PART_UNITS + 1))
                .is_err(),
            "a value has a size limit"
        );
        assert_eq!(store.get(&key).unwrap().as_deref(), Some(new.as_str()));
    }

    #[test]
    fn values_are_cut_on_character_boundaries() {
        assert_eq!(split_units("", 3), vec![""]);
        assert_eq!(split_units("abcdefg", 3), vec!["abc", "def", "g"]);
        // A character outside the BMP takes two units and is never split.
        assert_eq!(split_units("ab😀cd", 3), vec!["ab", "😀c", "d"]);
        assert_eq!(split_units("şğü", 3), vec!["şğü"]);
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

    /// A long command list on the real OS keychain (Windows keeps at most 2560 bytes, as UTF-16,
    /// per entry), with dummy values:
    /// `cargo test -p pl-afk -- --ignored long_values_round_trip_on_the_real_keychain`.
    #[test]
    #[ignore = "uses the real OS keychain"]
    fn long_values_round_trip_on_the_real_keychain() {
        struct Cleanup(KeyringSecretStore, String);
        impl Drop for Cleanup {
            fn drop(&mut self) {
                let _ = self.0.delete(&self.1);
            }
        }
        let store = KeyringSecretStore::with_service("gg.projects.launcher.test");
        let key = login_commands_key("pl-afk-long-selftest");
        let _cleanup = Cleanup(store.clone(), key.clone());
        let commands: Vec<String> = (0..40)
            .map(|i| format!("/msg yardımcı {i} {}", "ğ".repeat(40)))
            .collect();
        let value = serde_json::to_string(&commands).unwrap();
        assert!(value.encode_utf16().count() > 1280);
        store.set(&key, &value).unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some(value.as_str()));
        store.set(&key, "[\"/kit\"]").unwrap();
        assert_eq!(store.get(&key).unwrap().as_deref(), Some("[\"/kit\"]"));
        store.delete(&key).unwrap();
        assert_eq!(store.get(&key).unwrap(), None);
    }
}
