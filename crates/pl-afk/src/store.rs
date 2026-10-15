//! `accounts.json` + `settings.json` persistence and the login-commands secrets.
//!
//! - Writes are atomic (`.tmp` + rename) and serialized by one lock.
//! - An unreadable `accounts.json` is copied to `accounts.json.bozuk-<yyyyMMdd-HHmmss>` and the store
//!   starts empty (legacy behavior); an I/O error other than "missing" is reported instead, so a
//!   locked file is never overwritten.
//! - Login commands live in the OS keychain. Legacy files that still carry them in plain text are
//!   migrated on open: keychain first, then the file is rewritten without them. If the keychain
//!   refuses, the commands stay where they were (no data loss) and the migration is retried on the
//!   next open.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use parking_lot::Mutex;

use crate::error::{AfkError, Result};
use crate::form::AccountSettings;
use crate::legacy;
use crate::model::{
    AfkAccount, AfkSettings, is_safe_account_id, is_valid_nickname, new_account_id,
};
use crate::paths::AfkPaths;
use crate::secrets::{SecretStore, login_commands_key};
use crate::util::{eq_ignore_case, write_atomic};

/// What [`AccountStore::open`] had to repair.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct LoadReport {
    /// `accounts.json` could not be parsed; it was copied here and the store started empty.
    pub corrupt_backup: Option<PathBuf>,
    /// Accounts whose plain-text login commands moved to the keychain.
    pub migrated_login_commands: usize,
    /// Accounts whose plain-text login commands had to stay in the file (keychain unavailable).
    pub pending_migrations: usize,
    /// Accounts that got a new id (empty, unsafe or duplicate id).
    pub regenerated_ids: usize,
    /// Duplicate kick messages removed (the legacy launcher appended the defaults on every load).
    pub removed_duplicate_kick_messages: usize,
}

struct State {
    accounts: Vec<AfkAccount>,
    /// Login commands that are still only in `accounts.json`, by account id.
    plaintext: HashMap<String, Vec<String>>,
    settings: AfkSettings,
}

/// Thread-safe AFK account storage.
pub struct AccountStore {
    paths: AfkPaths,
    secrets: Arc<dyn SecretStore>,
    state: Mutex<State>,
}

impl std::fmt::Debug for AccountStore {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AccountStore")
            .field("root", &self.paths.root())
            .finish_non_exhaustive()
    }
}

impl AccountStore {
    /// Loads (and repairs/migrates) the store. May block on the keychain.
    pub fn open(paths: AfkPaths, secrets: Arc<dyn SecretStore>) -> Result<(Self, LoadReport)> {
        let mut report = LoadReport::default();
        let accounts_file = paths.accounts_file();

        let parsed = match std::fs::read(&accounts_file) {
            Ok(bytes) => match legacy::parse_accounts(&String::from_utf8_lossy(&bytes)) {
                Ok(list) => list,
                Err(error) => {
                    tracing::warn!(%error, "accounts.json is unreadable, starting empty");
                    report.corrupt_backup = backup_corrupt_file(&accounts_file);
                    Vec::new()
                }
            },
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Vec::new(),
            Err(e) => return Err(AfkError::io(&accounts_file, e)),
        };

        let mut changed = false;
        let mut seen_ids = HashSet::new();
        let mut accounts = Vec::with_capacity(parsed.len());
        let mut plaintext = HashMap::new();

        for entry in parsed {
            let mut account = entry.account;

            if !is_safe_account_id(&account.id) || seen_ids.contains(&account.id) {
                account.id = new_account_id();
                report.regenerated_ids += 1;
                changed = true;
            }
            seen_ids.insert(account.id.clone());

            let removed = dedupe(&mut account.auto_relog.kick_messages);
            if removed > 0 {
                report.removed_duplicate_kick_messages += removed;
                changed = true;
            }

            if !entry.login_commands.is_empty() {
                let commands = entry.login_commands;
                if commands.iter().all(|c| c.trim().is_empty()) {
                    changed = true; // nothing secret to keep
                } else {
                    match store_commands(secrets.as_ref(), &account.id, &commands) {
                        Ok(()) => {
                            report.migrated_login_commands += 1;
                            changed = true;
                        }
                        Err(error) => {
                            tracing::warn!(
                                account = %account.id,
                                %error,
                                "login commands stay in accounts.json until the keychain works"
                            );
                            report.pending_migrations += 1;
                            plaintext.insert(account.id.clone(), commands);
                        }
                    }
                }
            }

            accounts.push(account);
        }

        let settings = match std::fs::read(paths.settings_file()) {
            Ok(bytes) => legacy::parse_settings(&String::from_utf8_lossy(&bytes)),
            Err(_) => AfkSettings::default(),
        };

        let store = Self {
            paths,
            secrets,
            state: Mutex::new(State {
                accounts,
                plaintext,
                settings,
            }),
        };
        if changed {
            let state = store.state.lock();
            store.save_locked(&state)?;
        }
        Ok((store, report))
    }

    pub fn paths(&self) -> &AfkPaths {
        &self.paths
    }

    pub fn accounts(&self) -> Vec<AfkAccount> {
        self.state.lock().accounts.clone()
    }

    pub fn account(&self, id: &str) -> Option<AfkAccount> {
        self.state
            .lock()
            .accounts
            .iter()
            .find(|a| a.id == id)
            .cloned()
    }

    /// Adds an account with legacy defaults. The nickname is trimmed and must be a valid,
    /// unused (case-insensitive) nickname.
    pub fn add_account(&self, nickname: &str) -> Result<AfkAccount> {
        let nickname = nickname.trim();
        if !is_valid_nickname(nickname) {
            return Err(AfkError::InvalidNickname);
        }
        let mut state = self.state.lock();
        if state
            .accounts
            .iter()
            .any(|a| eq_ignore_case(&a.nickname, nickname))
        {
            return Err(AfkError::DuplicateNickname);
        }
        let account = AfkAccount::new(nickname);
        state.accounts.push(account.clone());
        self.save_locked(&state)?;
        Ok(account)
    }

    /// Replaces the stored account with the same id. Id and nickname are never changed here
    /// (legacy: the settings page cannot rename an account).
    pub fn update_account(&self, account: AfkAccount) -> Result<AfkAccount> {
        let mut state = self.state.lock();
        let slot = state
            .accounts
            .iter_mut()
            .find(|a| a.id == account.id)
            .ok_or_else(|| AfkError::AccountNotFound(account.id.clone()))?;
        let nickname = std::mem::take(&mut slot.nickname);
        *slot = AfkAccount {
            nickname,
            ..account
        };
        let updated = slot.clone();
        self.save_locked(&state)?;
        Ok(updated)
    }

    /// Removes the account and its keychain entry. Returns false when it did not exist. The
    /// account folder is the caller's business (its session must be stopped first).
    pub fn remove_account(&self, id: &str) -> Result<bool> {
        {
            let mut state = self.state.lock();
            let Some(index) = state.accounts.iter().position(|a| a.id == id) else {
                return Ok(false);
            };
            state.accounts.remove(index);
            state.plaintext.remove(id);
            self.save_locked(&state)?;
        }
        if let Err(error) = self.secrets.delete(&login_commands_key(id)) {
            tracing::warn!(account = %id, %error, "could not delete the login commands secret");
        }
        Ok(true)
    }

    /// The account's login commands (may block on the keychain).
    pub fn login_commands(&self, id: &str) -> Result<Vec<String>> {
        if let Some(commands) = self.state.lock().plaintext.get(id) {
            return Ok(commands.clone());
        }
        match self.secrets.get(&login_commands_key(id))? {
            None => Ok(Vec::new()),
            Some(json) => serde_json::from_str(&json)
                .map_err(|_| AfkError::Secret("stored login commands are not a list".into())),
        }
    }

    /// Stores the login commands in the keychain (an all-blank list deletes the entry). Fails
    /// without touching anything when the keychain refuses.
    pub fn set_login_commands(&self, id: &str, commands: &[String]) -> Result<()> {
        if self.account(id).is_none() {
            return Err(AfkError::AccountNotFound(id.to_owned()));
        }
        if commands.iter().all(|c| c.trim().is_empty()) {
            self.secrets.delete(&login_commands_key(id))?;
        } else {
            store_commands(self.secrets.as_ref(), id, commands)?;
        }
        let mut state = self.state.lock();
        if state.plaintext.remove(id).is_some() {
            // The plain-text copy is obsolete now; drop it from disk too.
            self.save_locked(&state)?;
        }
        Ok(())
    }

    /// The settings page for an account, including its login commands.
    pub fn settings_form(&self, id: &str) -> Result<AccountSettings> {
        let account = self
            .account(id)
            .ok_or_else(|| AfkError::AccountNotFound(id.to_owned()))?;
        let commands = self.login_commands(id)?;
        Ok(AccountSettings::from_account(&account, &commands))
    }

    /// Validates and saves the settings page. Login commands are stored first: if the keychain
    /// refuses, nothing is saved.
    pub fn save_settings(&self, id: &str, settings: &AccountSettings) -> Result<AfkAccount> {
        let mut account = self
            .account(id)
            .ok_or_else(|| AfkError::AccountNotFound(id.to_owned()))?;
        let commands = settings
            .apply(&mut account)
            .map_err(AfkError::InvalidSettings)?;
        self.set_login_commands(id, &commands)?;
        self.update_account(account)
    }

    pub fn afk_settings(&self) -> AfkSettings {
        self.state.lock().settings
    }

    /// Persists the console filter toggle. Returns whether the value changed.
    pub fn set_show_script_commands(&self, show: bool) -> Result<bool> {
        let mut state = self.state.lock();
        if state.settings.show_script_commands == show {
            return Ok(false);
        }
        state.settings.show_script_commands = show;
        let path = self.paths.settings_file();
        let text = legacy::write_settings(&state.settings)?;
        if let Err(error) = write_atomic(&path, text.as_bytes(), false) {
            // Legacy: the value still applies for this session.
            tracing::warn!(%error, "could not save AFK settings.json");
        }
        Ok(true)
    }

    fn save_locked(&self, state: &State) -> Result<()> {
        let path = self.paths.accounts_file();
        let text = legacy::write_accounts(&state.accounts, &state.plaintext)?;
        write_atomic(&path, text.as_bytes(), !state.plaintext.is_empty())
            .map_err(|e| AfkError::io(&path, e))
    }
}

fn store_commands(secrets: &dyn SecretStore, id: &str, commands: &[String]) -> Result<()> {
    let json = serde_json::to_string(commands).map_err(|e| AfkError::json("login commands", e))?;
    let key = login_commands_key(id);
    secrets.set(&key, &json)?;
    // Only trust the keychain once the value reads back.
    match secrets.get(&key)? {
        Some(stored) if stored == json => Ok(()),
        _ => Err(AfkError::Secret(
            "the keychain did not keep the login commands".into(),
        )),
    }
}

/// Removes exact duplicates, keeping the first occurrence. Returns how many were removed.
fn dedupe(items: &mut Vec<String>) -> usize {
    let before = items.len();
    let mut seen = HashSet::new();
    items.retain(|item| seen.insert(item.clone()));
    before - items.len()
}

fn backup_corrupt_file(path: &Path) -> Option<PathBuf> {
    let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
    let mut name = path.file_name()?.to_os_string();
    name.push(format!(".bozuk-{stamp}"));
    let backup = path.with_file_name(name);
    match std::fs::copy(path, &backup) {
        Ok(_) => Some(backup),
        Err(error) => {
            tracing::warn!(%error, "could not back up the corrupt accounts.json");
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::secrets::MemorySecretStore;

    fn open(dir: &Path, secrets: &Arc<MemorySecretStore>) -> (AccountStore, LoadReport) {
        AccountStore::open(AfkPaths::new(dir), secrets.clone()).unwrap()
    }

    #[test]
    fn add_validates_and_rejects_duplicates() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecretStore::new());
        let (store, report) = open(dir.path(), &secrets);
        assert_eq!(report, LoadReport::default());
        let account = store.add_account("  Afk_One ").unwrap();
        assert_eq!(account.nickname, "Afk_One");
        assert!(matches!(
            store.add_account("afk_one"),
            Err(AfkError::DuplicateNickname)
        ));
        assert!(matches!(
            store.add_account("x"),
            Err(AfkError::InvalidNickname)
        ));
        let (reopened, _) = open(dir.path(), &secrets);
        assert_eq!(reopened.accounts(), vec![account]);
    }

    #[test]
    fn corrupt_file_is_backed_up_and_store_starts_empty() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("accounts.json"), b"{ not json").unwrap();
        let secrets = Arc::new(MemorySecretStore::new());
        let (store, report) = open(dir.path(), &secrets);
        assert!(store.accounts().is_empty());
        let backup = report.corrupt_backup.expect("backup path");
        assert!(
            backup
                .file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("accounts.json.bozuk-")
        );
        assert_eq!(std::fs::read(&backup).unwrap(), b"{ not json");
        // The broken file is only replaced by the next save.
        store.add_account("Fresh").unwrap();
        assert_eq!(open(dir.path(), &secrets).0.accounts().len(), 1);
    }

    #[test]
    fn settings_toggle_persists_and_survives_corruption() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecretStore::new());
        let (store, _) = open(dir.path(), &secrets);
        assert!(!store.afk_settings().show_script_commands);
        assert!(store.set_show_script_commands(true).unwrap());
        assert!(!store.set_show_script_commands(true).unwrap());
        assert!(
            open(dir.path(), &secrets)
                .0
                .afk_settings()
                .show_script_commands
        );
        std::fs::write(dir.path().join("settings.json"), b"garbage").unwrap();
        assert!(
            !open(dir.path(), &secrets)
                .0
                .afk_settings()
                .show_script_commands
        );
    }

    #[test]
    fn login_commands_live_in_the_secret_store() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecretStore::new());
        let (store, _) = open(dir.path(), &secrets);
        let account = store.add_account("Secretive").unwrap();
        store
            .set_login_commands(&account.id, &["/login hunter22".to_owned()])
            .unwrap();
        assert_eq!(
            store.login_commands(&account.id).unwrap(),
            vec!["/login hunter22"]
        );
        let file = std::fs::read_to_string(dir.path().join("accounts.json")).unwrap();
        assert!(!file.contains("hunter22"));

        secrets.set_failing(true);
        assert!(
            store
                .set_login_commands(&account.id, &["/login x".to_owned()])
                .is_err()
        );
        secrets.set_failing(false);
        assert_eq!(
            store.login_commands(&account.id).unwrap(),
            vec!["/login hunter22"]
        );

        store
            .set_login_commands(&account.id, &[" ".to_owned()])
            .unwrap();
        assert!(store.login_commands(&account.id).unwrap().is_empty());
        assert!(secrets.entries().is_empty());

        store
            .set_login_commands(&account.id, &["/kit".to_owned()])
            .unwrap();
        assert!(store.remove_account(&account.id).unwrap());
        assert!(
            secrets.entries().is_empty(),
            "removing an account drops its secret"
        );
        assert!(!store.remove_account(&account.id).unwrap());
    }

    #[test]
    fn save_settings_validates_and_stores_commands_first() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecretStore::new());
        let (store, _) = open(dir.path(), &secrets);
        let account = store.add_account("Former").unwrap();
        let mut form = store.settings_form(&account.id).unwrap();
        form.login_commands.enabled = true;
        assert!(matches!(
            store.save_settings(&account.id, &form),
            Err(AfkError::InvalidSettings(_))
        ));
        form.login_commands.commands = vec!["/login pw1".into()];
        form.auto_start = true;
        secrets.set_failing(true);
        assert!(matches!(
            store.save_settings(&account.id, &form),
            Err(AfkError::Secret(_))
        ));
        assert!(
            !store.account(&account.id).unwrap().auto_start,
            "nothing saved"
        );
        secrets.set_failing(false);
        let saved = store.save_settings(&account.id, &form).unwrap();
        assert!(saved.auto_start && saved.login_commands.enabled);
        assert_eq!(saved.nickname, "Former");
        assert_eq!(
            store
                .settings_form(&account.id)
                .unwrap()
                .login_commands
                .commands,
            vec!["/login pw1"]
        );
    }

    #[test]
    fn update_keeps_identity() {
        let dir = tempfile::tempdir().unwrap();
        let secrets = Arc::new(MemorySecretStore::new());
        let (store, _) = open(dir.path(), &secrets);
        let account = store.add_account("Keeper").unwrap();
        let mut changed = account.clone();
        changed.nickname = "Renamed".into();
        changed.auto_start = true;
        let updated = store.update_account(changed).unwrap();
        assert_eq!(updated.nickname, "Keeper");
        assert!(updated.auto_start);
        let mut ghost = account;
        ghost.id = "missing".into();
        assert!(matches!(
            store.update_account(ghost),
            Err(AfkError::AccountNotFound(_))
        ));
    }
}
