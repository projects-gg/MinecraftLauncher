//! Files instances share: game versions (`versions/<id>/`), loader profiles (versions that inherit
//! from another), Mojang Java runtimes (`runtimes/<component>/`), libraries, assets and extracted
//! natives. [`scan`] finds what no instance uses any more; [`delete`] removes chosen items after
//! checking again that nothing uses them. Everything removed is downloaded again when needed.
//!
//! What is used starts from [`GameRef`]s (the active version of every instance and of every parked
//! version set) and follows `inheritsFrom`. A used version keeps its folder, its natives, every
//! library variant it lists (whatever the platform), the files its Forge/NeoForge installer
//! produced (`projects-loader.json`), its asset index with every object in it, its log
//! configuration and its Java runtime.
//!
//! Conservative by design: when the references of a used version cannot be read completely (an
//! unreadable or missing JSON in an existing folder, a Forge/NeoForge profile without its installer
//! record, an unreadable asset index), libraries, assets and Java runtimes are not offered at all.
//! Files changed within [`ScanOptions::min_age`] are never offered (a download may be writing
//! them). Version folders are offered by name: no instance refers to them.
//!
//! Sizes are what deleting frees. A file hard-linked into several version folders (a loader
//! profile's copy of the game jar) frees nothing while one of the links stays: it is left out of
//! the items and reported as a [`SharedGroup`] when every holder is itself unused.

use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime};

use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use walkdir::WalkDir;

use crate::error::{Error, Result};
use crate::loaders::{LoaderKind, installer_outputs_blocking, loader_profile_id};
use crate::paths::DataDirs;
use crate::version::{Library, VersionJson, component_major};

/// Files younger than this are never offered.
pub const DEFAULT_MIN_AGE: Duration = Duration::from_secs(24 * 60 * 60);
/// Deepest `inheritsFrom` chain followed.
const MAX_CHAIN: usize = 16;

/// A version an instance (or one of its parked sets) needs.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GameRef {
    pub minecraft: String,
    /// `None`: the loader is unknown; every profile built on `minecraft` stays.
    pub loader: Option<LoaderKind>,
    /// `None`: not chosen yet; every profile built on `minecraft` stays.
    pub loader_version: Option<String>,
}

impl GameRef {
    pub fn new(
        minecraft: impl Into<String>,
        loader: Option<LoaderKind>,
        loader_version: Option<String>,
    ) -> Self {
        Self {
            minecraft: minecraft.into(),
            loader,
            loader_version: loader_version.filter(|v| !v.trim().is_empty()),
        }
    }
}

/// How [`scan`] judges files.
#[derive(Debug, Clone)]
pub struct ScanOptions {
    pub now: SystemTime,
    /// Files changed more recently than this are kept.
    pub min_age: Duration,
    /// Files in use that may live inside the shared folders (an instance's custom Java
    /// executable inside `runtimes/`): whatever holds them stays.
    pub protected: Vec<PathBuf>,
}

impl Default for ScanOptions {
    fn default() -> Self {
        Self {
            now: SystemTime::now(),
            min_age: DEFAULT_MIN_AGE,
            protected: Vec::new(),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SharedKind {
    GameVersion,
    LoaderProfile,
    JavaRuntime,
    Libraries,
    Assets,
}

impl SharedKind {
    fn order(self) -> u8 {
        match self {
            Self::GameVersion => 0,
            Self::LoaderProfile => 1,
            Self::JavaRuntime => 2,
            Self::Libraries => 3,
            Self::Assets => 4,
        }
    }
}

/// Something no instance uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SharedItem {
    /// `version:<id>`, `runtime:<component>`, `libraries` or `assets`.
    pub id: String,
    pub kind: SharedKind,
    /// Version id or runtime component (empty for libraries and assets).
    pub name: String,
    /// The game version (of a profile: the one it builds on).
    pub minecraft: Option<String>,
    pub loader: Option<LoaderKind>,
    pub loader_version: Option<String>,
    /// Java runtimes: the Java version.
    pub java_major: Option<u32>,
    /// Bytes freed when only this item is deleted.
    pub bytes: u64,
    pub files: u64,
}

/// Bytes freed only when every listed item is deleted (files hard-linked into all of them).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SharedGroup {
    pub items: Vec<String>,
    pub bytes: u64,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SharedScan {
    pub items: Vec<SharedItem>,
    pub groups: Vec<SharedGroup>,
    /// The references of every used version were readable (libraries, assets and Java runtimes
    /// could be judged).
    pub complete: bool,
}

impl SharedScan {
    /// Bytes freed by deleting the items with these ids.
    pub fn bytes_for(&self, ids: &[String]) -> u64 {
        let chosen: HashSet<&str> = ids.iter().map(String::as_str).collect();
        let items: u64 = self
            .items
            .iter()
            .filter(|item| chosen.contains(item.id.as_str()))
            .map(|item| item.bytes)
            .sum();
        let groups: u64 = self
            .groups
            .iter()
            .filter(|group| group.items.iter().all(|id| chosen.contains(id.as_str())))
            .map(|group| group.bytes)
            .sum();
        items + groups
    }

    /// Bytes freed by deleting everything.
    pub fn total_bytes(&self) -> u64 {
        let ids: Vec<String> = self.items.iter().map(|item| item.id.clone()).collect();
        self.bytes_for(&ids)
    }
}

/// What [`delete`] did.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SharedDeleteReport {
    pub freed_bytes: u64,
    pub removed_files: u64,
    /// Files that could not be removed (in use) and stayed.
    pub failed_files: u64,
}

/// One path an item removes.
#[derive(Debug, Clone)]
struct Removal {
    path: PathBuf,
    dir: bool,
    files: u64,
    /// Bytes on disk (before hard-link adjustments).
    size: u64,
}

struct Plan {
    scan: SharedScan,
    removals: HashMap<String, Vec<Removal>>,
}

struct VersionDir {
    dir: PathBuf,
    /// The JSON file exists.
    json_present: bool,
    json: Option<VersionJson>,
}

/// What no instance uses, with the bytes each item frees. Blocking (walks the shared folders).
pub fn scan(dirs: &DataDirs, refs: &[GameRef], options: &ScanOptions) -> Result<SharedScan> {
    Ok(plan(dirs, refs, options)?.scan)
}

/// Deletes the items `ids` that are still unused (the folders are judged again with `refs`), then
/// prunes folders left empty. Items no longer unused are skipped. Blocking.
pub fn delete(
    dirs: &DataDirs,
    refs: &[GameRef],
    options: &ScanOptions,
    ids: &[String],
    cancel: &CancellationToken,
    progress: &dyn Fn(u64, u64),
) -> Result<SharedDeleteReport> {
    let plan = plan(dirs, refs, options)?;
    let chosen: Vec<&SharedItem> = plan
        .scan
        .items
        .iter()
        .filter(|item| ids.contains(&item.id))
        .collect();
    let total: u64 = chosen.iter().map(|item| item.files.max(1)).sum();
    let mut report = SharedDeleteReport::default();
    let mut done = 0u64;
    let mut complete: HashSet<&str> = HashSet::new();
    progress(0, total);
    for item in &chosen {
        let mut failed_bytes = 0u64;
        let mut failed = false;
        for removal in plan.removals.get(&item.id).into_iter().flatten() {
            if cancel.is_cancelled() {
                return Err(Error::Cancelled);
            }
            let removed = if removal.dir {
                remove_dir_all_robust(&removal.path)
            } else {
                remove_file_robust(&removal.path)
            };
            match removed {
                Ok(()) => report.removed_files += removal.files,
                Err(error) => {
                    tracing::debug!(path = %removal.path.display(), %error, "kept a shared file in use");
                    report.failed_files += removal.files.max(1);
                    failed_bytes += removal.size;
                    failed = true;
                }
            }
            done += removal.files.max(1);
            progress(done.min(total), total);
        }
        report.freed_bytes += item.bytes.saturating_sub(failed_bytes);
        if !failed {
            complete.insert(item.id.as_str());
        }
    }
    for group in &plan.scan.groups {
        if group.items.iter().all(|id| complete.contains(id.as_str())) {
            report.freed_bytes += group.bytes;
        }
    }
    for folder in [dirs.libraries(), dirs.asset_objects(), dirs.asset_virtual()] {
        prune_empty_dirs(&folder);
    }
    progress(total, total);
    tracing::info!(
        items = chosen.len(),
        freed = report.freed_bytes,
        removed = report.removed_files,
        failed = report.failed_files,
        "deleted unused shared files"
    );
    Ok(report)
}

// ---------------------------------------------------------------------------------------------
// Planning

fn plan(dirs: &DataDirs, refs: &[GameRef], options: &ScanOptions) -> Result<Plan> {
    let versions = read_versions(dirs)?;
    let used = used_versions(&versions, refs);

    // References of the used versions.
    let mut refs_known = true;
    let mut libraries: HashSet<String> = HashSet::new();
    let mut indexes: HashSet<String> = HashSet::new();
    let mut log_configs: HashSet<String> = HashSet::new();
    let mut components: HashSet<String> = HashSet::new();
    for id in &used {
        let Some(version) = versions.get(id) else {
            continue; // not installed: nothing on disk to keep
        };
        let Some(json) = &version.json else {
            tracing::info!(%id, present = version.json_present, "a used version has no readable JSON; keeping libraries and assets");
            refs_known = false;
            continue;
        };
        let mut generated = false;
        for library in &json.libraries {
            for path in library_paths(library) {
                libraries.insert(norm(&path));
            }
            generated |= library.artifact().is_some_and(|a| a.urls.is_empty());
        }
        match installer_outputs_blocking(dirs, id) {
            // Outputs outside `libraries/` live in the (kept) version folder.
            Some(files) => {
                for file in files {
                    if let Some(rest) = norm(&file).strip_prefix("libraries/") {
                        libraries.insert(rest.to_owned());
                    }
                }
            }
            None if generated => {
                tracing::info!(%id, "a used loader profile has no installer record; keeping libraries and assets");
                refs_known = false;
            }
            None => {}
        }
        if let Some(file) = json
            .logging
            .as_ref()
            .and_then(|l| l.client.as_ref())
            .and_then(|c| c.file.as_ref())
        {
            log_configs.insert(file.id.clone());
        }
        let chain = chain_of(&versions, id);
        let asset_id = chain.iter().find_map(|level| {
            let json = versions.get(level)?.json.as_ref()?;
            json.asset_index
                .as_ref()
                .map(|a| a.id.clone())
                .or_else(|| json.assets.clone())
                .filter(|a| !a.trim().is_empty())
        });
        indexes.insert(asset_id.unwrap_or_else(|| "legacy".to_owned()));
        let component = chain.iter().find_map(|level| {
            let json = versions.get(level)?.json.as_ref()?;
            json.java_version
                .as_ref()
                .and_then(|j| j.component.clone())
                .filter(|c| !c.trim().is_empty())
        });
        components.insert(component.unwrap_or_else(|| "jre-legacy".to_owned()));
    }

    let mut items: BTreeMap<String, SharedItem> = BTreeMap::new();
    let mut removals: HashMap<String, Vec<Removal>> = HashMap::new();
    // Changed recently (or dated in the future): may belong to a running download.
    let young = |modified: Option<SystemTime>| {
        modified.is_some_and(|m| match options.now.duration_since(m) {
            Ok(age) => age < options.min_age,
            Err(_) => true,
        })
    };

    // Version folders no instance refers to.
    for (id, version) in &versions {
        if used.contains(id) || young(newest_change(&version.dir)) {
            continue;
        }
        let (size, files) = tree_size(&version.dir);
        let item_id = format!("version:{id}");
        let parent = version
            .json
            .as_ref()
            .and_then(|j| j.inherits_from.clone())
            .filter(|p| !p.trim().is_empty());
        let guess = guess_loader(id, parent.as_deref());
        let profile = parent.is_some() || guess.is_some();
        items.insert(
            item_id.clone(),
            SharedItem {
                id: item_id.clone(),
                kind: if profile {
                    SharedKind::LoaderProfile
                } else {
                    SharedKind::GameVersion
                },
                name: id.clone(),
                minecraft: parent
                    .clone()
                    .or_else(|| guess.as_ref().and_then(|g| g.2.clone()))
                    .or_else(|| (!profile).then(|| id.clone())),
                loader: guess.as_ref().map(|g| g.0),
                loader_version: guess.and_then(|g| g.1),
                java_major: None,
                bytes: size,
                files,
            },
        );
        removals.entry(item_id).or_default().push(Removal {
            path: version.dir.clone(),
            dir: true,
            files,
            size,
        });
    }

    // Natives: kept for used versions, removed with their version, orphans with the libraries.
    let mut orphan_natives: Vec<Removal> = Vec::new();
    for entry in read_dir_entries(&dirs.natives()) {
        let path = entry.path();
        if !path.is_dir() || young(newest_change(&path)) {
            continue;
        }
        let key = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = natives_version(&key) else {
            continue; // not ours: keep
        };
        if used.contains(id) {
            continue;
        }
        let (size, files) = tree_size(&path);
        let removal = Removal {
            path,
            dir: true,
            files,
            size,
        };
        let item_id = format!("version:{id}");
        match items.get_mut(&item_id) {
            Some(item) => {
                item.bytes += size;
                item.files += files;
                removals.entry(item_id).or_default().push(removal);
            }
            None if !versions.contains_key(id) => orphan_natives.push(removal),
            None => {} // the version folder is young: keep its natives too
        }
    }

    if refs_known {
        // Java runtimes no used version asks for.
        for entry in read_dir_entries(&dirs.runtimes()) {
            let path = entry.path();
            if !path.is_dir() {
                continue;
            }
            let component = entry.file_name().to_string_lossy().into_owned();
            let protected = options.protected.iter().any(|p| p.starts_with(&path));
            if components.contains(&component) || protected || young(newest_change(&path)) {
                continue;
            }
            let (size, files) = tree_size(&path);
            let item_id = format!("runtime:{component}");
            items.insert(
                item_id.clone(),
                SharedItem {
                    id: item_id.clone(),
                    kind: SharedKind::JavaRuntime,
                    java_major: runtime_major(&path).or_else(|| component_major(&component)),
                    name: component,
                    minecraft: None,
                    loader: None,
                    loader_version: None,
                    bytes: size,
                    files,
                },
            );
            removals.entry(item_id).or_default().push(Removal {
                path,
                dir: true,
                files,
                size,
            });
        }

        // Libraries no used version lists (plus natives of versions that are gone).
        let mut library_removals = orphan_natives;
        let root = dirs.libraries();
        for entry in WalkDir::new(&root)
            .follow_links(false)
            .into_iter()
            .filter_map(std::result::Result::ok)
            .filter(|e| e.file_type().is_file())
        {
            let Ok(rel) = entry.path().strip_prefix(&root) else {
                continue;
            };
            if libraries.contains(&norm_path(rel)) {
                continue;
            }
            let Ok(meta) = entry.metadata() else { continue };
            if young(meta.modified().ok()) {
                continue;
            }
            library_removals.push(Removal {
                path: entry.path().to_path_buf(),
                dir: false,
                files: 1,
                size: meta.len(),
            });
        }
        add_aggregate(
            &mut items,
            &mut removals,
            "libraries",
            SharedKind::Libraries,
            library_removals,
        );

        // Assets: objects, indexes, old virtual layouts and log configurations.
        if let Some(asset_removals) = asset_removals(dirs, &indexes, &log_configs, &young) {
            add_aggregate(
                &mut items,
                &mut removals,
                "assets",
                SharedKind::Assets,
                asset_removals,
            );
        }
    }
    let groups = adjust_hard_links(&versions, &used, &mut items);
    // Versions newest game version first, Java newest first.
    let mut list: Vec<SharedItem> = items.into_values().collect();
    list.sort_by(|a, b| {
        a.kind
            .order()
            .cmp(&b.kind.order())
            .then_with(|| b.java_major.cmp(&a.java_major))
            .then_with(|| {
                natural_cmp(
                    b.minecraft.as_deref().unwrap_or_default(),
                    a.minecraft.as_deref().unwrap_or_default(),
                )
            })
            .then_with(|| natural_cmp(&b.name, &a.name))
    });
    Ok(Plan {
        scan: SharedScan {
            items: list,
            groups,
            complete: refs_known,
        },
        removals,
    })
}

fn add_aggregate(
    items: &mut BTreeMap<String, SharedItem>,
    removals: &mut HashMap<String, Vec<Removal>>,
    id: &str,
    kind: SharedKind,
    list: Vec<Removal>,
) {
    if list.is_empty() {
        return;
    }
    let bytes = list.iter().map(|r| r.size).sum();
    let files = list.iter().map(|r| r.files).sum();
    items.insert(
        id.to_owned(),
        SharedItem {
            id: id.to_owned(),
            kind,
            name: String::new(),
            minecraft: None,
            loader: None,
            loader_version: None,
            java_major: None,
            bytes,
            files,
        },
    );
    removals.insert(id.to_owned(), list);
}

/// Every `versions/<id>/` folder with its JSON (when readable).
fn read_versions(dirs: &DataDirs) -> Result<BTreeMap<String, VersionDir>> {
    let mut versions = BTreeMap::new();
    for entry in read_dir_entries(&dirs.versions()) {
        let dir = entry.path();
        if !dir.is_dir() {
            continue;
        }
        let Some(id) = entry.file_name().to_str().map(str::to_owned) else {
            continue; // a name we never write: not ours to judge
        };
        let file = dir.join(format!("{id}.json"));
        let (json_present, json) = match std::fs::read(&file) {
            Ok(bytes) => (
                true,
                VersionJson::from_slice(&bytes, &file.display().to_string())
                    .ok()
                    .map(|mut json| {
                        if json.id.is_empty() {
                            json.id.clone_from(&id);
                        }
                        json
                    }),
            ),
            Err(_) => (false, None),
        };
        versions.insert(
            id,
            VersionDir {
                dir,
                json_present,
                json,
            },
        );
    }
    Ok(versions)
}

fn parent_of(versions: &BTreeMap<String, VersionDir>, id: &str) -> Option<String> {
    versions
        .get(id)?
        .json
        .as_ref()?
        .inherits_from
        .clone()
        .filter(|p| !p.trim().is_empty())
}

/// `id` and the versions it inherits from, leaf first.
fn chain_of(versions: &BTreeMap<String, VersionDir>, id: &str) -> Vec<String> {
    let mut chain = vec![id.to_owned()];
    while chain.len() < MAX_CHAIN {
        let Some(parent) = chain.last().and_then(|last| parent_of(versions, last)) else {
            break;
        };
        if chain.contains(&parent) {
            break;
        }
        chain.push(parent);
    }
    chain
}

/// Version ids instances use: their game versions, their loader profiles (all profiles built on
/// a game version when the loader or its version is unknown), and everything those inherit from.
fn used_versions(versions: &BTreeMap<String, VersionDir>, refs: &[GameRef]) -> HashSet<String> {
    let mut used: HashSet<String> = HashSet::new();
    let profiles_of = |minecraft: &str| -> Vec<String> {
        versions
            .keys()
            .filter(|id| id.as_str() != minecraft)
            .filter(|id| {
                chain_of(versions, id)
                    .iter()
                    .any(|level| level == minecraft)
            })
            .cloned()
            .collect()
    };
    for game in refs {
        let minecraft = game.minecraft.trim();
        if minecraft.is_empty() {
            continue;
        }
        used.insert(minecraft.to_owned());
        match (game.loader, game.loader_version.as_deref()) {
            (Some(LoaderKind::Vanilla), _) => {}
            (Some(kind), Some(version)) => match loader_profile_id(kind, minecraft, version) {
                Ok(id) => {
                    used.insert(id);
                }
                Err(_) => used.extend(profiles_of(minecraft)),
            },
            _ => used.extend(profiles_of(minecraft)),
        }
    }
    // Everything a used version inherits from, or takes its game jar from.
    let mut stack: Vec<String> = used.iter().cloned().collect();
    while let Some(id) = stack.pop() {
        let json = versions.get(&id).and_then(|v| v.json.as_ref());
        let next = [
            parent_of(versions, &id),
            json.and_then(|j| j.jar.clone())
                .filter(|jar| !jar.trim().is_empty()),
        ];
        for other in next.into_iter().flatten() {
            if used.insert(other.clone()) {
                stack.push(other);
            }
        }
    }
    used
}

/// Every file a library entry may refer to, relative to `libraries/` (all platforms and
/// architectures: never deleting what another rule could need).
fn library_paths(library: &Library) -> Vec<String> {
    let mut paths = Vec::new();
    if let Some(artifact) = library.artifact() {
        paths.push(artifact.path);
    }
    let coord = library.coord().ok();
    if let Some(coord) = &coord {
        paths.push(coord.path());
    }
    if let Some(classifiers) = library
        .downloads
        .as_ref()
        .and_then(|d| d.classifiers.as_ref())
    {
        for (key, artifact) in classifiers {
            match artifact.path.as_deref().filter(|p| !p.trim().is_empty()) {
                Some(path) => paths.push(path.to_owned()),
                None => {
                    if let Some(coord) = &coord {
                        paths.push(coord.with_classifier(key).path());
                    }
                }
            }
        }
    }
    if let (Some(natives), Some(coord)) = (&library.natives, &coord) {
        for classifier in natives.values() {
            for bits in ["32", "64"] {
                paths.push(
                    coord
                        .with_classifier(&classifier.replace("${arch}", bits))
                        .path(),
                );
            }
        }
    }
    paths
}

/// What the asset folders hold beyond the used indexes. `None` when a used index cannot be read
/// (then nothing there is judged).
fn asset_removals(
    dirs: &DataDirs,
    indexes: &HashSet<String>,
    log_configs: &HashSet<String>,
    young: &dyn Fn(Option<SystemTime>) -> bool,
) -> Option<Vec<Removal>> {
    let mut used_objects: HashSet<String> = HashSet::new();
    for id in indexes {
        let Ok(path) = dirs.asset_index_file(id) else {
            return None;
        };
        match std::fs::read(&path) {
            Ok(bytes) => {
                let index: AssetIndexFile = serde_json::from_slice(&bytes).ok()?;
                used_objects.extend(index.objects.into_values().map(|o| o.hash.to_lowercase()));
            }
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {} // not downloaded yet
            Err(_) => return None,
        }
    }

    let mut out = Vec::new();
    let mut file_removal = |path: PathBuf, meta: &std::fs::Metadata| {
        if !young(meta.modified().ok()) {
            out.push(Removal {
                path,
                dir: false,
                files: 1,
                size: meta.len(),
            });
        }
    };
    // Objects (`objects/<xx>/<hash>`).
    for entry in WalkDir::new(dirs.asset_objects())
        .min_depth(2)
        .max_depth(2)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
    {
        let hash = entry.file_name().to_string_lossy().to_lowercase();
        if used_objects.contains(&hash) {
            continue;
        }
        if let Ok(meta) = entry.metadata() {
            file_removal(entry.path().to_path_buf(), &meta);
        }
    }
    // Index files.
    for entry in read_dir_entries(&dirs.asset_indexes()) {
        let path = entry.path();
        let name = entry.file_name().to_string_lossy().into_owned();
        let Some(id) = name.strip_suffix(".json") else {
            continue;
        };
        if indexes.contains(id) {
            continue;
        }
        if let Some(meta) = entry.metadata().ok().filter(|m| m.is_file()) {
            file_removal(path, &meta);
        }
    }
    // Log configurations.
    for entry in read_dir_entries(&dirs.log_configs()) {
        let name = entry.file_name().to_string_lossy().into_owned();
        if log_configs.contains(&name) {
            continue;
        }
        if let Some(meta) = entry.metadata().ok().filter(|m| m.is_file()) {
            file_removal(entry.path(), &meta);
        }
    }
    // Old "virtual" layouts: hard links to objects, so only files that are not an object count.
    for entry in read_dir_entries(&dirs.asset_virtual()) {
        let path = entry.path();
        let id = entry.file_name().to_string_lossy().into_owned();
        if !path.is_dir() || indexes.contains(&id) || young(newest_change(&path)) {
            continue;
        }
        let (size, files) = tree_size(&path);
        let linked = linked_object_bytes(dirs, &id, &path);
        out.push(Removal {
            path,
            dir: true,
            files,
            size: size.saturating_sub(linked),
        });
    }
    Some(out)
}

#[derive(Deserialize)]
struct AssetIndexFile {
    #[serde(default)]
    objects: BTreeMap<String, AssetIndexObject>,
}

#[derive(Deserialize)]
struct AssetIndexObject {
    hash: String,
}

/// Bytes of a virtual layout's files that are the same file as their object.
fn linked_object_bytes(dirs: &DataDirs, id: &str, virtual_dir: &Path) -> u64 {
    let Ok(index_path) = dirs.asset_index_file(id) else {
        return 0;
    };
    let Some(index) = std::fs::read(index_path)
        .ok()
        .and_then(|bytes| serde_json::from_slice::<AssetIndexFile>(&bytes).ok())
    else {
        return 0;
    };
    index
        .objects
        .iter()
        .filter_map(|(name, object)| {
            let file = crate::paths::safe_join(virtual_dir, name).ok()?;
            let object = dirs.asset_object(&object.hash).ok()?;
            let size = std::fs::metadata(&file).ok()?.len();
            same_file::is_same_file(&file, &object)
                .unwrap_or(false)
                .then_some(size)
        })
        .sum()
}

/// Game jars hard-linked into several version folders: left out of every holder's bytes; a group
/// when every holder is unused, nothing when a used version keeps a link.
fn adjust_hard_links(
    versions: &BTreeMap<String, VersionDir>,
    used: &HashSet<String>,
    items: &mut BTreeMap<String, SharedItem>,
) -> Vec<SharedGroup> {
    let mut by_size: HashMap<u64, Vec<(String, PathBuf)>> = HashMap::new();
    for (id, version) in versions {
        let jar = version.dir.join(format!("{id}.jar"));
        if let Some(meta) = std::fs::metadata(&jar)
            .ok()
            .filter(|m| m.is_file() && m.len() > 0)
        {
            by_size
                .entry(meta.len())
                .or_default()
                .push((id.clone(), jar));
        }
    }
    let mut groups: Vec<SharedGroup> = Vec::new();
    for (size, jars) in by_size {
        if jars.len() < 2 {
            continue;
        }
        let mut seen = vec![false; jars.len()];
        for first in 0..jars.len() {
            if seen[first] {
                continue;
            }
            let mut class = vec![first];
            for other in (first + 1)..jars.len() {
                if !seen[other]
                    && same_file::is_same_file(&jars[first].1, &jars[other].1).unwrap_or(false)
                {
                    class.push(other);
                    seen[other] = true;
                }
            }
            if class.len() < 2 {
                continue;
            }
            let holders: Vec<&String> = class.iter().map(|&i| &jars[i].0).collect();
            let unused: Vec<String> = holders
                .iter()
                .map(|id| format!("version:{id}"))
                .filter(|item| items.contains_key(item))
                .collect();
            for item in &unused {
                if let Some(item) = items.get_mut(item) {
                    item.bytes = item.bytes.saturating_sub(size);
                }
            }
            let kept =
                holders.iter().any(|id| used.contains(id.as_str())) || unused.len() < holders.len();
            if !kept && !unused.is_empty() {
                let mut members = unused;
                members.sort();
                match groups.iter_mut().find(|g| g.items == members) {
                    Some(group) => group.bytes += size,
                    None => groups.push(SharedGroup {
                        items: members,
                        bytes: size,
                    }),
                }
            }
        }
    }
    groups.sort_by(|a, b| a.items.cmp(&b.items));
    groups
}

/// `(loader, loader version, game version)` from a loader profile id, when it has one of the
/// shapes the launcher writes.
fn guess_loader(
    id: &str,
    parent: Option<&str>,
) -> Option<(LoaderKind, Option<String>, Option<String>)> {
    let split_tail = |rest: &str| -> (Option<String>, Option<String>) {
        match parent {
            Some(mc) => (
                rest.strip_suffix(&format!("-{mc}")).map(str::to_owned),
                Some(mc.to_owned()),
            ),
            None => match rest.rsplit_once('-') {
                Some((loader, mc)) => (Some(loader.to_owned()), Some(mc.to_owned())),
                None => (Some(rest.to_owned()), None),
            },
        }
    };
    if let Some(rest) = id.strip_prefix("fabric-loader-") {
        let (loader, mc) = split_tail(rest);
        return Some((LoaderKind::Fabric, loader, mc));
    }
    if let Some(rest) = id.strip_prefix("quilt-loader-") {
        let (loader, mc) = split_tail(rest);
        return Some((LoaderKind::Quilt, loader, mc));
    }
    if let Some(rest) = id.strip_prefix("neoforge-") {
        return Some((
            LoaderKind::NeoForge,
            Some(rest.to_owned()),
            parent.map(str::to_owned),
        ));
    }
    if let Some((mc, loader)) = id.split_once("-neoforge-") {
        return Some((
            LoaderKind::NeoForge,
            Some(loader.to_owned()),
            Some(mc.to_owned()),
        ));
    }
    if let Some((mc, loader)) = id.split_once("-forge-") {
        return Some((
            LoaderKind::Forge,
            Some(loader.to_owned()),
            Some(mc.to_owned()),
        ));
    }
    None
}

/// The version id of a natives folder (`<id>-<10 hex>`), `None` for other names.
fn natives_version(key: &str) -> Option<&str> {
    let (id, hash) = key.rsplit_once('-')?;
    let ours = hash.len() == 10 && hash.bytes().all(|b| b.is_ascii_hexdigit()) && !id.is_empty();
    ours.then_some(id)
}

/// Java major version of an installed runtime (from its marker in any platform folder).
fn runtime_major(component_dir: &Path) -> Option<u32> {
    #[derive(Deserialize)]
    struct Marker {
        version: String,
    }
    read_dir_entries(component_dir)
        .into_iter()
        .find_map(|entry| {
            let bytes = std::fs::read(entry.path().join(".projects-runtime.json")).ok()?;
            let marker: Marker = serde_json::from_slice(&bytes).ok()?;
            crate::java::parse_major(&marker.version)
        })
}

// ---------------------------------------------------------------------------------------------
// File helpers

/// A relative library path, `/`-separated and case-folded where the file system ignores case.
fn norm(path: &str) -> String {
    let path = path.replace('\\', "/");
    let path = path.trim_start_matches('/');
    if cfg!(any(windows, target_os = "macos")) {
        path.to_lowercase()
    } else {
        path.to_owned()
    }
}

fn norm_path(path: &Path) -> String {
    let joined = path
        .components()
        .map(|c| c.as_os_str().to_string_lossy())
        .collect::<Vec<_>>()
        .join("/");
    norm(&joined)
}

fn read_dir_entries(dir: &Path) -> Vec<std::fs::DirEntry> {
    std::fs::read_dir(dir)
        .map(|entries| entries.filter_map(std::result::Result::ok).collect())
        .unwrap_or_default()
}

/// Bytes and number of files under `path` (links not followed).
fn tree_size(path: &Path) -> (u64, u64) {
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_file())
        .filter_map(|e| e.metadata().ok())
        .fold((0, 0), |(bytes, files), meta| {
            (bytes + meta.len(), files + 1)
        })
}

/// Latest change to a folder or anything inside it.
fn newest_change(path: &Path) -> Option<SystemTime> {
    // Asked per path, not from the directory listing: on Windows a listing can carry stale times
    // for a file hard-linked elsewhere (a loader profile's copy of the game jar).
    WalkDir::new(path)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter_map(|e| std::fs::symlink_metadata(e.path()).ok()?.modified().ok())
        .max()
}

fn clear_readonly(path: &Path) {
    if let Ok(meta) = std::fs::symlink_metadata(path) {
        let mut permissions = meta.permissions();
        if permissions.readonly() {
            #[allow(clippy::permissions_set_readonly_false)]
            permissions.set_readonly(false);
            let _ = std::fs::set_permissions(path, permissions);
        }
    }
}

fn remove_file_robust(path: &Path) -> std::io::Result<()> {
    match std::fs::remove_file(path) {
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::PermissionDenied && cfg!(windows) => {
            clear_readonly(path);
            std::fs::remove_file(path)
        }
        other => other,
    }
}

fn remove_dir_all_robust(dir: &Path) -> std::io::Result<()> {
    match std::fs::remove_dir_all(dir) {
        Ok(()) => Ok(()),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
        Err(first) => {
            if cfg!(windows) {
                for entry in WalkDir::new(dir)
                    .follow_links(false)
                    .into_iter()
                    .filter_map(std::result::Result::ok)
                {
                    clear_readonly(entry.path());
                }
            }
            std::fs::remove_dir_all(dir).map_err(|_| first)
        }
    }
}

/// Removes folders under `root` that became empty (deepest first); `root` itself stays.
fn prune_empty_dirs(root: &Path) {
    let dirs: Vec<PathBuf> = WalkDir::new(root)
        .min_depth(1)
        .contents_first(true)
        .follow_links(false)
        .into_iter()
        .filter_map(std::result::Result::ok)
        .filter(|e| e.file_type().is_dir())
        .map(|e| e.path().to_path_buf())
        .collect();
    for dir in dirs {
        // Fails harmlessly when the folder is not empty.
        let _ = std::fs::remove_dir(&dir);
    }
}

/// Orders version-like names by their numbers (`1.21.10` after `1.21.9`).
fn natural_cmp(a: &str, b: &str) -> std::cmp::Ordering {
    let parts = |s: &str| -> Vec<(u64, String)> {
        s.split(|c: char| !c.is_ascii_alphanumeric())
            .map(|part| (part.parse::<u64>().unwrap_or(0), part.to_owned()))
            .collect()
    };
    parts(a).cmp(&parts(b))
}

#[cfg(test)]
mod tests;
