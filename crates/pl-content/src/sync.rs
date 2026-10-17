//! Modpack sync: diff a pack manifest against a game directory, then apply the difference.
//!
//! Ownership is tracked in `<game_dir>/.projects/pack-state.json` (what we installed and with
//! which hash), which gives the rules:
//! - only files the pack installed earlier and that are no longer wanted (left the manifest,
//!   item switched off, group disabled, now server-only) are deleted, and only while they still
//!   have the hash we installed (user-added files are never touched);
//! - `overwrite` files always end up matching the pack, except jars in `mods/`: a jar the player
//!   replaced is kept, like `preserveUserChanges` (the mod center offers to bring the pack's
//!   file back); `ifMissing` files are installed only when absent; `preserveUserChanges` files
//!   are updated only while unchanged since we installed them;
//! - items ([`crate::items`]) are installed while effectively enabled. Changes the player made
//!   to item files are adopted, never fought ([`ManualChange`]): a deleted or disabled file
//!   switches its item off (it comes back only when an enabled item requires it) and a
//!   disabled copy is kept while the item is off; a disabled copy renamed back switches the item
//!   on. When the mods folder vanished, or most pack mods disappeared at once, the set counts as
//!   wiped and the selection is installed again instead ([`SyncPlan::reset`]);
//! - any other pack file the player disabled (`<path>.disabled`) stays disabled;
//! - optional groups switch the files no item covers; server-only files are skipped;
//! - a path whose spelling only changes in letter case between versions (`config/Foo.toml` →
//!   `config/foo.toml`) keeps its file where the disk sees both as one (Windows, macOS): the
//!   record carries over, nothing is deleted, and the file takes the new spelling;
//! - files that cannot be read, renamed or deleted never stop a sync: they are left as they are
//!   and reported ([`SyncIssue`]);
//! - every path is validated (no absolute paths, `..`, drive letters, `.projects/`).
//!
//! Per-version sets ([`crate::sets`]): every Minecraft version of an instance keeps its own
//! [`PARKED_DIRS`] and pack state. [`PackState::switch_version`] splits the state when the
//! instance moves to another version.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;
use pl_net::{Checksum, DownloadItem, DownloadOptions, DownloadProgress, Downloader, HashAlgo};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil::{normalize_hex, read_optional, write_durable};
use crate::items::{
    AdoptedChange, ItemStatus, ManualChange, PackSelection, installed_record, resolve_items,
    resolve_with,
};
use crate::model::{EnvSupport, FilePolicy, PackFile, PackItem, PackManifest, ProjectsExtension};
use crate::path::{LAUNCHER_DIR, check_relative_path, join_relative};

/// File name of the pack state inside `<game_dir>/.projects/` and inside a parked set folder.
pub const PACK_STATE_FILE: &str = "pack-state.json";
/// Top-level game folders that belong to one Minecraft version and move with its set. Other
/// folders (resource packs, shader packs, `options.txt`, worlds, `servers.dat`) stay shared.
pub const PARKED_DIRS: [&str; 2] = ["mods", "config"];
/// The [`PARKED_DIRS`] a first visit of a version copies from the version the player came
/// from; the others start empty.
pub const COPIED_DIRS: [&str; 1] = ["config"];
const DISABLED_SUFFIX: &str = ".disabled";
const HASH_CONCURRENCY: usize = 8;
/// A sync that finds at least this many pack mods gone at once, and more than half of them,
/// treats the mods folder as wiped and installs the selection again.
const RESET_MIN_MISSING: usize = 3;

/// What the launcher installed into a game directory: the active set of a pack.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PackState {
    pub pack_id: String,
    pub version_id: String,
    /// Family of the installed pack (`projects.family`, else the pack id).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub family: String,
    /// Minecraft version of the installed pack; it names the set.
    #[serde(skip_serializing_if = "String::is_empty")]
    pub minecraft: String,
    /// Relative path → the file as we last installed it.
    pub files: BTreeMap<String, InstalledFile>,
    /// Optional group id → enabled, as chosen at the last sync.
    pub groups: BTreeMap<String, bool>,
    /// Item id → the player's choice. Defaults are recorded on the first sync; items added
    /// later have no entry until the player picks them.
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub items: BTreeMap<String, bool>,
    /// Item ids the player has been shown; manifest items missing here are new.
    #[serde(skip_serializing_if = "BTreeSet::is_empty")]
    pub seen_items: BTreeSet<String>,
    /// Manual changes a sync adopted that the player has not confirmed in the mod center yet
    /// (item id → change).
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    pub manual: BTreeMap<String, ManualChange>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstalledFile {
    pub sha1: String,
    #[serde(default)]
    pub policy: FilePolicy,
    /// The item that installed the file (none for files outside the catalog).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// The player switched the item off by renaming its file: it sits at `<path>.disabled`
    /// and is kept while the item is off.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub disabled: bool,
    /// At the last sync the file on disk was not this one: the player replaced it, so it is
    /// left alone.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub modified: bool,
}

impl InstalledFile {
    fn new(sha1: String, policy: FilePolicy, item: Option<String>) -> Self {
        Self {
            sha1,
            policy,
            item,
            disabled: false,
            modified: false,
        }
    }
}

/// Result of [`PackState::switch_version`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VersionSwitch {
    /// The leaving set: its records under [`PARKED_DIRS`] and its choices. It is saved next
    /// to the parked folders.
    pub parked: PackState,
    /// The new `pack-state.json`. The next [`plan_sync`] with the target manifest starts
    /// from it.
    pub active: PackState,
}

impl PackState {
    /// `<game_dir>/.projects/pack-state.json`: the active set.
    pub fn path(game_dir: &Path) -> PathBuf {
        game_dir.join(LAUNCHER_DIR).join(PACK_STATE_FILE)
    }

    /// Loads the active state; `None` when there is none. A corrupt file is reported and
    /// treated as absent: the next sync then deletes nothing and keeps changed configs.
    pub async fn load(game_dir: &Path) -> Result<Option<Self>> {
        Self::load_from(&Self::path(game_dir)).await
    }

    /// Writes the active state atomically.
    pub async fn save(&self, game_dir: &Path) -> Result<()> {
        self.save_to(&Self::path(game_dir)).await
    }

    /// Splits this (active) state when the instance moves from `from_minecraft` to
    /// `to_minecraft`. `returning` is the target set's parked state; `None` on the first
    /// visit.
    ///
    /// - The parked state keeps this state's records under [`PARKED_DIRS`] and its choices.
    /// - Returning: the new active state takes the target's records under [`PARKED_DIRS`],
    ///   this state's records elsewhere (shared files stay where they are) and the target's
    ///   choices.
    /// - First visit: the choices carry over (items the target's pack has and this set never
    ///   saw are new and stay off) together with the records under [`COPIED_DIRS`], which the
    ///   switch copies; no `mods/` record carries over, so the next sync installs the target's
    ///   mods. The state names no pack version until that sync.
    pub fn switch_version(
        self,
        from_minecraft: &str,
        to_minecraft: &str,
        returning: Option<PackState>,
    ) -> VersionSwitch {
        let PackState {
            pack_id,
            version_id,
            family,
            minecraft: _,
            files,
            groups,
            items,
            seen_items,
            manual,
        } = self;
        let (parked_files, shared_files): (BTreeMap<_, _>, BTreeMap<_, _>) = files
            .into_iter()
            .partition(|(path, _)| in_dirs(path, &PARKED_DIRS));
        let active = match returning {
            Some(returning) => {
                let mut files: BTreeMap<String, InstalledFile> = returning
                    .files
                    .into_iter()
                    .filter(|(path, _)| in_dirs(path, &PARKED_DIRS))
                    .collect();
                files.extend(shared_files);
                PackState {
                    family: if returning.family.is_empty() {
                        family.clone()
                    } else {
                        returning.family
                    },
                    minecraft: to_minecraft.to_owned(),
                    files,
                    ..returning
                }
            }
            None => {
                let mut files: BTreeMap<String, InstalledFile> = parked_files
                    .iter()
                    .filter(|(path, _)| in_dirs(path, &COPIED_DIRS))
                    .map(|(path, record)| (path.clone(), record.clone()))
                    .collect();
                files.extend(shared_files);
                PackState {
                    pack_id: String::new(),
                    version_id: String::new(),
                    family: family.clone(),
                    minecraft: to_minecraft.to_owned(),
                    files,
                    groups: groups.clone(),
                    items: items.clone(),
                    seen_items: seen_items.clone(),
                    manual: BTreeMap::new(),
                }
            }
        };
        VersionSwitch {
            parked: PackState {
                pack_id,
                version_id,
                family,
                minecraft: from_minecraft.to_owned(),
                files: parked_files,
                groups,
                items,
                seen_items,
                manual,
            },
            active,
        }
    }

    /// The player looked at the mod center without applying anything: records the current
    /// choices, marks every item of `manifest` as seen (clears the "new" flags) and
    /// acknowledges adopted manual changes, without touching files. Save the state afterwards.
    pub fn mark_items_seen(&mut self, manifest: &PackManifest) {
        let resolution = resolve_items(manifest, Some(&*self), Some(&PackSelection::default()));
        self.items = resolution.choices;
        self.seen_items = resolution.seen;
        self.manual.clear();
    }

    pub(crate) async fn load_from(path: &Path) -> Result<Option<Self>> {
        let Some(bytes) = read_optional(path).await? else {
            return Ok(None);
        };
        match serde_json::from_slice(&bytes) {
            Ok(state) => Ok(Some(state)),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "ignoring a corrupt pack state");
                Ok(None)
            }
        }
    }

    pub(crate) async fn save_to(&self, path: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| Error::json(PACK_STATE_FILE, e))?;
        // The pack state records the player's choices: flushed before it replaces the old one.
        write_durable(path, &bytes).await
    }
}

/// True when `path`'s first component is one of `dirs` (case-insensitively).
pub(crate) fn in_dirs(path: &str, dirs: &[&str]) -> bool {
    let first = path.split('/').next().unwrap_or(path);
    dirs.iter().any(|dir| first.eq_ignore_ascii_case(dir))
}

/// Knobs for [`plan_sync`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct SyncPolicy {
    /// "Repair": make every pack file match the pack again, overriding `ifMissing` and
    /// `preserveUserChanges` (user-added and disabled files are still left alone).
    pub repair: bool,
    /// Skip hashing files whose size matches and whose recorded hash already equals the
    /// manifest (fast repeat launches). Removals are always hashed.
    pub quick_check: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FileCategory {
    Mod,
    ResourcePack,
    ShaderPack,
    Config,
    Other,
}

impl FileCategory {
    pub fn of(path: &str) -> Self {
        let (first, rest) = path.split_once('/').unwrap_or(("", path));
        let is = |name: &str| first.eq_ignore_ascii_case(name);
        if is("mods") {
            Self::Mod
        } else if is("resourcepacks") {
            Self::ResourcePack
        } else if is("shaderpacks") {
            Self::ShaderPack
        } else if is("config")
            || is("defaultconfigs")
            || (first.is_empty()
                && [
                    ".txt",
                    ".json",
                    ".json5",
                    ".toml",
                    ".properties",
                    ".cfg",
                    ".yml",
                    ".yaml",
                ]
                .iter()
                .any(|ext| rest.to_ascii_lowercase().ends_with(ext)))
        {
            Self::Config
        } else {
            Self::Other
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum DownloadAction {
    /// The file does not exist yet.
    Add,
    /// An existing file is replaced (or, for an item, its old file is replaced by a new one).
    Update,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlannedDownload {
    pub path: String,
    pub urls: Vec<String>,
    pub sha1: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
    pub size: u64,
    pub action: DownloadAction,
    pub category: FileCategory,
    /// The item this file belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
}

/// An enabled item whose disabled copy (`<path>.disabled`) is exactly the pack's file: it is
/// renamed back instead of downloaded.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlannedRestore {
    pub path: String,
    pub size: u64,
    pub category: FileCategory,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlannedDelete {
    /// The file to delete (possibly `<path>.disabled`).
    pub path: String,
    pub category: FileCategory,
    /// Size on disk when planned, in bytes.
    #[serde(default)]
    pub size: u64,
    /// The item that installed the file.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub item: Option<String>,
    /// The file is superseded by a download of the same item (an update, not a removal).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub replaced: bool,
    /// The state record behind the file, put back when the deletion fails so the next sync
    /// tries again.
    #[serde(skip)]
    pub(crate) record: Option<(String, InstalledFile)>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum KeepReason {
    /// The player changed a file the pack manages (or owns a file at that path).
    UserModified,
    /// `ifMissing`: the file exists, so it is left as is.
    ExistingFile,
    /// The player disabled it (`<path>.disabled`; files outside the item catalog only).
    UserDisabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeptFile {
    pub path: String,
    pub reason: KeepReason,
    pub category: FileCategory,
}

/// Why a file was left alone without a decision.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SyncIssueKind {
    /// The file could not be read (permissions, an antivirus, a disk error).
    Unreadable,
    /// Something other than a file (a folder) is where the pack's file belongs.
    NotAFile,
    /// The file could not be renamed or deleted (another program has it open).
    Busy,
}

/// A file the sync could not handle. It never stops the sync or the launch; the file stays as
/// it is and the next sync looks at it again.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncIssue {
    pub path: String,
    pub kind: SyncIssueKind,
    /// Technical details for logs and support.
    pub detail: String,
}

/// The difference between a manifest and a game directory. Nothing on disk changes until
/// [`apply_sync`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncPlan {
    pub game_dir: PathBuf,
    pub pack_id: String,
    pub version_id: String,
    pub downloads: Vec<PlannedDownload>,
    /// Disabled copies of enabled items renamed back to their path.
    pub restores: Vec<PlannedRestore>,
    pub deletes: Vec<PlannedDelete>,
    pub unchanged: Vec<String>,
    pub kept_user_modified: Vec<KeptFile>,
    /// The mod center view of the manifest's items under this plan's selection.
    pub items: Vec<ItemStatus>,
    /// Manual changes found by this plan that earlier syncs had not seen (for a notice).
    #[serde(default)]
    pub adopted: Vec<AdoptedChange>,
    /// The mods folder looked wiped (gone, or most pack mods missing at once): the selection
    /// is installed again instead of switching everything off.
    #[serde(default)]
    pub reset: bool,
    /// Files that could not be checked; they are left as they are.
    #[serde(default)]
    pub issues: Vec<SyncIssue>,
    /// Recorded files whose path only changed in letter case and which the disk sees as the
    /// wanted file (old path, new path): kept, and renamed to the new spelling on apply.
    #[serde(skip)]
    pub(crate) case_renames: Vec<(String, String)>,
    /// State written after a successful apply.
    pub next_state: PackState,
    /// `next_state` differs from the state the plan started from (choices, new version, …).
    pub state_changed: bool,
}

/// Per-category counts for one action.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CategoryCounts {
    pub mods: usize,
    pub resource_packs: usize,
    pub shader_packs: usize,
    pub configs: usize,
    pub other: usize,
}

impl CategoryCounts {
    fn add(&mut self, category: FileCategory) {
        *match category {
            FileCategory::Mod => &mut self.mods,
            FileCategory::ResourcePack => &mut self.resource_packs,
            FileCategory::ShaderPack => &mut self.shader_packs,
            FileCategory::Config => &mut self.configs,
            FileCategory::Other => &mut self.other,
        } += 1;
    }

    pub fn total(&self) -> usize {
        self.mods + self.resource_packs + self.shader_packs + self.configs + self.other
    }

    fn entries(&self) -> [(usize, &'static str); 5] {
        [
            (self.mods, "mod"),
            (self.resource_packs, "kaynak paketi"),
            (self.shader_packs, "shader paketi"),
            (self.configs, "yapılandırma dosyası"),
            (self.other, "dosya"),
        ]
    }
}

/// Dry-run summary for the UI (the mod center's "Uygula" and update notifications).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    /// Files that appear (downloads of new files and re-enabled items).
    pub added: CategoryCounts,
    /// Files replaced by a newer version (including an item's new file name).
    pub updated: CategoryCounts,
    /// Files that disappear (updates are not counted here).
    pub removed: CategoryCounts,
    pub kept: usize,
    pub unchanged: usize,
    /// Bytes to download.
    pub download_bytes: u64,
    /// Bytes deleted from the disk (removals and the old files of updated items).
    pub remove_bytes: u64,
}

impl SyncSummary {
    /// True when applying would not change any file.
    pub fn is_noop(&self) -> bool {
        self.added.total() + self.updated.total() + self.removed.total() == 0
    }

    /// Turkish one-liner, e.g. "3 mod güncellenecek, 1 mod kaldırılacak". The UI localizes
    /// from the counts; this is for notifications and logs.
    pub fn describe_tr(&self) -> String {
        let mut parts = Vec::new();
        for (counts, verb) in [
            (&self.added, "eklenecek"),
            (&self.updated, "güncellenecek"),
            (&self.removed, "kaldırılacak"),
        ] {
            for (count, noun) in counts.entries() {
                if count > 0 {
                    parts.push(format!("{count} {noun} {verb}"));
                }
            }
        }
        if self.kept > 0 {
            parts.push(format!("{} değiştirilmiş dosya korunacak", self.kept));
        }
        if parts.is_empty() {
            "Paket güncel".to_owned()
        } else {
            parts.join(", ")
        }
    }
}

impl SyncPlan {
    pub fn summary(&self) -> SyncSummary {
        let mut summary = SyncSummary {
            kept: self.kept_user_modified.len(),
            unchanged: self.unchanged.len(),
            ..SyncSummary::default()
        };
        for download in &self.downloads {
            summary.download_bytes += download.size;
            match download.action {
                DownloadAction::Add => summary.added.add(download.category),
                DownloadAction::Update => summary.updated.add(download.category),
            }
        }
        for restore in &self.restores {
            summary.added.add(restore.category);
        }
        for delete in &self.deletes {
            summary.remove_bytes += delete.size;
            if !delete.replaced {
                summary.removed.add(delete.category);
            }
        }
        summary
    }

    /// True when the plan downloads, renames or deletes something.
    pub fn changes_files(&self) -> bool {
        !(self.downloads.is_empty() && self.restores.is_empty() && self.deletes.is_empty())
    }

    /// True when applying would change neither files nor the pack state.
    pub fn is_noop(&self) -> bool {
        !self.changes_files() && !self.state_changed
    }
}

/// What a sync would download for a set, estimated from its state alone (no disk access).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DownloadEstimate {
    pub files: usize,
    pub bytes: u64,
}

/// Files a sync of `manifest` would download for the set described by `state`: the wanted
/// files whose recorded hash is not the manifest's. Used to preview a version switch before
/// the set's folders are in place.
pub fn estimate_downloads(manifest: &PackManifest, state: Option<&PackState>) -> DownloadEstimate {
    let resolution = resolve_items(manifest, state, None);
    let enabled: HashSet<&str> = resolution
        .items
        .iter()
        .filter(|status| status.effectively_enabled)
        .map(|status| status.id.as_str())
        .collect();
    let groups = group_choices(&manifest.projects, state, None);
    let mut estimate = DownloadEstimate::default();
    for (file, _, _) in desired_files(manifest, &enabled, &groups) {
        let expected = normalize_hex(&file.hashes.sha1, 40);
        let current = state
            .and_then(|state| state.files.get(&file.path))
            .is_some_and(|record| {
                !record.disabled
                    && expected
                        .as_deref()
                        .is_some_and(|sha| record.sha1.eq_ignore_ascii_case(sha))
            });
        if !current {
            estimate.files += 1;
            estimate.bytes += file.file_size;
        }
    }
    estimate
}

enum Decision {
    Unchanged(InstalledFile),
    /// Download; the optional path and size name a stale disabled copy of the same item to
    /// delete afterwards.
    Download(DownloadAction, InstalledFile, Option<(String, u64)>),
    Restore(InstalledFile),
    Keep(KeepReason, Option<InstalledFile>),
    Issue(SyncIssueKind, String),
}

enum Removal {
    Delete { path: String, size: u64 },
    Keep,
    Gone,
    Issue(SyncIssueKind, String),
}

/// What became of a file the last sync installed.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Seen {
    /// The file is at its path.
    Present,
    /// Only `<path>.disabled` exists.
    Disabled,
    /// Neither exists.
    Missing,
    /// It could not be checked (a folder in its place, no permission): left alone.
    Unknown,
}

/// Choices adopted from the disk before the items are resolved.
#[derive(Default)]
struct Adoption {
    /// Item id → selected.
    choices: BTreeMap<String, bool>,
    changes: Vec<AdoptedChange>,
}

/// Computes what a sync would do. `previous` is the state from the last sync
/// ([`PackState::load`]). `selection` carries the mod center's choices (missing entries keep
/// the recorded or adopted choice; see [`crate::items`]); `None` for background syncs before a
/// launch.
pub async fn plan_sync(
    manifest: &PackManifest,
    game_dir: &Path,
    previous: Option<&PackState>,
    selection: Option<&PackSelection>,
    policy: &SyncPolicy,
) -> Result<SyncPlan> {
    manifest.validate()?;
    let ext = &manifest.projects;
    let empty = PackState::default();
    let prev = previous.unwrap_or(&empty);

    // Manual changes: what became of the files the last sync installed.
    let disk = observe_records(game_dir, ext, prev).await;
    let reset = looks_wiped(game_dir, prev, &disk).await;
    let mut adoption = adopt(ext, prev, selection, &disk, reset);

    let resolution = resolve_with(manifest, previous, selection, &adoption.choices);
    let enabled_items: HashSet<&str> = resolution
        .items
        .iter()
        .filter(|status| status.effectively_enabled)
        .map(|status| status.id.as_str())
        .collect();
    // Libraries are not choices: one the player removed comes back because something needs it,
    // and the mod center says so.
    for item in ext
        .items
        .iter()
        .filter(|item| item.library && enabled_items.contains(item.id.as_str()))
    {
        let Some((path, record)) = installed_record(prev, item) else {
            continue;
        };
        let change = match (record.disabled, disk.get(path.as_str())) {
            (false, Some(Seen::Disabled)) => Some(ManualChange::Disabled),
            (false, Some(Seen::Missing)) if !reset => Some(ManualChange::Removed),
            _ => None,
        };
        if let Some(change) = change {
            adoption.changes.push(AdoptedChange {
                item: item.id.clone(),
                change,
            });
        }
    }

    let groups = group_choices(ext, previous, selection);
    let desired = desired_files(manifest, &enabled_items, &groups);
    let desired_paths: HashSet<&str> = desired
        .iter()
        .map(|(file, _, _)| file.path.as_str())
        .collect();
    // A path whose spelling only changed in letter case names the same file on most Windows and
    // macOS disks: its record carries over, and it is never removed as no longer wanted (that
    // would delete the new path's file too).
    let renamed = case_renames(game_dir, prev, &desired).await;
    let carried: HashSet<&str> = renamed.values().map(|old| old.as_str()).collect();
    let previous_of = |path: &str| {
        prev.files.get(path).or_else(|| {
            renamed
                .get(path)
                .and_then(|old| prev.files.get(old.as_str()))
        })
    };

    // Futures are built eagerly so the plan future stays `Send` for spawning callers.
    let inspections: Vec<_> = desired
        .iter()
        .map(|(file, file_policy, item)| {
            let reverted = item.is_some_and(|id| selection.is_some_and(|s| s.revert.contains(id)));
            inspect_desired(
                game_dir,
                file,
                *file_policy,
                previous_of(&file.path),
                *item,
                Inspect {
                    repair: policy.repair || reverted,
                    quick_check: policy.quick_check,
                },
            )
        })
        .collect();
    let decisions: Vec<Result<Decision>> = futures::stream::iter(inspections)
        .buffered(HASH_CONCURRENCY)
        .collect()
        .await;

    let mut plan = SyncPlan {
        game_dir: game_dir.to_path_buf(),
        pack_id: ext.pack_id.clone(),
        version_id: manifest.version_id.clone(),
        downloads: Vec::new(),
        restores: Vec::new(),
        deletes: Vec::new(),
        unchanged: Vec::new(),
        kept_user_modified: Vec::new(),
        items: resolution.items.clone(),
        adopted: Vec::new(),
        reset: false,
        issues: Vec::new(),
        case_renames: Vec::new(),
        next_state: PackState {
            pack_id: ext.pack_id.clone(),
            version_id: manifest.version_id.clone(),
            family: ext.family_id().to_owned(),
            minecraft: manifest.dependencies.minecraft.clone(),
            files: BTreeMap::new(),
            groups,
            items: resolution.choices.clone(),
            seen_items: resolution.seen.clone(),
            manual: BTreeMap::new(),
        },
        state_changed: false,
    };

    let mut modified_items: HashSet<String> = HashSet::new();
    for ((file, _, item), decision) in desired.iter().zip(decisions) {
        let category = FileCategory::of(&file.path);
        let item = item.map(str::to_owned);
        let decision = decision?;
        if let Some(&old) = renamed.get(&file.path) {
            if let Decision::Issue(..) = decision {
                // Nothing was decided: the record stays under the old spelling for next time.
                if let Some(record) = prev.files.get(old) {
                    plan.next_state.files.insert(old.clone(), record.clone());
                }
            } else {
                plan.case_renames.push((old.clone(), file.path.clone()));
            }
        }
        match decision {
            Decision::Unchanged(record) => {
                plan.unchanged.push(file.path.clone());
                plan.next_state.files.insert(file.path.clone(), record);
            }
            Decision::Download(action, record, stale) => {
                plan.downloads.push(PlannedDownload {
                    path: file.path.clone(),
                    urls: file.downloads.clone(),
                    sha1: record.sha1.clone(),
                    sha512: file
                        .hashes
                        .sha512
                        .as_deref()
                        .and_then(|h| normalize_hex(h, 128)),
                    size: file.file_size,
                    action,
                    category,
                    item: item.clone(),
                });
                if let Some((path, size)) = stale {
                    plan.deletes.push(PlannedDelete {
                        path,
                        category,
                        size,
                        item,
                        replaced: true,
                        record: None,
                    });
                }
                plan.next_state.files.insert(file.path.clone(), record);
            }
            Decision::Restore(record) => {
                plan.restores.push(PlannedRestore {
                    path: file.path.clone(),
                    size: file.file_size,
                    category,
                    item,
                });
                plan.next_state.files.insert(file.path.clone(), record);
            }
            Decision::Keep(reason, record) => {
                plan.kept_user_modified.push(KeptFile {
                    path: file.path.clone(),
                    reason,
                    category,
                });
                if reason == KeepReason::UserModified
                    && let Some(id) = &item
                {
                    // Tell the player once, when the replaced file first shows up.
                    let known = previous_of(&file.path).map(|record| record.modified);
                    if known == Some(false) {
                        adoption.changes.push(AdoptedChange {
                            item: id.clone(),
                            change: ManualChange::Replaced,
                        });
                    }
                    modified_items.insert(id.clone());
                }
                if let Some(record) = record {
                    plan.next_state.files.insert(file.path.clone(), record);
                }
            }
            Decision::Issue(kind, detail) => {
                plan.issues.push(SyncIssue {
                    path: file.path.clone(),
                    kind,
                    detail,
                });
                if let Some(record) = prev.files.get(&file.path) {
                    plan.next_state
                        .files
                        .insert(file.path.clone(), record.clone());
                }
            }
        }
    }

    // Records of files no longer wanted. An item that ends up off keeps the disabled copy the
    // player made of it; everything else is deleted while unchanged.
    let mut kept_disabled: HashSet<String> = HashSet::new();
    let mut checked: Vec<(&String, &InstalledFile)> = Vec::new();
    for (path, record) in prev.files.iter().filter(|(path, _)| {
        !desired_paths.contains(path.as_str()) && !carried.contains(path.as_str())
    }) {
        let item = record_item(ext, path, record);
        if let Some(id) = item
            && !enabled_items.contains(id)
            && disk.get(path.as_str()) == Some(&Seen::Disabled)
        {
            plan.next_state.files.insert(
                path.clone(),
                InstalledFile {
                    disabled: true,
                    ..record.clone()
                },
            );
            kept_disabled.insert(id.to_owned());
            continue;
        }
        checked.push((path, record));
    }
    let removal_checks: Vec<_> = checked
        .iter()
        .map(|(path, record)| inspect_removed(game_dir, path, record))
        .collect();
    let removals: Vec<Removal> = futures::stream::iter(removal_checks)
        .buffered(HASH_CONCURRENCY)
        .collect()
        .await;
    for ((path, record), removal) in checked.into_iter().zip(removals) {
        match removal {
            Removal::Delete { path: target, size } => plan.deletes.push(PlannedDelete {
                category: FileCategory::of(path),
                path: target,
                size,
                item: record.item.clone(),
                replaced: false,
                record: Some((path.clone(), record.clone())),
            }),
            Removal::Keep => plan.kept_user_modified.push(KeptFile {
                path: path.clone(),
                reason: KeepReason::UserModified,
                category: FileCategory::of(path),
            }),
            Removal::Gone => {}
            Removal::Issue(kind, detail) => {
                plan.issues.push(SyncIssue {
                    path: path.clone(),
                    kind,
                    detail,
                });
                plan.next_state.files.insert(path.clone(), record.clone());
            }
        }
    }

    // An enabled item whose file name changed: deleting its old file completes an update.
    for delete in plan.deletes.iter_mut().filter(|delete| !delete.replaced) {
        let Some(item) = delete.item.as_deref() else {
            continue;
        };
        if let Some(download) = plan.downloads.iter_mut().find(|download| {
            download.action == DownloadAction::Add && download.item.as_deref() == Some(item)
        }) {
            download.action = DownloadAction::Update;
            delete.replaced = true;
        }
    }

    // Adopted changes stay visible in the mod center until the player confirms it.
    let mut manual = if selection.is_some() {
        BTreeMap::new()
    } else {
        prev.manual.clone()
    };
    for change in &adoption.changes {
        if change.change != ManualChange::Replaced {
            manual.insert(change.item.clone(), change.change);
        }
    }
    manual.retain(|id, _| ext.item(id).is_some());
    plan.next_state.manual = manual;
    for status in &mut plan.items {
        status.manual = plan.next_state.manual.get(&status.id).copied();
        status.modified = modified_items.contains(&status.id);
        status.disabled_copy = kept_disabled.contains(&status.id);
    }
    plan.adopted = adoption.changes;
    plan.reset = reset;
    plan.state_changed = previous != Some(&plan.next_state);
    Ok(plan)
}

/// Group choices for the next state: the selection, else the recorded choice, else the
/// group's default.
fn group_choices(
    ext: &ProjectsExtension,
    previous: Option<&PackState>,
    selection: Option<&PackSelection>,
) -> BTreeMap<String, bool> {
    ext.optional_groups
        .iter()
        .map(|group| {
            let enabled = selection
                .and_then(|s| s.groups.get(&group.id))
                .or_else(|| previous.and_then(|p| p.groups.get(&group.id)))
                .copied()
                .unwrap_or(group.default_enabled);
            (group.id.clone(), enabled)
        })
        .collect()
}

/// The files the client should have, with their (manifest) policy and item: items follow the
/// selection (groups do not apply to them), other files their groups; server-only files are
/// skipped.
fn desired_files<'a>(
    manifest: &'a PackManifest,
    enabled_items: &HashSet<&str>,
    groups: &BTreeMap<String, bool>,
) -> Vec<(&'a PackFile, FilePolicy, Option<&'a str>)> {
    let ext: &'a ProjectsExtension = &manifest.projects;
    let item_by_path: HashMap<&'a str, &'a PackItem> = ext
        .items
        .iter()
        .map(|item| (item.path.as_str(), item))
        .collect();
    let mut membership: HashMap<&'a str, Vec<&'a str>> = HashMap::new();
    for group in &ext.optional_groups {
        for path in &group.files {
            membership.entry(path).or_default().push(&group.id);
        }
    }
    manifest
        .files
        .iter()
        .filter(|file| file.client_support() != EnvSupport::Unsupported)
        .filter_map(|file| {
            let wanted = match item_by_path.get(file.path.as_str()) {
                Some(item) => enabled_items
                    .contains(item.id.as_str())
                    .then_some(Some(item.id.as_str())),
                None => membership
                    .get(file.path.as_str())
                    .is_none_or(|ids| {
                        ids.iter()
                            .any(|id| groups.get(*id).copied().unwrap_or(false))
                    })
                    .then_some(None),
            };
            wanted.map(|item| (file, ext.config_policy.policy_for(&file.path), item))
        })
        .collect()
}

/// The manifest item a state record belongs to: its tag, or for records written before
/// items, the item at its path. `None` for items that left the pack.
fn record_item<'a>(
    ext: &'a ProjectsExtension,
    path: &str,
    record: &InstalledFile,
) -> Option<&'a str> {
    match &record.item {
        Some(id) => ext.item(id).map(|item| item.id.as_str()),
        None => ext.item_for_path(path).map(|item| item.id.as_str()),
    }
}

/// Looks at the recorded files that manual changes can concern: item files and everything
/// under `mods/`.
async fn observe_records(
    game_dir: &Path,
    ext: &ProjectsExtension,
    prev: &PackState,
) -> HashMap<String, Seen> {
    let paths: Vec<&String> = prev
        .files
        .iter()
        .filter(|(path, record)| {
            FileCategory::of(path) == FileCategory::Mod || record_item(ext, path, record).is_some()
        })
        .map(|(path, _)| path)
        .collect();
    let checks: Vec<_> = paths.iter().map(|path| observe(game_dir, path)).collect();
    let seen: Vec<Seen> = futures::stream::iter(checks)
        .buffered(HASH_CONCURRENCY)
        .collect()
        .await;
    paths.into_iter().cloned().zip(seen).collect()
}

async fn observe(game_dir: &Path, path: &str) -> Seen {
    let Ok(target) = join_relative(game_dir, path) else {
        return Seen::Unknown;
    };
    match tokio::fs::metadata(&target).await {
        Ok(metadata) if metadata.is_file() => Seen::Present,
        Ok(_) => Seen::Unknown,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
            match tokio::fs::metadata(disabled_path(&target)).await {
                Ok(metadata) if metadata.is_file() => Seen::Disabled,
                Ok(_) => Seen::Unknown,
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => Seen::Missing,
                Err(_) => Seen::Unknown,
            }
        }
        Err(_) => Seen::Unknown,
    }
}

/// The mods folder looks wiped rather than edited: it is gone, or at least
/// [`RESET_MIN_MISSING`] pack mods and more than half of them disappeared at once.
async fn looks_wiped(game_dir: &Path, prev: &PackState, disk: &HashMap<String, Seen>) -> bool {
    let mods: Vec<&String> = prev
        .files
        .iter()
        .filter(|(path, record)| FileCategory::of(path) == FileCategory::Mod && !record.disabled)
        .map(|(path, _)| path)
        .collect();
    let missing = mods
        .iter()
        .filter(|path| disk.get(path.as_str()) == Some(&Seen::Missing))
        .count();
    if missing == 0 {
        return false;
    }
    let folder = tokio::fs::metadata(game_dir.join("mods"))
        .await
        .is_ok_and(|metadata| metadata.is_dir());
    !folder || (missing >= RESET_MIN_MISSING && missing * 2 > mods.len())
}

/// Item choices the disk shows: files the last sync installed that the player deleted or
/// disabled switch their item off, a kept disabled copy renamed back switches it on. Items
/// the selection names are the player's explicit choice and are skipped.
fn adopt(
    ext: &ProjectsExtension,
    prev: &PackState,
    selection: Option<&PackSelection>,
    disk: &HashMap<String, Seen>,
    reset: bool,
) -> Adoption {
    let mut adoption = Adoption::default();
    for item in ext.items.iter().filter(|item| !item.library) {
        if selection.is_some_and(|s| s.items.contains_key(&item.id)) {
            continue;
        }
        let Some((path, record)) = installed_record(prev, item) else {
            continue;
        };
        let seen = disk.get(path.as_str()).copied().unwrap_or(Seen::Unknown);
        let adopted = match (record.disabled, seen) {
            (false, Seen::Disabled) => Some((false, ManualChange::Disabled)),
            (false, Seen::Missing) if !reset => Some((false, ManualChange::Removed)),
            (true, Seen::Present) => Some((true, ManualChange::Enabled)),
            _ => None,
        };
        if let Some((selected, change)) = adopted {
            adoption.choices.insert(item.id.clone(), selected);
            adoption.changes.push(AdoptedChange {
                item: item.id.clone(),
                change,
            });
        }
    }
    adoption
}

/// How one desired file is checked.
#[derive(Debug, Clone, Copy)]
struct Inspect {
    /// Make the file match the pack whatever its policy says.
    repair: bool,
    quick_check: bool,
}

async fn inspect_desired(
    game_dir: &Path,
    file: &PackFile,
    file_policy: FilePolicy,
    previous: Option<&InstalledFile>,
    item: Option<&str>,
    inspect: Inspect,
) -> Result<Decision> {
    let target = join_relative(game_dir, &file.path)?;
    let expected = normalize_hex(&file.hashes.sha1, 40)
        .ok_or_else(|| Error::Invalid(format!("bad sha1 for {}", file.path)))?;
    let record = InstalledFile::new(expected.clone(), file_policy, item.map(str::to_owned));

    let metadata = match tokio::fs::metadata(&target).await {
        Ok(metadata) => Some(metadata),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Ok(Decision::Issue(SyncIssueKind::Unreadable, e.to_string())),
    };
    let Some(metadata) = metadata else {
        let disabled = disabled_path(&target);
        let Some(disabled_size) = file_size(&disabled).await else {
            return Ok(Decision::Download(DownloadAction::Add, record, None));
        };
        if item.is_none() {
            return Ok(Decision::Keep(KeepReason::UserDisabled, previous.cloned()));
        }
        // An enabled item whose file sits disabled (selected again, or required by another
        // enabled item) comes back on.
        let current = match pl_net::hash_file(&disabled, HashAlgo::Sha1).await {
            Ok(hash) => hash,
            Err(e) => return Ok(Decision::Issue(SyncIssueKind::Unreadable, e.to_string())),
        };
        if current == expected {
            return Ok(Decision::Restore(record));
        }
        let stale = previous
            .is_some_and(|p| p.sha1.eq_ignore_ascii_case(&current))
            .then(|| (format!("{}{DISABLED_SUFFIX}", file.path), disabled_size));
        return Ok(Decision::Download(DownloadAction::Add, record, stale));
    };
    if !metadata.is_file() {
        return Ok(Decision::Issue(
            SyncIssueKind::NotAFile,
            format!("{} is not a file", target.display()),
        ));
    }

    if inspect.quick_check
        && !inspect.repair
        && metadata.len() == file.file_size
        && previous.is_some_and(|p| !p.modified && p.sha1.eq_ignore_ascii_case(&expected))
    {
        return Ok(Decision::Unchanged(record));
    }
    let current = match pl_net::hash_file(&target, HashAlgo::Sha1).await {
        Ok(hash) => hash,
        Err(e) => return Ok(Decision::Issue(SyncIssueKind::Unreadable, e.to_string())),
    };
    if current == expected {
        return Ok(Decision::Unchanged(record));
    }
    // A jar the player replaced is theirs: `overwrite` acts like `preserveUserChanges` there.
    let effective = if inspect.repair {
        FilePolicy::Overwrite
    } else if file_policy == FilePolicy::Overwrite
        && FileCategory::of(&file.path) == FileCategory::Mod
    {
        FilePolicy::PreserveUserChanges
    } else {
        file_policy
    };
    Ok(match effective {
        FilePolicy::Overwrite => Decision::Download(DownloadAction::Update, record, None),
        FilePolicy::IfMissing => Decision::Keep(KeepReason::ExistingFile, previous.cloned()),
        FilePolicy::PreserveUserChanges => match previous {
            // Untouched since we installed it: safe to update.
            Some(prev) if prev.sha1.eq_ignore_ascii_case(&current) => {
                Decision::Download(DownloadAction::Update, record, None)
            }
            _ => Decision::Keep(
                KeepReason::UserModified,
                previous.map(|prev| InstalledFile {
                    modified: true,
                    ..prev.clone()
                }),
            ),
        },
    })
}

async fn inspect_removed(game_dir: &Path, path: &str, record: &InstalledFile) -> Removal {
    // The state file is local and could be edited; never delete outside the game dir.
    let Ok(target) = join_relative(game_dir, path) else {
        tracing::warn!(path, "ignoring an unsafe path in the pack state");
        return Removal::Gone;
    };
    let installed = record.sha1.to_ascii_lowercase();
    if let Some(size) = file_size(&target).await {
        return match pl_net::hash_file(&target, HashAlgo::Sha1).await {
            Ok(current) if current == installed => Removal::Delete {
                path: path.to_owned(),
                size,
            },
            Ok(_) => Removal::Keep,
            Err(e) => Removal::Issue(SyncIssueKind::Unreadable, e.to_string()),
        };
    }
    let disabled = disabled_path(&target);
    if let Some(size) = file_size(&disabled).await {
        return match pl_net::hash_file(&disabled, HashAlgo::Sha1).await {
            Ok(current) if current == installed => Removal::Delete {
                path: format!("{path}{DISABLED_SUFFIX}"),
                size,
            },
            Ok(_) => Removal::Gone,
            Err(e) => Removal::Issue(SyncIssueKind::Unreadable, e.to_string()),
        };
    }
    Removal::Gone
}

fn disabled_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(DISABLED_SUFFIX);
    PathBuf::from(name)
}

/// Size of a regular file; `None` when there is no file at `path`.
async fn file_size(path: &Path) -> Option<u64> {
    tokio::fs::metadata(path)
        .await
        .ok()
        .filter(|metadata| metadata.is_file())
        .map(|metadata| metadata.len())
}

/// Wanted paths whose record sits under a spelling that differs only in letter case (Studio
/// renamed `config/Foo.toml` to `config/foo.toml`) and that the disk sees as the same file: wanted
/// path → recorded path. On a disk that minds case the two are separate files and nothing
/// carries over.
async fn case_renames<'a>(
    game_dir: &Path,
    prev: &'a PackState,
    desired: &[(&PackFile, FilePolicy, Option<&str>)],
) -> HashMap<String, &'a String> {
    let by_spelling: HashMap<String, &String> = prev
        .files
        .keys()
        .map(|path| (path.to_lowercase(), path))
        .collect();
    let mut renamed = HashMap::new();
    for (file, _, _) in desired {
        if prev.files.contains_key(&file.path) {
            continue;
        }
        let Some(old) = by_spelling.get(&file.path.to_lowercase()) else {
            continue;
        };
        let (Ok(old_path), Ok(new_path)) = (
            join_relative(game_dir, old),
            join_relative(game_dir, &file.path),
        ) else {
            continue;
        };
        if same_file(&old_path, &new_path).await
            || same_file(&disabled_path(&old_path), &disabled_path(&new_path)).await
        {
            renamed.insert(file.path.clone(), *old);
        }
    }
    renamed
}

/// Whether two paths reach the same existing file.
async fn same_file(a: &Path, b: &Path) -> bool {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        match (tokio::fs::metadata(a).await, tokio::fs::metadata(b).await) {
            (Ok(x), Ok(y)) => x.is_file() && x.dev() == y.dev() && x.ino() == y.ino(),
            _ => false,
        }
    }
    #[cfg(not(unix))]
    {
        // The canonical form carries the spelling stored on disk.
        match (
            tokio::fs::canonicalize(a).await,
            tokio::fs::canonicalize(b).await,
        ) {
            (Ok(x), Ok(y)) => x == y && file_size(&x).await.is_some(),
            _ => false,
        }
    }
}

/// Gives the file of `path` (or its disabled copy) the spelling of `path` when the disk holds it
/// under other letter case. The rename goes through a temporary name: some file systems ignore a
/// rename that only changes case.
async fn fix_case(game_dir: &Path, path: &str) -> std::io::Result<()> {
    let target = join_relative(game_dir, path).map_err(std::io::Error::other)?;
    let (Some(dir), Some(name)) = (target.parent(), target.file_name()) else {
        return Ok(());
    };
    let name = name.to_string_lossy().into_owned();
    for wanted in [name.clone(), format!("{name}{DISABLED_SUFFIX}")] {
        let folded = wanted.to_lowercase();
        let mut found = None;
        let mut entries = tokio::fs::read_dir(dir).await?;
        while let Some(entry) = entries.next_entry().await? {
            let current = entry.file_name().to_string_lossy().into_owned();
            if current == wanted {
                return Ok(());
            }
            if current.to_lowercase() == folded {
                found = Some(current);
            }
        }
        if let Some(current) = found {
            let temporary = dir.join(format!("{wanted}.pl-case"));
            tokio::fs::rename(dir.join(&current), &temporary).await?;
            if let Err(error) = tokio::fs::rename(&temporary, dir.join(&wanted)).await {
                let _ = tokio::fs::rename(&temporary, dir.join(&current)).await;
                return Err(error);
            }
            return Ok(());
        }
    }
    Ok(())
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SyncPhase {
    Downloading,
    Removing,
    Finished,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncProgress {
    pub phase: SyncPhase,
    pub done_files: usize,
    pub total_files: usize,
    pub done_bytes: u64,
    pub total_bytes: u64,
    pub bytes_per_sec: u64,
}

pub type SyncProgressFn = Arc<dyn Fn(SyncProgress) + Send + Sync>;

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub downloaded: usize,
    /// Disabled item files renamed back.
    pub restored: usize,
    pub deleted: usize,
    pub bytes: u64,
    /// Renames and deletions that failed (the file stays; the next sync tries again).
    #[serde(default)]
    pub issues: Vec<SyncIssue>,
}

/// Applies a plan: downloads (verified by SHA-512 when known, else SHA-1, written atomically),
/// then restores of disabled item files, then new spellings of renamed paths, then deletions,
/// then the new state. Nothing is renamed or deleted and no state is written when a download
/// fails, so the next plan simply resumes. A rename or deletion that fails (the file is open
/// somewhere) does not stop the
/// sync: it is reported in [`SyncReport::issues`] and the state keeps the file's record, so
/// the next sync tries again. `options.cancel` aborts the downloads; once they are done the
/// quick renames and deletions always finish and the state is written, because a state that
/// still records files a stopped sync already deleted would read as the player's own changes
/// (a deleted item file switches its item off).
pub async fn apply_sync(
    plan: &SyncPlan,
    downloader: &Downloader,
    options: &DownloadOptions,
    progress: Option<SyncProgressFn>,
) -> Result<SyncReport> {
    let mut items = Vec::with_capacity(plan.downloads.len());
    for download in &plan.downloads {
        let dest = join_relative(&plan.game_dir, &download.path)?;
        let checksum = match &download.sha512 {
            Some(sha512) => Checksum::sha512(sha512),
            None => Checksum::sha1(&download.sha1),
        };
        items.push(
            DownloadItem::with_urls(download.urls.clone(), dest)
                .checksum(Some(checksum))
                .size(Some(download.size)),
        );
    }
    let mut restores = Vec::with_capacity(plan.restores.len());
    for restore in &plan.restores {
        let target = join_relative(&plan.game_dir, &restore.path)?;
        restores.push((disabled_path(&target), target));
    }
    for delete in &plan.deletes {
        check_relative_path(&delete.path)?;
    }

    let mut options = options.clone();
    if let Some(progress) = &progress {
        let progress = Arc::clone(progress);
        options.progress = Some(Arc::new(move |p: DownloadProgress| {
            progress(SyncProgress {
                phase: SyncPhase::Downloading,
                done_files: p.done_files,
                total_files: p.total_files,
                done_bytes: p.done_bytes,
                total_bytes: p.total_bytes,
                bytes_per_sec: p.bytes_per_sec,
            });
        }));
    }
    let mut report = SyncReport::default();
    if !items.is_empty() {
        let summary = downloader
            .download_all(items, &options)
            .await
            .map_err(|e| {
                if e.is_cancelled() {
                    Error::Cancelled
                } else {
                    e.into()
                }
            })?;
        report.downloaded = summary.downloaded;
        report.bytes = summary.bytes;
    }

    let mut state = plan.next_state.clone();
    for (restore, (disabled, target)) in plan.restores.iter().zip(&restores) {
        match tokio::fs::rename(disabled, target).await {
            Ok(()) => report.restored += 1,
            Err(e) => {
                tracing::warn!(path = %restore.path, error = %e, "could not turn a mod back on");
                // Not recorded as installed: the next sync looks at it again.
                state.files.remove(&restore.path);
                report.issues.push(SyncIssue {
                    path: restore.path.clone(),
                    kind: SyncIssueKind::Busy,
                    detail: e.to_string(),
                });
            }
        }
    }

    // Files whose path only changed in letter case take the new spelling (before the deletions;
    // none of those concern them).
    for (old, new) in &plan.case_renames {
        if let Err(e) = fix_case(&plan.game_dir, new).await {
            tracing::warn!(path = %new, error = %e, "could not give a pack file its new spelling");
            // It keeps the old spelling: recorded there, the next sync tries again.
            if let Some(record) = state.files.remove(new) {
                state.files.insert(old.clone(), record);
            }
            report.issues.push(SyncIssue {
                path: new.clone(),
                kind: SyncIssueKind::Busy,
                detail: e.to_string(),
            });
        }
    }

    let total = plan.deletes.len();
    for (done, delete) in plan.deletes.iter().enumerate() {
        if let Some(progress) = &progress {
            progress(SyncProgress {
                phase: SyncPhase::Removing,
                done_files: done,
                total_files: total,
                done_bytes: 0,
                total_bytes: 0,
                bytes_per_sec: 0,
            });
        }
        let target = join_relative(&plan.game_dir, &delete.path)?;
        match tokio::fs::remove_file(&target).await {
            Ok(()) => report.deleted += 1,
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => {
                tracing::warn!(path = %delete.path, error = %e, "could not delete a pack file");
                if let Some((path, record)) = &delete.record {
                    state
                        .files
                        .entry(path.clone())
                        .or_insert_with(|| record.clone());
                }
                report.issues.push(SyncIssue {
                    path: delete.path.clone(),
                    kind: SyncIssueKind::Busy,
                    detail: e.to_string(),
                });
            }
        }
    }

    state.save(&plan.game_dir).await?;
    if let Some(progress) = &progress {
        progress(SyncProgress {
            phase: SyncPhase::Finished,
            done_files: total,
            total_files: total,
            done_bytes: report.bytes,
            total_bytes: report.bytes,
            bytes_per_sec: 0,
        });
    }
    tracing::info!(
        pack = %plan.pack_id,
        version = %plan.version_id,
        downloaded = report.downloaded,
        restored = report.restored,
        deleted = report.deleted,
        issues = report.issues.len(),
        "pack synced"
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ConfigPolicy, ItemCategory, OptionalGroup, PackDependencies, PackEnv, PackHashes,
        PolicyRule, ProjectsExtension,
    };
    use crate::testutil::{TestServer, sample_manifest, sha1_hex, test_http};
    use std::sync::Mutex;

    const URL: &str = "https://cdn.example/files/x";

    fn pack_file(path: &str, content: &[u8]) -> PackFile {
        PackFile {
            path: path.into(),
            hashes: PackHashes {
                sha1: sha1_hex(content),
                sha512: None,
            },
            env: None,
            downloads: vec![URL.into()],
            file_size: content.len() as u64,
        }
    }

    fn manifest(files: Vec<PackFile>, ext: ProjectsExtension) -> PackManifest {
        PackManifest {
            format_version: 1,
            game: "minecraft".into(),
            version_id: "2.0.0".into(),
            name: "Projects".into(),
            summary: None,
            files,
            dependencies: PackDependencies {
                minecraft: "1.21.1".into(),
                ..PackDependencies::default()
            },
            projects: ProjectsExtension {
                pack_id: "projects".into(),
                ..ext
            },
        }
    }

    fn config_policy() -> ProjectsExtension {
        ProjectsExtension {
            config_policy: ConfigPolicy {
                default: FilePolicy::Overwrite,
                rules: vec![
                    PolicyRule {
                        glob: "options.txt".into(),
                        policy: FilePolicy::IfMissing,
                    },
                    PolicyRule {
                        glob: "config/**".into(),
                        policy: FilePolicy::PreserveUserChanges,
                    },
                ],
            },
            ..ProjectsExtension::default()
        }
    }

    fn write(dir: &Path, path: &str, content: &[u8]) {
        let target = join_relative(dir, path).unwrap();
        std::fs::create_dir_all(target.parent().unwrap()).unwrap();
        std::fs::write(target, content).unwrap();
    }

    fn state(entries: &[(&str, &[u8], FilePolicy)]) -> PackState {
        PackState {
            pack_id: "projects".into(),
            version_id: "1.0.0".into(),
            files: entries
                .iter()
                .map(|(path, content, policy)| {
                    (
                        (*path).to_owned(),
                        InstalledFile::new(sha1_hex(content), *policy, None),
                    )
                })
                .collect(),
            ..PackState::default()
        }
    }

    fn paths(downloads: &[PlannedDownload]) -> Vec<(&str, DownloadAction)> {
        downloads
            .iter()
            .map(|d| (d.path.as_str(), d.action))
            .collect()
    }

    fn deleted(plan: &SyncPlan) -> Vec<&str> {
        plan.deletes.iter().map(|d| d.path.as_str()).collect()
    }

    #[tokio::test]
    async fn fresh_install_downloads_everything() {
        let dir = tempfile::tempdir().unwrap();
        let m = manifest(
            vec![
                pack_file("mods/a.jar", b"a"),
                pack_file("config/a.json", b"{}"),
            ],
            config_policy(),
        );
        let plan = plan_sync(&m, dir.path(), None, None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(
            paths(&plan.downloads),
            [
                ("mods/a.jar", DownloadAction::Add),
                ("config/a.json", DownloadAction::Add)
            ]
        );
        assert!(plan.deletes.is_empty() && plan.unchanged.is_empty());
        assert_eq!(plan.next_state.files.len(), 2);
        assert_eq!(
            plan.next_state.files["config/a.json"].policy,
            FilePolicy::PreserveUserChanges
        );
        assert_eq!(
            plan.summary().describe_tr(),
            "1 mod eklenecek, 1 yapılandırma dosyası eklenecek"
        );
        assert!(plan.adopted.is_empty() && !plan.reset && plan.issues.is_empty());
    }

    #[tokio::test]
    async fn update_remove_and_user_files() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        write(game, "mods/keep.jar", b"same");
        write(game, "mods/update.jar", b"v1");
        write(game, "mods/removed.jar", b"old");
        write(game, "mods/removed-but-edited.jar", b"edited");
        write(game, "mods/user-added.jar", b"mine");
        let previous = state(&[
            ("mods/keep.jar", b"same", FilePolicy::Overwrite),
            ("mods/update.jar", b"v1", FilePolicy::Overwrite),
            ("mods/removed.jar", b"old", FilePolicy::Overwrite),
            (
                "mods/removed-but-edited.jar",
                b"original",
                FilePolicy::Overwrite,
            ),
            ("mods/already-gone.jar", b"x", FilePolicy::Overwrite),
        ]);
        let m = manifest(
            vec![
                pack_file("mods/keep.jar", b"same"),
                pack_file("mods/update.jar", b"v2"),
                pack_file("mods/new.jar", b"new"),
            ],
            ProjectsExtension::default(),
        );
        let plan = plan_sync(&m, game, Some(&previous), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(
            paths(&plan.downloads),
            [
                ("mods/update.jar", DownloadAction::Update),
                ("mods/new.jar", DownloadAction::Add)
            ]
        );
        assert_eq!(plan.unchanged, ["mods/keep.jar"]);
        assert_eq!(deleted(&plan), ["mods/removed.jar"]);
        assert_eq!(plan.deletes[0].size, 3);
        assert_eq!(plan.kept_user_modified.len(), 1);
        assert_eq!(
            plan.kept_user_modified[0].path,
            "mods/removed-but-edited.jar"
        );
        assert!(!plan.next_state.files.contains_key("mods/removed.jar"));
        assert!(!plan.next_state.files.contains_key("mods/user-added.jar"));
        assert_eq!(
            plan.summary().describe_tr(),
            "1 mod eklenecek, 1 mod güncellenecek, 1 mod kaldırılacak, 1 değiştirilmiş dosya korunacak"
        );
        assert_eq!(plan.summary().remove_bytes, 3);
        assert!(!plan.reset, "one missing file is not a wiped folder");

        // A manifest without items keeps the state free of item bookkeeping.
        assert!(plan.items.is_empty());
        assert!(plan.next_state.items.is_empty() && plan.next_state.seen_items.is_empty());
        let json = serde_json::to_value(&plan.next_state).unwrap();
        assert!(json.get("items").is_none() && json.get("seenItems").is_none());
        assert!(json.get("manual").is_none());
        assert_eq!(json["family"], "projects");
        assert_eq!(json["minecraft"], "1.21.1");
        assert!(json["files"]["mods/keep.jar"].get("item").is_none());
        assert!(json["files"]["mods/keep.jar"].get("disabled").is_none());
    }

    #[tokio::test]
    async fn config_policies() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        write(game, "options.txt", b"user options");
        write(game, "config/edited.json", b"user edit");
        write(game, "config/untouched.json", b"v1");
        write(game, "config/foreign.json", b"theirs");
        write(game, "config/forced.toml", b"tweaked");
        write(game, "mods/replaced.jar", b"player build");
        let mut ext = config_policy();
        ext.config_policy.rules.insert(
            0,
            PolicyRule {
                glob: "config/forced.toml".into(),
                policy: FilePolicy::Overwrite,
            },
        );
        let previous = state(&[
            ("config/edited.json", b"v1", FilePolicy::PreserveUserChanges),
            (
                "config/untouched.json",
                b"v1",
                FilePolicy::PreserveUserChanges,
            ),
            ("config/forced.toml", b"pack", FilePolicy::Overwrite),
            ("mods/replaced.jar", b"good", FilePolicy::Overwrite),
        ]);
        let m = manifest(
            vec![
                pack_file("options.txt", b"pack options"),
                pack_file("config/edited.json", b"v2"),
                pack_file("config/untouched.json", b"v2"),
                pack_file("config/foreign.json", b"pack"),
                pack_file("config/missing.json", b"pack"),
                pack_file("config/forced.toml", b"pack"),
                pack_file("mods/replaced.jar", b"good"),
            ],
            ext,
        );
        let plan = plan_sync(&m, game, Some(&previous), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(
            paths(&plan.downloads),
            [
                ("config/untouched.json", DownloadAction::Update),
                ("config/missing.json", DownloadAction::Add),
                ("config/forced.toml", DownloadAction::Update),
            ],
            "`overwrite` configs follow the pack; a jar the player replaced does not"
        );
        let kept: Vec<_> = plan
            .kept_user_modified
            .iter()
            .map(|k| (k.path.as_str(), k.reason))
            .collect();
        assert_eq!(
            kept,
            [
                ("options.txt", KeepReason::ExistingFile),
                ("config/edited.json", KeepReason::UserModified),
                ("config/foreign.json", KeepReason::UserModified),
                ("mods/replaced.jar", KeepReason::UserModified),
            ]
        );
        // Changed files keep the hash we installed, so they stay "modified" next time.
        assert_eq!(
            plan.next_state.files["config/edited.json"].sha1,
            sha1_hex(b"v1")
        );
        let replaced = &plan.next_state.files["mods/replaced.jar"];
        assert!(replaced.modified && replaced.sha1 == sha1_hex(b"good"));
        assert!(!plan.next_state.files.contains_key("config/foreign.json"));
        assert!(!plan.next_state.files.contains_key("options.txt"));

        // Repair overrides the policies.
        let repair = plan_sync(
            &m,
            game,
            Some(&previous),
            None,
            &SyncPolicy {
                repair: true,
                quick_check: false,
            },
        )
        .await
        .unwrap();
        assert_eq!(repair.downloads.len(), 7);
        assert!(repair.kept_user_modified.is_empty());
    }

    #[tokio::test]
    async fn optional_groups_and_environments() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        write(game, "shaderpacks/shaders.zip", b"shaders");
        let previous = state(&[("shaderpacks/shaders.zip", b"shaders", FilePolicy::Overwrite)]);
        let mut server_only = pack_file("mods/server.jar", b"srv");
        server_only.env = Some(PackEnv {
            client: EnvSupport::Unsupported,
            server: EnvSupport::Required,
        });
        let ext = ProjectsExtension {
            optional_groups: vec![
                OptionalGroup {
                    id: "shaders".into(),
                    name: "Shader".into(),
                    description: String::new(),
                    default_enabled: true,
                    files: vec!["shaderpacks/shaders.zip".into()],
                },
                OptionalGroup {
                    id: "minimap".into(),
                    name: "Mini harita".into(),
                    description: String::new(),
                    default_enabled: false,
                    files: vec!["mods/minimap.jar".into()],
                },
            ],
            ..ProjectsExtension::default()
        };
        let m = manifest(
            vec![
                pack_file("shaderpacks/shaders.zip", b"shaders"),
                pack_file("mods/minimap.jar", b"map"),
                server_only,
            ],
            ext,
        );

        // Defaults: shaders on (already installed), minimap off, server mod skipped.
        let plan = plan_sync(&m, game, Some(&previous), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(plan.downloads.is_empty() && plan.deletes.is_empty());
        assert_eq!(plan.unchanged, ["shaderpacks/shaders.zip"]);
        assert!(!plan.next_state.groups["minimap"]);

        // The player turns shaders off and the minimap on.
        let selection = PackSelection {
            groups: BTreeMap::from([("shaders".to_owned(), false), ("minimap".to_owned(), true)]),
            ..PackSelection::default()
        };
        let plan = plan_sync(
            &m,
            game,
            Some(&previous),
            Some(&selection),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            paths(&plan.downloads),
            [("mods/minimap.jar", DownloadAction::Add)]
        );
        assert_eq!(deleted(&plan), ["shaderpacks/shaders.zip"]);
        assert_eq!(plan.summary().removed.shader_packs, 1);

        // Without an explicit selection the previous choice is remembered.
        let plan = plan_sync(
            &m,
            game,
            Some(&plan.next_state),
            None,
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(plan.next_state.groups["minimap"]);
        assert!(!plan.next_state.groups["shaders"]);
    }

    #[tokio::test]
    async fn disabled_mods_stay_disabled() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        write(game, "mods/a.jar.disabled", b"a");
        write(game, "mods/gone.jar.disabled", b"gone");
        let previous = state(&[
            ("mods/a.jar", b"a", FilePolicy::Overwrite),
            ("mods/gone.jar", b"gone", FilePolicy::Overwrite),
        ]);
        let m = manifest(
            vec![pack_file("mods/a.jar", b"a")],
            ProjectsExtension::default(),
        );
        let plan = plan_sync(&m, game, Some(&previous), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(plan.downloads.is_empty() && plan.restores.is_empty());
        assert_eq!(plan.kept_user_modified[0].reason, KeepReason::UserDisabled);
        assert!(plan.next_state.files.contains_key("mods/a.jar"));
        assert_eq!(deleted(&plan), ["mods/gone.jar.disabled"]);
    }

    #[tokio::test]
    async fn quick_check_skips_hashing() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        // Same size as the expected content, different bytes.
        write(game, "config/a.txt", b"xx");
        let previous = state(&[("config/a.txt", b"ok", FilePolicy::Overwrite)]);
        let m = manifest(
            vec![pack_file("config/a.txt", b"ok")],
            ProjectsExtension::default(),
        );
        let quick = SyncPolicy {
            repair: false,
            quick_check: true,
        };
        let plan = plan_sync(&m, game, Some(&previous), None, &quick)
            .await
            .unwrap();
        assert_eq!(plan.unchanged, ["config/a.txt"]);
        let full = plan_sync(&m, game, Some(&previous), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(
            paths(&full.downloads),
            [("config/a.txt", DownloadAction::Update)]
        );
    }

    #[tokio::test]
    async fn unsafe_paths_are_rejected() {
        let dir = tempfile::tempdir().unwrap();
        for path in [
            "../escape.jar",
            "mods/../../escape.jar",
            "/abs.jar",
            "C:/x.jar",
            ".projects/pack-state.json",
        ] {
            let m = manifest(vec![pack_file(path, b"x")], ProjectsExtension::default());
            let result = plan_sync(&m, dir.path(), None, None, &SyncPolicy::default()).await;
            assert!(matches!(result, Err(Error::UnsafePath { .. })), "{path}");
        }

        // A tampered state file cannot make the engine delete outside the game dir.
        let outside = dir.path().join("outside.txt");
        std::fs::write(&outside, b"x").unwrap();
        let game = dir.path().join("game");
        std::fs::create_dir_all(&game).unwrap();
        let previous = state(&[("../outside.txt", b"x", FilePolicy::Overwrite)]);
        let m = manifest(Vec::new(), ProjectsExtension::default());
        let plan = plan_sync(&m, &game, Some(&previous), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(plan.deletes.is_empty());

        // A forged plan is re-validated before anything is applied.
        let mut forged = plan.clone();
        forged.deletes.push(PlannedDelete {
            path: "../outside.txt".into(),
            category: FileCategory::Other,
            size: 1,
            item: None,
            replaced: false,
            record: None,
        });
        let downloader = Downloader::new(test_http());
        let error = apply_sync(&forged, &downloader, &DownloadOptions::default(), None)
            .await
            .unwrap_err();
        assert!(matches!(error, Error::UnsafePath { .. }));
        assert!(outside.exists());
    }

    #[tokio::test]
    async fn apply_against_local_server() {
        let server = TestServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let m = sample_manifest(
            &server,
            &[
                ("mods/sodium.jar", b"sodium v1".as_slice()),
                ("mods/old.jar", b"old mod"),
                ("config/sodium.json", b"{\"v\":1}"),
            ],
        );
        write(game, "mods/user.jar", b"user mod");

        let plan = plan_sync(&m, game, None, None, &SyncPolicy::default())
            .await
            .unwrap();
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let progress: SyncProgressFn = Arc::new(move |p| sink.lock().unwrap().push(p.phase));
        let downloader = Downloader::new(test_http());
        let report = apply_sync(
            &plan,
            &downloader,
            &DownloadOptions::default(),
            Some(progress),
        )
        .await
        .unwrap();
        assert_eq!(report.downloaded, 3);
        assert!(report.issues.is_empty());
        assert_eq!(
            std::fs::read(game.join("mods/sodium.jar")).unwrap(),
            b"sodium v1"
        );
        assert_eq!(events.lock().unwrap().last(), Some(&SyncPhase::Finished));
        let saved = PackState::load(game).await.unwrap().unwrap();
        assert_eq!(saved, plan.next_state);

        // Version 2: sodium updated, old.jar removed, config edited by the player.
        write(game, "config/sodium.json", b"{\"v\":\"mine\"}");
        let m2 = {
            let mut m2 = sample_manifest(
                &server,
                &[
                    ("mods/sodium.jar", b"sodium v2".as_slice()),
                    ("config/sodium.json", b"{\"v\":2}"),
                ],
            );
            m2.version_id = "1.1.0".into();
            m2
        };
        let plan = plan_sync(&m2, game, Some(&saved), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(
            plan.summary().describe_tr(),
            "1 mod güncellenecek, 1 mod kaldırılacak, 1 değiştirilmiş dosya korunacak"
        );
        apply_sync(&plan, &downloader, &DownloadOptions::default(), None)
            .await
            .unwrap();
        assert_eq!(
            std::fs::read(game.join("mods/sodium.jar")).unwrap(),
            b"sodium v2"
        );
        assert!(!game.join("mods/old.jar").exists());
        assert_eq!(
            std::fs::read(game.join("mods/user.jar")).unwrap(),
            b"user mod"
        );
        assert_eq!(
            std::fs::read(game.join("config/sodium.json")).unwrap(),
            b"{\"v\":\"mine\"}"
        );
        let again = plan_sync(
            &m2,
            game,
            PackState::load(game).await.unwrap().as_ref(),
            None,
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(again.is_noop());
        assert!(again.summary().describe_tr().contains("korunacak"));
    }

    #[tokio::test]
    async fn failed_download_changes_nothing_else() {
        let server = TestServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        write(game, "mods/old.jar", b"old");
        let previous = state(&[("mods/old.jar", b"old", FilePolicy::Overwrite)]);
        let mut m = sample_manifest(&server, &[("mods/new.jar", b"new".as_slice())]);
        // The server serves different bytes than the manifest promises.
        server.put(&format!("/files/{}", sha1_hex(b"new")), b"evil".to_vec());
        m.files[0].hashes.sha512 = None;
        let plan = plan_sync(&m, game, Some(&previous), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(plan.deletes.len(), 1);
        let downloader = Downloader::new(test_http());
        assert!(
            apply_sync(&plan, &downloader, &DownloadOptions::default(), None)
                .await
                .is_err()
        );
        assert!(
            game.join("mods/old.jar").exists(),
            "no deletes after a failed download"
        );
        assert!(!game.join("mods/new.jar").exists());
        assert!(PackState::load(game).await.unwrap().is_none());
    }

    #[tokio::test]
    async fn corrupt_state_is_treated_as_missing() {
        let dir = tempfile::tempdir().unwrap();
        let path = PackState::path(dir.path());
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(&path, b"not json").unwrap();
        assert!(PackState::load(dir.path()).await.unwrap().is_none());
    }

    #[test]
    fn categories_and_summary_text() {
        assert_eq!(FileCategory::of("mods/a.jar"), FileCategory::Mod);
        assert_eq!(
            FileCategory::of("resourcepacks/a.zip"),
            FileCategory::ResourcePack
        );
        assert_eq!(
            FileCategory::of("shaderpacks/a.zip"),
            FileCategory::ShaderPack
        );
        assert_eq!(FileCategory::of("config/a/b.toml"), FileCategory::Config);
        assert_eq!(FileCategory::of("options.txt"), FileCategory::Config);
        assert_eq!(FileCategory::of("servers.dat"), FileCategory::Other);
        assert_eq!(FileCategory::of("kubejs/x.js"), FileCategory::Other);
        assert_eq!(SyncSummary::default().describe_tr(), "Paket güncel");
        let summary = SyncSummary {
            updated: CategoryCounts {
                mods: 3,
                ..CategoryCounts::default()
            },
            removed: CategoryCounts {
                resource_packs: 1,
                ..CategoryCounts::default()
            },
            ..SyncSummary::default()
        };
        assert_eq!(
            summary.describe_tr(),
            "3 mod güncellenecek, 1 kaynak paketi kaldırılacak"
        );
        assert!(!summary.is_noop());
    }

    // ---- items (mod center) ----

    fn pack_item(id: &str, path: &str, default_enabled: bool, requires: &[&str]) -> PackItem {
        PackItem {
            id: id.into(),
            path: path.into(),
            title: id.to_uppercase(),
            summary: None,
            category: ItemCategory::Performance,
            icon_url: None,
            page_url: None,
            modrinth: None,
            default_enabled,
            requires: requires.iter().map(|r| (*r).to_owned()).collect(),
            library: false,
        }
    }

    /// sodium (on) → fabric-api (library); iris (off) → sodium; plus a config outside items.
    fn item_manifest(sodium_jar: &str, extra: &[(&str, bool)]) -> PackManifest {
        let mut files = vec![
            pack_file("mods/fabric-api.jar", b"fabric"),
            pack_file(sodium_jar, b"sodium"),
            pack_file("mods/iris.jar", b"iris"),
            pack_file("config/sodium.json", b"{}"),
        ];
        let mut library = pack_item("fabric-api", "mods/fabric-api.jar", false, &[]);
        library.library = true;
        let mut items = vec![
            library,
            pack_item("sodium", sodium_jar, true, &["fabric-api"]),
            pack_item("iris", "mods/iris.jar", false, &["sodium"]),
        ];
        for (id, default_enabled) in extra {
            let path = format!("mods/{id}.jar");
            files.push(pack_file(&path, id.as_bytes()));
            items.push(pack_item(id, &path, *default_enabled, &[]));
        }
        manifest(
            files,
            ProjectsExtension {
                items,
                ..config_policy()
            },
        )
    }

    fn select(pairs: &[(&str, bool)]) -> PackSelection {
        PackSelection {
            items: pairs
                .iter()
                .map(|(id, on)| ((*id).to_owned(), *on))
                .collect(),
            ..PackSelection::default()
        }
    }

    /// Writes what `plan` would download (content = what `pack_file` hashed) and returns the
    /// state an apply would save.
    fn installed(game: &Path, plan: &SyncPlan, contents: &[(&str, &[u8])]) -> PackState {
        for download in &plan.downloads {
            let content = contents
                .iter()
                .find(|(path, _)| *path == download.path)
                .map(|(_, content)| *content)
                .unwrap();
            write(game, &download.path, content);
        }
        plan.next_state.clone()
    }

    /// Default selection of [`item_manifest`] plus zoom (on), installed in `game`.
    async fn installed_with_zoom(game: &Path) -> (PackManifest, PackState) {
        let m = item_manifest("mods/sodium.jar", &[("zoom", true), ("minimap", true)]);
        let first = plan_sync(&m, game, None, None, &SyncPolicy::default())
            .await
            .unwrap();
        let state = installed(
            game,
            &first,
            &[
                ("mods/fabric-api.jar", b"fabric"),
                ("mods/sodium.jar", b"sodium"),
                ("mods/zoom.jar", b"zoom"),
                ("mods/minimap.jar", b"minimap"),
                ("config/sodium.json", b"{}"),
            ],
        );
        (m, state)
    }

    fn status<'a>(plan: &'a SyncPlan, id: &str) -> &'a ItemStatus {
        plan.items.iter().find(|s| s.id == id).unwrap()
    }

    #[tokio::test]
    async fn items_install_defaults_on_first_sync() {
        let dir = tempfile::tempdir().unwrap();
        let m = item_manifest("mods/sodium.jar", &[]);
        let plan = plan_sync(&m, dir.path(), None, None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(
            paths(&plan.downloads),
            [
                ("mods/fabric-api.jar", DownloadAction::Add),
                ("mods/sodium.jar", DownloadAction::Add),
                ("config/sodium.json", DownloadAction::Add),
            ],
            "default items, the library they need and files outside items"
        );
        let tagged: Vec<_> = plan.downloads.iter().map(|d| d.item.as_deref()).collect();
        assert_eq!(tagged, [Some("fabric-api"), Some("sodium"), None]);
        let state = &plan.next_state;
        assert_eq!(
            state.files["mods/sodium.jar"].item.as_deref(),
            Some("sodium")
        );
        assert_eq!(state.files["config/sodium.json"].item, None);
        assert_eq!(
            state.items,
            BTreeMap::from([("iris".to_owned(), false), ("sodium".to_owned(), true)])
        );
        assert_eq!(state.seen_items.len(), 3);
        assert_eq!(
            (state.family.as_str(), state.minecraft.as_str()),
            ("projects", "1.21.1")
        );
        assert!(plan.items.iter().all(|status| !status.is_new));
    }

    #[tokio::test]
    async fn deselecting_removes_pack_files_only() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let m = item_manifest("mods/sodium.jar", &[("zoom", true)]);
        let first = plan_sync(&m, game, None, None, &SyncPolicy::default())
            .await
            .unwrap();
        let state = installed(
            game,
            &first,
            &[
                ("mods/fabric-api.jar", b"fabric"),
                ("mods/sodium.jar", b"sodium"),
                ("mods/zoom.jar", b"zoom"),
                ("config/sodium.json", b"{}"),
            ],
        );
        write(game, "mods/player.jar", b"mine");
        // The player edited the zoom jar: it is no longer the pack's copy.
        write(game, "mods/zoom.jar", b"patched zoom");

        let plan = plan_sync(
            &m,
            game,
            Some(&state),
            Some(&select(&[("sodium", false), ("zoom", false)])),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(plan.downloads.is_empty());
        assert_eq!(
            deleted(&plan),
            ["mods/fabric-api.jar", "mods/sodium.jar"],
            "the deselected item and the library nobody needs any more"
        );
        assert_eq!(plan.kept_user_modified.len(), 1);
        assert_eq!(plan.kept_user_modified[0].path, "mods/zoom.jar");
        assert!(plan.deletes.iter().all(|d| d.path != "mods/player.jar"));
        assert_eq!(plan.unchanged, ["config/sodium.json"]);
        let summary = plan.summary();
        assert_eq!(summary.removed.mods, 2);
        assert_eq!(summary.remove_bytes, 12);
        assert!(!plan.next_state.items["sodium"]);

        // Applying it really deletes them and keeps the player's files.
        let downloader = Downloader::new(test_http());
        apply_sync(&plan, &downloader, &DownloadOptions::default(), None)
            .await
            .unwrap();
        assert!(!game.join("mods/sodium.jar").exists());
        assert!(!game.join("mods/fabric-api.jar").exists());
        assert!(game.join("mods/player.jar").exists());
        assert!(game.join("mods/zoom.jar").exists());
    }

    #[tokio::test]
    async fn new_items_are_not_installed_for_existing_players() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let v1 = item_manifest("mods/sodium.jar", &[]);
        let first = plan_sync(&v1, game, None, None, &SyncPolicy::default())
            .await
            .unwrap();
        let state = installed(
            game,
            &first,
            &[
                ("mods/fabric-api.jar", b"fabric"),
                ("mods/sodium.jar", b"sodium"),
                ("config/sodium.json", b"{}"),
            ],
        );

        // Version 2 adds a default-on item.
        let mut v2 = item_manifest("mods/sodium.jar", &[("zoom", true)]);
        v2.version_id = "2.1.0".into();
        let plan = plan_sync(&v2, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(!plan.changes_files(), "{:?}", plan.downloads);
        let zoom = status(&plan, "zoom");
        assert!(zoom.is_new && !zoom.effectively_enabled);
        // Only the version moved; the item stays new until the mod center is confirmed.
        assert!(plan.state_changed && !plan.is_noop());
        assert!(!plan.next_state.seen_items.contains("zoom"));

        // A brand-new install of the same version gets it.
        let fresh = tempfile::tempdir().unwrap();
        let plan = plan_sync(&v2, fresh.path(), None, None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(plan.downloads.iter().any(|d| d.path == "mods/zoom.jar"));
    }

    #[tokio::test]
    async fn renamed_item_file_is_an_update() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let v1 = item_manifest("mods/sodium-0.5.jar", &[]);
        let first = plan_sync(&v1, game, None, None, &SyncPolicy::default())
            .await
            .unwrap();
        let state = installed(
            game,
            &first,
            &[
                ("mods/fabric-api.jar", b"fabric"),
                ("mods/sodium-0.5.jar", b"sodium"),
                ("config/sodium.json", b"{}"),
            ],
        );

        let mut v2 = item_manifest("mods/sodium-0.6.jar", &[]);
        v2.files[1] = pack_file("mods/sodium-0.6.jar", b"sodium 0.6");
        let plan = plan_sync(&v2, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(
            paths(&plan.downloads),
            [("mods/sodium-0.6.jar", DownloadAction::Update)]
        );
        assert_eq!(deleted(&plan), ["mods/sodium-0.5.jar"]);
        assert!(plan.deletes[0].replaced);
        let summary = plan.summary();
        assert_eq!(summary.describe_tr(), "1 mod güncellenecek");
        assert_eq!(summary.removed.total(), 0);
        assert_eq!(summary.remove_bytes, 6);
        let sodium = status(&plan, "sodium");
        assert!(sodium.selected && sodium.installed && sodium.update_available);

        // The choice follows the id: switched off, the renamed file is not installed and the
        // old one goes away.
        let mut off = state.clone();
        off.items.insert("sodium".into(), false);
        let plan = plan_sync(&v2, game, Some(&off), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(plan.downloads.is_empty());
        assert_eq!(
            deleted(&plan),
            ["mods/fabric-api.jar", "mods/sodium-0.5.jar"]
        );
    }

    /// Whether this disk ignores letter case in file names (Windows, macOS by default).
    fn ignores_case(dir: &Path) -> bool {
        std::fs::write(dir.join("case-probe"), b"x").unwrap();
        let ignores = dir.join("CASE-PROBE").exists();
        std::fs::remove_file(dir.join("case-probe")).unwrap();
        ignores
    }

    fn names_in(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    /// Studio spells a path with other letter case in the next version (`config/Foo.toml` →
    /// `config/foo.toml`). Where the disk ignores case both name one file: it is kept (not
    /// deleted as "no longer wanted", which deleted the new file too), takes the new spelling,
    /// and an item file is not mistaken for the player's change.
    #[tokio::test]
    async fn a_path_that_only_changes_letter_case_keeps_its_file() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let v1 = manifest(
            vec![
                pack_file("config/Foo.toml", b"foo"),
                pack_file("mods/Zoom.jar", b"zoom"),
            ],
            ProjectsExtension {
                items: vec![pack_item("zoom", "mods/Zoom.jar", true, &[])],
                ..config_policy()
            },
        );
        let first = plan_sync(&v1, game, None, None, &SyncPolicy::default())
            .await
            .unwrap();
        let state = installed(
            game,
            &first,
            &[("config/Foo.toml", b"foo"), ("mods/Zoom.jar", b"zoom")],
        );
        state.save(game).await.unwrap();

        let mut v2 = manifest(
            vec![
                pack_file("config/foo.toml", b"foo"),
                pack_file("mods/zoom.jar", b"zoom"),
            ],
            ProjectsExtension {
                items: vec![pack_item("zoom", "mods/zoom.jar", true, &[])],
                ..config_policy()
            },
        );
        v2.version_id = "2.1.0".into();
        let plan = plan_sync(&v2, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        if !ignores_case(game) {
            // Two different files on a disk that minds case: the old one goes, the new one comes.
            assert_eq!(deleted(&plan), ["config/Foo.toml", "mods/Zoom.jar"]);
            assert_eq!(plan.downloads.len(), 2);
            return;
        }
        assert!(plan.deletes.is_empty(), "{:?}", plan.deletes);
        assert!(plan.downloads.is_empty(), "{:?}", plan.downloads);
        assert!(plan.adopted.is_empty(), "{:?}", plan.adopted);
        assert!(status(&plan, "zoom").effectively_enabled);
        let keys: Vec<&str> = plan.next_state.files.keys().map(String::as_str).collect();
        assert_eq!(keys, ["config/foo.toml", "mods/zoom.jar"]);

        let downloader = Downloader::new(test_http());
        let report = apply_sync(&plan, &downloader, &DownloadOptions::default(), None)
            .await
            .unwrap();
        assert!(report.issues.is_empty(), "{:?}", report.issues);
        assert_eq!(names_in(&game.join("config")), ["foo.toml"]);
        assert_eq!(names_in(&game.join("mods")), ["zoom.jar"]);
        assert_eq!(std::fs::read(game.join("mods/zoom.jar")).unwrap(), b"zoom");

        // The next sync has nothing to do and adopts nothing.
        let saved = PackState::load(game).await.unwrap().unwrap();
        let next = plan_sync(&v2, game, Some(&saved), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(next.is_noop(), "{next:?}");
        assert!(next.adopted.is_empty());
    }

    /// The same rename together with new content: an update in place, never a deletion of the
    /// freshly downloaded file.
    #[tokio::test]
    async fn a_case_only_rename_with_new_content_is_an_update() {
        let server = TestServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let downloader = Downloader::new(test_http());
        let v1 = sample_manifest(&server, &[("config/Foo.toml", b"v1".as_slice())]);
        let plan = plan_sync(&v1, game, None, None, &SyncPolicy::default())
            .await
            .unwrap();
        apply_sync(&plan, &downloader, &DownloadOptions::default(), None)
            .await
            .unwrap();
        let saved = PackState::load(game).await.unwrap().unwrap();

        let mut v2 = sample_manifest(&server, &[("config/foo.toml", b"v2".as_slice())]);
        v2.version_id = "1.1.0".into();
        let plan = plan_sync(&v2, game, Some(&saved), None, &SyncPolicy::default())
            .await
            .unwrap();
        apply_sync(&plan, &downloader, &DownloadOptions::default(), None)
            .await
            .unwrap();
        if ignores_case(game) {
            assert!(plan.deletes.is_empty(), "{:?}", plan.deletes);
            assert_eq!(
                paths(&plan.downloads),
                [("config/foo.toml", DownloadAction::Update)]
            );
        }
        assert_eq!(names_in(&game.join("config")), ["foo.toml"]);
        assert_eq!(std::fs::read(game.join("config/foo.toml")).unwrap(), b"v2");
    }

    /// A launch cancelled while the sync deletes old files: the deletions finish and the state
    /// is written, so the next background sync does not mistake the update for the player's
    /// own removal (which would switch the item off).
    #[tokio::test]
    async fn a_cancel_during_removals_still_records_the_sync() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let mut v1 = item_manifest("mods/sodium-0.5.jar", &[]);
        v1.files.push(pack_file("resourcepacks/x.zip", b"pack"));
        let first = plan_sync(&v1, game, None, None, &SyncPolicy::default())
            .await
            .unwrap();
        let state = installed(
            game,
            &first,
            &[
                ("mods/fabric-api.jar", b"fabric"),
                ("mods/sodium-0.5.jar", b"sodium"),
                ("config/sodium.json", b"{}"),
                ("resourcepacks/x.zip", b"pack"),
            ],
        );
        state.save(game).await.unwrap();

        // Version 2 renames the sodium jar (already in place) and drops the resource pack.
        let mut v2 = item_manifest("mods/sodium-0.6.jar", &[]);
        v2.files[1] = pack_file("mods/sodium-0.6.jar", b"sodium 0.6");
        write(game, "mods/sodium-0.6.jar", b"sodium 0.6");
        let plan = plan_sync(&v2, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(plan.downloads.is_empty());
        assert_eq!(
            deleted(&plan),
            ["mods/sodium-0.5.jar", "resourcepacks/x.zip"]
        );

        // The launch is cancelled right after the first deletion started.
        let options = DownloadOptions::default();
        let cancel = options.cancel.clone();
        let progress: SyncProgressFn = Arc::new(move |p: SyncProgress| {
            if p.phase == SyncPhase::Removing && p.done_files == 0 {
                cancel.cancel();
            }
        });
        let downloader = Downloader::new(test_http());
        let _ = apply_sync(&plan, &downloader, &options, Some(progress)).await;
        assert!(!game.join("mods/sodium-0.5.jar").exists());
        assert!(!game.join("resourcepacks/x.zip").exists());

        let saved = PackState::load(game).await.unwrap().unwrap();
        let next = plan_sync(&v2, game, Some(&saved), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(next.adopted.is_empty(), "{:?}", next.adopted);
        assert!(status(&next, "sodium").effectively_enabled);
        assert!(!next.changes_files());
    }

    #[tokio::test]
    async fn deleted_item_files_switch_their_items_off() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let (m, state) = installed_with_zoom(game).await;
        std::fs::remove_file(game.join("mods/zoom.jar")).unwrap();

        // A background sync adopts it: zoom is off, nothing is downloaded or deleted.
        let plan = plan_sync(&m, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(!plan.changes_files(), "{plan:?}");
        assert!(!plan.reset);
        assert_eq!(
            plan.adopted,
            [AdoptedChange {
                item: "zoom".into(),
                change: ManualChange::Removed
            }]
        );
        let zoom = status(&plan, "zoom");
        assert!(!zoom.selected && !zoom.effectively_enabled);
        assert_eq!(zoom.manual, Some(ManualChange::Removed));
        assert!(!plan.next_state.items["zoom"]);
        assert!(!plan.next_state.files.contains_key("mods/zoom.jar"));
        assert!(plan.state_changed);

        // The next sync has nothing new to say, the note stays until the mod center is
        // confirmed.
        let next = plan_sync(
            &m,
            game,
            Some(&plan.next_state),
            None,
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(next.is_noop() && next.adopted.is_empty());
        assert_eq!(status(&next, "zoom").manual, Some(ManualChange::Removed));
        let confirmed = plan_sync(
            &m,
            game,
            Some(&plan.next_state),
            Some(&PackSelection::default()),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(confirmed.next_state.manual.is_empty());
        assert!(!confirmed.changes_files());

        // Picking it again in the mod center downloads it.
        let back = plan_sync(
            &m,
            game,
            Some(&plan.next_state),
            Some(&select(&[("zoom", true)])),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            paths(&back.downloads),
            [("mods/zoom.jar", DownloadAction::Add)]
        );
    }

    #[tokio::test]
    async fn a_required_item_the_player_removed_comes_back() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let m = item_manifest("mods/sodium.jar", &[]);
        let first = plan_sync(
            &m,
            game,
            None,
            Some(&select(&[("iris", true)])),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        let state = installed(
            game,
            &first,
            &[
                ("mods/fabric-api.jar", b"fabric"),
                ("mods/sodium.jar", b"sodium"),
                ("mods/iris.jar", b"iris"),
                ("config/sodium.json", b"{}"),
            ],
        );
        // Iris needs Sodium (and Sodium the library); the player deleted Sodium and disabled
        // the library.
        std::fs::remove_file(game.join("mods/sodium.jar")).unwrap();
        std::fs::rename(
            game.join("mods/fabric-api.jar"),
            game.join("mods/fabric-api.jar.disabled"),
        )
        .unwrap();
        let plan = plan_sync(&m, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(
            paths(&plan.downloads),
            [("mods/sodium.jar", DownloadAction::Add)]
        );
        let restored: Vec<_> = plan.restores.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(restored, ["mods/fabric-api.jar"]);
        let sodium = status(&plan, "sodium");
        assert!(!sodium.selected && sodium.effectively_enabled);
        assert_eq!(sodium.required_by, ["iris"]);
        assert_eq!(sodium.manual, Some(ManualChange::Removed));
        let fabric = status(&plan, "fabric-api");
        assert_eq!(fabric.manual, Some(ManualChange::Disabled));
        assert_eq!(fabric.required_by, ["sodium"]);
        let changes: Vec<_> = plan.adopted.iter().map(|c| c.item.as_str()).collect();
        assert_eq!(changes, ["sodium", "fabric-api"]);
    }

    #[tokio::test]
    async fn disabled_item_files_are_adopted_and_kept() {
        let server = TestServer::start().await;
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let mut m = sample_manifest(
            &server,
            &[
                ("mods/sodium.jar", b"sodium v1".as_slice()),
                ("mods/iris.jar", b"iris v1"),
            ],
        );
        m.projects.items = vec![
            pack_item("sodium", "mods/sodium.jar", true, &[]),
            pack_item("iris", "mods/iris.jar", true, &[]),
        ];
        let first = plan_sync(&m, game, None, None, &SyncPolicy::default())
            .await
            .unwrap();
        let downloader = Downloader::new(test_http());
        apply_sync(&first, &downloader, &DownloadOptions::default(), None)
            .await
            .unwrap();
        // The player disabled both jars outside the launcher.
        for jar in ["mods/sodium.jar", "mods/iris.jar"] {
            std::fs::rename(game.join(jar), game.join(format!("{jar}.disabled"))).unwrap();
        }

        // Both count as switched off; the disabled copies stay, even when Iris gets an update.
        let mut m2 = sample_manifest(
            &server,
            &[
                ("mods/sodium.jar", b"sodium v1".as_slice()),
                ("mods/iris.jar", b"iris v2"),
            ],
        );
        m2.projects.items = m.projects.items.clone();
        let state = PackState::load(game).await.unwrap();
        let plan = plan_sync(&m2, game, state.as_ref(), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(!plan.changes_files(), "{plan:?}");
        assert_eq!(plan.adopted.len(), 2);
        for id in ["sodium", "iris"] {
            let item = status(&plan, id);
            assert!(!item.effectively_enabled && item.disabled_copy);
            assert_eq!(item.manual, Some(ManualChange::Disabled));
            assert!(!plan.next_state.items[id]);
        }
        assert!(plan.next_state.files["mods/sodium.jar"].disabled);
        apply_sync(&plan, &downloader, &DownloadOptions::default(), None)
            .await
            .unwrap();
        assert!(game.join("mods/sodium.jar.disabled").exists());
        assert!(game.join("mods/iris.jar.disabled").exists());

        // Confirming the mod center with them off keeps the copies.
        let state = PackState::load(game).await.unwrap();
        let confirmed = plan_sync(
            &m2,
            game,
            state.as_ref(),
            Some(&select(&[("sodium", false), ("iris", false)])),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(!confirmed.changes_files());
        assert!(confirmed.next_state.manual.is_empty());

        // The player renames Sodium back: adopted as switched on, nothing else happens.
        std::fs::rename(
            game.join("mods/sodium.jar.disabled"),
            game.join("mods/sodium.jar"),
        )
        .unwrap();
        let plan = plan_sync(
            &m2,
            game,
            Some(&confirmed.next_state),
            None,
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(!plan.changes_files());
        assert_eq!(
            plan.adopted,
            [AdoptedChange {
                item: "sodium".into(),
                change: ManualChange::Enabled
            }]
        );
        assert!(plan.next_state.items["sodium"]);
        assert!(!plan.next_state.files["mods/sodium.jar"].disabled);

        // Iris selected in the mod center: its stale copy is replaced by the update.
        let plan = plan_sync(
            &m2,
            game,
            Some(&plan.next_state),
            Some(&select(&[("iris", true)])),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            paths(&plan.downloads),
            [("mods/iris.jar", DownloadAction::Add)]
        );
        assert_eq!(deleted(&plan), ["mods/iris.jar.disabled"]);
        let report = apply_sync(&plan, &downloader, &DownloadOptions::default(), None)
            .await
            .unwrap();
        assert_eq!((report.downloaded, report.deleted), (1, 1));
        assert_eq!(
            std::fs::read(game.join("mods/iris.jar")).unwrap(),
            b"iris v2"
        );
        let again = plan_sync(
            &m2,
            game,
            PackState::load(game).await.unwrap().as_ref(),
            None,
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(again.is_noop(), "{again:?}");

        // An item selected while its copy sits disabled is renamed back, not downloaded.
        std::fs::rename(
            game.join("mods/iris.jar"),
            game.join("mods/iris.jar.disabled"),
        )
        .unwrap();
        let state = PackState::load(game).await.unwrap();
        let plan = plan_sync(
            &m2,
            game,
            state.as_ref(),
            Some(&select(&[("iris", true)])),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        let restored: Vec<_> = plan.restores.iter().map(|r| r.path.as_str()).collect();
        assert_eq!(restored, ["mods/iris.jar"]);
        assert!(plan.downloads.is_empty());
    }

    #[tokio::test]
    async fn replaced_item_files_are_kept_until_reverted() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let (m, state) = installed_with_zoom(game).await;
        write(game, "mods/zoom.jar", b"zoom, patched by the player");

        let plan = plan_sync(&m, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(!plan.changes_files());
        let zoom = status(&plan, "zoom");
        assert!(zoom.modified && zoom.effectively_enabled && zoom.manual.is_none());
        assert_eq!(
            plan.adopted,
            [AdoptedChange {
                item: "zoom".into(),
                change: ManualChange::Replaced
            }]
        );
        assert!(plan.next_state.files["mods/zoom.jar"].modified);
        // Said once.
        let next = plan_sync(
            &m,
            game,
            Some(&plan.next_state),
            None,
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(next.adopted.is_empty() && status(&next, "zoom").modified);
        // A pack update does not overwrite it either.
        let mut updated = m.clone();
        let zoom_file = updated
            .files
            .iter_mut()
            .find(|f| f.path == "mods/zoom.jar")
            .unwrap();
        *zoom_file = pack_file("mods/zoom.jar", b"zoom 2");
        let update = plan_sync(
            &updated,
            game,
            Some(&next.next_state),
            None,
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(update.downloads.is_empty());

        // "Önerilene döndür" brings the pack's file back without touching the choice.
        let revert = PackSelection {
            revert: BTreeSet::from(["zoom".to_owned()]),
            ..PackSelection::default()
        };
        let plan = plan_sync(
            &m,
            game,
            Some(&next.next_state),
            Some(&revert),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert_eq!(
            paths(&plan.downloads),
            [("mods/zoom.jar", DownloadAction::Update)]
        );
        assert!(plan.next_state.items["zoom"]);
        assert!(!plan.next_state.files["mods/zoom.jar"].modified);
    }

    #[tokio::test]
    async fn wiped_mods_folder_is_reinstalled() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let (m, state) = installed_with_zoom(game).await;
        write(game, "mods/player.jar", b"mine");

        // Most pack mods gone at once: a reset, not four manual removals.
        for jar in ["fabric-api", "sodium", "zoom", "minimap"] {
            std::fs::remove_file(game.join(format!("mods/{jar}.jar"))).unwrap();
        }
        let plan = plan_sync(&m, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(plan.reset && plan.adopted.is_empty());
        assert_eq!(plan.downloads.len(), 4);
        assert_eq!(plan.next_state.items, state.items);

        // The folder itself gone: also a reset, even for a single mod.
        std::fs::remove_dir_all(game.join("mods")).unwrap();
        let mut small = state.clone();
        small
            .files
            .retain(|path, _| path != "mods/zoom.jar" && path != "mods/minimap.jar");
        small.files.retain(|path, _| path != "mods/fabric-api.jar");
        let plan = plan_sync(&m, game, Some(&small), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(plan.reset);
        assert!(plan.downloads.iter().any(|d| d.path == "mods/sodium.jar"));

        // Two of four missing is a choice, not a wipe.
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let (m, state) = installed_with_zoom(game).await;
        for jar in ["zoom", "minimap"] {
            std::fs::remove_file(game.join(format!("mods/{jar}.jar"))).unwrap();
        }
        let plan = plan_sync(&m, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(!plan.reset && plan.downloads.is_empty());
        assert_eq!(plan.adopted.len(), 2);
    }

    #[tokio::test]
    async fn explicit_selection_beats_adoption() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let (m, state) = installed_with_zoom(game).await;
        std::fs::remove_file(game.join("mods/zoom.jar")).unwrap();
        let plan = plan_sync(
            &m,
            game,
            Some(&state),
            Some(&select(&[("zoom", true)])),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(plan.adopted.is_empty());
        assert_eq!(
            paths(&plan.downloads),
            [("mods/zoom.jar", DownloadAction::Add)]
        );
    }

    #[tokio::test]
    async fn unreadable_files_are_issues_not_errors() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let (m, state) = installed_with_zoom(game).await;
        // A folder where the pack's jar belongs.
        std::fs::remove_file(game.join("mods/zoom.jar")).unwrap();
        std::fs::create_dir_all(game.join("mods/zoom.jar")).unwrap();
        let plan = plan_sync(&m, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(plan.issues.len(), 1);
        assert_eq!(plan.issues[0].path, "mods/zoom.jar");
        assert_eq!(plan.issues[0].kind, SyncIssueKind::NotAFile);
        assert!(plan.adopted.is_empty(), "not a manual change");
        assert!(plan.downloads.is_empty());
        assert!(
            plan.next_state.files.contains_key("mods/zoom.jar"),
            "the record stays for the next sync"
        );

        // A deletion that fails is reported and its record kept.
        let mut forged = plan.clone();
        std::fs::create_dir_all(game.join("mods/stuck.jar")).unwrap();
        forged.deletes.push(PlannedDelete {
            path: "mods/stuck.jar".into(),
            category: FileCategory::Mod,
            size: 1,
            item: None,
            replaced: false,
            record: Some((
                "mods/stuck.jar".into(),
                InstalledFile::new(sha1_hex(b"stuck"), FilePolicy::Overwrite, None),
            )),
        });
        let downloader = Downloader::new(test_http());
        let report = apply_sync(&forged, &downloader, &DownloadOptions::default(), None)
            .await
            .unwrap();
        assert_eq!(report.issues.len(), 1);
        assert_eq!(report.issues[0].kind, SyncIssueKind::Busy);
        let saved = PackState::load(game).await.unwrap().unwrap();
        assert!(saved.files.contains_key("mods/stuck.jar"));
    }

    #[tokio::test]
    async fn choices_without_file_changes_are_saved() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let m = item_manifest("mods/sodium.jar", &[("zoom", false)]);
        let first = plan_sync(&m, game, None, None, &SyncPolicy::default())
            .await
            .unwrap();
        let state = installed(
            game,
            &first,
            &[
                ("mods/fabric-api.jar", b"fabric"),
                ("mods/sodium.jar", b"sodium"),
                ("config/sodium.json", b"{}"),
            ],
        );
        state.save(game).await.unwrap();
        let same = plan_sync(&m, game, Some(&state), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert!(same.is_noop());

        // Turning off Iris (already off) changes nothing on disk, but confirming the mod
        // center is still recorded.
        let mut unseen = state.clone();
        unseen.seen_items.remove("zoom");
        let plan = plan_sync(
            &m,
            game,
            Some(&unseen),
            Some(&select(&[("iris", false)])),
            &SyncPolicy::default(),
        )
        .await
        .unwrap();
        assert!(!plan.changes_files() && plan.state_changed && !plan.is_noop());
        assert!(plan.next_state.seen_items.contains("zoom"));

        // The same without a selection: nothing to apply.
        let mut seen = state.clone();
        seen.mark_items_seen(&m);
        assert_eq!(seen, state, "already confirmed");
    }

    #[test]
    fn download_estimates_need_no_disk() {
        let m = item_manifest("mods/sodium.jar", &[("zoom", false)]);
        // A first visit: choices carried over, no records → every wanted file.
        let carried = PackState {
            items: BTreeMap::from([("sodium".into(), true), ("iris".into(), true)]),
            seen_items: BTreeSet::from(["sodium".into(), "iris".into(), "fabric-api".into()]),
            ..PackState::default()
        };
        let estimate = estimate_downloads(&m, Some(&carried));
        assert_eq!(estimate.files, 4, "fabric-api, sodium, iris and the config");
        assert_eq!(estimate.bytes, (6 + 6 + 4 + 2) as u64);

        // A set whose files are recorded with the manifest's hashes needs nothing.
        let mut synced = carried.clone();
        for file in &m.files {
            if file.path != "mods/zoom.jar" {
                synced.files.insert(
                    file.path.clone(),
                    InstalledFile::new(file.hashes.sha1.clone(), FilePolicy::Overwrite, None),
                );
            }
        }
        assert_eq!(estimate_downloads(&m, Some(&synced)).files, 0);
    }

    #[test]
    fn version_switch_splits_the_state() {
        let record = |sha: &str, item: Option<&str>| {
            InstalledFile::new(sha.into(), FilePolicy::Overwrite, item.map(str::to_owned))
        };
        let v1211 = PackState {
            pack_id: "projects".into(),
            version_id: "2.3.0".into(),
            family: "projects".into(),
            minecraft: "1.21.1".into(),
            files: BTreeMap::from([
                (
                    "mods/sodium-1.21.1.jar".into(),
                    record("aa", Some("sodium")),
                ),
                ("config/sodium.json".into(), record("bb", None)),
                ("options.txt".into(), record("ee", None)),
            ]),
            items: BTreeMap::from([("sodium".into(), true), ("iris".into(), false)]),
            seen_items: BTreeSet::from(["sodium".into(), "iris".into()]),
            manual: BTreeMap::from([("iris".into(), ManualChange::Removed)]),
            ..PackState::default()
        };

        // First visit of 1.21.4: the choices and the copied config carry over.
        let switch = v1211.clone().switch_version("1.21.1", "1.21.4", None);
        assert_eq!(
            switch.parked.files.keys().collect::<Vec<_>>(),
            ["config/sodium.json", "mods/sodium-1.21.1.jar"]
        );
        assert_eq!(switch.parked.items, v1211.items);
        assert_eq!(switch.parked.manual, v1211.manual);
        assert_eq!(switch.parked.minecraft, "1.21.1");
        assert_eq!(
            switch.active.files.keys().collect::<Vec<_>>(),
            ["config/sodium.json", "options.txt"]
        );
        assert_eq!(switch.active.items, v1211.items);
        assert_eq!(switch.active.seen_items, v1211.seen_items);
        assert!(switch.active.manual.is_empty());
        assert!(switch.active.pack_id.is_empty() && switch.active.version_id.is_empty());
        assert_eq!(
            (
                switch.active.family.as_str(),
                switch.active.minecraft.as_str()
            ),
            ("projects", "1.21.4")
        );

        // Later 1.21.4 has its own mods, config and choices.
        let mut v1214 = switch.active;
        v1214.pack_id = "projects-1.21.4".into();
        v1214.files.insert(
            "mods/sodium-1.21.4.jar".into(),
            record("cc", Some("sodium")),
        );
        v1214
            .files
            .insert("config/sodium.json".into(), record("dd", None));
        v1214.items.insert("sodium".into(), false);

        // Back to 1.21.1: its mods, config records and choices return; shared records are
        // the active set's.
        let back = v1214.switch_version("1.21.4", "1.21.1", Some(switch.parked.clone()));
        assert_eq!(back.active.items, v1211.items);
        assert_eq!(back.active.seen_items, v1211.seen_items);
        assert_eq!(back.active.pack_id, "projects");
        assert_eq!(back.active.files["mods/sodium-1.21.1.jar"].sha1, "aa");
        assert_eq!(back.active.files["config/sodium.json"].sha1, "bb");
        assert_eq!(back.active.files["options.txt"].sha1, "ee");
        assert!(!back.active.files.contains_key("mods/sodium-1.21.4.jar"));
        assert_eq!(
            back.parked.files.keys().collect::<Vec<_>>(),
            ["config/sodium.json", "mods/sodium-1.21.4.jar"]
        );
        assert_eq!(back.parked.files["config/sodium.json"].sha1, "dd");
        assert!(!back.parked.items["sodium"]);
        assert_eq!(back.parked.pack_id, "projects-1.21.4");
    }
}
