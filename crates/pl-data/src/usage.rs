//! How much space the data folder's caches and logs take, and free space on its disk. What can
//! be cleaned, and when, is decided in [`crate::housekeeping`].

use std::path::Path;

use walkdir::WalkDir;

/// `cache/` under the data root.
pub const CACHE_DIR: &str = "cache";
/// `logs/` under the data root (launcher logs and captured game logs).
pub const LOGS_DIR: &str = "logs";

/// Bytes used by every file under `path` (0 when it does not exist). Links are not followed.
pub fn dir_size(path: &Path) -> u64 {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
        .filter_map(|entry| entry.metadata().ok())
        .map(|meta| meta.len())
        .sum()
}

/// Free space on the disk that holds `path`. `path` may not exist yet: its nearest existing
/// parent decides.
pub fn free_space(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|p| p.exists())?;
    fs4::available_space(existing).ok()
}

/// Size of the disk that holds `path` (its nearest existing parent decides).
pub fn disk_size(path: &Path) -> Option<u64> {
    let existing = path.ancestors().find(|p| p.exists())?;
    fs4::total_space(existing).ok()
}

/// Space used by the data folder's caches and logs, and free space on its disk.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Usage {
    pub cache_bytes: u64,
    pub logs_bytes: u64,
    pub free_bytes: Option<u64>,
}

pub fn usage(root: &Path) -> Usage {
    Usage {
        cache_bytes: dir_size(&root.join(CACHE_DIR)),
        logs_bytes: dir_size(&root.join(LOGS_DIR)),
        free_bytes: free_space(root),
    }
}

#[cfg(test)]
mod tests {
    use std::fs;

    use super::*;

    fn write(path: &Path, bytes: usize) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, vec![b'x'; bytes]).unwrap();
    }

    #[test]
    fn sizes_cache_and_logs() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("cache/content/docs/a.json"), 100);
        write(&root.join("cache/modrinth/icons/x.png"), 50);
        write(&root.join("logs/launcher.2026-10-15.log"), 7);
        write(&root.join("instances/a/minecraft/mods/m.jar"), 1000);
        let usage = usage(root);
        assert_eq!(usage.cache_bytes, 150);
        assert_eq!(usage.logs_bytes, 7);
        assert!(usage.free_bytes.is_some());
        assert_eq!(dir_size(&root.join("missing")), 0);
        assert!(free_space(&root.join("not/yet/here")).is_some());
        let disk = disk_size(&root.join("not/yet/here")).unwrap();
        assert!(disk >= usage.free_bytes.unwrap());
    }
}
