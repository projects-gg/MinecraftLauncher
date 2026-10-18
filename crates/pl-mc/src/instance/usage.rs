//! The room an instance's folder takes on the disk ("Kapladığı yer" on the instance overview).
//!
//! Everything under `instances/<id>/` counts, recursively: every version set (the active one and
//! the parked ones), worlds and their backups, screenshots, resource and shader packs, mod
//! settings, the game's logs and crash reports. The game files all instances share (Minecraft
//! versions, libraries, assets and Java runtimes in the data folder) live outside it and are
//! never counted: they are downloaded once for every instance, so adding them to each one would
//! count the same gigabytes again on every instance, and the instances' sizes would add up to
//! more than the disk holds (the cleanup in Settings → Veri accounts for them). Links inside the
//! folder (a `saves` junction to another disk) are not followed: their target lives elsewhere and
//! may be shared. A file with several hard links counts once where the listing tells (Unix);
//! the launcher itself never hard-links into instance folders.
//!
//! Only directory listings are read (no file is opened), off the async threads, and the walk
//! stops as soon as it is cancelled. [`stamp`] is a cheap look at the folder's top levels for
//! deciding whether a measurement is still good.

use std::hash::{DefaultHasher, Hash, Hasher};
use std::path::Path;
use std::sync::atomic::{AtomicBool, Ordering};

use crate::error::{Error, Result};

/// What [`measure`] found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DiskUsage {
    pub bytes: u64,
    pub files: u64,
}

/// How often the walk looks at its cancel flag (entries).
const CANCEL_EVERY: u64 = 256;

/// Measures the folder at `dir` (see the module docs). An unreadable entry is skipped; a missing
/// folder takes no room. Returns [`Error::Cancelled`] once `cancel` is set.
pub fn measure(dir: &Path, cancel: &AtomicBool) -> Result<DiskUsage> {
    let mut usage = DiskUsage::default();
    #[cfg(unix)]
    let mut linked = std::collections::HashSet::new();
    let mut seen: u64 = 0;
    // The folder itself is followed when it is a link (an instance moved elsewhere by hand);
    // links below it are not.
    for entry in walkdir::WalkDir::new(dir).follow_links(false) {
        seen += 1;
        if seen.is_multiple_of(CANCEL_EVERY) && cancel.load(Ordering::Relaxed) {
            return Err(Error::Cancelled);
        }
        let Ok(entry) = entry else { continue };
        if !entry.file_type().is_file() {
            continue;
        }
        let Ok(meta) = entry.metadata() else { continue };
        #[cfg(unix)]
        {
            use std::os::unix::fs::MetadataExt;
            if meta.nlink() > 1 && !linked.insert((meta.dev(), meta.ino())) {
                continue;
            }
        }
        usage.bytes = usage.bytes.saturating_add(meta.len());
        usage.files += 1;
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(Error::Cancelled);
    }
    Ok(usage)
}

/// A fingerprint of the entries in the top three levels of `dir` (names, kinds, sizes and change
/// times), read from at most two levels of directory listings. Anything the launcher or the game
/// does to an instance changes it: an added or removed mod, pack, screenshot or backup, a world
/// that was played (the game rewrites its `level.dat`, which touches the world's folder), a
/// session's log, a version switch. A change deeper down alone (a file edited inside a mod's
/// settings folder) does not; the next change above it brings it along.
pub fn stamp(dir: &Path) -> u64 {
    let mut hasher = DefaultHasher::new();
    hash_level(dir, Path::new(""), 0, &mut hasher);
    hasher.finish()
}

fn hash_level(dir: &Path, relative: &Path, depth: u8, hasher: &mut DefaultHasher) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    let mut entries: Vec<_> = entries.filter_map(|entry| entry.ok()).collect();
    entries.sort_by_key(|entry| entry.file_name());
    for entry in entries {
        let name = relative.join(entry.file_name());
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        // A folder's own change time: the copy a listing shows (NTFS keeps it in the parent's
        // index) catches up with a new entry only later, so the same folder could look changed
        // twice. Files are fine from the listing once they are closed.
        let meta = if kind.is_dir() {
            std::fs::metadata(entry.path()).ok()
        } else {
            entry.metadata().ok()
        };
        name.hash(hasher);
        (kind.is_dir(), kind.is_symlink()).hash(hasher);
        if let Some(meta) = &meta {
            meta.len().hash(hasher);
            meta.modified().ok().hash(hasher);
        }
        if kind.is_dir() && depth < 2 {
            hash_level(&entry.path(), &name, depth + 1, hasher);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(path: &Path, bytes: usize) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(path, vec![7_u8; bytes]).unwrap();
    }

    #[test]
    fn counts_every_file_below_the_folder() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("instance.json"), 100);
        write(&root.join("minecraft/mods/sodium.jar"), 2_000);
        write(&root.join("minecraft/saves/Dünya/region/r.0.0.mca"), 30_000);
        write(
            &root.join("minecraft/.launcher/sets/1.21.1/mods/old.jar"),
            4_000,
        );
        std::fs::create_dir_all(root.join("minecraft/empty")).unwrap();
        let usage = measure(root, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            usage,
            DiskUsage {
                bytes: 36_100,
                files: 4
            }
        );
        let missing = measure(&root.join("gone"), &AtomicBool::new(false)).unwrap();
        assert_eq!(missing, DiskUsage::default());
    }

    #[test]
    fn stops_when_cancelled() {
        let dir = tempfile::tempdir().unwrap();
        for i in 0..(CANCEL_EVERY + 10) {
            write(&dir.path().join(format!("config/{i}.json")), 1);
        }
        let cancelled = AtomicBool::new(true);
        assert!(matches!(
            measure(dir.path(), &cancelled),
            Err(Error::Cancelled)
        ));
        // A small folder that finishes before the first look still says it was cancelled.
        let small = tempfile::tempdir().unwrap();
        assert!(matches!(
            measure(small.path(), &cancelled),
            Err(Error::Cancelled)
        ));
    }

    #[cfg(unix)]
    #[test]
    fn links_are_not_followed_and_hard_links_count_once() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        write(&elsewhere.path().join("big.mca"), 50_000);
        let root = dir.path().join("instance");
        write(&root.join("minecraft/mods/a.jar"), 1_000);
        std::os::unix::fs::symlink(elsewhere.path(), root.join("minecraft/saves")).unwrap();
        std::fs::hard_link(
            root.join("minecraft/mods/a.jar"),
            root.join("minecraft/mods/a-copy.jar"),
        )
        .unwrap();
        let usage = measure(&root, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            usage,
            DiskUsage {
                bytes: 1_000,
                files: 1
            }
        );
    }

    #[cfg(windows)]
    #[test]
    fn junctions_are_not_followed() {
        let dir = tempfile::tempdir().unwrap();
        let elsewhere = tempfile::tempdir().unwrap();
        write(&elsewhere.path().join("big.mca"), 50_000);
        let root = dir.path().join("instance");
        write(&root.join("minecraft/mods/a.jar"), 1_000);
        // Backslashes only: cmd reads "/saves" in a mixed path as a switch.
        let link = root.join("minecraft").join("saves");
        let made = std::process::Command::new("cmd")
            .args(["/C", "mklink", "/J"])
            .arg(&link)
            .arg(elsewhere.path())
            .output()
            .unwrap();
        assert!(made.status.success(), "{made:?}");
        let usage = measure(&root, &AtomicBool::new(false)).unwrap();
        assert_eq!(
            usage,
            DiskUsage {
                bytes: 1_000,
                files: 1
            }
        );
    }

    #[test]
    fn the_stamp_follows_what_the_game_and_the_launcher_change() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        write(&root.join("instance.json"), 10);
        write(&root.join("minecraft/mods/a.jar"), 10);
        write(&root.join("minecraft/saves/World/level.dat"), 10);
        write(&root.join("minecraft/config/mod/deep.json"), 10);
        let first = stamp(root);
        assert_eq!(stamp(root), first, "nothing changed");
        // A new mod.
        write(&root.join("minecraft/mods/b.jar"), 10);
        let second = stamp(root);
        assert_ne!(second, first);
        // A world that was played: the game rewrites level.dat through a new file (a moment
        // later, so the folder's change time can differ on any file system).
        std::thread::sleep(std::time::Duration::from_millis(50));
        std::fs::rename(
            root.join("minecraft/saves/World/level.dat"),
            root.join("minecraft/saves/World/level.dat_old"),
        )
        .unwrap();
        let third = stamp(root);
        assert_ne!(third, second);
        // A file deep in a settings folder alone is not looked at.
        write(&root.join("minecraft/config/mod/deep.json"), 20);
        assert_eq!(stamp(root), third);
    }
}
