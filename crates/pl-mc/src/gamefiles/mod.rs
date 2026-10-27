//! Files of a game directory: resource packs, shader packs, worlds (with backups) and
//! screenshots (with cached thumbnails), plus safe import of files into the right folder and
//! trash-first removal. Also the game logs (captured by the launcher and the game's own) and the
//! launcher's own trash folder.
//!
//! Everything takes the game directory (or the item's own path) explicitly.

mod backups;
mod detect;
pub(crate) mod fsops;
mod logs;
mod packs;
mod screenshots;
mod shot_watch;
mod thumbs;
mod trash;
mod worlds;

pub use backups::{
    RestoreMode, RestoredWorld, WorldBackupInfo, list_world_backups, restore_world_backup,
};
pub use detect::{
    DroppedFile, DroppedKind, classify_files, classify_path, import_dropped, import_name,
};
pub use fsops::{
    ImportKind, RemovalMethod, dir_size, import_file, import_file_named, import_into, remove_path,
};
pub use logs::{
    GameLogContent, GameLogFile, LogSource, MAX_ARCHIVE_INFLATE_BYTES, MAX_READ_BYTES, MCLOGS_API,
    MCLOGS_MAX_BYTES, MCLOGS_MAX_LINES, SharedLog, game_log_id, is_continuation, is_game_log_name,
    list_game_folder_logs, list_game_logs, mask_secrets, parse_log_text, read_game_log,
    read_log_for_upload, shorten_for_upload, upload_to_mclogs,
};
pub use packs::{
    ResourcePackInfo, ShaderPackInfo, datapacks_dir, list_datapacks, list_resource_packs,
    list_shader_packs,
};
pub use screenshots::{AnnouncedShots, ScreenshotInfo, ScreenshotSnapshot, list_screenshots};
pub use shot_watch::{
    ScreenshotEvent, ScreenshotWatcher, Timing as ScreenshotWatchTiming, ends_like_png,
    is_screenshot_name,
};
pub use thumbs::{
    THUMB_SIZE, ensure_thumbnail, existing_thumbnail, make_thumbnail, prune_thumbnails,
    thumbnail_name, thumbnails_dir,
};
pub use trash::{
    INCOMPLETE_SLOT_AGE, LauncherTrash, PurgeReport, TRASH_RETENTION, TrashEntry, TrashKind,
    TrashLimits, TrashUsage,
};
pub(crate) use worlds::world_summary;
pub use worlds::{
    BackupProgress, BackupProgressFn, BackupReason, BackupSummary, WorldGameMode, WorldInfo,
    backup_world, backup_world_marked, default_backup_path, delete_world, list_worlds, read_world,
};
