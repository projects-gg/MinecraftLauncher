//! AFK account model.
//!
//! These are the in-memory and UI-facing types (camelCase, `specta::Type`). Their defaults are the
//! legacy C# initializers (`AfkModels.cs`); persistence in the legacy PascalCase `accounts.json`
//! shape lives in [`crate::legacy`]. Login commands are deliberately *not* part of the model: they
//! may contain passwords and live in the OS keychain (see [`crate::store::AccountStore`]).

use pl_lang::Lang;
use serde::{Deserialize, Serialize};

/// Server used when an account leaves its host empty.
pub const DEFAULT_SERVER_HOST: &str = "play.projects.gg";
/// Recommended game version of the built-in version list (used while the content index publishes
/// none; see [`crate::VersionCatalog`]).
pub const DEFAULT_MINECRAFT_VERSION: &str = "26.3";
/// "Ask the server" version value understood by MCC.
pub const AUTO_MINECRAFT_VERSION: &str = "auto";
/// Name given to scheduler tasks without one.
pub const DEFAULT_TASK_NAME: &str = "Görev";

/// One entry of the game version picker (see [`crate::VersionCatalog::options`]). Versions sharing
/// a protocol are listed once; `value` is written to MCC as-is, `label` is the range shown to the
/// user (the UI words the `auto` entry and marks the recommended one).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionOption {
    pub value: String,
    pub label: String,
    pub recommended: bool,
    pub auto: bool,
}

/// Same rule as the main menu nickname: 3-16 characters of `[A-Za-z0-9_]`.
pub fn is_valid_nickname(nickname: &str) -> bool {
    (3..=16).contains(&nickname.len())
        && nickname
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

/// A fresh account id in the legacy `Guid.ToString("N")` format (32 lowercase hex digits).
pub fn new_account_id() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Ids name per-account folders, so anything that could escape `accounts/` is rejected.
pub(crate) fn is_safe_account_id(id: &str) -> bool {
    !id.is_empty()
        && id.len() <= 64
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AfkAccount {
    pub id: String,
    pub nickname: String,
    /// Empty means [`DEFAULT_SERVER_HOST`].
    pub server_host: String,
    /// 0 (or out of range) means "not set": MCC resolves the SRV record, then falls back to 25565.
    pub server_port: i32,
    /// Connect this account when the launcher starts.
    pub auto_start: bool,
    /// Empty: follow the recommended version (new accounts); [`AUTO_MINECRAFT_VERSION`]; or a value
    /// of the version list (see [`crate::VersionCatalog`]).
    pub minecraft_version: String,
    /// When false the server resource pack is not downloaded (the server is still told it loaded).
    pub allow_resource_packs: bool,
    pub anti_afk: AntiAfkOptions,
    pub auto_relog: AutoRelogOptions,
    pub auto_eat: AutoEatOptions,
    pub auto_respond: AutoRespondOptions,
    pub chat_log: ChatLogOptions,
    pub alerts: AlertsOptions,
    pub scheduler: SchedulerOptions,
    pub login_commands: LoginCommandsOptions,
}

impl AfkAccount {
    /// A new account with a fresh id and legacy defaults.
    pub fn new(nickname: impl Into<String>) -> Self {
        Self {
            id: new_account_id(),
            nickname: nickname.into(),
            ..Self::default()
        }
    }

    /// `AfkAccount.EffectiveHost`.
    pub fn effective_host(&self) -> &str {
        let host = self.server_host.trim();
        if host.is_empty() {
            DEFAULT_SERVER_HOST
        } else {
            host
        }
    }
}

/// Periodic movement/command that keeps the server from flagging the account as AFK.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AntiAfkOptions {
    pub enabled: bool,
    /// Seconds; MCC picks a random delay in `[delay_min, delay_max]`.
    pub delay_min: f64,
    pub delay_max: f64,
    /// Sent every round; empty means "move only".
    pub command: String,
    pub use_sneak: bool,
    /// Random walking; forces MCC terrain handling on (see the config writer).
    pub use_terrain_handling: bool,
    pub walk_range: i32,
    pub walk_retries: i32,
}

impl Default for AntiAfkOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            delay_min: 60.0,
            delay_max: 90.0,
            command: String::new(),
            use_sneak: true,
            use_terrain_handling: false,
            walk_range: 5,
            walk_retries: 20,
        }
    }
}

/// Reconnect after a disconnect. With the launcher bridge MCC exits on every failure, so these
/// settings drive the launcher-side restart loop (delays, retry limit and kick-message filter; see
/// [`crate::RestartSchedule`]).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoRelogOptions {
    pub enabled: bool,
    /// Seconds before the first restart; later restarts wait longer, up to `delay_max`.
    pub delay_min: f64,
    pub delay_max: f64,
    /// -1 means unlimited.
    pub retries: i32,
    /// True: reconnect whatever the kick message says.
    pub ignore_kick_message: bool,
    pub kick_messages: Vec<String>,
}

/// Kick messages of a new account in `lang`: what servers say when a connection drops, they
/// restart or they are full. MCC writes the servers' translated texts in the launcher's language
/// ([`crate::build_config_in`]), so a Turkish player's list is Turkish. The English list is the
/// legacy one, kept word for word: stored lists are recognised by it ([`is_default_kick_messages`]).
pub fn default_kick_messages_in(lang: Lang) -> Vec<String> {
    let lines: &[&str] = match lang {
        Lang::Tr => &[
            "Bağlantı koptu",
            "Sunucu yeniden başlatılıyor",
            "Sunucu dolu",
            "Çok fazla oyuncu",
        ],
        Lang::En => &[
            "Connection has been lost",
            "Server is restarting",
            "Server is full",
            "Too Many people",
        ],
    };
    lines.iter().copied().map(str::to_owned).collect()
}

/// Kick messages of a new account, in the launcher's language ([`Lang::current`]).
pub fn default_kick_messages() -> Vec<String> {
    default_kick_messages_in(Lang::current())
}

/// A list nobody edited: the default list of either language (spaces around the lines aside).
pub fn is_default_kick_messages(list: &[String]) -> bool {
    let lines: Vec<&str> = list
        .iter()
        .map(|line| line.trim())
        .filter(|line| !line.is_empty())
        .collect();
    [Lang::Tr, Lang::En]
        .into_iter()
        .any(|lang| default_kick_messages_in(lang) == lines)
}

/// The list as the player reads it in `lang`: an untouched default list is that language's, so it
/// follows a language change; a list the player wrote stays as written.
pub fn kick_messages_in(list: &[String], lang: Lang) -> Vec<String> {
    if is_default_kick_messages(list) {
        default_kick_messages_in(lang)
    } else {
        list.to_vec()
    }
}

impl Default for AutoRelogOptions {
    fn default() -> Self {
        Self {
            enabled: true,
            delay_min: 5.0,
            delay_max: 10.0,
            retries: -1,
            ignore_kick_message: true,
            kick_messages: default_kick_messages(),
        }
    }
}

/// Eat when the food bar drops below the threshold; forces MCC inventory handling on.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoEatOptions {
    pub enabled: bool,
    /// Food bar value, 0-20.
    pub threshold: i32,
}

impl Default for AutoEatOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            threshold: 6,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoRespondOptions {
    pub enabled: bool,
    pub match_colors: bool,
    /// Play a notification sound when the account answers someone.
    pub sound_enabled: bool,
    pub matches: Vec<AutoRespondMatch>,
}

/// One `[Match]` block of `matches.ini`. Actions are raw MCC actions (`send /home`, `log hi`).
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AutoRespondMatch {
    pub pattern: String,
    /// The pattern is a regular expression (`regex=` instead of `match=`).
    pub is_regex: bool,
    pub action: String,
    /// Used instead of `action` when the trigger came as a private message.
    pub private_action: String,
    /// Used for messages that do not come from a player (server announcements).
    pub other_action: String,
    pub owners_only: bool,
    pub cooldown_seconds: i32,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ChatLogOptions {
    pub enabled: bool,
    pub add_date_time: bool,
    /// Show the date next to the time in the live console (does not affect the file).
    pub show_date: bool,
    /// MCC values: `all`, `messages`, `chat`.
    pub filter: String,
}

impl Default for ChatLogOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            add_date_time: true,
            show_date: false,
            filter: "messages".to_owned(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AlertsOptions {
    pub enabled: bool,
    pub beep_enabled: bool,
    pub trigger_by_words: bool,
    pub log_to_file: bool,
    pub matches: Vec<String>,
    pub excludes: Vec<String>,
}

impl Default for AlertsOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            beep_enabled: false,
            trigger_by_words: true,
            log_to_file: true,
            matches: Vec::new(),
            excludes: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct SchedulerOptions {
    pub enabled: bool,
    pub tasks: Vec<ScheduledTask>,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ScheduledTask {
    pub name: String,
    /// Raw MCC action: `send /kit`, `log hello`, `script file.cs`.
    pub action: String,
    pub on_login: bool,
    pub on_first_login: bool,
    pub interval_enabled: bool,
    /// Seconds; MCC waits a random time in `[min_seconds, max_seconds]` between runs.
    pub min_seconds: f64,
    pub max_seconds: f64,
}

impl Default for ScheduledTask {
    fn default() -> Self {
        Self {
            name: DEFAULT_TASK_NAME.to_owned(),
            action: String::new(),
            on_login: false,
            on_first_login: false,
            interval_enabled: true,
            min_seconds: 300.0,
            max_seconds: 600.0,
        }
    }
}

/// Account-independent AFK preferences (`settings.json`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct AfkSettings {
    /// Show login-script lines in the console. Lines with passwords stay hidden (and are masked
    /// at the source) either way.
    pub show_script_commands: bool,
    /// Studio's console switches as 7/24 Bulut's offer last said them (console looks, dialog
    /// cards): an account that connects before the offer is read again uses these. Not the UI's.
    #[serde(skip)]
    pub console_looks: bool,
    #[serde(skip)]
    pub dialog_cards: bool,
}

/// Lines typed after joining. The command list itself is a secret kept in the OS keychain.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct LoginCommandsOptions {
    pub enabled: bool,
    /// True: first login of a client process only; false: every login, reconnects included.
    pub only_first_login: bool,
    /// Wait before the first command and between commands (anti-spam).
    pub delay_seconds: f64,
}

impl Default for LoginCommandsOptions {
    fn default() -> Self {
        Self {
            enabled: false,
            only_first_login: false,
            delay_seconds: 2.0,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn new_accounts_follow_the_recommended_version() {
        assert_eq!(AfkAccount::new("Test").minecraft_version, "");
    }

    #[test]
    fn nickname_rule() {
        assert!(is_valid_nickname("Afk_01"));
        assert!(!is_valid_nickname("ab"));
        assert!(!is_valid_nickname("seventeen_chars_x"));
        assert!(!is_valid_nickname("Çılgın"));
        assert!(!is_valid_nickname("with space"));
    }

    #[test]
    fn ids_are_guid_n_format_and_validated() {
        let id = new_account_id();
        assert_eq!(id.len(), 32);
        assert!(is_safe_account_id(&id));
        assert!(!is_safe_account_id("../evil"));
        assert!(!is_safe_account_id(""));
    }

    #[test]
    fn default_kick_messages_follow_the_language() {
        let turkish = default_kick_messages_in(Lang::Tr);
        let english = default_kick_messages_in(Lang::En);
        assert_eq!(turkish.len(), english.len());
        assert!(turkish.contains(&"Sunucu yeniden başlatılıyor".to_owned()));
        // The legacy list, word for word (stored accounts carry it).
        assert_eq!(
            english,
            [
                "Connection has been lost",
                "Server is restarting",
                "Server is full",
                "Too Many people"
            ]
        );
        assert!(is_default_kick_messages(&turkish));
        assert!(is_default_kick_messages(&english));
        // Spaces and an empty line typed by the form do not make it the player's own list.
        let spaced: Vec<String> = english
            .iter()
            .map(|line| format!(" {line} "))
            .chain([String::new()])
            .collect();
        assert!(is_default_kick_messages(&spaced));
    }

    #[test]
    fn an_untouched_list_reads_in_the_players_language() {
        let english = default_kick_messages_in(Lang::En);
        assert_eq!(
            kick_messages_in(&english, Lang::Tr),
            default_kick_messages_in(Lang::Tr)
        );
        assert_eq!(
            kick_messages_in(&default_kick_messages_in(Lang::Tr), Lang::En),
            english
        );
        // One line changed, one removed or added: the player's list, as written.
        let mut own = english.clone();
        own[1] = "Restart".into();
        assert_eq!(kick_messages_in(&own, Lang::Tr), own);
        let shorter = english[..3].to_vec();
        assert_eq!(kick_messages_in(&shorter, Lang::Tr), shorter);
        assert!(kick_messages_in(&[], Lang::Tr).is_empty());
    }

    #[test]
    fn effective_host_falls_back_to_default() {
        let mut account = AfkAccount::new("Test");
        assert_eq!(account.effective_host(), DEFAULT_SERVER_HOST);
        account.server_host = "  mc.example.org ".into();
        assert_eq!(account.effective_host(), "mc.example.org");
    }
}
