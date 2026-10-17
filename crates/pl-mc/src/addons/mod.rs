//! Add-ons of a game directory (mods, resource packs, shader packs) and where they came from.
//!
//! [`Addons`] keeps a small index next to the instance ([`AddonIndex`]) with each file's size,
//! modification time and SHA-1 plus the Modrinth project/version it belongs to. The UI can then
//! show titles, icons, "installed" and "update available" without hashing every jar or asking
//! the network each time. [`Addons::install`] resolves required dependencies, downloads through
//! [`pl_net::Downloader`] (every file is verified against the hash Modrinth publishes and moved
//! into place atomically) and replaces older files of the same project.

mod index;
mod install;
#[cfg(test)]
mod tests;

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};

use futures::StreamExt;
use pl_net::{HashAlgo, Http};
use serde::{Deserialize, Serialize};

pub use index::{ADDON_INDEX_FILE, AddonIndex, AddonRecord, AddonSource, index_key};
pub use install::{
    AddonInstallOptions, AddonInstallReport, AddonInstalled, AddonProgress, AddonProgressFn,
};

use crate::error::Result;
use crate::gamefiles::ImportKind;
use crate::gamefiles::fsops::{
    RemovalMethod, blocking, modified_ms, read_dir_or_empty, remove_path,
};
use crate::modpack::MrFile;
use crate::modpack::safe_path::validate_file_name;
use crate::modrinth::{
    ModrinthClient, ModrinthHashAlgorithm, ModrinthResolveTarget, ModrinthVersion,
    ModrinthVersionType,
};
use crate::mods::is_mod_file_name;
use index::DISABLED_SUFFIX;

/// Files Modrinth did not know are asked about again after this long.
const UNKNOWN_RECHECK_MS: i64 = 3 * 24 * 3600 * 1000;
const HASH_CONCURRENCY: usize = 8;
/// Project icons larger than this are not cached.
const MAX_ICON_BYTES: usize = 1024 * 1024;
const ICON_EXTENSIONS: [&str; 6] = ["png", "webp", "gif", "jpg", "jpeg", "svg"];

pub(crate) fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// The add-on kind stored in a game directory folder (`mods`, `resourcepacks`, `shaderpacks`).
pub fn kind_for_folder(folder: &str) -> Option<ImportKind> {
    [
        ImportKind::Mod,
        ImportKind::ResourcePack,
        ImportKind::ShaderPack,
    ]
    .into_iter()
    .find(|kind| kind.folder() == folder)
}

/// A newer compatible version of an installed add-on.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddonUpdate {
    pub version_id: String,
    pub version_number: String,
    pub version_type: ModrinthVersionType,
    pub date_published: String,
}

/// An add-on file and what is known about it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddonEntry {
    pub kind: ImportKind,
    /// File name in its folder (with `.disabled` when disabled).
    pub file_name: String,
    pub enabled: bool,
    pub size: u64,
    /// Lowercase hex SHA-1.
    pub sha1: String,
    pub source: Option<AddonSource>,
    /// Set by [`Addons::check_updates`] when a newer compatible version exists.
    pub update: Option<AddonUpdate>,
}

/// Result of [`Addons::identify`] and [`Addons::check_updates`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddonScan {
    pub entries: Vec<AddonEntry>,
    /// Modrinth could not be reached: entries only carry what was known locally.
    pub offline: bool,
}

/// A file found in an add-on folder.
#[derive(Debug, Clone)]
pub(crate) struct SeenFile {
    pub kind: ImportKind,
    pub file_name: String,
    pub key: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub size: u64,
    pub sha1: String,
}

/// Add-ons of one game directory.
#[derive(Debug, Clone)]
pub struct Addons {
    game_dir: PathBuf,
    index_path: PathBuf,
    icon_dir: Option<PathBuf>,
    use_trash: bool,
}

impl Addons {
    /// `index_path` is where [`AddonIndex`] lives (outside the game directory is best).
    pub fn new(game_dir: impl Into<PathBuf>, index_path: impl Into<PathBuf>) -> Self {
        Self {
            game_dir: game_dir.into(),
            index_path: index_path.into(),
            icon_dir: None,
            use_trash: true,
        }
    }

    /// Delete removed files for good instead of moving them to the OS trash (tests).
    pub fn without_trash(mut self) -> Self {
        self.use_trash = false;
        self
    }

    /// Remove a file: OS trash when possible (and enabled), deleted otherwise.
    pub(crate) async fn delete_file(&self, path: &Path) -> Result<RemovalMethod> {
        if self.use_trash {
            return remove_path(path, true).await;
        }
        tokio::fs::remove_file(path)
            .await
            .map_err(|e| crate::error::Error::io(path, e))?;
        Ok(RemovalMethod::Permanent)
    }

    /// Cache project icons in `dir` after installs (see [`cached_icon`]).
    pub fn with_icon_dir(mut self, dir: impl Into<PathBuf>) -> Self {
        self.icon_dir = Some(dir.into());
        self
    }

    pub fn game_dir(&self) -> &Path {
        &self.game_dir
    }

    pub fn index_path(&self) -> &Path {
        &self.index_path
    }

    pub async fn load_index(&self) -> AddonIndex {
        AddonIndex::load(&self.index_path).await
    }

    pub(crate) async fn save_index(&self, index: &AddonIndex) -> Result<()> {
        index.save(&self.index_path).await
    }

    /// Files of `kind` with their SHA-1 (reused from the index while size and modification
    /// time are unchanged). Records of files that are gone are dropped from `index`.
    pub(crate) async fn scan_into(
        &self,
        index: &mut AddonIndex,
        kind: ImportKind,
    ) -> Result<Vec<SeenFile>> {
        let folder = kind.folder();
        let dir = self.game_dir.join(folder);
        let listed: Vec<(String, PathBuf, u64, Option<i64>)> = blocking(move || {
            let mut out = Vec::new();
            for entry in read_dir_or_empty(&dir)? {
                let Ok(file_type) = entry.file_type() else {
                    continue;
                };
                let Some(name) = entry.file_name().to_str().map(str::to_owned) else {
                    continue;
                };
                if !file_type.is_file() || name.starts_with('.') || !accepts(kind, &name) {
                    continue;
                }
                let Ok(meta) = entry.metadata() else {
                    continue;
                };
                out.push((name, entry.path(), meta.len(), modified_ms(&meta)));
            }
            Ok(out)
        })
        .await?;

        let mut files = Vec::with_capacity(listed.len());
        let mut to_hash = Vec::new();
        for (name, path, size, modified) in listed {
            let key = index_key(folder, &name);
            let known = index
                .files
                .get(&key)
                .filter(|record| record.matches(size, modified))
                .map(|record| record.sha1.clone());
            match known {
                Some(sha1) => files.push(SeenFile {
                    kind,
                    enabled: !name.ends_with(DISABLED_SUFFIX),
                    file_name: name,
                    key,
                    path,
                    size,
                    sha1,
                }),
                None => to_hash.push((name, key, path, size, modified)),
            }
        }

        // Futures are built eagerly so the returned future stays `Send` for Tauri commands.
        let jobs: Vec<_> = to_hash
            .into_iter()
            .map(|(name, key, path, size, modified)| async move {
                let hash = pl_net::hash_file(&path, HashAlgo::Sha1).await;
                (name, key, path, size, modified, hash)
            })
            .collect();
        let hashed: Vec<_> = futures::stream::iter(jobs)
            .buffer_unordered(HASH_CONCURRENCY)
            .collect()
            .await;
        for (name, key, path, size, modified, hash) in hashed {
            let sha1 = match hash {
                Ok(sha1) => sha1,
                Err(error) => {
                    tracing::warn!(path = %path.display(), %error, "could not hash add-on");
                    continue;
                }
            };
            index.files.insert(
                key.clone(),
                AddonRecord {
                    size,
                    modified_ms: modified,
                    sha1: sha1.clone(),
                    source: None,
                    looked_up_ms: None,
                },
            );
            files.push(SeenFile {
                kind,
                enabled: !name.ends_with(DISABLED_SUFFIX),
                file_name: name,
                key,
                path,
                size,
                sha1,
            });
        }

        let present: HashSet<&str> = files.iter().map(|f| f.key.as_str()).collect();
        let prefix = format!("{folder}/");
        index
            .files
            .retain(|key, _| !key.starts_with(&prefix) || present.contains(key.as_str()));
        files.sort_by_cached_key(|f| f.file_name.to_lowercase());
        Ok(files)
    }

    /// Ask Modrinth about files of `folder` it was not asked about yet (by SHA-1), then fill in
    /// missing project titles and icons. Returns `true` when Modrinth could not be reached.
    pub(crate) async fn lookup_sources(
        &self,
        client: &ModrinthClient,
        index: &mut AddonIndex,
        folder: &str,
    ) -> bool {
        let now = now_ms();
        let prefix = format!("{folder}/");
        let needs: Vec<(String, String)> = index
            .files
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .filter(|(_, record)| match &record.source {
                None => record
                    .looked_up_ms
                    .is_none_or(|at| now - at > UNKNOWN_RECHECK_MS),
                Some(source) => source.version_number.is_none(),
            })
            .map(|(key, record)| (key.clone(), record.sha1.clone()))
            .collect();

        if !needs.is_empty() {
            let hashes: Vec<String> = needs.iter().map(|(_, sha1)| sha1.clone()).collect();
            match client
                .version_files(&hashes, ModrinthHashAlgorithm::Sha1)
                .await
            {
                Ok(found) => {
                    for (key, sha1) in needs {
                        let Some(record) = index.files.get_mut(&key) else {
                            continue;
                        };
                        record.looked_up_ms = Some(now);
                        let previous = record.source.take();
                        record.source = found.get(&sha1).map(|version| {
                            let previous = previous.filter(|s| s.project_id == version.project_id);
                            AddonSource {
                                project_id: version.project_id.clone(),
                                version_id: version.id.clone(),
                                version_number: Some(version.version_number.clone()),
                                title: previous.as_ref().and_then(|s| s.title.clone()),
                                icon_url: previous.and_then(|s| s.icon_url),
                            }
                        });
                    }
                }
                Err(error) => {
                    tracing::debug!(%error, "Modrinth hash lookup failed");
                    return true;
                }
            }
        }

        let mut missing: Vec<String> = index
            .files
            .iter()
            .filter(|(key, _)| key.starts_with(&prefix))
            .filter_map(|(_, record)| record.source.as_ref())
            .filter(|source| source.title.is_none())
            .map(|source| source.project_id.clone())
            .collect();
        missing.sort();
        missing.dedup();
        if missing.is_empty() {
            return false;
        }
        match client.projects(&missing).await {
            Ok(projects) => {
                let by_id: HashMap<&str, _> = projects.iter().map(|p| (p.id.as_str(), p)).collect();
                for record in index.files.values_mut() {
                    if let Some(source) = record.source.as_mut()
                        && let Some(project) = by_id.get(source.project_id.as_str())
                    {
                        source.title = Some(project.title.clone());
                        source.icon_url = project.icon_url.clone();
                    }
                }
                false
            }
            Err(error) => {
                tracing::debug!(%error, "Modrinth project lookup failed");
                true
            }
        }
    }

    /// The files of `kind` and what the index knows about them. No network.
    pub async fn scan(&self, kind: ImportKind) -> Result<Vec<AddonEntry>> {
        let mut index = self.load_index().await;
        let files = self.scan_into(&mut index, kind).await?;
        self.save_index(&index).await?;
        Ok(entries(&files, &index))
    }

    /// [`Addons::scan`] plus a Modrinth lookup for files not identified yet. Offline, the
    /// local knowledge is returned with `offline` set.
    pub async fn identify(&self, client: &ModrinthClient, kind: ImportKind) -> Result<AddonScan> {
        let mut index = self.load_index().await;
        let files = self.scan_into(&mut index, kind).await?;
        let offline = self.lookup_sources(client, &mut index, kind.folder()).await;
        self.save_index(&index).await?;
        let entries = entries(&files, &index);
        if !offline {
            self.cache_icons(client.http(), &entries).await;
        }
        Ok(AddonScan { entries, offline })
    }

    /// [`Addons::identify`] plus the newest compatible version of every Modrinth file. A file on
    /// a release only gets release updates, one on a beta release or beta updates.
    pub async fn check_updates(
        &self,
        client: &ModrinthClient,
        kind: ImportKind,
        target: &ModrinthResolveTarget,
    ) -> Result<AddonScan> {
        let mut scan = self.identify(client, kind).await?;
        if scan.offline {
            return Ok(scan);
        }
        let mut current_ids: Vec<String> = scan
            .entries
            .iter()
            .filter_map(|e| e.source.as_ref().map(|s| s.version_id.clone()))
            .collect();
        current_ids.sort();
        current_ids.dedup();
        if current_ids.is_empty() {
            return Ok(scan);
        }
        let current: HashMap<String, ModrinthVersion> = match client.versions(&current_ids).await {
            Ok(list) => list.into_iter().map(|v| (v.id.clone(), v)).collect(),
            Err(error) => {
                tracing::debug!(%error, "could not fetch installed versions");
                scan.offline = true;
                return Ok(scan);
            }
        };

        // Group the hashes by the version types they may move to.
        let mut groups: HashMap<Vec<ModrinthVersionType>, Vec<String>> = HashMap::new();
        for entry in &scan.entries {
            let Some(source) = &entry.source else {
                continue;
            };
            let allowed = match current.get(&source.version_id).map(|v| v.version_type) {
                Some(ModrinthVersionType::Release) => vec![ModrinthVersionType::Release],
                Some(ModrinthVersionType::Beta) => {
                    vec![ModrinthVersionType::Release, ModrinthVersionType::Beta]
                }
                _ => Vec::new(),
            };
            groups.entry(allowed).or_default().push(entry.sha1.clone());
        }
        let mut latest: HashMap<String, ModrinthVersion> = HashMap::new();
        for (types, hashes) in groups {
            match client
                .update_check_typed(
                    &hashes,
                    ModrinthHashAlgorithm::Sha1,
                    &target.loaders,
                    &target.game_versions,
                    &types,
                )
                .await
            {
                Ok(found) => latest.extend(found),
                Err(error) => {
                    tracing::debug!(%error, "Modrinth update check failed");
                    scan.offline = true;
                    return Ok(scan);
                }
            }
        }

        for entry in &mut scan.entries {
            let (Some(source), Some(newest)) = (&entry.source, latest.get(&entry.sha1)) else {
                continue;
            };
            if newest.id == source.version_id || newest.project_id != source.project_id {
                continue;
            }
            let newer = current.get(&source.version_id).is_none_or(|installed| {
                is_later(&newest.date_published, &installed.date_published)
            });
            if newer {
                entry.update = Some(AddonUpdate {
                    version_id: newest.id.clone(),
                    version_number: newest.version_number.clone(),
                    version_type: newest.version_type,
                    date_published: newest.date_published.clone(),
                });
            }
        }
        Ok(scan)
    }

    /// Remove an add-on file (OS trash when possible) and forget it.
    pub async fn remove(&self, kind: ImportKind, file_name: &str) -> Result<RemovalMethod> {
        validate_file_name(file_name)?;
        let path = self.game_dir.join(kind.folder()).join(file_name);
        let method = self.delete_file(&path).await?;
        let mut index = self.load_index().await;
        if index
            .files
            .remove(&index_key(kind.folder(), file_name))
            .is_some()
        {
            self.save_index(&index).await?;
        }
        Ok(method)
    }

    /// Remember files an `.mrpack` import downloaded (their hashes are known), so they need no
    /// hashing later. Modrinth is asked about them on the next [`Addons::identify`].
    pub async fn record_pack_files(&self, files: &[MrFile]) -> Result<()> {
        let mut index = self.load_index().await;
        for file in files {
            let Some((folder, name)) = file.path.split_once('/') else {
                continue;
            };
            if name.contains('/') || kind_for_folder(folder).is_none() {
                continue;
            }
            let path = self.game_dir.join(folder).join(name);
            let Ok(meta) = tokio::fs::metadata(&path).await else {
                continue;
            };
            index.files.insert(
                index_key(folder, name),
                AddonRecord {
                    size: meta.len(),
                    modified_ms: modified_ms(&meta),
                    sha1: file.hashes.sha1.to_ascii_lowercase(),
                    source: None,
                    looked_up_ms: None,
                },
            );
        }
        self.save_index(&index).await
    }

    /// Remember (or forget) the Modrinth modpack the instance was created from.
    pub async fn set_modpack(&self, source: Option<AddonSource>) -> Result<()> {
        let mut index = self.load_index().await;
        index.modpack = source;
        self.save_index(&index).await
    }

    /// Download missing project icons into the icon cache (best effort).
    pub(crate) async fn cache_icons(&self, http: &Http, entries: &[AddonEntry]) {
        let Some(dir) = &self.icon_dir else {
            return;
        };
        let mut wanted: Vec<(String, String)> = entries
            .iter()
            .filter_map(|e| e.source.as_ref())
            .filter_map(|s| Some((s.project_id.clone(), s.icon_url.clone()?)))
            .filter(|(id, url)| is_plain_id(id) && url.starts_with("https://"))
            .filter(|(id, _)| cached_icon(dir, id).is_none())
            .collect();
        wanted.sort();
        wanted.dedup();
        let jobs: Vec<_> = wanted
            .into_iter()
            .map(|(id, url)| {
                let dir = dir.clone();
                let http = http.clone();
                async move {
                    let bytes = match http.get_bytes(&url).await {
                        Ok(bytes) if bytes.len() <= MAX_ICON_BYTES => bytes,
                        Ok(_) => return,
                        Err(error) => {
                            tracing::debug!(%url, %error, "could not fetch project icon");
                            return;
                        }
                    };
                    let path = dir.join(format!("{id}.{}", icon_extension(&url)));
                    if let Err(error) = crate::paths::write_atomic(&path, &bytes).await {
                        tracing::debug!(%error, "could not cache project icon");
                    }
                }
            })
            .collect();
        futures::stream::iter(jobs)
            .buffer_unordered(4)
            .collect::<Vec<()>>()
            .await;
    }
}

fn accepts(kind: ImportKind, file_name: &str) -> bool {
    match kind {
        ImportKind::Mod => is_mod_file_name(file_name),
        _ => kind.accepts(file_name.strip_suffix(DISABLED_SUFFIX).unwrap_or(file_name)),
    }
}

fn entries(files: &[SeenFile], index: &AddonIndex) -> Vec<AddonEntry> {
    files
        .iter()
        .map(|f| AddonEntry {
            kind: f.kind,
            file_name: f.file_name.clone(),
            enabled: f.enabled,
            size: f.size,
            sha1: f.sha1.clone(),
            source: index.files.get(&f.key).and_then(|r| r.source.clone()),
            update: None,
        })
        .collect()
}

/// `a` is a later RFC 3339 timestamp than `b` (unparseable timestamps compare as text).
fn is_later(a: &str, b: &str) -> bool {
    match (a.parse::<jiff::Timestamp>(), b.parse::<jiff::Timestamp>()) {
        (Ok(a), Ok(b)) => a > b,
        _ => a > b,
    }
}

/// Modrinth ids are short alphanumeric strings; anything else never becomes a file name.
fn is_plain_id(id: &str) -> bool {
    !id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric())
}

fn icon_extension(url: &str) -> &'static str {
    let name = url
        .split(['?', '#'])
        .next()
        .unwrap_or(url)
        .rsplit('/')
        .next()
        .unwrap_or("");
    let ext = name
        .rsplit_once('.')
        .map(|(_, ext)| ext.to_ascii_lowercase())
        .unwrap_or_default();
    ICON_EXTENSIONS
        .into_iter()
        .find(|known| *known == ext)
        .unwrap_or("png")
}

/// The cached icon of a Modrinth project in `icon_dir`, if one was downloaded.
pub fn cached_icon(icon_dir: &Path, project_id: &str) -> Option<PathBuf> {
    if !is_plain_id(project_id) {
        return None;
    }
    ICON_EXTENSIONS
        .iter()
        .map(|ext| icon_dir.join(format!("{project_id}.{ext}")))
        .find(|path| path.is_file())
}
