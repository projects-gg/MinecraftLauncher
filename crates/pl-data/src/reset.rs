//! "Launcher'ı sıfırla" (Settings → Veri): the launcher forgets the player's accounts and its own
//! settings, and keeps the player's game.
//!
//! A reset runs at the next start, before anything opens these files: the app leaves a note
//! ([`request`]) and restarts; the start finds it ([`pending`]), deletes the OS keychain entries of
//! the accounts the files name (the app's part: it knows the stores), removes [`TARGETS`]
//! ([`remove_launcher_data`]) and drops the note ([`finish`]). The page's own storage (its
//! preferences, what was read, the welcome) is cleared by the app on the first page load after.
//!
//! Removed: the account list and the skins kept for it, AFK accounts with their folders and
//! settings, cloud AFK copies, the friends feature's local data, world-sharing links, the faces
//! cache, the launcher's settings files and its logs (they carry names and chat).
//! Kept: instances (worlds, screenshots, mods, packs, game settings, server lists), the launcher
//! trash, shared game files and Java, AFK client builds, the content cache, the install id and
//! where the data folder is (the pointer outside it). A link inside a removed folder (a junction on
//! Windows, a symbolic link) goes as a link: what it points at is never touched.

use std::io;
use std::path::Path;

use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::links;

/// The note a requested reset leaves in the data folder for the next start.
pub const REQUEST_FILE: &str = "reset-request.json";

/// What a reset removes, relative to the data folder (`/`-separated). Everything else stays.
pub const TARGETS: &[&str] = &[
    // Accounts (pl_auth): the list (their secrets are in the OS keychain), the skins kept on this
    // computer (Microsoft skin history, Projects skins) and the faces cache.
    "accounts.json",
    "skins",
    "cache/heads",
    // AFK accounts (pl_afk): the list, the settings and each account's folder (MCC's working
    // directory: its config, chat log and alerts). The client builds in `afk/bin` stay.
    "afk/accounts.json",
    "afk/settings.json",
    "afk/accounts",
    // Cloud AFK copies (pl_cloud_afk), the friends feature (pl_social: chat, lists, invites, the
    // beta record) and world sharing (copies made for a friend's world, the guest binding key).
    "cloud-afk",
    "social",
    "beta-access.json",
    "host",
    // Settings the backend keeps: the language pick (pl_lang), the in-game overlay, fast packs and
    // the background preparation.
    "language",
    "overlay.json",
    "fast-packs.json",
    "prepare.json",
    // The launcher's own logs and the captured game logs.
    "logs",
];

/// Copies the account lists' stores leave beside them (a damaged list moved aside, an unfinished
/// write), as `(folder relative to the data folder, file name prefix)`: they name accounts too.
const LIST_COPIES: &[(&str, &str)] = &[("", "accounts.json."), ("afk", "accounts.json.")];

/// A reset the player asked for.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Request {
    /// When (ms since the Unix epoch). It also names this reset to the page, whose storage is
    /// cleared once for it.
    pub at_ms: i64,
    /// The launcher that asked, as its process id and start time: the next start waits until it
    /// is gone, so nothing it still writes while it quits comes back after the reset.
    #[serde(default)]
    pub asked_by: Option<(u32, u64)>,
}

/// Leaves the note for the next start (flushed to disk before it replaces an older one).
pub fn request(root: &Path, request: &Request) -> pl_mc::Result<()> {
    let bytes = serde_json::to_vec(request).unwrap_or_default();
    pl_mc::paths::write_durable_blocking(&root.join(REQUEST_FILE), &bytes)
}

/// The reset asked for that has not run yet. A note that cannot be read is no request: a damaged
/// file never wipes accounts on its own. It goes, and the start goes on as usual.
pub fn pending(root: &Path) -> Option<Request> {
    let path = root.join(REQUEST_FILE);
    let bytes = std::fs::read(&path).ok()?;
    match serde_json::from_slice::<Request>(&bytes) {
        Ok(request) => Some(request),
        Err(error) => {
            tracing::warn!(%error, "ignoring an unreadable reset request");
            finish(root);
            None
        }
    }
}

/// Drops the note once the reset ran.
pub fn finish(root: &Path) {
    if let Err(error) = std::fs::remove_file(root.join(REQUEST_FILE))
        && error.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "could not drop the reset request");
    }
}

/// What [`remove_launcher_data`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ResetReport {
    /// Files and links removed.
    pub removed_files: u64,
    /// The size of the files removed.
    pub removed_bytes: u64,
    /// Files that could not be removed (open elsewhere); they stay, and so do their folders.
    pub kept_files: u64,
}

/// Removes [`TARGETS`] from the data folder `root`.
pub fn remove_launcher_data(root: &Path) -> ResetReport {
    let mut report = ResetReport::default();
    for target in TARGETS {
        let path = target
            .split('/')
            .fold(root.to_path_buf(), |path, part| path.join(part));
        remove_target(&path, &mut report);
    }
    for (folder, prefix) in LIST_COPIES {
        let dir = root.join(folder);
        let Ok(entries) = std::fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.filter_map(Result::ok) {
            let copy = entry.file_name().to_string_lossy().starts_with(prefix)
                && entry.file_type().is_ok_and(|kind| kind.is_file());
            if copy {
                remove_target(&entry.path(), &mut report);
            }
        }
    }
    tracing::info!(
        files = report.removed_files,
        bytes = report.removed_bytes,
        kept = report.kept_files,
        "removed the launcher's accounts, settings and logs"
    );
    report
}

fn remove_target(path: &Path, report: &mut ResetReport) {
    let Ok(meta) = std::fs::symlink_metadata(path) else {
        return;
    };
    if meta.file_type().is_symlink() {
        // The target is a link itself (e.g. `logs` kept on another disk): only the link goes.
        tally(links::remove(path), 0, report);
        return;
    }
    if !meta.is_dir() {
        tally(std::fs::remove_file(path), meta.len(), report);
        return;
    }
    // Files and links first, deepest first; then the folders that are empty by then.
    let entries = WalkDir::new(path)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_map(Result::ok);
    for entry in entries {
        if entry.path_is_symlink() {
            tally(links::remove(entry.path()), 0, report);
        } else if entry.file_type().is_dir() {
            // Fails harmlessly while something inside could not be removed.
            let _ = std::fs::remove_dir(entry.path());
        } else {
            let size = entry.metadata().map(|meta| meta.len()).unwrap_or(0);
            tally(std::fs::remove_file(entry.path()), size, report);
        }
    }
}

fn tally(result: io::Result<()>, bytes: u64, report: &mut ResetReport) {
    match result {
        Ok(()) => {
            report.removed_files += 1;
            report.removed_bytes += bytes;
        }
        Err(error) if error.kind() == io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::debug!(%error, "kept a file in use");
            report.kept_files += 1;
        }
    }
}

#[cfg(test)]
mod tests {
    use std::fs;
    use std::path::PathBuf;

    use super::*;

    fn write(root: &Path, path: &str, bytes: usize) -> PathBuf {
        let path = root.join(path);
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(&path, vec![b'x'; bytes]).unwrap();
        path
    }

    /// What a player's data folder holds that a reset must leave alone.
    const KEPT: &[&str] = &[
        "instances/projects/instance.json",
        "instances/projects/minecraft/saves/Dünyam/level.dat",
        "instances/projects/minecraft/screenshots/2026-11-01_12.00.00.png",
        "instances/projects/minecraft/mods/sodium.jar",
        "instances/projects/minecraft/resourcepacks/pack.zip",
        "instances/projects/minecraft/options.txt",
        "instances/projects/minecraft/servers.dat",
        "instances/projects/minecraft/logs/latest.log",
        "trash/trash.json",
        "trash/items/1/world.zip",
        "versions/26.1.2/26.1.2.jar",
        "libraries/org/lwjgl/lwjgl.jar",
        "assets/indexes/26.json",
        "runtimes/java-runtime-delta/windows-x64/bin/java.exe",
        "natives/26.1.2/lwjgl.dll",
        "meta/version_manifest_v2.json",
        "cache/content/index.json",
        "cache/modrinth/icons/a.png",
        "afk/bin/1.2.3/MinecraftClient.exe",
        "install-id",
        "import.json",
        "cleanup-kept.json",
        "java-updates.json",
    ];

    /// What a reset removes, one file (or more) under each target.
    const REMOVED: &[&str] = &[
        "accounts.json",
        "accounts.json.corrupt-1762000000",
        "afk/accounts.json.bozuk-20261101-120000",
        "skins/history/0f.png",
        "skins/projects/GeceKartali.png",
        "cache/heads/GeceKartali.png",
        "afk/accounts.json",
        "afk/settings.json",
        "afk/accounts/a1/MinecraftClient.ini",
        "afk/accounts/a1/sohbet.txt",
        "cloud-afk/copies.json",
        "social/chat/123.json",
        "social/lists.json",
        "beta-access.json",
        "host/copies.json",
        "host/guest-binding.key",
        "language",
        "overlay.json",
        "fast-packs.json",
        "prepare.json",
        "logs/launcher-2026-11-01.log",
        "logs/game-projects-1.log",
    ];

    #[test]
    fn removes_accounts_settings_and_logs_and_keeps_the_game() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        for path in KEPT {
            write(root, path, 10);
        }
        for path in REMOVED {
            write(root, path, 7);
        }

        let report = remove_launcher_data(root);

        assert_eq!(
            report,
            ResetReport {
                removed_files: REMOVED.len() as u64,
                removed_bytes: 7 * REMOVED.len() as u64,
                kept_files: 0,
            }
        );
        for path in REMOVED {
            assert!(!root.join(path).exists(), "{path} stayed");
        }
        for folder in [
            "skins",
            "cache/heads",
            "afk/accounts",
            "cloud-afk",
            "social",
            "host",
            "logs",
        ] {
            assert!(!root.join(folder).exists(), "{folder} stayed");
        }
        for path in KEPT {
            assert!(root.join(path).is_file(), "{path} went");
        }
        // Run again (a start that finds the note twice): nothing more to do.
        assert_eq!(remove_launcher_data(root), ResetReport::default());
    }

    #[test]
    fn a_link_goes_without_what_it_points_at() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("data");
        let elsewhere = dir.path().join("other-disk");
        let kept_world = write(&elsewhere, "saves/Dünyam/level.dat", 5);
        let kept_logs = write(&elsewhere, "old-logs/launcher.log", 5);
        write(&root, "logs/launcher-2026-11-01.log", 3);
        // A link inside a removed folder, and a removed folder that is a link itself.
        links::create(&root.join("logs/saves"), &elsewhere.join("saves"), true).unwrap();
        links::create(&root.join("skins"), &elsewhere.join("old-logs"), true).unwrap();

        let report = remove_launcher_data(&root);

        assert_eq!(report.removed_files, 3, "{report:?}");
        assert_eq!(report.kept_files, 0);
        assert!(!root.join("logs").exists());
        assert!(fs::symlink_metadata(root.join("skins")).is_err());
        assert!(kept_world.is_file());
        assert!(kept_logs.is_file());
    }

    #[cfg(windows)]
    #[test]
    fn a_file_in_use_stays_and_the_rest_goes() {
        use std::os::windows::fs::OpenOptionsExt;

        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let open = write(root, "logs/launcher-2026-11-01.log", 4);
        write(root, "logs/game-projects-1.log", 4);
        write(root, "accounts.json", 4);
        // Opened without sharing, as a running program may hold it.
        let held = fs::OpenOptions::new()
            .read(true)
            .share_mode(0)
            .open(&open)
            .unwrap();

        let report = remove_launcher_data(root);

        assert_eq!(
            report,
            ResetReport {
                removed_files: 2,
                removed_bytes: 8,
                kept_files: 1,
            }
        );
        assert!(open.is_file());
        assert!(!root.join("accounts.json").exists());
        drop(held);
    }

    #[test]
    fn the_request_waits_for_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        assert_eq!(pending(root), None);

        let asked = Request {
            at_ms: 1_762_000_000_000,
            asked_by: Some((4321, 1_762_000_000)),
        };
        request(root, &asked).unwrap();
        assert_eq!(pending(root), Some(asked));
        // Still there until the reset ran.
        assert!(pending(root).is_some());
        finish(root);
        assert_eq!(pending(root), None);
        finish(root);

        // A note without the asking launcher still counts.
        fs::write(root.join(REQUEST_FILE), b"{\"atMs\":5}").unwrap();
        assert_eq!(
            pending(root),
            Some(Request {
                at_ms: 5,
                asked_by: None
            })
        );
        // A damaged note is no request, and it goes.
        fs::write(root.join(REQUEST_FILE), b"{\"atMs\":").unwrap();
        assert_eq!(pending(root), None);
        assert!(!root.join(REQUEST_FILE).exists());
    }

    #[test]
    fn targets_stay_inside_the_data_folder_and_spare_its_layout() {
        for target in TARGETS {
            assert!(!target.is_empty());
            assert!(
                target
                    .split('/')
                    .all(|part| !part.is_empty() && part != "." && part != ".."),
                "{target}"
            );
            assert!(!target.contains('\\') && !target.contains(':'), "{target}");
        }
        // Whole folders the game and the cleanup own are never targets themselves.
        for kept in [
            "instances",
            "trash",
            "versions",
            "libraries",
            "assets",
            "runtimes",
            "natives",
            "meta",
            "cache",
            "afk",
            "afk/bin",
            "install-id",
            REQUEST_FILE,
        ] {
            assert!(!TARGETS.contains(&kept), "{kept}");
        }
    }
}
