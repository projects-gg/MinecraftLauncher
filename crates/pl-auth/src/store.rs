//! `accounts.json`: saved accounts (no secrets) and the active account. Every change is written
//! atomically (`accounts.json.tmp`, then rename) before it becomes visible in memory.

use std::fs;
use std::io::{self, Write as _};
use std::path::{Path, PathBuf};

use jiff::Timestamp;
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AuthError, Result};
use crate::model::{Account, AccountKind};

const FILE_VERSION: u32 = 1;

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AccountsFile {
    #[serde(default)]
    version: u32,
    #[serde(default)]
    active_account_id: Option<Uuid>,
    #[serde(default)]
    accounts: Vec<Account>,
}

/// The saved accounts. Thread-safe; cheap reads from memory, writes go to disk first.
#[derive(Debug)]
pub struct AccountStore {
    path: PathBuf,
    state: Mutex<AccountsFile>,
}

impl AccountStore {
    /// Opens the store at `path` (the caller passes `<data root>/accounts.json`). A missing file
    /// is an empty store. A damaged file is moved aside to `accounts.json.corrupt-<time>` and
    /// the store starts empty, so a bad write can never lock the player out of the launcher.
    pub fn open(path: impl Into<PathBuf>) -> Result<Self> {
        let path = path.into();
        let mut state = match fs::read(&path) {
            Ok(bytes) => match serde_json::from_slice::<AccountsFile>(&bytes) {
                Ok(file) => file,
                Err(error) => {
                    let backup =
                        sibling_path(&path, &format!(".corrupt-{}", Timestamp::now().as_second()));
                    tracing::warn!(
                        %error,
                        backup = %backup.display(),
                        "accounts file is damaged, starting with no accounts"
                    );
                    if let Err(error) = fs::rename(&path, &backup) {
                        tracing::warn!(%error, "could not move the damaged accounts file aside");
                    }
                    AccountsFile::default()
                }
            },
            Err(error) if error.kind() == io::ErrorKind::NotFound => AccountsFile::default(),
            Err(error) => return Err(AuthError::storage("read", &path, error)),
        };
        state.version = FILE_VERSION;
        if let Some(active) = state.active_account_id {
            if !state.accounts.iter().any(|account| account.id == active) {
                state.active_account_id = None;
            }
        }
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// All accounts, in the order they were added.
    pub fn list(&self) -> Vec<Account> {
        self.state.lock().accounts.clone()
    }

    pub fn get(&self, id: Uuid) -> Option<Account> {
        self.state
            .lock()
            .accounts
            .iter()
            .find(|account| account.id == id)
            .cloned()
    }

    pub fn active_id(&self) -> Option<Uuid> {
        self.state.lock().active_account_id
    }

    pub fn active(&self) -> Option<Account> {
        let state = self.state.lock();
        let active = state.active_account_id?;
        state
            .accounts
            .iter()
            .find(|account| account.id == active)
            .cloned()
    }

    /// The account of this kind with this profile UUID, if saved.
    pub fn find_by_profile(&self, kind: AccountKind, uuid: Uuid) -> Option<Account> {
        self.state
            .lock()
            .accounts
            .iter()
            .find(|account| account.kind == kind && account.uuid == uuid)
            .cloned()
    }

    /// Inserts `account`, or replaces the saved account with the same local id.
    pub fn put(&self, account: Account) -> Result<()> {
        self.mutate(|file| {
            match file
                .accounts
                .iter_mut()
                .find(|saved| saved.id == account.id)
            {
                Some(saved) => *saved = account,
                None => file.accounts.push(account),
            }
            Ok(())
        })
    }

    /// Updates the saved account with the same kind and profile UUID, or creates it. Signing in
    /// again therefore never duplicates an account and keeps its local id. Returns the stored
    /// account and whether it was created.
    pub fn upsert(
        &self,
        kind: AccountKind,
        uuid: Uuid,
        apply: impl FnOnce(&mut Account),
    ) -> Result<(Account, bool)> {
        self.mutate(|file| {
            if let Some(saved) = file
                .accounts
                .iter_mut()
                .find(|saved| saved.kind == kind && saved.uuid == uuid)
            {
                apply(saved);
                return Ok((saved.clone(), false));
            }
            let mut account = Account::new(kind, String::new(), uuid);
            apply(&mut account);
            file.accounts.push(account.clone());
            Ok((account, true))
        })
    }

    /// Changes a saved account in place.
    pub fn update(&self, id: Uuid, apply: impl FnOnce(&mut Account)) -> Result<Account> {
        self.mutate(|file| {
            let saved = file
                .accounts
                .iter_mut()
                .find(|saved| saved.id == id)
                .ok_or(AuthError::AccountNotFound { id })?;
            apply(saved);
            Ok(saved.clone())
        })
    }

    /// Records that the account was just used to play.
    pub fn mark_used(&self, id: Uuid) -> Result<Account> {
        self.update(id, |account| account.last_used_at = Some(Timestamp::now()))
    }

    /// Removes an account (clearing the active id if it pointed at it).
    pub fn remove(&self, id: Uuid) -> Result<Option<Account>> {
        self.mutate(|file| {
            let Some(index) = file.accounts.iter().position(|saved| saved.id == id) else {
                return Ok(None);
            };
            let removed = file.accounts.remove(index);
            if file.active_account_id == Some(id) {
                file.active_account_id = None;
            }
            Ok(Some(removed))
        })
    }

    /// Selects the active account (`None` clears it).
    pub fn set_active(&self, id: Option<Uuid>) -> Result<()> {
        self.mutate(|file| {
            if let Some(id) = id {
                if !file.accounts.iter().any(|saved| saved.id == id) {
                    return Err(AuthError::AccountNotFound { id });
                }
            }
            file.active_account_id = id;
            Ok(())
        })
    }

    /// Applies `change` to a copy, writes it, then publishes it: memory never runs ahead of disk.
    fn mutate<T>(&self, change: impl FnOnce(&mut AccountsFile) -> Result<T>) -> Result<T> {
        let mut state = self.state.lock();
        let mut next = state.clone();
        let value = change(&mut next)?;
        write_atomic(&self.path, &next)?;
        *state = next;
        Ok(value)
    }
}

fn write_atomic(path: &Path, file: &AccountsFile) -> Result<()> {
    let json = serde_json::to_vec_pretty(file)
        .map_err(|error| AuthError::storage("write", path, io::Error::other(error)))?;
    if let Some(parent) = path.parent() {
        if !parent.as_os_str().is_empty() {
            fs::create_dir_all(parent)
                .map_err(|error| AuthError::storage("write", parent, error))?;
        }
    }
    let tmp = sibling_path(path, ".tmp");
    if let Err(error) = write_then_rename(&tmp, path, &json) {
        let _ = fs::remove_file(&tmp);
        return Err(AuthError::storage("write", path, error));
    }
    Ok(())
}

fn write_then_rename(tmp: &Path, path: &Path, contents: &[u8]) -> io::Result<()> {
    let mut out = fs::File::create(tmp)?;
    out.write_all(contents)?;
    out.write_all(b"\n")?;
    out.sync_all()?;
    drop(out);
    fs::rename(tmp, path)
}

/// `accounts.json` + `suffix` in the same directory.
fn sibling_path(path: &Path, suffix: &str) -> PathBuf {
    let mut name = path
        .file_name()
        .map(|name| name.to_os_string())
        .unwrap_or_else(|| "accounts.json".into());
    name.push(suffix);
    path.with_file_name(name)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offline::offline_uuid;

    fn temp_store() -> (tempfile::TempDir, AccountStore) {
        let dir = tempfile::tempdir().unwrap();
        let store = AccountStore::open(dir.path().join("data").join("accounts.json")).unwrap();
        (dir, store)
    }

    #[test]
    fn persists_accounts_and_active_id() {
        let (dir, store) = temp_store();
        assert!(store.list().is_empty());
        let (alex, created) = store
            .upsert(AccountKind::Offline, offline_uuid("Alex"), |account| {
                account.username = "Alex".into();
            })
            .unwrap();
        assert!(created);
        store.set_active(Some(alex.id)).unwrap();
        store.mark_used(alex.id).unwrap();

        let reopened = AccountStore::open(store.path()).unwrap();
        assert_eq!(reopened.list().len(), 1);
        assert_eq!(reopened.active_id(), Some(alex.id));
        let saved = reopened.active().unwrap();
        assert_eq!(saved.username, "Alex");
        assert!(saved.last_used_at.is_some());

        let raw = fs::read_to_string(store.path()).unwrap();
        assert!(raw.contains("\"activeAccountId\""));
        assert!(raw.contains("\"version\": 1"));
        let leftovers: Vec<_> = fs::read_dir(dir.path().join("data"))
            .unwrap()
            .map(|entry| entry.unwrap().file_name())
            .collect();
        assert_eq!(leftovers, vec![std::ffi::OsString::from("accounts.json")]);
    }

    #[test]
    fn upsert_does_not_duplicate_profiles() {
        let (_dir, store) = temp_store();
        let uuid = offline_uuid("Steve");
        let (first, created) = store
            .upsert(AccountKind::Microsoft, uuid, |account| {
                account.username = "Steve".into()
            })
            .unwrap();
        assert!(created);
        let (second, created) = store
            .upsert(AccountKind::Microsoft, uuid, |account| {
                account.username = "Steve2".into()
            })
            .unwrap();
        assert!(!created);
        assert_eq!(first.id, second.id);
        assert_eq!(store.list().len(), 1);
        assert_eq!(store.get(first.id).unwrap().username, "Steve2");
        // Same UUID but another kind is a different account.
        store
            .upsert(AccountKind::Offline, uuid, |account| {
                account.username = "Steve".into()
            })
            .unwrap();
        assert_eq!(store.list().len(), 2);
        assert!(store.find_by_profile(AccountKind::Offline, uuid).is_some());
    }

    #[test]
    fn remove_clears_the_active_account() {
        let (_dir, store) = temp_store();
        let (account, _) = store
            .upsert(AccountKind::Offline, offline_uuid("Bob"), |_| {})
            .unwrap();
        store.set_active(Some(account.id)).unwrap();
        assert_eq!(store.remove(account.id).unwrap().unwrap().id, account.id);
        assert_eq!(store.active_id(), None);
        assert!(store.remove(account.id).unwrap().is_none());
        assert!(matches!(
            store.set_active(Some(account.id)),
            Err(AuthError::AccountNotFound { .. })
        ));
        assert!(matches!(
            store.update(account.id, |_| {}),
            Err(AuthError::AccountNotFound { .. })
        ));
    }

    #[test]
    fn damaged_file_is_moved_aside() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.json");
        fs::write(&path, b"{ not json").unwrap();
        let store = AccountStore::open(&path).unwrap();
        assert!(store.list().is_empty());
        let names: Vec<String> = fs::read_dir(dir.path())
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert!(
            names
                .iter()
                .any(|name| name.starts_with("accounts.json.corrupt-"))
        );
        // The store keeps working.
        store
            .upsert(AccountKind::Offline, offline_uuid("Eve"), |_| {})
            .unwrap();
        assert_eq!(AccountStore::open(&path).unwrap().list().len(), 1);
    }

    #[test]
    fn dangling_active_id_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("accounts.json");
        fs::write(
            &path,
            format!(
                r#"{{"version":1,"activeAccountId":"{}","accounts":[]}}"#,
                Uuid::new_v4()
            ),
        )
        .unwrap();
        assert_eq!(AccountStore::open(&path).unwrap().active_id(), None);
    }
}
