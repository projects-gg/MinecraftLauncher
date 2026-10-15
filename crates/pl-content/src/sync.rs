//! Modpack sync: diff a pack manifest against a game directory, then apply the difference.
//!
//! Ownership is tracked in `<game_dir>/.projects/pack-state.json` (what we installed and with
//! which hash), which gives the rules:
//! - only files the pack installed earlier and that left the manifest are deleted, and only
//!   while they still have the hash we installed (user-added files are never touched);
//! - `overwrite` files always end up matching the pack; `ifMissing` files are installed only
//!   when absent; `preserveUserChanges` files are updated only while unchanged since we
//!   installed them;
//! - a pack file the player disabled (`<path>.disabled`) stays disabled;
//! - optional groups add or remove their files; server-only files are skipped;
//! - every path is validated (no absolute paths, `..`, drive letters, `.projects/`).

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;

use futures::StreamExt;
use pl_net::{Checksum, DownloadItem, DownloadOptions, DownloadProgress, Downloader, HashAlgo};
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::fsutil::{normalize_hex, read_optional, write_atomic};
use crate::model::{EnvSupport, FilePolicy, PackFile, PackManifest};
use crate::path::{LAUNCHER_DIR, check_relative_path, join_relative};

/// File name of the pack state inside `<game_dir>/.projects/`.
pub const PACK_STATE_FILE: &str = "pack-state.json";
const DISABLED_SUFFIX: &str = ".disabled";
const HASH_CONCURRENCY: usize = 8;

/// What the launcher installed into a game directory.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PackState {
    pub pack_id: String,
    pub version_id: String,
    /// Relative path → the file as we last installed it.
    pub files: BTreeMap<String, InstalledFile>,
    /// Optional group id → enabled, as chosen at the last sync.
    pub groups: BTreeMap<String, bool>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InstalledFile {
    pub sha1: String,
    #[serde(default)]
    pub policy: FilePolicy,
}

impl PackState {
    pub fn path(game_dir: &Path) -> PathBuf {
        game_dir.join(LAUNCHER_DIR).join(PACK_STATE_FILE)
    }

    /// Loads the state; `None` when there is none. A corrupt file is reported and treated as
    /// absent: the next sync then deletes nothing and keeps changed configs.
    pub async fn load(game_dir: &Path) -> Result<Option<Self>> {
        let path = Self::path(game_dir);
        let Some(bytes) = read_optional(&path).await? else {
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

    /// Writes the state atomically.
    pub async fn save(&self, game_dir: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| Error::json(PACK_STATE_FILE, e))?;
        write_atomic(&Self::path(game_dir), &bytes).await
    }
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
    /// An existing file is replaced.
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlannedDelete {
    pub path: String,
    pub category: FileCategory,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum KeepReason {
    /// The player changed a file the pack manages (or owns a file at that path).
    UserModified,
    /// `ifMissing`: the file exists, so it is left as is.
    ExistingFile,
    /// The player disabled it (`<path>.disabled`).
    UserDisabled,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct KeptFile {
    pub path: String,
    pub reason: KeepReason,
    pub category: FileCategory,
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
    pub deletes: Vec<PlannedDelete>,
    pub unchanged: Vec<String>,
    pub kept_user_modified: Vec<KeptFile>,
    /// State written after a successful apply.
    pub next_state: PackState,
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

/// Dry-run summary for the UI.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncSummary {
    pub added: CategoryCounts,
    pub updated: CategoryCounts,
    pub removed: CategoryCounts,
    pub kept: usize,
    pub unchanged: usize,
    pub download_bytes: u64,
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
        for delete in &self.deletes {
            summary.removed.add(delete.category);
        }
        summary
    }

    pub fn is_noop(&self) -> bool {
        self.downloads.is_empty() && self.deletes.is_empty()
    }
}

enum Decision {
    Unchanged(InstalledFile),
    Download(DownloadAction, InstalledFile),
    Keep(KeepReason, Option<InstalledFile>),
}

enum Removal {
    Delete(String),
    Keep,
    Gone,
}

/// Computes what a sync would do. `previous` is the state from the last sync
/// ([`PackState::load`]); `selection` overrides optional groups (missing groups use the
/// previous choice, then the group default).
pub async fn plan_sync(
    manifest: &PackManifest,
    game_dir: &Path,
    previous: Option<&PackState>,
    selection: Option<&BTreeMap<String, bool>>,
    policy: &SyncPolicy,
) -> Result<SyncPlan> {
    manifest.validate()?;
    let ext = &manifest.projects;
    let groups: BTreeMap<String, bool> = ext
        .optional_groups
        .iter()
        .map(|group| {
            let enabled = selection
                .and_then(|s| s.get(&group.id))
                .or_else(|| previous.and_then(|p| p.groups.get(&group.id)))
                .copied()
                .unwrap_or(group.default_enabled);
            (group.id.clone(), enabled)
        })
        .collect();
    let mut membership: HashMap<&str, Vec<&str>> = HashMap::new();
    for group in &ext.optional_groups {
        for path in &group.files {
            membership.entry(path).or_default().push(&group.id);
        }
    }

    let desired: Vec<(&PackFile, FilePolicy)> = manifest
        .files
        .iter()
        .filter(|file| file.client_support() != EnvSupport::Unsupported)
        .filter(|file| {
            membership.get(file.path.as_str()).is_none_or(|ids| {
                ids.iter()
                    .any(|id| groups.get(*id).copied().unwrap_or(false))
            })
        })
        .map(|file| (file, ext.config_policy.policy_for(&file.path)))
        .collect();
    let desired_paths: HashSet<&str> = desired.iter().map(|(file, _)| file.path.as_str()).collect();

    let empty = PackState::default();
    let prev = previous.unwrap_or(&empty);

    // Futures are built eagerly so the plan future stays `Send` for spawning callers.
    let inspections: Vec<_> = desired
        .iter()
        .map(|(file, file_policy)| {
            inspect_desired(
                game_dir,
                file,
                *file_policy,
                prev.files.get(&file.path),
                policy,
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
        deletes: Vec::new(),
        unchanged: Vec::new(),
        kept_user_modified: Vec::new(),
        next_state: PackState {
            pack_id: ext.pack_id.clone(),
            version_id: manifest.version_id.clone(),
            files: BTreeMap::new(),
            groups,
        },
    };

    for ((file, _), decision) in desired.iter().zip(decisions) {
        let category = FileCategory::of(&file.path);
        match decision? {
            Decision::Unchanged(record) => {
                plan.unchanged.push(file.path.clone());
                plan.next_state.files.insert(file.path.clone(), record);
            }
            Decision::Download(action, record) => {
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
                });
                plan.next_state.files.insert(file.path.clone(), record);
            }
            Decision::Keep(reason, record) => {
                plan.kept_user_modified.push(KeptFile {
                    path: file.path.clone(),
                    reason,
                    category,
                });
                if let Some(record) = record {
                    plan.next_state.files.insert(file.path.clone(), record);
                }
            }
        }
    }

    let removed: Vec<(&String, &InstalledFile)> = prev
        .files
        .iter()
        .filter(|(path, _)| !desired_paths.contains(path.as_str()))
        .collect();
    let removal_checks: Vec<_> = removed
        .iter()
        .map(|(path, record)| inspect_removed(game_dir, path, record))
        .collect();
    let removals: Vec<Result<Removal>> = futures::stream::iter(removal_checks)
        .buffered(HASH_CONCURRENCY)
        .collect()
        .await;
    for ((path, _), removal) in removed.iter().zip(removals) {
        match removal? {
            Removal::Delete(target) => plan.deletes.push(PlannedDelete {
                category: FileCategory::of(path),
                path: target,
            }),
            Removal::Keep => plan.kept_user_modified.push(KeptFile {
                path: (*path).clone(),
                reason: KeepReason::UserModified,
                category: FileCategory::of(path),
            }),
            Removal::Gone => {}
        }
    }
    Ok(plan)
}

async fn inspect_desired(
    game_dir: &Path,
    file: &PackFile,
    file_policy: FilePolicy,
    previous: Option<&InstalledFile>,
    policy: &SyncPolicy,
) -> Result<Decision> {
    let target = join_relative(game_dir, &file.path)?;
    let expected = normalize_hex(&file.hashes.sha1, 40)
        .ok_or_else(|| Error::Invalid(format!("bad sha1 for {}", file.path)))?;
    let record = InstalledFile {
        sha1: expected.clone(),
        policy: file_policy,
    };

    let metadata = match tokio::fs::metadata(&target).await {
        Ok(metadata) => Some(metadata),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => None,
        Err(e) => return Err(Error::io(&target, e)),
    };
    let Some(metadata) = metadata else {
        if is_file(&disabled_path(&target)).await {
            return Ok(Decision::Keep(KeepReason::UserDisabled, previous.cloned()));
        }
        return Ok(Decision::Download(DownloadAction::Add, record));
    };
    if !metadata.is_file() {
        return Err(Error::Invalid(format!(
            "{} exists but is not a file",
            file.path
        )));
    }

    if policy.quick_check
        && !policy.repair
        && metadata.len() == file.file_size
        && previous.is_some_and(|p| p.sha1 == expected)
    {
        return Ok(Decision::Unchanged(record));
    }
    let current = pl_net::hash_file(&target, HashAlgo::Sha1).await?;
    if current == expected {
        return Ok(Decision::Unchanged(record));
    }
    let effective = if policy.repair {
        FilePolicy::Overwrite
    } else {
        file_policy
    };
    Ok(match effective {
        FilePolicy::Overwrite => Decision::Download(DownloadAction::Update, record),
        FilePolicy::IfMissing => Decision::Keep(KeepReason::ExistingFile, previous.cloned()),
        FilePolicy::PreserveUserChanges => match previous {
            // Untouched since we installed it: safe to update.
            Some(prev) if prev.sha1 == current => {
                Decision::Download(DownloadAction::Update, record)
            }
            _ => Decision::Keep(KeepReason::UserModified, previous.cloned()),
        },
    })
}

async fn inspect_removed(game_dir: &Path, path: &str, record: &InstalledFile) -> Result<Removal> {
    // The state file is local and could be edited; never delete outside the game dir.
    let Ok(target) = join_relative(game_dir, path) else {
        tracing::warn!(path, "ignoring an unsafe path in the pack state");
        return Ok(Removal::Gone);
    };
    let installed = record.sha1.to_ascii_lowercase();
    if is_file(&target).await {
        let current = pl_net::hash_file(&target, HashAlgo::Sha1).await?;
        return Ok(if current == installed {
            Removal::Delete(path.to_owned())
        } else {
            Removal::Keep
        });
    }
    let disabled = disabled_path(&target);
    if is_file(&disabled).await && pl_net::hash_file(&disabled, HashAlgo::Sha1).await? == installed
    {
        return Ok(Removal::Delete(format!("{path}{DISABLED_SUFFIX}")));
    }
    Ok(Removal::Gone)
}

fn disabled_path(target: &Path) -> PathBuf {
    let mut name = target.as_os_str().to_os_string();
    name.push(DISABLED_SUFFIX);
    PathBuf::from(name)
}

async fn is_file(path: &Path) -> bool {
    tokio::fs::metadata(path)
        .await
        .is_ok_and(|metadata| metadata.is_file())
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

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SyncReport {
    pub downloaded: usize,
    pub deleted: usize,
    pub bytes: u64,
}

/// Applies a plan: downloads (verified by SHA-512 when known, else SHA-1, written atomically),
/// then deletions, then the new state. Nothing is deleted and no state is written when a
/// download fails, so the next plan simply resumes. `options.cancel` aborts the sync.
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

    let total = plan.deletes.len();
    for (done, delete) in plan.deletes.iter().enumerate() {
        if options.cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
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
            Err(e) => return Err(Error::io(&target, e)),
        }
    }

    plan.next_state.save(&plan.game_dir).await?;
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
        deleted = report.deleted,
        "pack synced"
    );
    Ok(report)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{
        ConfigPolicy, OptionalGroup, PackDependencies, PackEnv, PackHashes, PolicyRule,
        ProjectsExtension,
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
                        InstalledFile {
                            sha1: sha1_hex(content),
                            policy: *policy,
                        },
                    )
                })
                .collect(),
            groups: BTreeMap::new(),
        }
    }

    fn paths(downloads: &[PlannedDownload]) -> Vec<(&str, DownloadAction)> {
        downloads
            .iter()
            .map(|d| (d.path.as_str(), d.action))
            .collect()
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
        let deletes: Vec<_> = plan.deletes.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(deletes, ["mods/removed.jar"]);
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
    }

    #[tokio::test]
    async fn config_policies() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        write(game, "options.txt", b"user options");
        write(game, "config/edited.json", b"user edit");
        write(game, "config/untouched.json", b"v1");
        write(game, "config/foreign.json", b"theirs");
        write(game, "mods/overwritten.jar", b"tampered");
        let previous = state(&[
            ("config/edited.json", b"v1", FilePolicy::PreserveUserChanges),
            (
                "config/untouched.json",
                b"v1",
                FilePolicy::PreserveUserChanges,
            ),
            ("mods/overwritten.jar", b"good", FilePolicy::Overwrite),
        ]);
        let m = manifest(
            vec![
                pack_file("options.txt", b"pack options"),
                pack_file("config/edited.json", b"v2"),
                pack_file("config/untouched.json", b"v2"),
                pack_file("config/foreign.json", b"pack"),
                pack_file("config/missing.json", b"pack"),
                pack_file("mods/overwritten.jar", b"good"),
            ],
            config_policy(),
        );
        let plan = plan_sync(&m, game, Some(&previous), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(
            paths(&plan.downloads),
            [
                ("config/untouched.json", DownloadAction::Update),
                ("config/missing.json", DownloadAction::Add),
                ("mods/overwritten.jar", DownloadAction::Update),
            ]
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
            ]
        );
        // The edited config keeps the hash we installed, so it stays "modified" next time.
        assert_eq!(
            plan.next_state.files["config/edited.json"].sha1,
            sha1_hex(b"v1")
        );
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
        assert_eq!(repair.downloads.len(), 6);
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
        let selection =
            BTreeMap::from([("shaders".to_owned(), false), ("minimap".to_owned(), true)]);
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
        let deletes: Vec<_> = plan.deletes.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(deletes, ["shaderpacks/shaders.zip"]);
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
        assert!(plan.downloads.is_empty());
        assert_eq!(plan.kept_user_modified[0].reason, KeepReason::UserDisabled);
        assert!(plan.next_state.files.contains_key("mods/a.jar"));
        let deletes: Vec<_> = plan.deletes.iter().map(|d| d.path.as_str()).collect();
        assert_eq!(deletes, ["mods/gone.jar.disabled"]);
    }

    #[tokio::test]
    async fn quick_check_skips_hashing() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        // Same size as the expected content, different bytes.
        write(game, "mods/a.jar", b"xx");
        let previous = state(&[("mods/a.jar", b"ok", FilePolicy::Overwrite)]);
        let m = manifest(
            vec![pack_file("mods/a.jar", b"ok")],
            ProjectsExtension::default(),
        );
        let quick = SyncPolicy {
            repair: false,
            quick_check: true,
        };
        let plan = plan_sync(&m, game, Some(&previous), None, &quick)
            .await
            .unwrap();
        assert_eq!(plan.unchanged, ["mods/a.jar"]);
        let full = plan_sync(&m, game, Some(&previous), None, &SyncPolicy::default())
            .await
            .unwrap();
        assert_eq!(
            paths(&full.downloads),
            [("mods/a.jar", DownloadAction::Update)]
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
}
