//! The data folder's housekeeping policy: every age and size rule in one place ([`Policy`]).
//!
//! Automatic ([`tidy`], a round shortly after start and at most once a day, never while a game
//! runs; the launcher schedules it):
//! - **Launcher trash** (`trash/`, [`pl_mc::gamefiles::LauncherTrash`]): items deleted more than
//!   30 days ago go; when the trash takes more than its cap ([`trash_cap`]) the oldest go first,
//!   never the newest item. Leftovers of interrupted moves go after a day.
//! - **Junk** ([`LeftoverKind::Downloads`]), once older than a day: unfinished downloads and
//!   temporary files (`*.part`, `*.tmp`) in the shared game folders and caches, scratch folders of
//!   loader installers and modpack downloads, Java argument files (`logs/launch-*.args`) and the
//!   installers the updater leaves in the system temp folder (those of the running version or an
//!   older one right away: the update went through).
//! - **Captured game logs** (`logs/game-*.log`): over [`Policy::captured_logs_max_bytes`] the
//!   oldest go first, never the newest few.
//! - **Thumbnails of deleted instances** (`cache/thumbs/<instance>/` without
//!   `instances/<instance>/`), and their Java AOT caches (`cache/aot/<instance>.*`).
//!
//! With "Otomatik Temizlik" on ([`CleanupState::auto`], the default) the round also applies the
//! on-request rules below to logs and crash reports, keeps mod icons and thumbnails under their
//! caps (oldest first), drops the start caches of instances not played for
//! [`Policy::unused_age`], and deletes shared game files unused for that long ([`sweep_shared`]).
//!
//! On request, with a size preview first ([`scan_leftovers`], [`clean_leftovers`]):
//! - [`LeftoverKind::Cache`]: Modrinth icons (they download again) and screenshot thumbnails
//!   (made again when the gallery shows them);
//! - [`LeftoverKind::StartCache`]: Java's AOT caches of game starts (`cache/aot`, AOT-CACHE; made
//!   again after the next start);
//! - [`LeftoverKind::Downloads`]: the junk above;
//! - [`LeftoverKind::Logs`]: captured game logs older than 14 days beyond each instance's newest
//!   five (or over the cap; the newest five sessions always stay, support needs the last one
//!   whatever its age), and the game's own archived logs (`<game>/logs/*.log.gz`) older than 30
//!   days;
//! - [`LeftoverKind::CrashReports`]: crash reports older than 30 days beyond the newest five of
//!   each instance, and JVM crash logs (`<game>/hs_err_pid*.log`) older than a day beyond each
//!   instance's newest.
//!
//! A version set the player put back from the trash ([`KeptSets`]) is not offered as an old set
//! again until it has gone unused for the threshold since then.
//!
//! Never touched: worlds, mods, resource and shader packs, screenshots, settings and every other
//! instance file, accounts, AFK data, the launcher's own logs (they rotate by themselves,
//! [`crate::logfile`]), the signed content cache (pruned by its owner) and anything changed within
//! [`Policy::fresh`] (it may be open). The running game's folder and log file ([`InUse`]) are left
//! alone, and files that cannot be removed (open elsewhere) are skipped silently. Links (symbolic
//! links, junctions: a folder kept on another disk) are never followed: a folder that is a link
//! is not judged, and a link inside a folder that goes is removed itself, not what it points at.

use std::cmp::Reverse;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use pl_mc::gamefiles::{
    INCOMPLETE_SLOT_AGE, LauncherTrash, PurgeReport, TRASH_RETENTION, TrashLimits,
};
use pl_mc::launch::aot::DIR_NAME as AOT_DIR;
use pl_mc::paths::DataDirs;
use pl_mc::shared_files::{self, GameRef, ScanOptions, SharedDeleteReport, SharedKind};
use pl_net::CancellationToken;
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
    /// JVM crash logs older than this go (the newest of each instance stays).
    pub jvm_crash_age: Duration,
    /// Automatic: unused shared game files and the start caches of instances not played for
    /// this long go (by the launcher's own count, [`CleanupState`]).
    pub unused_age: Duration,
    /// Automatic: mod icons together stay under this (the oldest go first)...
    pub icons_max_bytes: u64,
    /// ...and screenshot thumbnails under this.
    pub thumbs_max_bytes: u64,
    /// Automatic rounds run at most this often (restarts included).
    pub round_every: Duration,
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
            jvm_crash_age: DAY,
            unused_age: 14 * DAY,
            icons_max_bytes: 64 * MIB,
            thumbs_max_bytes: 256 * MIB,
            round_every: Duration::from_secs(20 * 60 * 60),
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
    /// The running launcher's version: update installers up to it did their job.
    pub app_version: String,
    /// When each instance (by id) was last played, ms since the Unix epoch.
    pub last_played: HashMap<String, i64>,
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
    /// Java's AOT caches of game starts.
    StartCache,
    /// Unfinished downloads, temporary files, scratch folders, old update installers.
    Downloads,
    /// Old captured game logs and the game's own archived logs.
    Logs,
    /// Old crash reports.
    CrashReports,
}

pub const ALL_LEFTOVERS: [LeftoverKind; 5] = [
    LeftoverKind::Cache,
    LeftoverKind::StartCache,
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
    for candidate in candidates(places, policy, in_use, now, Mode::Request) {
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
    let chosen = candidates(places, policy, in_use, now, Mode::Request)
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

/// The automatic part of the policy: trash limits, junk and the captured log cap; with `deep`
/// ("Otomatik Temizlik" on) also old logs, crash reports and JVM crash logs, the cache caps and
/// the start caches of instances not played for [`Policy::unused_age`].
pub fn tidy(
    places: &Places,
    policy: &Policy,
    in_use: &InUse,
    now: SystemTime,
    trash_cap: Option<u64>,
    deep: bool,
) -> TidyReport {
    let trash = LauncherTrash::new(places.root.join("trash"));
    let now_ms = millis(now);
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
    let junk = remove_all(candidates(places, policy, in_use, now, Mode::Auto { deep }).into_iter());
    if junk.removed_files > 0 {
        tracing::info!(
            freed = junk.freed_bytes,
            files = junk.removed_files,
            "removed leftovers"
        );
    }
    TidyReport { trash, junk }
}

/// Who asks for the candidates.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Mode {
    /// The player, with a size preview ("Temizle").
    Request,
    /// The automatic round; `deep` while "Otomatik Temizlik" is on.
    Auto { deep: bool },
}

fn candidates(
    places: &Places,
    policy: &Policy,
    in_use: &InUse,
    now: SystemTime,
    mode: Mode,
) -> Vec<Candidate> {
    let automatic = matches!(mode, Mode::Auto { .. });
    let deep = matches!(mode, Mode::Auto { deep: true });
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

    // Mod icons and screenshot thumbnails: all on request; automatically what is over each cap,
    // the oldest first (both are made again when needed).
    let thumbs = cache.join(THUMBS_DIR);
    if automatic {
        if deep {
            for (folder, cap) in [
                (cache.join("modrinth").join("icons"), policy.icons_max_bytes),
                (thumbs.clone(), policy.thumbs_max_bytes),
            ] {
                let mut files: Vec<(PathBuf, std::fs::Metadata)> = walk_files(&folder)
                    .filter_map(|e| Some((e.path().to_path_buf(), e.metadata().ok()?)))
                    .collect();
                files.sort_by_key(|(_, meta)| Reverse(meta.modified().ok()));
                let mut kept = 0u64;
                for (path, meta) in files {
                    if kept + meta.len() > cap && older(&meta, Duration::ZERO) {
                        file(path, &meta, LeftoverKind::Cache);
                    } else {
                        kept += meta.len();
                    }
                }
            }
        }
    } else {
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

    // Java's AOT caches (`<instance>.<key>.<ext>`): all on request; automatically those of
    // instances that no longer exist, and (deep) of instances not played for `unused_age` (made
    // again after their next start). A running game's cache cannot be removed and stays.
    let idle_since = now.checked_sub(policy.unused_age);
    for entry in read_dir(&cache.join(AOT_DIR)) {
        let Ok(meta) = entry.metadata() else { continue };
        if !meta.is_file() || !older(&meta, Duration::ZERO) {
            continue;
        }
        let name = entry.file_name().to_string_lossy().into_owned();
        let instance = name.split('.').next().unwrap_or_default();
        let orphan = !instance.is_empty() && !root.join("instances").join(instance).is_dir();
        let idle = deep
            && !instance.is_empty()
            && idle_since.is_some_and(|since| {
                let played = places
                    .last_played
                    .get(instance)
                    .and_then(|ms| u64::try_from(*ms).ok())
                    .map(|ms| UNIX_EPOCH + Duration::from_millis(ms));
                played.or(meta.modified().ok()).is_some_and(|at| at < since)
            });
        let running = in_use
            .game_dirs
            .iter()
            .any(|game| game.parent().and_then(Path::file_name) == Some(OsStr::new(instance)));
        if !automatic || orphan || (idle && !running) {
            file(entry.path(), &meta, LeftoverKind::StartCache);
        }
    }

    // Thumbnails of instances that no longer exist (automatic; each instance's own stale ones
    // go whenever its gallery is listed, `pl_mc::gamefiles::prune_thumbnails`).
    let mut orphans: Vec<Candidate> = Vec::new();
    if automatic {
        for entry in read_dir(&thumbs) {
            let orphan = entry
                .file_type()
                .is_ok_and(|t| t.is_dir() || t.is_symlink())
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
    let mut per_instance: HashMap<String, usize> = HashMap::new();
    for (index, (path, meta)) in captured.into_iter().enumerate() {
        // The newest few of each instance never go for their age (the cap still applies).
        let name = path.file_name().unwrap_or_default().to_string_lossy();
        let seen = per_instance
            .entry(captured_log_instance(&name))
            .or_default();
        let instance_rank = *seen;
        *seen += 1;
        if index < policy.captured_logs_keep || in_use_file(&path) || !older(&meta, Duration::ZERO)
        {
            kept_bytes += meta.len();
            continue;
        }
        let over_cap = kept_bytes + meta.len() > policy.captured_logs_max_bytes;
        let too_old = (deep || !automatic)
            && instance_rank >= policy.captured_logs_keep
            && older(&meta, policy.captured_log_age);
        if over_cap || too_old {
            file(path, &meta, LeftoverKind::Logs);
        } else {
            kept_bytes += meta.len();
        }
    }

    // Instance folders: on request, and automatically while "Otomatik Temizlik" is on; never the
    // running game's, and never through a link (a game folder kept on another disk).
    if !automatic || deep {
        for game in &places.game_dirs {
            if in_use.game_dirs.iter().any(|busy| busy == game) || is_link(game) {
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
            // JVM crash logs: the newest stays (support asks for it), the rest go after a day.
            let mut jvm_logs: Vec<(PathBuf, std::fs::Metadata)> = read_dir(game)
                .into_iter()
                .filter(|e| {
                    let name = e.file_name().to_string_lossy().to_ascii_lowercase();
                    name.starts_with("hs_err_pid") && name.ends_with(".log")
                })
                .filter_map(|e| Some((e.path(), e.metadata().ok().filter(|m| m.is_file())?)))
                .collect();
            jvm_logs.sort_by_key(|(_, meta)| Reverse(meta.modified().ok()));
            for (path, meta) in jvm_logs.into_iter().skip(1) {
                if older(&meta, policy.jvm_crash_age) {
                    file(path, &meta, LeftoverKind::CrashReports);
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

    // Installers the updater left in the system temp folder: a day later, or as soon as the
    // launcher runs that version or a newer one (the update went through).
    if let Some(temp) = &places.temp_dir {
        for (path, version) in update_leftovers(temp, &places.app_name) {
            let done = version_at_most(&version, &places.app_version);
            let wait = if done {
                policy.fresh
            } else {
                policy.leftover_age
            };
            let stale = newest_change(&path)
                .is_some_and(|changed| now.duration_since(changed).unwrap_or_default() >= wait);
            if stale {
                out.push(dir_candidate(path, LeftoverKind::Downloads));
            }
        }
    }
    out
}

/// Tauri's updater unpacks each update into `<temp>/<app name>-<version>-updater-<random>/` and
/// leaves it there. Only folders of exactly that shape holding nothing but installers count; each
/// comes with the version it installs.
fn update_leftovers(temp: &Path, app_name: &str) -> Vec<(PathBuf, String)> {
    if app_name.trim().is_empty() {
        return Vec::new();
    }
    let prefix = format!("{app_name}-");
    read_dir(temp)
        .into_iter()
        .filter(|entry| entry.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|entry| {
            let name = entry.file_name().to_string_lossy().into_owned();
            let rest = name.strip_prefix(&prefix)?;
            let (version, random) = rest.rsplit_once("-updater-")?;
            let shaped = version.starts_with(|c: char| c.is_ascii_digit())
                && !random.is_empty()
                && random.chars().all(|c| c.is_ascii_alphanumeric());
            shaped.then(|| (entry.path(), version.to_owned()))
        })
        .filter(|(dir, _)| {
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

/// `version` is `current` or older (dotted numbers; anything after `-` or `+` is ignored, an
/// unreadable version never counts).
fn version_at_most(version: &str, current: &str) -> bool {
    let parts = |v: &str| -> Option<Vec<u64>> {
        let core = v.split(['-', '+']).next()?.trim();
        if core.is_empty() {
            return None;
        }
        core.split('.').map(|p| p.parse().ok()).collect()
    };
    match (parts(version), parts(current)) {
        (Some(a), Some(b)) => a <= b,
        _ => false,
    }
}

/// The instance a captured log (`game-<instance>-<yyyyMMdd>-<HHmmss>.log`) belongs to.
fn captured_log_instance(name: &str) -> String {
    let stem = name
        .strip_prefix("game-")
        .and_then(|rest| rest.strip_suffix(".log"))
        .unwrap_or(name);
    let mut parts = stem.rsplitn(3, '-');
    let (_time, _date, instance) = (parts.next(), parts.next(), parts.next());
    instance.unwrap_or(stem).to_owned()
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
/// removed (in use) stay, and so do their folders. A link (symbolic link, junction) is removed
/// itself, never what it points at. Returns the bytes and files removed.
fn remove_tree(dir: &Path) -> (u64, u64) {
    if is_link(dir) {
        if let Err(error) = crate::links::remove(dir) {
            tracing::debug!(path = %dir.display(), %error, "kept a link");
        }
        return (0, 0);
    }
    let entries: Vec<_> = WalkDir::new(dir)
        .follow_links(false)
        .contents_first(true)
        .into_iter()
        .filter_map(Result::ok)
        .collect();
    let (mut bytes, mut files) = (0, 0);
    for entry in entries {
        if entry.path_is_symlink() || entry.file_type().is_symlink() {
            let _ = crate::links::remove(entry.path());
        } else if entry.file_type().is_dir() {
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

/// A symbolic link or (on Windows) a junction.
fn is_link(path: &Path) -> bool {
    std::fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink())
}

/// The entries of `dir`; none when `dir` is a link (nothing is judged through one).
fn read_dir(dir: &Path) -> Vec<std::fs::DirEntry> {
    if is_link(dir) {
        return Vec::new();
    }
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(Result::ok).collect())
        .unwrap_or_default()
}

/// Every file under `dir`, links neither followed nor listed; nothing when `dir` is a link.
fn walk_files(dir: &Path) -> impl Iterator<Item = walkdir::DirEntry> {
    let root = (!is_link(dir)).then(|| dir.to_path_buf());
    root.into_iter()
        .flat_map(|root| WalkDir::new(root).follow_links(false))
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

// ---------------------------------------------------------------------------------------------
// The automatic cleanup's own record

/// The file in the data folder with "Otomatik Temizlik" and the automatic rounds' record.
pub const STATE_FILE: &str = "cleanup-state.json";
/// Marks older than this are forgotten.
const MARK_FORGET: Duration = Duration::from_secs(365 * 24 * 60 * 60);
/// One writer at a time (a launch marking its version while a round saves).
static STATE_LOCK: Mutex<()> = Mutex::new(());

/// "Otomatik Temizlik" and what the automatic rounds remember: when the last round ran (one a
/// day, restarts included) and when each shared game file was last used. The launcher marks a
/// version and its Java whenever it readies them for a game; an unused item without a mark is
/// marked the first time a round finds it unused. Nothing goes before its mark is
/// [`Policy::unused_age`] old: the launcher's own count, file access times are not trusted.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct CleanupState {
    /// "Otomatik Temizlik" (on unless the player turned it off).
    pub auto: bool,
    /// When the last automatic round ran (ms since the Unix epoch).
    pub last_round_ms: i64,
    /// `version:<id>` / `runtime:<component>` → last readied for a game.
    used: BTreeMap<String, i64>,
    /// Unused shared items (by [`pl_mc::shared_files::SharedItem::id`]) → unused since.
    unused: BTreeMap<String, i64>,
}

impl Default for CleanupState {
    fn default() -> Self {
        Self {
            auto: true,
            last_round_ms: 0,
            used: BTreeMap::new(),
            unused: BTreeMap::new(),
        }
    }
}

impl CleanupState {
    /// The record in the data folder `root`; the defaults when it is missing or unreadable.
    pub fn load(root: &Path) -> Self {
        std::fs::read(root.join(STATE_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
    }

    /// Reads the record, changes it with `change` and stores it again, one caller at a time.
    pub fn update<T>(root: &Path, change: impl FnOnce(&mut Self) -> T) -> pl_mc::Result<T> {
        let _guard = STATE_LOCK
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        let mut state = Self::load(root);
        let out = change(&mut state);
        let bytes = serde_json::to_vec_pretty(&state).unwrap_or_default();
        pl_mc::paths::write_durable_blocking(&root.join(STATE_FILE), &bytes)?;
        Ok(out)
    }

    /// Whether an automatic round is due at `now_ms` (`every` since the last; a clock that went
    /// back counts as due).
    pub fn round_due(&self, now_ms: i64, every: Duration) -> bool {
        let every = i64::try_from(every.as_millis()).unwrap_or(i64::MAX);
        now_ms < self.last_round_ms || now_ms.saturating_sub(self.last_round_ms) >= every
    }

    /// Marks a version and its Java as used at `now_ms` (and forgets marks older than a year).
    pub fn mark_used(&mut self, version: &str, java_component: Option<&str>, now_ms: i64) {
        let forget = i64::try_from(MARK_FORGET.as_millis()).unwrap_or(i64::MAX);
        self.used
            .retain(|_, at| now_ms.saturating_sub(*at) < forget);
        if !version.trim().is_empty() {
            self.used.insert(format!("version:{version}"), now_ms);
        }
        if let Some(component) = java_component.filter(|c| !c.trim().is_empty()) {
            self.used.insert(format!("runtime:{component}"), now_ms);
        }
    }

    /// Names (`<id>` of `<prefix><id>`) used at or after `since_ms`.
    fn used_since(&self, prefix: &str, since_ms: i64) -> Vec<String> {
        self.used
            .iter()
            .filter(|(_, at)| **at >= since_ms)
            .filter_map(|(key, _)| key.strip_prefix(prefix).map(str::to_owned))
            .collect()
    }

    /// Splits unused items into those unused since `cutoff_ms` or earlier (due) and the rest.
    /// An item met for the first time counts as unused since its last use, or from `now_ms`.
    fn judge(&mut self, ids: &[String], now_ms: i64, cutoff_ms: i64) -> (Vec<String>, Vec<String>) {
        let (mut due, mut waiting) = (Vec::new(), Vec::new());
        for id in ids {
            let last_use = self.used.get(id).copied();
            let since = *self
                .unused
                .entry(id.clone())
                .or_insert_with(|| last_use.unwrap_or(now_ms).min(now_ms));
            if since.max(last_use.unwrap_or(i64::MIN)) <= cutoff_ms {
                due.push(id.clone());
            } else {
                waiting.push(id.clone());
            }
        }
        (due, waiting)
    }
}

/// What [`sweep_shared`] removed.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SweepReport {
    /// Items deleted (as [`pl_mc::shared_files::SharedItem::id`]).
    pub items: Vec<String>,
    pub deleted: SharedDeleteReport,
}

/// The automatic round's share of the shared game files (`pl_mc::shared_files`): what no
/// instance, no parked set and no recent start needs, unused for [`Policy::unused_age`] by
/// `state`'s marks. A version that waits keeps everything it needs (its Java, libraries,
/// assets); libraries and assets nothing waiting needs go with the versions that go, or once
/// their own mark is old enough. `refs` and `protected` are what the manual cleanup uses. Stops
/// between items when `cancel` fires (the rest goes next time). Blocking.
pub fn sweep_shared(
    dirs: &DataDirs,
    refs: &[GameRef],
    protected: &[PathBuf],
    state: &mut CleanupState,
    policy: &Policy,
    now: SystemTime,
    cancel: &CancellationToken,
) -> pl_mc::Result<SweepReport> {
    let now_ms = millis(now);
    let unused_ms = i64::try_from(policy.unused_age.as_millis()).unwrap_or(i64::MAX);
    let cutoff = now_ms.saturating_sub(unused_ms);
    let mut protected = protected.to_vec();
    protected.extend(
        state
            .used_since("runtime:", cutoff.saturating_add(1))
            .iter()
            .map(|component| dirs.runtimes().join(component)),
    );
    let mut options = ScanOptions {
        now,
        protected,
        keep: state.used_since("version:", cutoff.saturating_add(1)),
        ..ScanOptions::default()
    };
    let is_version =
        |kind: SharedKind| matches!(kind, SharedKind::GameVersion | SharedKind::LoaderProfile);

    // Versions first: those still waiting keep what they need in the second look.
    let first = shared_files::scan(dirs, refs, &options)?;
    let versions: Vec<String> = first
        .items
        .iter()
        .filter(|item| is_version(item.kind))
        .map(|item| item.id.clone())
        .collect();
    let (due_versions, waiting) = state.judge(&versions, now_ms, cutoff);
    options.keep.extend(
        waiting
            .iter()
            .filter_map(|id| id.strip_prefix("version:").map(str::to_owned)),
    );
    let second = shared_files::scan(dirs, refs, &options)?;
    let others: Vec<String> = second
        .items
        .iter()
        .filter(|item| !is_version(item.kind))
        .map(|item| item.id.clone())
        .collect();
    let (mut due_others, waiting_others) = state.judge(&others, now_ms, cutoff);
    if !due_versions.is_empty() {
        // Java, libraries and assets that nothing waiting and nothing started recently needs
        // leave with their versions.
        due_others.extend(waiting_others);
    }
    // Items no longer unused lose their mark.
    let unused_now: HashSet<&String> = versions.iter().chain(&others).collect();
    state.unused.retain(|id, _| unused_now.contains(id));

    let chosen: Vec<String> = due_versions.into_iter().chain(due_others).collect();
    // Marks of what goes are dropped by the next round's look (an item a file in use kept stays
    // due).
    let deleted = shared_files::delete(dirs, refs, &options, &chosen, cancel, &|_, _| {})?;
    if !chosen.is_empty() {
        tracing::info!(items = ?chosen, freed = deleted.freed_bytes, "removed unused game files");
    }
    Ok(SweepReport {
        items: chosen,
        deleted,
    })
}

fn millis(at: SystemTime) -> i64 {
    at.duration_since(UNIX_EPOCH)
        .ok()
        .and_then(|d| i64::try_from(d.as_millis()).ok())
        .unwrap_or(i64::MAX)
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
            ..Places::default()
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
        let report = tidy(&places(root), &policy, &none, now, None, false);
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
    fn aot_caches_go_on_request_and_with_their_instance() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::create_dir_all(root.join("instances/a/minecraft")).unwrap();
        for path in [
            "cache/aot/a.0123456789abcdef.aot",
            "cache/aot/gone.0123456789abcdef.aotconf",
        ] {
            write(&root.join(path), 10);
            age(&root.join(path), 1);
        }
        // Just written (a cache being made): left alone.
        write(&root.join("cache/aot/a.0123456789abcdef.tries"), 1);
        let policy = Policy::default();
        let none = InUse::default();
        let now = SystemTime::now();
        assert_eq!(
            kinds(&scan_leftovers(&places(root), &policy, &none, now)),
            [(LeftoverKind::StartCache, 20, 2)]
        );
        // Automatically only a deleted instance's files go.
        let report = tidy(&places(root), &policy, &none, now, None, false);
        assert_eq!(report.junk.removed_files, 1);
        assert!(root.join("cache/aot/a.0123456789abcdef.aot").is_file());
        assert!(
            !root
                .join("cache/aot/gone.0123456789abcdef.aotconf")
                .exists()
        );
        let cleaned = clean_leftovers(
            &places(root),
            &policy,
            &none,
            now,
            &[LeftoverKind::StartCache],
        );
        assert_eq!(cleaned.removed_files, 1);
        assert!(!root.join("cache/aot/a.0123456789abcdef.aot").exists());
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
        let report = tidy(&places(root), &policy, &none, later, None, false);
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
        let report = tidy(
            &places(root),
            &policy,
            &in_use,
            SystemTime::now(),
            None,
            false,
        );
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
        // The newest JVM crash log stays whatever its age (support asks for it).
        write(&game.join("hs_err_pid456.log"), 40);
        age(&game.join("hs_err_pid456.log"), 2);
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
        assert!(game.join("hs_err_pid456.log").exists());
        assert!(!game.join("hs_err_pid123.log").exists());
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
            temp_dir: Some(temp.clone()),
            app_name: "Projects Launcher".into(),
            ..Places::default()
        };
        // Folders themselves are "changed" now; two days later they count as stale.
        let later = SystemTime::now() + 2 * DAY;
        let groups = scan_leftovers(&places, &Policy::default(), &InUse::default(), later);
        assert_eq!(kinds(&groups), [(LeftoverKind::Downloads, 1000, 1)]);
        tidy(
            &places,
            &Policy::default(),
            &InUse::default(),
            later,
            None,
            false,
        );
        assert!(!ours.exists());
        assert!(other_app.exists() && mixed.exists() && odd.exists());
        // Without a product name nothing in the temp folder is judged.
        assert!(update_leftovers(&temp, "").is_empty());

        // Once the launcher runs the version an installer brought (or a newer one), its folder
        // goes at the next round; a newer version's waits its day.
        let done = temp.join("Projects Launcher-2.1.4-updater-Done1");
        write(&done.join("Projects Launcher-2.1.4-installer.exe"), 10);
        let next = temp.join("Projects Launcher-2.1.5-updater-Next1");
        write(&next.join("Projects Launcher-2.1.5-installer.exe"), 10);
        let places = Places {
            app_version: "2.1.4".into(),
            ..places
        };
        let soon = SystemTime::now() + Duration::from_secs(11 * 60);
        tidy(
            &places,
            &Policy::default(),
            &InUse::default(),
            soon,
            None,
            false,
        );
        assert!(!done.exists());
        assert!(next.exists());
        assert!(version_at_most("2.1.3", "2.1.4") && version_at_most("2.1.4", "2.1.4+e2e"));
        assert!(!version_at_most("2.1.10", "2.1.4") && !version_at_most("beta", "2.1.4"));
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
            false,
        );
        assert_eq!(report.trash.removed, 1, "over the cap: the oldest goes");
        let report = tidy(
            &places(root),
            &Policy::default(),
            &InUse::default(),
            SystemTime::now() + 31 * DAY,
            Some(1000),
            false,
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

    fn ms(at: SystemTime) -> i64 {
        millis(at)
    }

    #[test]
    fn deep_round_cleans_logs_crashes_and_caches_but_never_the_players_files() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        let game = root.join("instances/a/minecraft");
        let idle_game = root.join("instances/idle/minecraft");
        // The player's files, however old: never touched.
        let players = [
            "saves/World/level.dat",
            "saves/World/region/r.0.0.mca",
            "screenshots/2025-01-01_10.00.00.png",
            "mods/sodium.jar",
            "resourcepacks/pack.zip",
            "shaderpacks/shader.zip",
            "backups/World-2025.zip",
            "options.txt",
            "servers.dat",
            "config/sodium.json",
            "logs/latest.log",
        ];
        for rel in players {
            write(&game.join(rel), 10);
            age(&game.join(rel), 400);
        }
        // Old logs, crash reports and JVM crash logs.
        write(&game.join("logs/2025-01-01-1.log.gz"), 11);
        age(&game.join("logs/2025-01-01-1.log.gz"), 60);
        for i in 0..7 {
            let report = game.join(format!("crash-reports/crash-2025-0{i}.txt"));
            write(&report, 20);
            age(&report, 40 + i);
        }
        for (pid, days) in [(1, 2), (2, 5), (3, 9)] {
            let file = game.join(format!("hs_err_pid{pid}.log"));
            write(&file, 30);
            age(&file, days);
        }
        // Captured game logs: instance `a` has eight, three of them old.
        for i in 0..8u64 {
            let log = root.join(format!("logs/game-a-2026100{i}-120000.log"));
            write(&log, 10);
            age(&log, if i < 3 { 20 + i } else { 1 });
        }
        // Caches over their caps; start caches of an instance played long ago.
        for i in 0..4u64 {
            let icon = root.join(format!("cache/modrinth/icons/{i}.png"));
            write(&icon, 100);
            age(&icon, 1 + i);
        }
        fs::create_dir_all(&idle_game).unwrap();
        for name in ["a.0123456789abcdef.aot", "idle.0123456789abcdef.aot"] {
            write(&root.join("cache/aot").join(name), 50);
            age(&root.join("cache/aot").join(name), 30);
        }
        let now = SystemTime::now();
        let places = Places {
            root: root.to_path_buf(),
            game_dirs: vec![game.clone(), idle_game],
            last_played: HashMap::from([
                ("a".to_owned(), ms(now - DAY)),
                ("idle".to_owned(), ms(now - 20 * DAY)),
            ]),
            ..Places::default()
        };
        let policy = Policy {
            icons_max_bytes: 250,
            ..Policy::default()
        };

        // "Otomatik Temizlik" off: none of it goes.
        let report = tidy(&places, &policy, &InUse::default(), now, None, false);
        assert_eq!(report.junk.removed_files, 0);

        let report = tidy(&places, &policy, &InUse::default(), now, None, true);
        assert!(!game.join("logs/2025-01-01-1.log.gz").exists());
        assert_eq!(fs::read_dir(game.join("crash-reports")).unwrap().count(), 5);
        assert!(
            game.join("hs_err_pid1.log").exists(),
            "the newest JVM crash log stays"
        );
        assert!(!game.join("hs_err_pid2.log").exists() && !game.join("hs_err_pid3.log").exists());
        let captured = fs::read_dir(root.join("logs")).unwrap().count();
        assert_eq!(captured, 5, "the newest five of the instance stay");
        // Icons: the two newest fit under the cap.
        assert!(root.join("cache/modrinth/icons/0.png").exists());
        assert!(root.join("cache/modrinth/icons/1.png").exists());
        assert!(!root.join("cache/modrinth/icons/2.png").exists());
        assert!(!root.join("cache/modrinth/icons/3.png").exists());
        assert!(root.join("cache/aot/a.0123456789abcdef.aot").exists());
        assert!(!root.join("cache/aot/idle.0123456789abcdef.aot").exists());
        assert_eq!(report.junk.removed_files, 1 + 2 + 2 + 3 + 2 + 1);
        for rel in players {
            assert!(game.join(rel).exists(), "{rel} is the player's");
        }
    }

    #[test]
    fn links_are_removed_never_followed() {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path().join("root");
        let outside = dir.path().join("outside");
        for rel in [
            "thumbs/1.jpg",
            "game-logs/2025-01-01-1.log.gz",
            "libs/x.jar.part",
        ] {
            write(&outside.join(rel), 10);
            age(&outside.join(rel), 60);
        }
        fs::create_dir_all(root.join("cache/thumbs/gone")).unwrap();
        fs::create_dir_all(root.join("instances/a/minecraft")).unwrap();
        // An orphan thumbnail folder that is a link, one holding a link, a linked game log
        // folder and a linked shared folder.
        crate::links::create(
            &root.join("cache/thumbs/linked"),
            &outside.join("thumbs"),
            true,
        )
        .unwrap();
        crate::links::create(
            &root.join("cache/thumbs/gone/inner"),
            &outside.join("thumbs"),
            true,
        )
        .unwrap();
        crate::links::create(
            &root.join("instances/a/minecraft/logs"),
            &outside.join("game-logs"),
            true,
        )
        .unwrap();
        crate::links::create(&root.join("libraries"), &outside.join("libs"), true).unwrap();

        let later = SystemTime::now() + 2 * DAY;
        tidy(
            &places(&root),
            &Policy::default(),
            &InUse::default(),
            later,
            None,
            true,
        );
        // The links went (or stayed), never what they point at.
        assert!(outside.join("thumbs/1.jpg").is_file());
        assert!(outside.join("game-logs/2025-01-01-1.log.gz").is_file());
        assert!(outside.join("libs/x.jar.part").is_file());
        assert!(!root.join("cache/thumbs/linked").exists());
        assert!(!root.join("cache/thumbs/gone").exists());
        assert!(root.join("libraries").exists());
    }

    #[test]
    fn cleanup_state_marks_rounds_and_survives_a_restart() {
        let dir = tempfile::tempdir().unwrap();
        let day = 24 * 60 * 60 * 1000_i64;
        let state = CleanupState::load(dir.path());
        assert!(state.auto, "on by default");
        assert!(state.round_due(10 * day, Policy::default().round_every));

        CleanupState::update(dir.path(), |state| {
            state.last_round_ms = 10 * day;
            state.mark_used(
                "fabric-loader-0.19.5-26.1.2",
                Some("java-runtime-epsilon"),
                10 * day,
            );
        })
        .unwrap();
        let mut state = CleanupState::load(dir.path());
        let every = Policy::default().round_every;
        assert!(!state.round_due(10 * day + day / 2, every));
        assert!(state.round_due(11 * day, every));
        assert!(state.round_due(9 * day, every), "a clock that went back");
        assert_eq!(
            state.used_since("version:", 5 * day),
            ["fabric-loader-0.19.5-26.1.2"]
        );
        assert!(state.used_since("version:", 11 * day).is_empty());
        assert_eq!(
            state.used_since("runtime:", 5 * day),
            ["java-runtime-epsilon"]
        );

        // Unused since its last use, or from the first time a round finds it.
        let ids = [
            "version:fabric-loader-0.19.5-26.1.2".to_owned(),
            "version:1.20.1".to_owned(),
        ];
        let (due, waiting) = state.judge(&ids, 20 * day, 20 * day - 14 * day);
        assert!(due.is_empty());
        assert_eq!(waiting.len(), 2);
        let (due, _) = state.judge(&ids, 25 * day, 25 * day - 14 * day);
        assert_eq!(due, ["version:fabric-loader-0.19.5-26.1.2"]);
        let (due, _) = state.judge(&ids, 34 * day, 34 * day - 14 * day);
        assert_eq!(due.len(), 2);

        // Broken file: the defaults (automatic cleanup on, nothing marked).
        std::fs::write(dir.path().join(STATE_FILE), b"{not json").unwrap();
        assert_eq!(CleanupState::load(dir.path()), CleanupState::default());
        CleanupState::update(dir.path(), |state| state.auto = false).unwrap();
        assert!(!CleanupState::load(dir.path()).auto);
    }

    /// A data folder with game versions 1.21.1 (an instance uses it) and 1.20.1 (none does), each
    /// with its library, Java, assets and natives, plus a library nothing lists.
    fn shared_fixture(root: &Path) -> DataDirs {
        let dirs = DataDirs::new(root);
        dirs.ensure().unwrap();
        for (id, component, index, hash) in [
            ("1.21.1", "java-runtime-delta", "17", "aa11"),
            ("1.20.1", "java-runtime-gamma", "5", "bb22"),
        ] {
            let json = format!(
                r#"{{"id":"{id}","mainClass":"M","assetIndex":{{"id":"{index}"}},
                "javaVersion":{{"component":"{component}","majorVersion":21}},
                "libraries":[{{"name":"com.example:lib-{id}:1.0","downloads":{{"artifact":{{"path":"com/example/lib-{id}.jar","url":"https://x/l.jar"}}}}}}]}}"#
            );
            write_text(&dirs.version_json(id).unwrap(), &json);
            write(&dirs.version_jar(id).unwrap(), 100);
            write(
                &dirs.libraries().join(format!("com/example/lib-{id}.jar")),
                10,
            );
            write_text(
                &dirs.asset_index_file(index).unwrap(),
                &format!(r#"{{"objects":{{"a":{{"hash":"{hash}","size":3}}}}}}"#),
            );
            write(&dirs.asset_object(hash).unwrap(), 3);
            let java = dirs.runtime_dir(component, "windows-x64").unwrap();
            write(&java.join("bin/javaw.exe"), 50);
            write(
                &dirs
                    .natives()
                    .join(format!("{id}-0123456789"))
                    .join("x.dll"),
                5,
            );
        }
        write(&dirs.libraries().join("com/example/orphan.jar"), 7);
        dirs
    }

    fn write_text(path: &Path, text: &str) {
        fs::create_dir_all(path.parent().unwrap()).unwrap();
        fs::write(path, text).unwrap();
    }

    #[test]
    fn unused_game_files_go_after_fourteen_unused_days() {
        let dir = tempfile::tempdir().unwrap();
        let dirs = shared_fixture(dir.path());
        let refs = [GameRef::new("1.21.1", None, None)];
        let policy = Policy::default();
        let mut state = CleanupState::default();
        let none = CancellationToken::new();
        let start = SystemTime::now() + 2 * DAY;
        let old = dirs.version_dir("1.20.1").unwrap();
        let gamma = dirs.runtimes().join("java-runtime-gamma");
        let old_lib = dirs.libraries().join("com/example/lib-1.20.1.jar");
        let orphan = dirs.libraries().join("com/example/orphan.jar");

        // First seen unused: nothing goes, and thirteen days later still nothing.
        for days in [0, 13] {
            let report = sweep_shared(
                &dirs,
                &refs,
                &[],
                &mut state,
                &policy,
                start + days * DAY,
                &none,
            )
            .unwrap();
            assert!(report.items.is_empty(), "day {days}: {:?}", report.items);
        }
        assert!(old.is_dir() && gamma.is_dir() && old_lib.is_file() && orphan.is_file());

        // Started on day 10 (the launcher marked it): it and what it needs wait fourteen days
        // from then; the library nothing lists has been unused for fourteen days and goes.
        state.mark_used("1.20.1", Some("java-runtime-gamma"), ms(start + 10 * DAY));
        let report = sweep_shared(
            &dirs,
            &refs,
            &[],
            &mut state,
            &policy,
            start + 15 * DAY,
            &none,
        )
        .unwrap();
        assert_eq!(report.items, ["libraries"]);
        assert!(!orphan.exists());
        assert!(old.is_dir() && gamma.is_dir() && old_lib.is_file());

        // A cancelled round stops before deleting anything; the next one does the work.
        let cancelled = CancellationToken::new();
        cancelled.cancel();
        let at = start + 25 * DAY;
        let result = sweep_shared(&dirs, &refs, &[], &mut state, &policy, at, &cancelled);
        assert!(result.is_err());
        assert!(old.is_dir());
        let report = sweep_shared(&dirs, &refs, &[], &mut state, &policy, at, &none).unwrap();
        assert_eq!(
            report.items,
            [
                "version:1.20.1",
                "runtime:java-runtime-gamma",
                "libraries",
                "assets"
            ]
        );
        // The unused version left with its Java, library, assets and natives.
        assert!(!old.exists() && !gamma.exists() && !old_lib.exists());
        assert!(!dirs.asset_object("bb22").unwrap().exists());
        assert!(!dirs.natives().join("1.20.1-0123456789").exists());
        // What the instance uses stays.
        assert!(dirs.version_dir("1.21.1").unwrap().is_dir());
        assert!(dirs.runtimes().join("java-runtime-delta").is_dir());
        assert!(
            dirs.libraries()
                .join("com/example/lib-1.21.1.jar")
                .is_file()
        );
        assert!(dirs.asset_object("aa11").unwrap().is_file());
        assert!(dirs.natives().join("1.21.1-0123456789").is_dir());
        // Nothing left to do: the next round is quiet.
        let report = sweep_shared(&dirs, &refs, &[], &mut state, &policy, at + DAY, &none).unwrap();
        assert!(report.items.is_empty());
    }
}
