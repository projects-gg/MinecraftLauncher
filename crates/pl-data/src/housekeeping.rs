//! The data folder's housekeeping policy: every age and size rule in one place ([`Policy`]).
//!
//! Automatic ([`tidy`], shortly after start and then once a day):
//! - **Launcher trash** (`trash/`, [`pl_mc::gamefiles::LauncherTrash`]): items deleted more than
//!   30 days ago go; when the trash takes more than its cap ([`trash_cap`]) the oldest go first,
//!   never the newest item. Leftovers of interrupted moves go after a day.
//! - **Junk** ([`LeftoverKind::Downloads`]), once older than a day: unfinished downloads and
//!   temporary files (`*.part`, `*.tmp`) in the shared game folders and caches, scratch folders of
//!   loader installers and modpack downloads, Java argument files (`logs/launch-*.args`) and the
//!   installers the updater leaves in the system temp folder.
//! - **Captured game logs** (`logs/game-*.log`): over [`Policy::captured_logs_max_bytes`] the
//!   oldest go first, never the newest few.
//! - **Thumbnails of deleted instances** (`cache/thumbs/<instance>/` without
//!   `instances/<instance>/`).
//!
//! On request, with a size preview first ([`scan_leftovers`], [`clean_leftovers`]):
//! - [`LeftoverKind::Cache`]: Modrinth icons (they download again) and screenshot thumbnails
//!   (made again when the gallery shows them);
//! - [`LeftoverKind::Downloads`]: the junk above;
//! - [`LeftoverKind::Logs`]: captured game logs older than 14 days (or over the cap; the newest
//!   five sessions always stay, support needs the last one whatever its age), and the game's own
//!   archived logs (`<game>/logs/*.log.gz`) older than 30 days;
//! - [`LeftoverKind::CrashReports`]: crash reports older than 30 days beyond the newest five of
//!   each instance, and JVM crash logs (`<game>/hs_err_pid*.log`) older than 30 days.
//!
//! A version set the player put back from the trash ([`KeptSets`]) is not offered as an old set
//! again until it has gone unused for the threshold since then.
//!
//! Never touched: worlds, mods, settings and every other instance file, accounts, AFK data, the
//! launcher's own logs (they rotate by themselves, [`crate::logfile`]), the signed content cache
//! (pruned by its owner) and anything changed within [`Policy::fresh`] (it may be open). The
//! running game's folder and log file ([`InUse`]) are left alone, and files that cannot be removed
//! (open elsewhere) are skipped silently.

use std::cmp::Reverse;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pl_mc::gamefiles::{
    INCOMPLETE_SLOT_AGE, LauncherTrash, PurgeReport, TRASH_RETENTION, TrashLimits,
};
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::usage::{CACHE_DIR, LOGS_DIR};

const DAY: Duration = Duration::from_secs(24 * 60 * 60);
const GIB: u64 = 1024 * 1024 * 1024;
const MIB: u64 = 1024 * 1024;
/// Screenshot thumbnails below `cache/` (`pl_mc::gamefiles::thumbnails_dir`).
const THUMBS_DIR: &str = "thumbs";

/// Folders whose `*.part` / `*.tmp` files are download or write leftovers (never inside
/// instances, AFK data or the data root itself).
const JUNK_DIRS: &[&str] = &[
    CACHE_DIR,
    "meta",
    "versions",
    "libraries",
    "assets",
    "runtimes",
    "natives",
];

/// Every age and size rule of the cleanup.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Policy {
    /// Trash items deleted longer ago than this go.
    pub trash_max_age: Duration,
    /// Junk older than this goes (younger may belong to a job that is still running).
    pub leftover_age: Duration,
    /// On request: captured game logs older than this go.
    pub captured_log_age: Duration,
    /// Captured game logs together are kept under this (oldest go first).
    pub captured_logs_max_bytes: u64,
    /// The newest captured game logs that always stay.
    pub captured_logs_keep: usize,
    /// On request: the game's own archived logs older than this go.
    pub game_log_age: Duration,
    /// On request: crash reports older than this go...
    pub crash_report_age: Duration,
    /// ...except the newest few of each instance.
    pub crash_reports_keep: usize,
    /// Anything changed this recently is treated as in use.
    pub fresh: Duration,
}

impl Default for Policy {
    fn default() -> Self {
        Self {
            trash_max_age: TRASH_RETENTION,
            leftover_age: DAY,
            captured_log_age: 14 * DAY,
            captured_logs_max_bytes: 512 * MIB,
            captured_logs_keep: 5,
            game_log_age: 30 * DAY,
            crash_report_age: 30 * DAY,
            crash_reports_keep: 5,
            fresh: Duration::from_secs(10 * 60),
        }
    }
}

/// The launcher trash may take 5% of its disk, between 2 GiB and 20 GiB (10 GiB when the disk
/// size is unknown).
pub fn trash_cap(disk_size: Option<u64>) -> u64 {
    match disk_size {
        Some(total) => (total / 20).clamp(2 * GIB, 20 * GIB),
        None => 10 * GIB,
    }
}

/// Where the cleanup looks.
#[derive(Debug, Clone, Default)]
pub struct Places {
    /// The data folder.
    pub root: PathBuf,
    /// Every instance's game directory.
    pub game_dirs: Vec<PathBuf>,
    /// The system temp folder (update installers), when the updater may have used it.
    pub temp_dir: Option<PathBuf>,
    /// The app's product name as the updater spells it in its temp folders.
    pub app_name: String,
}

/// What the running game holds right now.
#[derive(Debug, Clone, Default)]
pub struct InUse {
    /// Game directories of running games: their logs and crash reports stay.
    pub game_dirs: Vec<PathBuf>,
    /// Open files (the running game's captured log).
    pub files: Vec<PathBuf>,
}

/// The groups of the on-request cleanup.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum LeftoverKind {
    /// Modrinth icons and screenshot thumbnails.
    Cache,
    /// Unfinished downloads, temporary files, scratch folders, old update installers.
    Downloads,
    /// Old captured game logs and the game's own archived logs.
    Logs,
    /// Old crash reports.
    CrashReports,
}

pub const ALL_LEFTOVERS: [LeftoverKind; 4] = [
    LeftoverKind::Cache,
    LeftoverKind::Downloads,
    LeftoverKind::Logs,
    LeftoverKind::CrashReports,
];

/// What one group would free.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LeftoverGroup {
    pub kind: LeftoverKind,
    pub bytes: u64,
    pub files: u64,
}

/// What a cleanup removed.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct CleanupReport {
    pub freed_bytes: u64,
    pub removed_files: u64,
}

impl CleanupReport {
    pub fn add(&mut self, other: CleanupReport) {
        self.freed_bytes += other.freed_bytes;
        self.removed_files += other.removed_files;
    }
}

/// What [`tidy`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct TidyReport {
    pub trash: PurgeReport,
    pub junk: CleanupReport,
}

#[derive(Debug, Clone)]
struct Candidate {
    path: PathBuf,
    kind: LeftoverKind,
    dir: bool,
    bytes: u64,
    files: u64,
    /// Folders this removal leaves empty go too, up to (not including) this one.
    prune_to: Option<PathBuf>,
}

/// What the on-request cleanup would free, per group (empty groups left out). Deletes nothing.
pub fn scan_leftovers(
    places: &Places,
    policy: &Policy,
    in_use: &InUse,
    now: SystemTime,
) -> Vec<LeftoverGroup> {
    let mut groups: Vec<LeftoverGroup> = Vec::new();
    for candidate in candidates(places, policy, in_use, now, false) {
        match groups.iter_mut().find(|g| g.kind == candidate.kind) {
            Some(group) => {
                group.bytes += candidate.bytes;
                group.files += candidate.files;
            }
            None => groups.push(LeftoverGroup {
                kind: candidate.kind,
                bytes: candidate.bytes,
                files: candidate.files,
            }),
        }
    }
    groups.sort_by_key(|g| ALL_LEFTOVERS.iter().position(|k| *k == g.kind));
    groups
}

/// Removes the on-request leftovers of the chosen groups.
pub fn clean_leftovers(
    places: &Places,
    policy: &Policy,
    in_use: &InUse,
    now: SystemTime,
    kinds: &[LeftoverKind],
) -> CleanupReport {
    let chosen = candidates(places, policy, in_use, now, false)
        .into_iter()
        .filter(|c| kinds.contains(&c.kind));
    let report = remove_all(chosen);
    tracing::info!(
        freed = report.freed_bytes,
        files = report.removed_files,
        ?kinds,
        "cleaned leftovers"
    );
    report
}

/// The automatic part of the policy: trash limits, junk and the captured log cap.
pub fn tidy(
    places: &Places,
    policy: &Policy,
    in_use: &InUse,
    now: SystemTime,
    trash_cap: Option<u64>,
) -> TidyReport {
    let trash = LauncherTrash::new(places.root.join("trash"));
    let now_ms = now
        .duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(i64::MAX);
    let limits = TrashLimits {
        max_age: policy.trash_max_age,
        max_bytes: trash_cap,
        incomplete_age: INCOMPLETE_SLOT_AGE,
    };
    let trash = trash
        .purge_blocking(limits, now_ms)
        .unwrap_or_else(|error| {
            tracing::warn!(%error, "could not tidy the launcher trash");
            PurgeReport::default()
        });
    let junk = remove_all(candidates(places, policy, in_use, now, true).into_iter());
    if junk.removed_files > 0 {
        tracing::info!(
            freed = junk.freed_bytes,
            files = junk.removed_files,
            "removed leftovers"
        );
    }
    TidyReport { trash, junk }
}

fn candidates(
    places: &Places,
    policy: &Policy,
    in_use: &InUse,
    now: SystemTime,
    automatic: bool,
) -> Vec<Candidate> {
    let age_of = |meta: &std::fs::Metadata| -> Option<Duration> {
        let modified = meta.modified().ok()?;
        // Dated in the future: treated as brand new.
        Some(now.duration_since(modified).unwrap_or_default())
    };
    let older = |meta: &std::fs::Metadata, age: Duration| {
        age_of(meta).is_some_and(|elapsed| elapsed >= age.max(policy.fresh))
    };
    let mut out: Vec<Candidate> = Vec::new();
    let mut file = |path: PathBuf, meta: &std::fs::Metadata, kind: LeftoverKind| {
        out.push(Candidate {
            path,
            kind,
            dir: false,
            bytes: meta.len(),
            files: 1,
            prune_to: None,
        });
    };
    let root = &places.root;
    let cache = root.join(CACHE_DIR);
    let logs = root.join(LOGS_DIR);
    let in_use_file = |path: &Path| in_use.files.iter().any(|f| f == path);

    // Mod icons and screenshot thumbnails (on request only; both are made again when needed).
    let thumbs = cache.join(THUMBS_DIR);
    if !automatic {
        for folder in [cache.join("modrinth").join("icons"), thumbs.clone()] {
            for entry in walk_files(&folder) {
                if let Ok(meta) = entry.metadata()
                    && older(&meta, Duration::ZERO)
                {
                    file(entry.path().to_path_buf(), &meta, LeftoverKind::Cache);
                }
            }
        }
    }

    // Thumbnails of instances that no longer exist (automatic; each instance's own stale ones
    // go whenever its gallery is listed, `pl_mc::gamefiles::prune_thumbnails`).
    let mut orphans: Vec<Candidate> = Vec::new();
    if automatic {
        for entry in read_dir(&thumbs) {
            let orphan = entry.file_type().is_ok_and(|t| t.is_dir())
                && !root.join("instances").join(entry.file_name()).is_dir();
            if orphan {
                orphans.push(dir_candidate(entry.path(), LeftoverKind::Cache));
            }
        }
    }

    // Unfinished downloads and temporary files (the folders they leave empty go too).
    let mut junk: Vec<Candidate> = Vec::new();
    for dir in JUNK_DIRS {
        for entry in walk_files(&root.join(dir)) {
            let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
            if !(name.ends_with(".part") || name.ends_with(".tmp")) {
                continue;
            }
            if let Ok(meta) = entry.metadata()
                && older(&meta, policy.leftover_age)
            {
                junk.push(Candidate {
                    path: entry.path().to_path_buf(),
                    kind: LeftoverKind::Downloads,
                    dir: false,
                    bytes: meta.len(),
                    files: 1,
                    prune_to: Some(root.join(dir)),
                });
            }
        }
    }

    // Java argument files and captured game logs.
    let mut captured: Vec<(PathBuf, std::fs::Metadata)> = Vec::new();
    for entry in read_dir(&logs) {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        if name.starts_with("launch-") && name.ends_with(".args") {
            if older(&meta, policy.leftover_age) {
                file(entry.path(), &meta, LeftoverKind::Downloads);
            }
        } else if name.starts_with("game-") && name.ends_with(".log") {
            captured.push((entry.path(), meta));
        }
    }
    captured.sort_by_key(|(_, meta)| Reverse(meta.modified().ok()));
    let mut kept_bytes = 0u64;
    for (index, (path, meta)) in captured.into_iter().enumerate() {
        if index < policy.captured_logs_keep || in_use_file(&path) || !older(&meta, Duration::ZERO)
        {
            kept_bytes += meta.len();
            continue;
        }
        let over_cap = kept_bytes + meta.len() > policy.captured_logs_max_bytes;
        let too_old = !automatic && older(&meta, policy.captured_log_age);
        if over_cap || too_old {
            file(path, &meta, LeftoverKind::Logs);
        } else {
            kept_bytes += meta.len();
        }
    }

    // Instance folders (on request only; never the running game's).
    if !automatic {
        for game in &places.game_dirs {
            if in_use.game_dirs.iter().any(|busy| busy == game) {
                continue;
            }
            for entry in read_dir(&game.join("logs")) {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if !name.ends_with(".log.gz") {
                    continue;
                }
                if let Ok(meta) = entry.metadata()
                    && meta.is_file()
                    && older(&meta, policy.game_log_age)
                {
                    file(entry.path(), &meta, LeftoverKind::Logs);
                }
            }
            let mut reports: Vec<(PathBuf, std::fs::Metadata)> =
                read_dir(&game.join("crash-reports"))
                    .into_iter()
                    .filter(|e| {
                        e.file_name()
                            .to_string_lossy()
                            .to_ascii_lowercase()
                            .ends_with(".txt")
                    })
                    .filter_map(|e| Some((e.path(), e.metadata().ok().filter(|m| m.is_file())?)))
                    .collect();
            reports.sort_by_key(|(_, meta)| Reverse(meta.modified().ok()));
            for (path, meta) in reports.into_iter().skip(policy.crash_reports_keep) {
                if older(&meta, policy.crash_report_age) {
                    file(path, &meta, LeftoverKind::CrashReports);
                }
            }
            for entry in read_dir(game) {
                let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                if !(name.starts_with("hs_err_pid") && name.ends_with(".log")) {
                    continue;
                }
                if let Ok(meta) = entry.metadata()
                    && meta.is_file()
                    && older(&meta, policy.crash_report_age)
                {
                    file(entry.path(), &meta, LeftoverKind::CrashReports);
                }
            }
        }
    }

    out.append(&mut junk);
    out.append(&mut orphans);

    // Scratch folders: loader installers and modpack downloads.
    for scratch in [
        cache.join("loader-install"),
        cache.join("modrinth").join("packs"),
    ] {
        for entry in read_dir(&scratch) {
            let path = entry.path();
            let Ok(meta) = entry.metadata() else { continue };
            if meta.is_dir() {
                if newest_change(&path).is_some_and(|changed| {
                    now.duration_since(changed).unwrap_or_default() >= policy.leftover_age
                }) {
                    out.push(dir_candidate(path, LeftoverKind::Downloads));
                }
            } else if older(&meta, policy.leftover_age) {
                out.push(Candidate {
                    path,
                    kind: LeftoverKind::Downloads,
                    dir: false,
                    bytes: meta.len(),
                    files: 1,
                    prune_to: None,
                });
            }
        }
    }

    // Installers the updater left in the system temp folder.
    if let Some(temp) = &places.temp_dir {
        for path in update_leftovers(temp, &places.app_name) {
            let stale = newest_change(&path).is_some_and(|changed| {
                now.duration_since(changed).unwrap_or_default() >= policy.leftover_age
            });
            if stale {
                out.push(dir_candidate(path, LeftoverKind::Downloads));
            }
        }
    }
    out
}

/// Tauri's updater unpacks each update into `<temp>/<app name>-<version>-updater-<random>/` and
/// leaves it there. Only folders of exactly that shape holding nothing but installers count.
fn update_leftovers(temp: &Path, app_name: &str) -> Vec<PathBuf> {
    if app_name.trim().is_empty() {
        return Vec::new();
    }
    let prefix = format!("{app_name}-");
    read_dir(temp)
        .into_iter()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let Some(rest) = name.strip_prefix(&prefix) else {
                return false;
            };
            let Some((version, random)) = rest.rsplit_once("-updater-") else {
                return false;
            };
            version.starts_with(|c: char| c.is_ascii_digit())
                && !random.is_empty()
                && random.chars().all(|c| c.is_ascii_alphanumeric())
        })
        .map(|entry| entry.path())
        .filter(|dir| {
            let entries = read_dir(dir);
            !entries.is_empty()
                && entries.iter().all(|entry| {
                    let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
                    entry.file_type().is_ok_and(|t| t.is_file())
                        && (name.ends_with(".exe") || name.ends_with(".msi"))
                })
        })
        .collect()
}

fn dir_candidate(path: PathBuf, kind: LeftoverKind) -> Candidate {
    let (bytes, files) = walk_files(&path)
        .filter_map(|e| e.metadata().ok())
        .fold((0, 0), |(bytes, files), meta| {
            (bytes + meta.len(), files + 1)
        });
    Candidate {
        path,
        kind,
        dir: true,
        bytes,
        files,
        prune_to: None,
    }
}

fn remove_all(candidates: impl Iterator<Item = Candidate>) -> CleanupReport {
    let mut report = CleanupReport::default();
    for candidate in candidates {
        let removed = if candidate.dir {
            remove_tree(&candidate.path)
        } else {
            match std::fs::remove_file(&candidate.path) {
                Ok(()) => {
                    if let Some(stop) = &candidate.prune_to {
                        prune_empty_parents(&candidate.path, stop);
                    }
                    (candidate.bytes, candidate.files)
                }
                Err(error) => {
                    tracing::debug!(path = %candidate.path.display(), %error, "kept a file in use");
                    (0, 0)
                }
            }
        };
        report.freed_bytes += removed.0;
        report.removed_files += removed.1;
    }
    report
}

/// Removes the folders above `path` that are empty now, stopping below `stop`.
/// The folders right under `stop` stay as well (`assets/objects`, `cache/content`, a library
/// group root): the layout itself is never pruned.
fn prune_empty_parents(path: &Path, stop: &Path) {
    let mut dir = path.parent();
    while let Some(current) = dir {
        let structural = current == stop || current.parent() == Some(stop);
        if structural || !current.starts_with(stop) || std::fs::remove_dir(current).is_err() {
            break;
        }
        dir = current.parent();
    }
}

/// Removes every file under `dir` and then the folders (deepest first). Files that cannot be
/// removed (in use) stay, and so do their folders. Returns the bytes and files removed.
fn remove_tree(dir: &Path) -> (u64, u64) {
    let entries: Vec<_> = WalkDir::new(dir)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_map(Result::ok)
        .collect();
    let (mut bytes, mut files) = (0, 0);
    for entry in entries {
        if entry.file_type().is_dir() {
            // Fails harmlessly when something inside could not be removed.
            let _ = std::fs::remove_dir(entry.path());
        } else {
            let size = entry.metadata().map(|m| m.len()).unwrap_or(0);
            if std::fs::remove_file(entry.path()).is_ok() {
                bytes += size;
                files += 1;
            }
        }
    }
    (bytes, files)
}

fn read_dir(dir: &Path) -> Vec<std::fs::DirEntry> {
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

fn walk_files(dir: &Path) -> impl Iterator<Item = walkdir::DirEntry> {
    WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter(|entry| entry.file_type().is_file())
}

/// Latest change to a folder or anything inside it (a scratch folder still in use is "new"
/// even when the files it unpacked carry old dates).
fn newest_change(dir: &Path) -> Option<SystemTime> {
    WalkDir::new(dir)
        .follow_links(false)
        .into_iter()
        .filter_map(Result::ok)
        .filter_map(|entry| entry.metadata().ok()?.modified().ok())
        .max()
}

// ---------------------------------------------------------------------------------------------
// Version sets the player kept

/// The file in the data folder that remembers the version sets the player put back.
pub const KEPT_SETS_FILE: &str = "cleanup-kept.json";
/// A kept mark is forgotten after this; the set is judged by its own last use again.
const KEPT_FORGET: Duration = Duration::from_secs(365 * 24 * 60 * 60);

/// Version sets the player put back from the trash after a cleanup: `<instance id>/<minecraft>`
/// → when (ms since the Unix epoch). Putting a set back says "I still want it", so it is not
/// offered as an old set again until it has gone unused for the threshold since then.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct KeptSets {
    sets: BTreeMap<String, i64>,
}

impl KeptSets {
    /// The marks stored in the data folder `root`; none when the file is missing or unreadable.
    pub fn load(root: &Path) -> Self {
        std::fs::read(root.join(KEPT_SETS_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Stores the marks in the data folder `root` (flushed to disk before replacing the old file).
    pub fn save(&self, root: &Path) -> pl_mc::Result<()> {
        let bytes = serde_json::to_vec_pretty(self).unwrap_or_default();
        pl_mc::paths::write_durable_blocking(&root.join(KEPT_SETS_FILE), &bytes)
    }

    /// Marks a set as kept at `now_ms` and forgets marks older than a year.
    pub fn keep(&mut self, key: impl Into<String>, now_ms: i64) {
        let forget = i64::try_from(KEPT_FORGET.as_millis()).unwrap_or(i64::MAX);
        self.sets
            .retain(|_, at| now_ms.saturating_sub(*at) < forget);
        self.sets.insert(key.into(), now_ms);
    }

    /// Kept at or after `cutoff_ms`: not an old set yet, whatever its last use says.
    pub fn kept_since(&self, key: &str, cutoff_ms: i64) -> bool {
        self.sets.get(key).is_some_and(|at| *at >= cutoff_ms)
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

    fn age(path: &Path, days: u64) {
        let when = SystemTime::now() - Duration::from_secs(days * 24 * 60 * 60 + 60);
        fs::File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(when)
            .unwrap();
    }

    fn places(root: &Path) -> Places {
        Places {
            root: root.to_path_buf(),
            game_dirs: vec![root.join("instances/a/minecraft")],
            temp_dir: None,
            app_name: String::new(),
        }
    }

    fn kinds(groups: &[LeftoverGroup]) -> Vec<(LeftoverKind, u64, u64)> {
        groups.iter().map(|g| (g.kind, g.bytes, g.files)).collect()
    }

    #[test]
    fn thumbnails_are_cache_and_go_with_their_instance() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("instances/a/minecraft")).unwrap();
        for path in ["cache/thumbs/a/1.jpg", "cache/thumbs/gone/2.jpg"] {
            write(&root.join(path), 10);
            age(&root.join(path), 1);
        }
        let policy = Policy::default();
        let none = InUse::default();
        let now = SystemTime::now();
        // On request every thumbnail counts as cache (the gallery makes them again).
        assert_eq!(
            kinds(&scan_leftovers(&places(root), &policy, &none, now)),
            [(LeftoverKind::Cache, 20, 2)]
        );
        // Automatically only the folder of a deleted instance goes.
        let report = tidy(&places(root), &policy, &none, now, None);
        assert_eq!(
            report.junk,
            CleanupReport {
                freed_bytes: 10,
                removed_files: 1
            }
        );
        assert!(root.join("cache/thumbs/a/1.jpg").is_file());
        assert!(!root.join("cache/thumbs/gone").exists());
    }

    #[test]
    fn cleans_only_rebuildable_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        // Never touched, however old: instance files, accounts, AFK data, launcher logs (they
        // rotate on their own), the signed content cache and the data root's own files.
        let protected = [
            "instances/a/minecraft/mods/x.jar.part",
            "instances/a/minecraft/saves/World/level.dat",
            "afk/bin/client.exe.part",
            "afk/accounts.json.tmp",
            "accounts.json",
            "accounts.json.tmp",
            "logs/launcher.2026-08-01.log",
            "cache/content/current.json",
        ];
        // Unfinished downloads and temporary files in the shared folders and caches.
        let leftovers = [
            "logs/launch-abc.args",
            "cache/modrinth/packs/p.mrpack",
            "cache/loader-install/job/data.lzma",
            "cache/content/media/x.part",
            "meta/manifest.json.tmp",
            "versions/1.21.1/1.21.1.jar.part",
            "libraries/x/y/z.jar.0123456789abcdef.tmp",
            "assets/objects/ab/abcd.part",
            "runtimes/java/bin/java.exe.part",
            "natives/1.21.1-0123456789/lwjgl.dll.tmp",
        ];
        for path in protected.iter().chain(&leftovers) {
            write(&root.join(path), 10);
        }
        for path in protected {
            age(&root.join(path), 30);
        }
        let old_log = root.join("logs/game-projects-20260901-120000.log");
        write(&old_log, 10);
        age(&old_log, 20);
        let new_logs: Vec<PathBuf> = (0..6)
            .map(|i| root.join(format!("logs/game-projects-2026101{i}-120000.log")))
            .collect();
        for log in &new_logs {
            write(log, 10);
        }
        write(&root.join("cache/modrinth/icons/a.png"), 10);
        write(&root.join("cache/modrinth/icons/b.webp"), 10);
        for icon in ["a.png", "b.webp"] {
            age(&root.join("cache/modrinth/icons").join(icon), 1);
        }
        let policy = Policy::default();
        let none = InUse::default();

        // Today: the preview shows icons and the old game log; fresh leftovers may belong to
        // running jobs.
        let now = SystemTime::now();
        assert_eq!(
            kinds(&scan_leftovers(&places(root), &policy, &none, now)),
            [(LeftoverKind::Cache, 20, 2), (LeftoverKind::Logs, 10, 1)]
        );
        let first = clean_leftovers(&places(root), &policy, &none, now, &ALL_LEFTOVERS);
        assert_eq!(
            first,
            CleanupReport {
                freed_bytes: 30,
                removed_files: 3
            }
        );
        assert!(!old_log.exists());
        assert!(new_logs.iter().all(|log| log.exists()));
        assert!(root.join("cache/modrinth/icons").is_dir());
        for path in leftovers {
            assert!(root.join(path).exists(), "{path} is still fresh");
        }

        // Two days later the leftovers are stale; recent game logs stay.
        let later = now + 2 * DAY;
        let report = tidy(&places(root), &policy, &none, later, None);
        assert_eq!(
            report.junk,
            CleanupReport {
                freed_bytes: 100,
                removed_files: 10
            }
        );
        for path in leftovers {
            assert!(!root.join(path).exists(), "{path} must go");
        }
        // Folders they leave empty go too; the layout's own folders stay.
        assert!(!root.join("libraries/x/y").exists());
        assert!(root.join("libraries/x").is_dir());
        assert!(!root.join("assets/objects/ab").exists());
        assert!(root.join("assets/objects").is_dir());
        assert!(!root.join("cache/loader-install/job").exists());
        assert!(new_logs.iter().all(|log| log.exists()));
        for path in protected {
            assert!(root.join(path).exists(), "{path} must stay");
        }
    }

    #[test]
    fn captured_logs_keep_a_size_cap_but_never_the_newest_or_the_open_one() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let logs: Vec<PathBuf> = (0..8)
            .map(|i| root.join(format!("logs/game-x-2026100{i}-120000.log")))
            .collect();
        for (i, log) in logs.iter().enumerate() {
            write(log, 100);
            age(log, 8 - i as u64); // logs[7] is the newest
        }
        let policy = Policy {
            captured_logs_max_bytes: 450,
            captured_logs_keep: 2,
            ..Policy::default()
        };
        // The oldest log is open (a long session): it stays.
        let in_use = InUse {
            game_dirs: Vec::new(),
            files: vec![logs[0].clone()],
        };
        let report = tidy(&places(root), &policy, &in_use, SystemTime::now(), None);
        assert_eq!(report.junk.removed_files, 3);
        // Kept: the newest two, the open one, and the next two that fit under the cap.
        let left: Vec<bool> = logs.iter().map(|log| log.exists()).collect();
        assert_eq!(
            left,
            [true, false, false, false, true, true, true, true],
            "{left:?}"
        );
    }

    #[test]
    fn instance_logs_and_crash_reports_on_request() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let game = root.join("instances/a/minecraft");
        write(&game.join("logs/latest.log"), 10);
        write(&game.join("logs/debug.log"), 10);
        write(&game.join("logs/2026-08-01-1.log.gz"), 11);
        write(&game.join("logs/2026-10-14-1.log.gz"), 12);
        age(&game.join("logs/latest.log"), 60);
        age(&game.join("logs/2026-08-01-1.log.gz"), 60);
        for i in 0..7 {
            let report = game.join(format!("crash-reports/crash-2026-0{i}.txt"));
            write(&report, 20);
            age(&report, 40 + i);
        }
        write(&game.join("hs_err_pid123.log"), 30);
        age(&game.join("hs_err_pid123.log"), 45);
        write(&game.join("options.txt"), 5);
        age(&game.join("options.txt"), 400);
        let policy = Policy::default();
        let now = SystemTime::now();
        let groups = scan_leftovers(&places(root), &policy, &InUse::default(), now);
        assert_eq!(
            kinds(&groups),
            [
                (LeftoverKind::Logs, 11, 1),
                // Seven old reports, the newest five stay; plus the JVM crash log.
                (LeftoverKind::CrashReports, 2 * 20 + 30, 3),
            ]
        );
        // While that instance's game runs, its folder is left alone.
        let running = InUse {
            game_dirs: vec![game.clone()],
            files: Vec::new(),
        };
        assert!(scan_leftovers(&places(root), &policy, &running, now).is_empty());

        let report = clean_leftovers(
            &places(root),
            &policy,
            &InUse::default(),
            now,
            &[LeftoverKind::CrashReports],
        );
        assert_eq!(report.removed_files, 3);
        assert!(game.join("logs/2026-08-01-1.log.gz").exists(), "not chosen");
        assert!(game.join("logs/latest.log").exists());
        assert!(game.join("options.txt").exists());
        assert_eq!(fs::read_dir(game.join("crash-reports")).unwrap().count(), 5);
    }

    #[test]
    fn update_installers_left_in_the_temp_folder() {
        let dir = tempfile::tempdir().unwrap();
        let temp = dir.path().join("temp");
        let ours = temp.join("Projects Launcher-2.0.3-updater-AbC123");
        write(&ours.join("Projects Launcher-2.0.3-installer.exe"), 1000);
        let other_app = temp.join("Other App-1.0.0-updater-XyZ789");
        write(&other_app.join("setup.exe"), 10);
        let mixed = temp.join("Projects Launcher-2.0.4-updater-QwE456");
        write(&mixed.join("Projects Launcher-2.0.4-installer.exe"), 10);
        write(&mixed.join("notes.txt"), 10);
        let odd = temp.join("Projects Launcher-beta-updater-x");
        write(&odd.join("a.exe"), 10);
        for file in [
            ours.join("Projects Launcher-2.0.3-installer.exe"),
            other_app.join("setup.exe"),
            mixed.join("Projects Launcher-2.0.4-installer.exe"),
            mixed.join("notes.txt"),
            odd.join("a.exe"),
        ] {
            age(&file, 3);
        }
        let root = dir.path().join("root");
        fs::create_dir_all(&root).unwrap();
        let places = Places {
            root,
            game_dirs: Vec::new(),
            temp_dir: Some(temp.clone()),
            app_name: "Projects Launcher".into(),
        };
        // Folders themselves are "changed" now; two days later they count as stale.
        let later = SystemTime::now() + 2 * DAY;
        let groups = scan_leftovers(&places, &Policy::default(), &InUse::default(), later);
        assert_eq!(kinds(&groups), [(LeftoverKind::Downloads, 1000, 1)]);
        tidy(&places, &Policy::default(), &InUse::default(), later, None);
        assert!(!ours.exists());
        assert!(other_app.exists() && mixed.exists() && odd.exists());
        // Without a product name nothing in the temp folder is judged.
        assert!(update_leftovers(&temp, "").is_empty());
    }

    #[test]
    fn tidy_applies_the_trash_limits() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let trash = LauncherTrash::new(root.join("trash"));
        for name in ["a", "b"] {
            let file = root.join(name);
            write(&file, 600);
            trash
                .put_blocking(&file, pl_mc::gamefiles::TrashKind::World, "i", name)
                .unwrap();
        }
        let report = tidy(
            &places(root),
            &Policy::default(),
            &InUse::default(),
            SystemTime::now(),
            Some(1000),
        );
        assert_eq!(report.trash.removed, 1, "over the cap: the oldest goes");
        let report = tidy(
            &places(root),
            &Policy::default(),
            &InUse::default(),
            SystemTime::now() + 31 * DAY,
            Some(1000),
        );
        assert_eq!(
            report.trash.removed, 1,
            "after 30 days the last one goes too"
        );
    }

    #[test]
    fn trash_cap_follows_the_disk() {
        assert_eq!(trash_cap(None), 10 * GIB);
        assert_eq!(trash_cap(Some(16 * GIB)), 2 * GIB);
        assert_eq!(trash_cap(Some(256 * GIB)), 256 * GIB / 20);
        assert_eq!(trash_cap(Some(4096 * GIB)), 20 * GIB);
    }

    #[test]
    fn kept_sets_wait_a_full_threshold_and_survive_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let day = 24 * 60 * 60 * 1000_i64;
        let now = 400 * day;
        assert_eq!(
            KeptSets::load(dir.path()),
            KeptSets::default(),
            "no file yet"
        );

        let mut kept = KeptSets::default();
        kept.keep("a/1.20.1", now - 10 * day);
        kept.keep("a/1.19.4", now);
        kept.save(dir.path()).unwrap();
        let kept = KeptSets::load(dir.path());
        // Threshold 30 days: kept 10 days ago is not old again yet.
        assert!(kept.kept_since("a/1.20.1", now - 30 * day));
        // Threshold 7 days: it is.
        assert!(!kept.kept_since("a/1.20.1", now - 7 * day));
        assert!(kept.kept_since("a/1.19.4", now - 7 * day));
        assert!(!kept.kept_since("b/1.20.1", 0), "another instance's set");

        // Marks older than a year are forgotten when a new one is written.
        let mut kept = kept;
        kept.keep("a/1.18.2", now + 366 * day);
        assert!(!kept.kept_since("a/1.20.1", 0));
        assert!(!kept.kept_since("a/1.19.4", 0));
        assert!(kept.kept_since("a/1.18.2", 0));

        // A broken file reads as no marks (nothing is hidden by mistake).
        std::fs::write(dir.path().join(KEPT_SETS_FILE), b"{not json").unwrap();
        assert_eq!(KeptSets::load(dir.path()), KeptSets::default());
    }
}
