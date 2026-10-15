//! File layout under the AFK root (`DataDirs` → `afk/`).
//!
//! ```text
//! afk/
//!   accounts.json, settings.json
//!   accounts/<id>/      MCC working directory: MinecraftClient.ini, matches.ini,
//!                       giris-komutlari.txt (only while needed), sohbet.txt, uyarilar.txt, konsol.txt
//!   bin/<version>/      installed client builds (see `runtime`)
//! ```
//!
//! Every account runs in its own folder because MCC writes its chat log, alerts and matches file
//! relative to the working directory.

use std::path::{Path, PathBuf};

use crate::config::{INI_FILE_NAME, LOGIN_SCRIPT_FILE_NAME, MATCHES_FILE_NAME};

/// Launcher-side console log of an account (already masked).
pub const CONSOLE_LOG_FILE_NAME: &str = "konsol.txt";
/// Files MCC itself writes in the account folder (see the config writer).
pub const CHAT_LOG_FILE_NAME: &str = "sohbet.txt";
pub const ALERTS_FILE_NAME: &str = "uyarilar.txt";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AfkPaths {
    root: PathBuf,
}

impl AfkPaths {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    pub fn root(&self) -> &Path {
        &self.root
    }

    pub fn accounts_file(&self) -> PathBuf {
        self.root.join("accounts.json")
    }

    pub fn settings_file(&self) -> PathBuf {
        self.root.join("settings.json")
    }

    pub fn bin_dir(&self) -> PathBuf {
        self.root.join("bin")
    }

    pub fn accounts_dir(&self) -> PathBuf {
        self.root.join("accounts")
    }

    /// Callers only pass validated ids (see the store), so this never leaves `accounts/`.
    pub fn account_dir(&self, account_id: &str) -> PathBuf {
        self.accounts_dir().join(account_id)
    }

    pub fn account_ini(&self, account_id: &str) -> PathBuf {
        self.account_dir(account_id).join(INI_FILE_NAME)
    }

    pub fn account_matches(&self, account_id: &str) -> PathBuf {
        self.account_dir(account_id).join(MATCHES_FILE_NAME)
    }

    pub fn account_login_script(&self, account_id: &str) -> PathBuf {
        self.account_dir(account_id).join(LOGIN_SCRIPT_FILE_NAME)
    }

    pub fn account_console_log(&self, account_id: &str) -> PathBuf {
        self.account_dir(account_id).join(CONSOLE_LOG_FILE_NAME)
    }
}
