//! Finding other launchers' data. Every detector is cheap: it checks folders, lists their
//! entries and reads small settings files, and the launchers' own lists of their instances (the
//! Modrinth App's database, CurseForge's instance list). Nothing big is opened (worlds, mod
//! jars), nothing is written.

mod curseforge;
mod folder;
mod modrinth;
mod modrinth_db;
mod multimc;

use std::path::Path;
use std::time::SystemTime;

use crate::journal::ImportedFrom;
use crate::locate::Locations;
use crate::model::{ImportProfile, ImportSource};

/// Id of the `.minecraft`-style source in Mojang's place.
pub(crate) const MINECRAFT_ID: &str = "minecraft";
/// Id of a TLauncher game folder somewhere else (its settings name it).
pub(crate) const TLAUNCHER_FOLDER_ID: &str = "tlauncher-folder";

/// Every source with something to bring: the `.minecraft`-style folders first, then the
/// instance launchers, most recently used first. `imported` (from
/// [`crate::imported_profiles`]) marks profiles an earlier import already made into instances.
pub fn scan(locations: &Locations, imported: &[ImportedFrom]) -> Vec<ImportSource> {
    let mut folders = folder::detect(locations, imported);
    let mut others = Vec::new();
    others.extend(curseforge::detect(locations, imported));
    others.extend(modrinth::detect(locations, imported));
    others.extend(multimc::detect(locations, imported));
    others.sort_by_key(|s| std::cmp::Reverse(s.last_used_ms));
    folders.extend(others);
    folders.retain(has_anything);
    folders
}

/// The one source with this id, read again (what [`scan`] would return for it).
pub fn scan_source(
    locations: &Locations,
    id: &str,
    imported: &[ImportedFrom],
) -> Option<ImportSource> {
    scan(locations, imported).into_iter().find(|s| s.id == id)
}

fn has_anything(source: &ImportSource) -> bool {
    let shared = source.shared.as_ref().is_some_and(|s| {
        s.options
            || s.servers > 0
            || s.worlds > 0
            || s.resource_packs > 0
            || s.shader_packs > 0
            || s.screenshots > 0
    });
    shared || !source.profiles.is_empty()
}

/// Milliseconds since 1970 of a file's modification time.
pub(crate) fn modified_ms(path: &Path) -> Option<i64> {
    let modified = std::fs::metadata(path).ok()?.modified().ok()?;
    let since = modified.duration_since(SystemTime::UNIX_EPOCH).ok()?;
    i64::try_from(since.as_millis()).ok()
}

/// Milliseconds since 1970 of an RFC 3339 time (`2024-05-01T12:00:00.000Z`); `None` for the
/// "never" placeholders launchers write (`0001-01-01T00:00:00`).
pub(crate) fn timestamp_ms(text: &str) -> Option<i64> {
    let ts: jiff::Timestamp = text.trim().parse().ok()?;
    let ms = ts.as_millisecond();
    (ms > 0).then_some(ms)
}

/// Reads a small text file whole; `None` when it is missing, unreadable or larger than `max`.
pub(crate) fn read_small(path: &Path, max: u64) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    if !meta.is_file() || meta.len() > max {
        return None;
    }
    let bytes = std::fs::read(path).ok()?;
    Some(String::from_utf8_lossy(&bytes).into_owned())
}

/// Files in `dir` whose names `keep` accepts (a missing folder has none).
pub(crate) fn count_entries(dir: &Path, keep: impl Fn(&std::fs::DirEntry) -> bool) -> u32 {
    std::fs::read_dir(dir)
        .map(|entries| entries.flatten().filter(|e| keep(e)).count() as u32)
        .unwrap_or(0)
}

/// A pack: a zip file or an unpacked folder.
pub(crate) fn is_pack_entry(entry: &std::fs::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
    if name.starts_with('.') {
        return false;
    }
    match entry.file_type() {
        Ok(t) if t.is_dir() => true,
        Ok(t) if t.is_file() => name.ends_with(".zip"),
        _ => false,
    }
}

pub(crate) fn is_screenshot_entry(entry: &std::fs::DirEntry) -> bool {
    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
    entry.file_type().is_ok_and(|t| t.is_file())
        && (name.ends_with(".png") || name.ends_with(".jpg") || name.ends_with(".jpeg"))
}

/// A folder name as the file system compares it (Windows ignores case).
pub(crate) fn folder_key(name: &str) -> String {
    if cfg!(windows) {
        name.to_lowercase()
    } else {
        name.to_owned()
    }
}

/// Mod files (`.jar`, `.jar.disabled`) in a mods folder.
pub(crate) fn count_mods(dir: &Path) -> u32 {
    count_entries(dir, |e| {
        e.file_type().is_ok_and(|t| t.is_file())
            && pl_mc::mods::scan::jar_name_state(&e.file_name().to_string_lossy()).is_some()
    })
}

/// Marks the profiles an earlier import made into instances that still exist.
pub(crate) fn mark_imported(
    source_root: &Path,
    profiles: &mut [ImportProfile],
    imported: &[ImportedFrom],
) {
    let root = crate::journal::path_key(source_root);
    for profile in profiles {
        profile.imported_as = imported
            .iter()
            .find(|i| i.source == root && i.profile == profile.id)
            .map(|i| i.instance_id.clone());
    }
}

/// The ones that can come over first; among them the most recently played, then the most
/// recently installed, then by name.
pub(crate) fn sort_profiles(profiles: &mut [ImportProfile]) {
    profiles.sort_by(|a, b| {
        b.supported()
            .cmp(&a.supported())
            .then_with(|| b.last_used_ms.cmp(&a.last_used_ms))
            .then_with(|| b.installed_ms.cmp(&a.installed_ms))
            .then_with(|| a.name.to_lowercase().cmp(&b.name.to_lowercase()))
    });
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn launcher_times() {
        assert_eq!(
            timestamp_ms("2024-05-01T12:00:00Z"),
            Some(1_714_564_800_000)
        );
        assert_eq!(
            timestamp_ms("2024-05-01T12:00:00.250+00:00"),
            Some(1_714_564_800_250)
        );
        assert_eq!(timestamp_ms("0001-01-01T00:00:00"), None);
        assert_eq!(timestamp_ms(""), None);
    }
}
