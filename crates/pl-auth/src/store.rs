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

/// An account taken out of the list, with what [`AccountStore::restore`] needs to put it back.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RemovedAccount {
    pub account: Account,
    /// Its position in the list.
    pub index: usize,
    /// It was the active account.
    pub was_active: bool,
}

/// The account to fall back on: the most recently played, then the most recently added, then
/// the first in the list.
fn most_recently_used(accounts: &[Account]) -> Option<Uuid> {
    accounts
        .iter()
        .enumerate()
        .max_by(|(ia, a), (ib, b)| {
            a.last_used_at
                .cmp(&b.last_used_at)
                .then(a.added_at.cmp(&b.added_at))
                .then(ib.cmp(ia))
        })
        .map(|(_, account)| account.id)
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
        if let Some(active) = state.active_account_id
            && !state.accounts.iter().any(|account| account.id == active)
        {
            state.active_account_id = None;
        }
        Ok(Self {
            path,
            state: Mutex::new(state),
        })
    }

    pub fn path(&self) -> &Path {
        &self.path
    }

    /// All accounts, in stored order: the pinned ones keep the player's order relative to each
    /// other; the UI lists them first and sorts the rest by activity.
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

    /// The offline account with this name, ignoring ASCII case like servers do.
    pub fn find_offline_name(&self, name: &str) -> Option<Account> {
        self.state
            .lock()
            .accounts
            .iter()
            .find(|account| {
                account.kind == AccountKind::Offline && account.username.eq_ignore_ascii_case(name)
            })
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

    /// Removes an account. If it was the active one, the most recently played remaining account
    /// (or else the most recently added) becomes active, in the same write.
    pub fn take(&self, id: Uuid) -> Result<Option<RemovedAccount>> {
        self.mutate(|file| {
            let Some(index) = file.accounts.iter().position(|saved| saved.id == id) else {
                return Ok(None);
            };
            let account = file.accounts.remove(index);
            let was_active = file.active_account_id == Some(id);
            if was_active {
                file.active_account_id = most_recently_used(&file.accounts);
            }
            Ok(Some(RemovedAccount {
                account,
                index,
                was_active,
            }))
        })
    }

    /// Puts a removed account back at its old position, active again if it was. When the same
    /// profile was added again in the meantime (for offline accounts: the same name in any letter
    /// case, as servers see it), that account stays (and is selected instead); the flag says
    /// whether `removed` itself went back into the list.
    pub fn restore(&self, removed: &RemovedAccount) -> Result<(Account, bool)> {
        self.mutate(|file| {
            let account = &removed.account;
            let offline = account.kind == AccountKind::Offline;
            let existing = file
                .accounts
                .iter()
                .find(|saved| {
                    saved.id == account.id
                        || (saved.kind == account.kind && saved.uuid == account.uuid)
                        || (offline
                            && saved.kind == AccountKind::Offline
                            && saved.username.eq_ignore_ascii_case(&account.username))
                })
                .cloned();
            let (restored, inserted) = match existing {
                Some(existing) => (existing, false),
                None => {
                    let index = removed.index.min(file.accounts.len());
                    file.accounts.insert(index, account.clone());
                    (account.clone(), true)
                }
            };
            if removed.was_active {
                file.active_account_id = Some(restored.id);
            }
            Ok((restored, inserted))
        })
    }

    /// Pins or unpins an account. A newly pinned account goes to the end of the pinned ones;
    /// unpinning leaves the others' order alone. Nothing is written when nothing changes.
    pub fn set_pinned(&self, id: Uuid, pinned: bool) -> Result<Account> {
        let saved = self.get(id).ok_or(AuthError::AccountNotFound { id })?;
        if saved.pinned == pinned {
            return Ok(saved);
        }
        self.mutate(|file| {
            let index = file
                .accounts
                .iter()
                .position(|saved| saved.id == id)
                .ok_or(AuthError::AccountNotFound { id })?;
            let mut account = file.accounts.remove(index);
            account.pinned = pinned;
            let at = if pinned {
                file.accounts
                    .iter()
                    .rposition(|saved| saved.pinned)
                    .map_or(0, |last| last + 1)
            } else {
                index
            };
            file.accounts.insert(at, account.clone());
            Ok(account)
        })
    }

    /// Puts the pinned accounts in the order of `ids`. Unknown and unpinned ids are ignored;
    /// pinned accounts missing from `ids` follow the listed ones in their old order. The pinned
    /// accounts end up first in the file, the others keep their relative order after them.
    pub fn reorder_pinned(&self, ids: &[Uuid]) -> Result<()> {
        self.mutate(|file| {
            let (mut pinned, unpinned): (Vec<Account>, Vec<Account>) =
                std::mem::take(&mut file.accounts)
                    .into_iter()
                    .partition(|saved| saved.pinned);
            let mut ordered = Vec::with_capacity(pinned.len() + unpinned.len());
            for id in ids {
                if let Some(position) = pinned.iter().position(|saved| saved.id == *id) {
                    ordered.push(pinned.remove(position));
                }
            }
            ordered.extend(pinned);
            ordered.extend(unpinned);
            file.accounts = ordered;
            Ok(())
        })
    }

    /// Selects the active account (`None` clears it).
    pub fn set_active(&self, id: Option<Uuid>) -> Result<()> {
        self.mutate(|file| {
            if let Some(id) = id
                && !file.accounts.iter().any(|saved| saved.id == id)
            {
                return Err(AuthError::AccountNotFound { id });
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
    if let Some(parent) = path.parent()
        && !parent.as_os_str().is_empty()
    {
        fs::create_dir_all(parent).map_err(|error| AuthError::storage("write", parent, error))?;
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

    fn add(
        store: &AccountStore,
        name: &str,
        added_mins_ago: i64,
        played_mins_ago: Option<i64>,
    ) -> Account {
        let ago = |mins: i64| {
            Timestamp::now()
                .checked_sub(jiff::SignedDuration::from_mins(mins))
                .unwrap()
        };
        store
            .upsert(AccountKind::Offline, offline_uuid(name), |account| {
                account.username = name.into();
                account.added_at = ago(added_mins_ago);
                account.last_used_at = played_mins_ago.map(ago);
            })
            .unwrap()
            .0
    }

    #[test]
    fn removing_the_active_account_selects_the_most_recently_played() {
        let (_dir, store) = temp_store();
        let old = add(&store, "Old", 300, Some(200));
        let recent = add(&store, "Recent", 250, Some(5));
        let fresh = add(&store, "Fresh", 1, None);
        let active = add(&store, "Active", 400, Some(1));
        store.set_active(Some(active.id)).unwrap();

        let removed = store.take(active.id).unwrap().unwrap();
        assert_eq!(removed.index, 3);
        assert!(removed.was_active);
        assert_eq!(store.active_id(), Some(recent.id));

        // Removing an inactive account leaves the selection alone.
        let removed_old = store.take(old.id).unwrap().unwrap();
        assert!(!removed_old.was_active);
        assert_eq!(store.active_id(), Some(recent.id));

        // Nobody played yet: the most recently added wins.
        store.take(recent.id).unwrap();
        assert_eq!(store.active_id(), Some(fresh.id));
        store.take(fresh.id).unwrap();
        assert_eq!(store.active_id(), None);
        assert!(store.take(fresh.id).unwrap().is_none());
    }

    #[test]
    fn restore_puts_the_account_back_where_it_was() {
        let (_dir, store) = temp_store();
        let a = add(&store, "Aaa", 30, None);
        let b = add(&store, "Bbb", 20, None);
        let c = add(&store, "Ccc", 10, None);
        store.set_active(Some(b.id)).unwrap();
        let removed = store.take(b.id).unwrap().unwrap();
        assert_ne!(store.active_id(), Some(b.id));

        let (restored, inserted) = store.restore(&removed).unwrap();
        assert!(inserted);
        assert_eq!(restored, b);
        let order: Vec<Uuid> = store.list().iter().map(|account| account.id).collect();
        assert_eq!(order, vec![a.id, b.id, c.id]);
        assert_eq!(store.active_id(), Some(b.id));
        // Persisted.
        assert_eq!(
            AccountStore::open(store.path()).unwrap().active_id(),
            Some(b.id)
        );

        // The same profile came back meanwhile: no duplicate, the newcomer is selected.
        let removed = store.take(c.id).unwrap().unwrap();
        let again = add(&store, "Ccc", 0, None);
        let (restored, inserted) = store.restore(&removed).unwrap();
        assert!(!inserted);
        assert_eq!(restored.id, again.id);
        assert_eq!(store.list().len(), 3);

        // Positions past the end clamp.
        let removed = RemovedAccount {
            account: Account::new(AccountKind::Offline, "Zed", offline_uuid("Zed")),
            index: 99,
            was_active: false,
        };
        store.restore(&removed).unwrap();
        assert_eq!(store.list().last().unwrap().username, "Zed");
    }

    fn ids(store: &AccountStore) -> Vec<Uuid> {
        store.list().iter().map(|account| account.id).collect()
    }

    #[test]
    fn pinned_accounts_join_the_end_of_the_pinned_group() {
        let (_dir, store) = temp_store();
        let a = add(&store, "Aaa", 40, None);
        let b = add(&store, "Bbb", 30, None);
        let c = add(&store, "Ccc", 20, None);
        let d = add(&store, "Ddd", 10, None);

        // The first pin goes to the front, later pins line up behind it.
        assert!(store.set_pinned(c.id, true).unwrap().pinned);
        assert_eq!(ids(&store), vec![c.id, a.id, b.id, d.id]);
        store.set_pinned(a.id, true).unwrap();
        assert_eq!(ids(&store), vec![c.id, a.id, b.id, d.id]);
        store.set_pinned(d.id, true).unwrap();
        assert_eq!(ids(&store), vec![c.id, a.id, d.id, b.id]);

        // Unpinning keeps everyone in place; pinning again lines up at the end.
        assert!(!store.set_pinned(a.id, false).unwrap().pinned);
        assert_eq!(ids(&store), vec![c.id, a.id, d.id, b.id]);
        store.set_pinned(a.id, true).unwrap();
        assert_eq!(ids(&store), vec![c.id, d.id, a.id, b.id]);

        // No change, no write.
        let before = fs::metadata(store.path()).unwrap().modified().unwrap();
        assert!(store.set_pinned(c.id, true).unwrap().pinned);
        assert_eq!(
            fs::metadata(store.path()).unwrap().modified().unwrap(),
            before
        );

        // Persisted, flags included.
        let reopened = AccountStore::open(store.path()).unwrap();
        assert_eq!(ids(&reopened), vec![c.id, d.id, a.id, b.id]);
        let pinned: Vec<bool> = reopened.list().iter().map(|a| a.pinned).collect();
        assert_eq!(pinned, vec![true, true, true, false]);
        assert!(matches!(
            store.set_pinned(Uuid::new_v4(), true),
            Err(AuthError::AccountNotFound { .. })
        ));
    }

    #[test]
    fn reorder_moves_only_pinned_accounts() {
        let (_dir, store) = temp_store();
        let a = add(&store, "Aaa", 40, None);
        let b = add(&store, "Bbb", 30, None);
        let c = add(&store, "Ccc", 20, None);
        let d = add(&store, "Ddd", 10, None);
        store.set_pinned(b.id, true).unwrap();
        store.set_pinned(d.id, true).unwrap();
        store.set_pinned(a.id, true).unwrap();
        assert_eq!(ids(&store), vec![b.id, d.id, a.id, c.id]);

        // Unknown and unpinned ids are ignored; unlisted pinned ones follow in their old order.
        store
            .reorder_pinned(&[a.id, c.id, Uuid::new_v4(), b.id, a.id])
            .unwrap();
        assert_eq!(ids(&store), vec![a.id, b.id, d.id, c.id]);
        assert!(!store.get(c.id).unwrap().pinned);
        assert_eq!(
            ids(&AccountStore::open(store.path()).unwrap()),
            vec![a.id, b.id, d.id, c.id]
        );

        // Pinned accounts move to the front of the file; the rest keep their order.
        store.set_pinned(b.id, false).unwrap();
        store.reorder_pinned(&[d.id, a.id]).unwrap();
        assert_eq!(ids(&store), vec![d.id, a.id, b.id, c.id]);
    }

    #[test]
    fn restore_never_brings_back_an_offline_name_taken_meanwhile() {
        let (_dir, store) = temp_store();
        let steve = add(&store, "Steve", 20, None);
        store.set_active(Some(steve.id)).unwrap();
        let removed = store.take(steve.id).unwrap().unwrap();
        // The same player in other letter case (servers treat both as one name).
        let lower = add(&store, "steve", 0, None);

        let (kept, inserted) = store.restore(&removed).unwrap();
        assert!(!inserted);
        assert_eq!(kept.id, lower.id);
        assert_eq!(ids(&store), vec![lower.id]);
        assert_eq!(store.active_id(), Some(lower.id));
    }

    #[test]
    fn restore_keeps_the_pin() {
        let (_dir, store) = temp_store();
        let a = add(&store, "Aaa", 30, None);
        let b = add(&store, "Bbb", 20, None);
        store.set_pinned(b.id, true).unwrap();
        let removed = store.take(b.id).unwrap().unwrap();
        assert!(removed.account.pinned);
        store.restore(&removed).unwrap();
        assert_eq!(ids(&store), vec![b.id, a.id]);
        assert!(store.get(b.id).unwrap().pinned);
    }

    #[test]
    fn offline_names_match_ignoring_case() {
        let (_dir, store) = temp_store();
        add(&store, "Aaa", 30, None);
        let b = add(&store, "Bbb", 20, None);

        assert_eq!(store.find_offline_name("bBB").unwrap().id, b.id);
        assert!(store.find_offline_name("Bb").is_none());
        store
            .upsert(AccountKind::Microsoft, Uuid::new_v4(), |account| {
                account.username = "Ddd".into();
            })
            .unwrap();
        assert!(store.find_offline_name("ddd").is_none());
    }
}
