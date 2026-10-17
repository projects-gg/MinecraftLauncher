//! AFK accounts powered by the Projects Console Client (a .NET fork of Minecraft Console Client).
//!
//! Offline-mode accounts are kept online by headless client processes, one per account:
//!
//! - [`AccountStore`]: `accounts.json` / `settings.json` in the legacy shape, atomic writes,
//!   corrupt-file backup, login commands in the OS keychain ([`SecretStore`]) with transparent
//!   migration of legacy plain-text commands.
//! - [`AccountSettings`]: the settings page with the legacy validation rules; [`SettingsPage`] is
//!   the same page for the UI, with login-command secrets left in the keychain.
//! - [`VersionCatalog`]: the game version picker, from the content index or the built-in table.
//! - [`check_pattern`]: which auto-respond patterns the client can use safely.
//! - [`build_config`]: `MinecraftClient.ini`, `matches.ini` and `giris-komutlari.txt`, byte-for-byte
//!   identical to the legacy writer (golden-tested).
//! - [`AfkRuntime`]: verified, side-by-side client installs with offline fallback.
//! - [`AfkManager`]: session actors (bridge events, masked console, `/quit` + kill, restart loop
//!   honoring the AutoRelog delays, retry limit and kick filter), staggered connect-all/auto-start.
//!
//! The client contract (`@@MCC@@` events, exit codes) is documented in [`bridge`].

pub mod bridge;
mod config;
mod console;
mod error;
mod form;
mod housekeeping;
pub mod legacy;
mod manager;
mod model;
mod orphans;
mod page;
mod paths;
mod pattern;
mod process;
mod restart;
mod runtime;
mod secrets;
mod session;
mod store;
mod util;
mod versions;

pub use config::{
    ConfigFiles, INI_FILE_NAME, LOGIN_SCRIPT_FILE_NAME, MATCHES_FILE_NAME, build_config,
    has_login_commands, write_config,
};
pub use console::{
    CONSOLE_CAPACITY, ConsoleLine, ConsoleLineKind, ConsoleOrigin, INPUT_PREFIX, LineFilter, MASK,
    STDERR_PREFIX, is_sensitive_command, sensitive_command_names,
};
pub use error::{AfkError, Result};
pub use form::{
    AccountSettings, AutoRespondRule, AutoRespondSettings, ChatLogFilter, ChatLogSettings,
    LoginCommandsSettings, MAX_COOLDOWN_SECONDS, MAX_RELOG_DELAY_SECONDS, MAX_TASK_SECONDS,
    MIN_RELOG_DELAY_SECONDS, MIN_TASK_SECONDS, ValidationIssue, action_to_display,
    display_to_action, is_valid_server_host, split_host_port, split_lines,
};
pub use housekeeping::DEFAULT_LOG_CAP_BYTES;
pub use manager::{AfkManager, BulkOutcome, ManagerConfig, ManagerOptions, SavedSettings};
pub use model::{
    AUTO_MINECRAFT_VERSION, AfkAccount, AfkSettings, AlertsOptions, AntiAfkOptions, AutoEatOptions,
    AutoRelogOptions, AutoRespondMatch, AutoRespondOptions, ChatLogOptions,
    DEFAULT_MINECRAFT_VERSION, DEFAULT_SERVER_HOST, DEFAULT_TASK_NAME, LoginCommandsOptions,
    ScheduledTask, SchedulerOptions, VersionOption, default_kick_messages, is_valid_nickname,
    new_account_id,
};
pub use orphans::RECORD_FILE_NAME;
pub use page::{LoginCommandLine, LoginCommandsPage, SettingsPage, secret_command_name};
pub use paths::{ALERTS_FILE_NAME, AfkPaths, CHAT_LOG_FILE_NAME, CONSOLE_LOG_FILE_NAME};
pub use pattern::{MAX_PATTERN_CHARS, PatternProblem, check_pattern};
pub use process::{
    BoxedReader, BoxedWriter, CONSOLE_MODE_ARG, ClientProcess, ClientSpawner, ExitFuture,
    LaunchSpec, ProcessExit, ProcessSpawner,
};
pub use restart::{FailureReason, KickCause, REPEATED_KICKS, RestartSchedule};
pub use runtime::{AfkClientBuild, AfkClientRelease, AfkRuntime, CLIENT_EXE_NAME, ClientExe};
pub use secrets::{
    KEYRING_SERVICE, KeyringSecretStore, MemorySecretStore, SecretStore, login_commands_key,
};
pub use session::{AfkEvent, AfkEventKind, EventSink, SessionSnapshot, SessionState, SessionStats};
pub use store::{AccountStore, LoadReport};
pub use versions::{MAX_LISTED_VERSIONS, ResolvedVersion, VersionCatalog, VersionEntry};
