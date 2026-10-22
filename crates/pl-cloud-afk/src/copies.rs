//! The words of the secret rows this launcher sealed, kept in the OS keychain so the player sees and
//! edits them without asking the machine (the contract's "Client copies"). A copy is shown only
//! while the account view's `fp` is the one recorded at sealing: another `fp` means the row was
//! sealed again elsewhere (the site, another computer, a key rotation).
//!
//! One keychain entry per cloud account (`cloud-afk-rows/<account id>`, the rows as JSON) through
//! pl-afk's store, the same service as the desktop AFK login commands. A small file beside the data
//! lists which accounts have an entry (ids only, no words) so the copies of an account that is gone,
//! or of another Discord account, can be deleted; a sign-out deletes them all.

use std::collections::BTreeMap;
use std::path::PathBuf;
use std::sync::Arc;

use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use zeroize::Zeroize;

use crate::error::{CloudError, Result};

/// One row's words and the fingerprint of the seal they were sealed into.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct RowCopy {
    pub text: String,
    pub fp: String,
}

impl std::fmt::Debug for RowCopy {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("RowCopy")
            .field("fp", &self.fp)
            .finish_non_exhaustive()
    }
}

impl Drop for RowCopy {
    fn drop(&mut self) {
        self.text.zeroize();
    }
}

/// The copies of one account, by row id.
pub type RowCopies = BTreeMap<String, RowCopy>;

#[derive(Serialize, Deserialize)]
struct Entry {
    v: u32,
    /// The Discord id the rows were sealed for.
    owner: String,
    rows: RowCopies,
}

#[derive(Debug, Default, Serialize, Deserialize)]
struct Index {
    owner: String,
    accounts: Vec<String>,
}

fn key_of(account_id: &str) -> String {
    format!("cloud-afk-rows/{account_id}")
}

/// The keychain copies. Calls may block (the keychain): async callers use `spawn_blocking`.
pub struct Copies {
    store: Arc<dyn pl_afk::SecretStore>,
    index_path: PathBuf,
    /// One change of the index at a time.
    lock: Mutex<()>,
}

impl std::fmt::Debug for Copies {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Copies")
            .field("index", &self.index_path)
            .finish_non_exhaustive()
    }
}

fn keychain(error: pl_afk::AfkError) -> CloudError {
    CloudError::Keychain(error.to_string())
}

impl Copies {
    /// `index_path`: the file that lists the accounts with an entry (e.g.
    /// `<data>/cloud-afk/copies.json`).
    pub fn new(store: Arc<dyn pl_afk::SecretStore>, index_path: impl Into<PathBuf>) -> Self {
        Self {
            store,
            index_path: index_path.into(),
            lock: Mutex::new(()),
        }
    }

    fn read_index(&self) -> Index {
        std::fs::read(&self.index_path)
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    fn write_index(&self, index: &Index) {
        let write = || -> std::io::Result<()> {
            if let Some(dir) = self.index_path.parent() {
                std::fs::create_dir_all(dir)?;
            }
            let tmp = self.index_path.with_extension("json.tmp");
            std::fs::write(&tmp, serde_json::to_vec(index).unwrap_or_default())?;
            std::fs::rename(&tmp, &self.index_path)
        };
        if let Err(error) = write() {
            tracing::warn!(%error, "could not write the cloud AFK copies list");
        }
    }

    /// The copies of `account_id` sealed for `owner` (none for another owner).
    pub fn load(&self, owner: &str, account_id: &str) -> Result<RowCopies> {
        let Some(mut value) = self.store.get(&key_of(account_id)).map_err(keychain)? else {
            return Ok(RowCopies::new());
        };
        let entry = serde_json::from_str::<Entry>(&value);
        value.zeroize();
        match entry {
            Ok(entry) if entry.v == 1 && entry.owner == owner => Ok(entry.rows),
            _ => Ok(RowCopies::new()),
        }
    }

    /// Replaces the copies of `account_id` (none: the entry goes).
    pub fn save(&self, owner: &str, account_id: &str, rows: &RowCopies) -> Result<()> {
        let _guard = self.lock.lock();
        if rows.is_empty() {
            self.store.delete(&key_of(account_id)).map_err(keychain)?;
            let mut index = self.read_index();
            index.accounts.retain(|id| id != account_id);
            self.write_index(&index);
            return Ok(());
        }
        let entry = Entry {
            v: 1,
            owner: owner.to_owned(),
            rows: rows.clone(),
        };
        let mut value =
            serde_json::to_string(&entry).map_err(|e| CloudError::Protocol(e.to_string()))?;
        let stored = self
            .store
            .set(&key_of(account_id), &value)
            .map_err(keychain);
        value.zeroize();
        stored?;
        let mut index = self.read_index();
        if index.owner != owner {
            // Another Discord account's copies were left: they go now.
            for id in std::mem::take(&mut index.accounts) {
                if id != account_id {
                    let _ = self.store.delete(&key_of(&id));
                }
            }
            index.owner = owner.to_owned();
        }
        if !index.accounts.iter().any(|id| id == account_id) {
            index.accounts.push(account_id.to_owned());
        }
        self.write_index(&index);
        Ok(())
    }

    /// Deletes the copies of the accounts `owner` no longer has, and every copy of another owner.
    pub fn prune(&self, owner: &str, accounts: &[String]) {
        let _guard = self.lock.lock();
        let mut index = self.read_index();
        if index.accounts.is_empty() && index.owner == owner {
            return;
        }
        let same_owner = index.owner == owner;
        let before = index.accounts.len();
        index.accounts.retain(|id| {
            let keep = same_owner && accounts.iter().any(|a| a == id);
            if !keep && let Err(error) = self.store.delete(&key_of(id)) {
                tracing::info!(%error, "a cloud AFK copy stays in the keychain for now");
                return true;
            }
            keep
        });
        if !same_owner {
            index.owner = owner.to_owned();
        }
        if index.accounts.len() != before || !same_owner {
            self.write_index(&index);
        }
    }

    /// Deletes every copy (a sign-out).
    pub fn forget_all(&self) {
        let _guard = self.lock.lock();
        let mut index = self.read_index();
        if index.accounts.is_empty() && index.owner.is_empty() {
            return;
        }
        index
            .accounts
            .retain(|id| self.store.delete(&key_of(id)).is_err());
        index.owner.clear();
        self.write_index(&index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn copies() -> (tempfile::TempDir, Arc<pl_afk::MemorySecretStore>, Copies) {
        let dir = tempfile::tempdir().unwrap();
        let store = Arc::new(pl_afk::MemorySecretStore::new());
        let copies = Copies::new(
            store.clone(),
            dir.path().join("cloud-afk").join("copies.json"),
        );
        (dir, store, copies)
    }

    fn row(text: &str, fp: &str) -> RowCopy {
        RowCopy {
            text: text.into(),
            fp: fp.into(),
        }
    }

    #[test]
    fn keeps_rows_per_account_and_owner() {
        let (_dir, store, copies) = copies();
        let mut rows = RowCopies::new();
        rows.insert("c1".into(), row("/login hunter2", "AAAAAAAAAAAAAAAA"));
        copies.save("111", "a_1", &rows).unwrap();
        assert_eq!(copies.load("111", "a_1").unwrap(), rows);
        // Another Discord account on this computer never reads them.
        assert!(copies.load("222", "a_1").unwrap().is_empty());
        assert!(copies.load("111", "a_2").unwrap().is_empty());
        assert_eq!(store.entries().len(), 1);
        copies.save("111", "a_1", &RowCopies::new()).unwrap();
        assert!(store.entries().is_empty());
    }

    #[test]
    fn prunes_gone_accounts_and_other_owners() {
        let (_dir, store, copies) = copies();
        let mut rows = RowCopies::new();
        rows.insert("c1".into(), row("/login a", "fp1"));
        copies.save("111", "a_1", &rows).unwrap();
        copies.save("111", "a_2", &rows).unwrap();
        copies.prune("111", &["a_2".to_owned()]);
        assert!(copies.load("111", "a_1").unwrap().is_empty());
        assert_eq!(copies.load("111", "a_2").unwrap().len(), 1);
        // Another player signed in here: every copy of the first one goes.
        copies.prune("222", &["a_2".to_owned()]);
        assert!(store.entries().is_empty());
    }

    #[test]
    fn a_sign_out_forgets_everything() {
        let (_dir, store, copies) = copies();
        let mut rows = RowCopies::new();
        rows.insert("r1".into(), row("/login b", "fp"));
        copies.save("111", "a_1", &rows).unwrap();
        copies.save("111", "a_9", &rows).unwrap();
        copies.forget_all();
        assert!(store.entries().is_empty());
        assert!(copies.load("111", "a_1").unwrap().is_empty());
    }

    #[test]
    fn a_new_owner_saving_drops_the_old_owners_copies() {
        let (_dir, store, copies) = copies();
        let mut rows = RowCopies::new();
        rows.insert("c1".into(), row("/login a", "fp"));
        copies.save("111", "a_1", &rows).unwrap();
        copies.save("222", "a_5", &rows).unwrap();
        assert_eq!(store.entries().len(), 1);
        assert_eq!(copies.load("222", "a_5").unwrap().len(), 1);
    }

    #[test]
    fn debug_never_shows_words() {
        let copy = row("/login hunter2", "fp");
        assert!(!format!("{copy:?}").contains("hunter2"));
    }
}
