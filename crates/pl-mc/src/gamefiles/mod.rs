//! Files of a game directory: resource packs, shader packs, worlds (with backups) and
//! screenshots, plus safe import of files into the right folder and trash-first removal.
//!
//! Everything takes the game directory (or the item's own path) explicitly.

pub(crate) mod fsops;
mod packs;
mod screenshots;
mod worlds;

pub use fsops::{ImportKind, RemovalMethod, dir_size, import_file, import_into, remove_path};
pub use packs::{ResourcePackInfo, ShaderPackInfo, list_resource_packs, list_shader_packs};
pub use screenshots::{ScreenshotInfo, list_screenshots};
pub use worlds::{
    BackupProgress, BackupProgressFn, BackupSummary, WorldGameMode, WorldInfo, backup_world,
    default_backup_path, delete_world, list_worlds, read_world,
};
