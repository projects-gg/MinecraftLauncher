//! Launching: who plays ([`identity`]), the command line ([`command`]), the running process
//! ([`process`]) and when its window is on screen ([`window`]), output parsing ([`log`]), crash
//! analysis ([`crash`]), where "Devam et" joins ([`resume`]) and how it enters a world
//! ([`world_entry`]), and a game the launcher left running when it quit ([`detached`]).
//!
//! Typical flow: `install::install_version` → [`build_launch_command`] → [`GameProcess::spawn`].

pub mod command;
pub mod crash;
pub mod detached;
pub mod identity;
pub mod log;
pub mod process;
pub mod resume;
pub mod window;
pub mod world_entry;

pub use command::{
    LAUNCHER_NAME, LAUNCHER_VERSION, LaunchCommand, LaunchOptions, QuickPlay, WindowOptions,
    build_launch_command, java_argfile, legacy_jvm_arguments, pick_quick_play, split_args,
    substitute,
};
pub use crash::{
    CrashDiagnosis, CrashInput, CrashKind, SHORT_RUN, SuggestedAction, analyze_crash,
    find_crash_report, find_jvm_error_log, is_failure,
};
pub use detached::{DETACHED_FILE, DetachedGame};
pub use identity::{GameAccount, LaunchIdentity, UserType, is_valid_offline_name, offline_uuid};
pub use log::{Log4jParser, LogLevel, LogLine, guess_level, parse_event, parse_plain_line};
pub use process::{
    GameEvent, GameEventFn, GameExit, GameProcess, LOADING_GRACE, MAX_LOG_BYTES, MAX_TAIL_LINES,
    SpawnOptions,
};
pub use resume::{ResumeTarget, quick_play_log_path, resume_target, supports_quick_play};
pub use window::{GIVE_UP_AFTER, LogMarker, WindowProbe, log_marker};
pub use world_entry::{WorldEntry, installed_breakers, quick_play_breakers, world_entry};
