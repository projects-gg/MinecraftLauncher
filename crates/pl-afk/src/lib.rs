//! AFK accounts powered by the Projects Console Client (a .NET fork of Minecraft Console Client).
//!
//! Offline-mode accounts are kept online by headless client processes, one per account:
//!
//! - [`AccountStore`]: `accounts.json` / `settings.json` in the legacy shape, atomic writes,
//!   corrupt-file backup, login commands in the OS keychain ([`SecretStore`]) with transparent
//!   migration of legacy plain-text commands.
//! - [`AccountSettings`]: the settings page with the legacy validation rules.
//! - [`build_config`]: `MinecraftClient.ini`, `matches.ini` and `giris-komutlari.txt`, byte-for-byte
//!   identical to the legacy writer (golden-tested).
//! - [`AfkRuntime`]: verified, side-by-side client installs with offline fallback.
//! - [`AfkManager`]: session actors (bridge events, masked console, `/quit` + kill, restart loop
//!   honoring the AutoRelog retry limit and kick filter), staggered connect-all/auto-start.
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
mod paths;
mod process;
mod restart;
mod runtime;
mod secrets;
mod session;
mod store;
mod util;

pub use config::{
    ConfigFiles, INI_FILE_NAME, LOGIN_SCRIPT_FILE_NAME, MATCHES_FILE_NAME, build_config,
    has_login_commands, write_config,
};
pub use console::{
    CONSOLE_CAPACITY, ConsoleLine, ConsoleLineKind, ConsoleOrigin, INPUT_PREFIX, LineFilter, MASK,
    STDERR_PREFIX, is_sensitive_command,
};
pub use error::{AfkError, Result};
pub use form::{
    AccountSettings, AutoRespondRule, AutoRespondSettings, ChatLogFilter, ChatLogSettings,
    LoginCommandsSettings, ValidationIssue, action_to_display, display_to_action, split_lines,
};
pub use housekeeping::DEFAULT_LOG_CAP_BYTES;
pub use manager::{AfkManager, BulkOutcome, ManagerConfig, ManagerOptions, SavedSettings};
pub use model::{
    AUTO_MINECRAFT_VERSION, AfkAccount, AfkSettings, AlertsOptions, AntiAfkOptions, AutoEatOptions,
    AutoRelogOptions, AutoRespondMatch, AutoRespondOptions, ChatLogOptions,
    DEFAULT_MINECRAFT_VERSION, DEFAULT_SERVER_HOST, DEFAULT_TASK_NAME, LoginCommandsOptions,
    ScheduledTask, SchedulerOptions, VersionOption, default_kick_messages, is_valid_nickname,
    minecraft_version_index, minecraft_versions, new_account_id, normalize_minecraft_version,
};
pub use paths::{ALERTS_FILE_NAME, AfkPaths, CHAT_LOG_FILE_NAME, CONSOLE_LOG_FILE_NAME};
pub use process::{
    BoxedReader, BoxedWriter, CONSOLE_MODE_ARG, ClientProcess, ClientSpawner, ExitFuture,
    LaunchSpec, ProcessExit, ProcessSpawner,
};
pub use restart::{FailureReason, RestartSchedule};
pub use runtime::{AfkClientBuild, AfkClientRelease, AfkRuntime, CLIENT_EXE_NAME, ClientExe};
pub use secrets::{
    KEYRING_SERVICE, KeyringSecretStore, MemorySecretStore, SecretStore, login_commands_key,
};
pub use session::{AfkEvent, AfkEventKind, EventSink, SessionSnapshot, SessionState, SessionStats};
pub use store::{AccountStore, LoadReport};
