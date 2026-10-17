//! Launching: who plays ([`identity`]), the command line ([`command`]), the running process
//! ([`process`]), output parsing ([`log`]), crash analysis ([`crash`]) and where "Devam et"
//! joins ([`resume`]).
//!
//! Typical flow: `install::install_version` → [`build_launch_command`] → [`GameProcess::spawn`].

pub mod command;
pub mod crash;
pub mod identity;
pub mod log;
pub mod process;
pub mod resume;

pub use command::{
    LAUNCHER_NAME, LAUNCHER_VERSION, LaunchCommand, LaunchOptions, QuickPlay, WindowOptions,
    build_launch_command, java_argfile, legacy_jvm_arguments, split_args, substitute,
};
pub use crash::{
    CrashDiagnosis, CrashInput, CrashKind, SHORT_RUN, SuggestedAction, analyze_crash,
    find_crash_report, find_jvm_error_log, is_failure,
};
pub use identity::{LaunchIdentity, UserType, is_valid_offline_name, offline_uuid};
pub use log::{Log4jParser, LogLevel, LogLine, guess_level, parse_event, parse_plain_line};
pub use process::{
    GameEvent, GameEventFn, GameExit, GameProcess, MAX_LOG_BYTES, MAX_TAIL_LINES, SpawnOptions,
    WINDOW_FALLBACK, looks_like_window,
};
pub use resume::{ResumeTarget, quick_play_log_path, resume_target, supports_quick_play};
