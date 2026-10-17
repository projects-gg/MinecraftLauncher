//! Keeping an instance made from a Modrinth modpack up to date with newer versions of the pack.
//!
//! When the pack is imported the launcher writes [`ModpackState`] into the instance's
//! `.launcher/` folder: the pack version, every file it put into the game folder with its SHA-1
//! (downloads and bundled overrides), and the optional files the player left out. An update reads
//! the new version's `.mrpack` ([`read_pack_contents`]), compares it with that state and with what
//! is on disk ([`plan_update`]), and the plan follows the mod center's rules:
//! - a pack file the player did not touch is replaced by the new version's, or removed when the
//!   new version dropped it;
//! - a pack file the player changed, turned off (`.disabled`) or deleted stays as the player left
//!   it, and so does a file of the player's that sits where the new version wants to put one;
//! - files the player added are never looked at;
//! - files new to the pack are added; a new optional file too, unless the player had left that
//!   optional file (by Modrinth project) out before.
//!
//! Instances imported before the state existed get it from the `.mrpack` of the version they were
//! made from ([`ModpackState::from_contents`]).

use std::collections::{BTreeMap, BTreeSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Read, Write};
use std::path::{Path, PathBuf};

use pl_net::{CancellationToken, Checksum, DownloadItem, HashAlgo};
use serde::{Deserialize, Serialize};
use sha1::Digest;
use zip::ZipArchive;

use super::mrpack::{MrEnvSupport, MrFile, MrIndex, read_index};
use super::safe_path::validate_relative_path;
use crate::error::{Error, Result};
use crate::loaders::LoaderKind;
use crate::modrinth::{ModrinthVersion, ModrinthVersionType, compatible_loaders};

/// File name of the state in the instance's `.launcher/` folder.
pub const MODPACK_STATE_FILE: &str = "modpack.json";

const OVERRIDE_PREFIXES: [&str; 2] = ["overrides/", "client-overrides/"];

/// What the launcher knows about the Modrinth modpack an instance was made from.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct ModpackState {
    pub project_id: String,
    pub version_id: String,
    pub version_number: Option<String>,
    /// The Minecraft version the files belong to (a version switch parks them with their set).
    pub minecraft: String,
    /// Game-relative path → lowercase SHA-1 of what the pack put there.
    pub files: BTreeMap<String, String>,
    /// Game-relative path → Modrinth project of the downloaded files: a new version of a mod
    /// usually has another file name, and the project tells it is the same mod.
    pub projects: BTreeMap<String, String>,
    /// Optional files the player left out ([`MrFile::optional_key`]).
    pub optional_off: BTreeSet<String>,
}

impl ModpackState {
    /// The state of a pack version as imported with `contents`.
    pub fn from_contents(
        project_id: &str,
        version_id: &str,
        version_number: Option<String>,
        contents: &PackContents,
    ) -> Self {
        Self {
            project_id: project_id.to_owned(),
            version_id: version_id.to_owned(),
            version_number,
            minecraft: contents
                .index
                .minecraft_version()
                .unwrap_or_default()
                .to_owned(),
            files: contents
                .files
                .iter()
                .map(|(path, file)| (path.clone(), file.sha1.clone()))
                .collect(),
            projects: contents
                .files
                .iter()
                .filter_map(|(path, file)| match &file.source {
                    PackFileSource::Download(f) => Some((path.clone(), f.modrinth_project()?)),
                    PackFileSource::Bundled { .. } => None,
                })
                .collect(),
            optional_off: contents.optional_off.clone(),
        }
    }

    /// The state after applying `plan` to reach `contents`: the new version's files, except
    /// that a mod the player turned off, deleted or changed keeps its old entry (its new file
    /// was not installed), so later updates still see the player's choice.
    pub fn after_update(
        &self,
        version_id: &str,
        version_number: Option<String>,
        contents: &PackContents,
        plan: &UpdatePlan,
    ) -> Self {
        let mut next = Self::from_contents(&self.project_id, version_id, version_number, contents);
        for (old_path, new_path) in &plan.held {
            next.files.remove(new_path);
            next.projects.remove(new_path);
            if let Some(sha) = self.files.get(old_path) {
                next.files.insert(old_path.clone(), sha.clone());
            }
            if let Some(project) = self.projects.get(old_path) {
                next.projects.insert(old_path.clone(), project.clone());
            }
        }
        next
    }

    /// The saved state; `None` when there is none (or it cannot be read).
    pub fn load(path: &Path) -> Option<Self> {
        let bytes = std::fs::read(path).ok()?;
        match serde_json::from_slice(&bytes) {
            Ok(state) => Some(state),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "ignoring an unreadable modpack state");
                None
            }
        }
    }

    pub fn save(&self, path: &Path) -> Result<()> {
        let bytes =
            serde_json::to_vec_pretty(self).map_err(|e| Error::json(MODPACK_STATE_FILE, e))?;
        crate::paths::write_durable_blocking(path, &bytes)
    }
}

/// Where a file of a pack version comes from.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PackFileSource {
    /// Downloaded: a file of the index.
    Download(MrFile),
    /// Bundled in the archive: the entry's name.
    Bundled { entry: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PackFile {
    /// Lowercase SHA-1.
    pub sha1: String,
    pub size: u64,
    pub source: PackFileSource,
}

/// One version of a pack as an instance gets it.
#[derive(Debug, Clone)]
pub struct PackContents {
    pub index: MrIndex,
    /// Game-relative path → what this version puts there (files the client does not support and
    /// the optional files left out are not here).
    pub files: BTreeMap<String, PackFile>,
    /// Optional files of this version that are left out.
    pub optional_off: BTreeSet<String>,
}

/// Reads an `.mrpack`: its index and every file it puts into the game folder with its SHA-1
/// (bundled files are hashed from the archive; `client-overrides/` wins over `overrides/`).
/// Optional files whose key is in `optional_off` are left out.
pub fn read_pack_contents(
    archive_path: &Path,
    optional_off: &BTreeSet<String>,
) -> Result<PackContents> {
    let file = File::open(archive_path).map_err(|e| Error::io(archive_path, e))?;
    let mut archive =
        ZipArchive::new(BufReader::new(file)).map_err(|e| Error::archive(archive_path, e))?;
    let index = read_index(&mut archive, archive_path)?;
    let mut files = BTreeMap::new();
    let mut left_out = BTreeSet::new();
    for prefix in OVERRIDE_PREFIXES {
        for i in 0..archive.len() {
            let mut entry = archive
                .by_index(i)
                .map_err(|e| Error::archive(archive_path, e))?;
            let Ok(name) = entry.name().map(|n| n.replace('\\', "/")) else {
                continue;
            };
            let Some(rel) = name.strip_prefix(prefix) else {
                continue;
            };
            if entry.is_dir() || entry.is_symlink() || rel.is_empty() || rel.ends_with('/') {
                continue;
            }
            if validate_relative_path(rel).is_err() {
                continue;
            }
            let mut hasher = sha1::Sha1::new();
            let mut buffer = [0u8; 64 * 1024];
            let mut size = 0u64;
            loop {
                let read = entry
                    .read(&mut buffer)
                    .map_err(|e| Error::archive(archive_path, format!("{name}: {e}")))?;
                if read == 0 {
                    break;
                }
                hasher.update(&buffer[..read]);
                size += read as u64;
            }
            files.insert(
                rel.to_owned(),
                PackFile {
                    sha1: hex::encode(hasher.finalize()),
                    size,
                    source: PackFileSource::Bundled {
                        entry: name.clone(),
                    },
                },
            );
        }
    }
    for file in &index.files {
        match file.client_support() {
            MrEnvSupport::Unsupported => continue,
            MrEnvSupport::Optional if optional_off.contains(&file.optional_key()) => {
                left_out.insert(file.optional_key());
                continue;
            }
            _ => {}
        }
        files.insert(
            file.path.clone(),
            PackFile {
                sha1: file.hashes.sha1.to_ascii_lowercase(),
                size: file.file_size,
                source: PackFileSource::Download(file.clone()),
            },
        );
    }
    Ok(PackContents {
        index,
        files,
        optional_off: left_out,
    })
}

/// What sits at a pack path in the game folder.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OnDisk {
    Missing,
    /// Only `<path>.disabled` (the player turned the mod off).
    Disabled,
    /// The file, with its lowercase SHA-1.
    File(String),
}

/// What [`disk_state`] looks for: the file's SHA-1, or a cached one when its size and
/// modification time match a record.
pub type HashCache<'a> = &'a dyn Fn(&str, u64, Option<i64>) -> Option<String>;

/// The state of one pack path: the file (hashed unless `cache` knows it), `<path>.disabled`, or
/// nothing.
pub fn disk_state(game_dir: &Path, rel: &str, cache: HashCache<'_>) -> OnDisk {
    let Ok(rel_path) = validate_relative_path(rel) else {
        return OnDisk::Missing;
    };
    let path = game_dir.join(rel_path);
    if let Ok(meta) = std::fs::metadata(&path)
        && meta.is_file()
    {
        let modified = crate::gamefiles::fsops::modified_ms(&meta);
        if let Some(sha1) = cache(rel, meta.len(), modified) {
            return OnDisk::File(sha1);
        }
        return match sha1_file(&path) {
            Some(sha1) => OnDisk::File(sha1),
            None => OnDisk::Missing,
        };
    }
    let mut disabled = path.into_os_string();
    disabled.push(".disabled");
    if PathBuf::from(disabled).is_file() {
        OnDisk::Disabled
    } else {
        OnDisk::Missing
    }
}

fn sha1_file(path: &Path) -> Option<String> {
    let mut file = File::open(path).ok()?;
    let mut hasher = sha1::Sha1::new();
    let mut buffer = [0u8; 64 * 1024];
    loop {
        let read = file.read(&mut buffer).ok()?;
        if read == 0 {
            break;
        }
        hasher.update(&buffer[..read]);
    }
    Some(hex::encode(hasher.finalize()))
}

/// Why a pack file is left as it is.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum KeptReason {
    /// The player changed the pack's file.
    Changed,
    /// The player deleted it.
    Removed,
    /// The player turned it off.
    Disabled,
    /// A file of the player's sits where the new version puts one.
    Theirs,
}

/// What an update does, by game-relative path.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct UpdatePlan {
    /// Files to put in place: new to the pack, or the new file of a mod whose file name changed
    /// (see `moved`).
    pub add: Vec<String>,
    /// Pack files the new version changed under the same name (and the player did not).
    pub replace: Vec<String>,
    /// Pack files to delete: dropped by the new version, or the old file of a moved mod (and the
    /// player did not change them).
    pub remove: Vec<String>,
    /// A mod's new version under another file name: (old path, new path), both also in
    /// `remove`/`add`. For the player this is an update.
    pub moved: Vec<(String, String)>,
    /// Pack files the new version changed or dropped that stay as the player left them.
    pub kept: Vec<(String, KeptReason)>,
    /// Mods the player turned off, deleted or changed whose new version comes under another
    /// file name: (old path, new path); the new file is not installed.
    pub held: Vec<(String, String)>,
    /// Pack files that stay as they are.
    pub unchanged: u64,
}

impl UpdatePlan {
    pub fn is_empty(&self) -> bool {
        self.add.is_empty() && self.replace.is_empty() && self.remove.is_empty()
    }
}

/// Compares the state of the installed version with a new version and what is on disk. A mod's
/// new version under another file name is matched by its Modrinth project, so a mod the player
/// turned off or deleted stays that way.
pub fn plan_update(
    old: &ModpackState,
    new: &PackContents,
    disk: &mut dyn FnMut(&str) -> OnDisk,
) -> UpdatePlan {
    let mut plan = UpdatePlan::default();
    // Old files by project, for mods whose file name changed.
    let mut old_by_project: std::collections::HashMap<&str, &str> = old
        .projects
        .iter()
        .filter(|(path, _)| !new.files.contains_key(*path))
        .map(|(path, project)| (project.as_str(), path.as_str()))
        .collect();
    let mut paired: BTreeSet<&str> = BTreeSet::new();
    for (path, file) in &new.files {
        if !old.files.contains_key(path)
            && let PackFileSource::Download(mr) = &file.source
            && let Some(project) = mr.modrinth_project()
            && let Some(old_path) = old_by_project.remove(project.as_str())
            && let Some(old_sha) = old.files.get(old_path)
        {
            paired.insert(old_path);
            let held = |plan: &mut UpdatePlan, reason| {
                plan.kept.push((old_path.to_owned(), reason));
                plan.held.push((old_path.to_owned(), path.clone()));
            };
            match disk(old_path) {
                OnDisk::File(sha) if sha == *old_sha => match disk(path) {
                    OnDisk::Missing => {
                        plan.add.push(path.clone());
                        plan.remove.push(old_path.to_owned());
                        plan.moved.push((old_path.to_owned(), path.clone()));
                    }
                    // The new file is there already: only the old one goes.
                    OnDisk::File(sha) if sha == file.sha1 => {
                        plan.remove.push(old_path.to_owned());
                        plan.moved.push((old_path.to_owned(), path.clone()));
                    }
                    _ => held(&mut plan, KeptReason::Theirs),
                },
                OnDisk::File(_) => held(&mut plan, KeptReason::Changed),
                OnDisk::Disabled => held(&mut plan, KeptReason::Disabled),
                OnDisk::Missing => held(&mut plan, KeptReason::Removed),
            }
            continue;
        }
        match old.files.get(path) {
            // The pack did not change this file: whatever the player did with it stays.
            Some(old_sha) if *old_sha == file.sha1 => plan.unchanged += 1,
            Some(old_sha) => match disk(path) {
                OnDisk::File(sha) if sha == *old_sha => plan.replace.push(path.clone()),
                OnDisk::File(sha) if sha == file.sha1 => plan.unchanged += 1,
                OnDisk::File(_) => plan.kept.push((path.clone(), KeptReason::Changed)),
                OnDisk::Disabled => plan.kept.push((path.clone(), KeptReason::Disabled)),
                OnDisk::Missing => plan.kept.push((path.clone(), KeptReason::Removed)),
            },
            None => match disk(path) {
                OnDisk::Missing => plan.add.push(path.clone()),
                OnDisk::File(sha) if sha == file.sha1 => plan.unchanged += 1,
                OnDisk::File(_) => plan.kept.push((path.clone(), KeptReason::Theirs)),
                OnDisk::Disabled => plan.kept.push((path.clone(), KeptReason::Disabled)),
            },
        }
    }
    for (path, old_sha) in &old.files {
        if new.files.contains_key(path) || paired.contains(path.as_str()) {
            continue;
        }
        match disk(path) {
            OnDisk::File(sha) if sha == *old_sha => plan.remove.push(path.clone()),
            OnDisk::File(_) => plan.kept.push((path.clone(), KeptReason::Changed)),
            OnDisk::Disabled => plan.kept.push((path.clone(), KeptReason::Disabled)),
            OnDisk::Missing => {}
        }
    }
    plan
}

/// The downloads of a plan (added and replaced files that come from the index). Checksums prefer
/// SHA-512; replaced files are re-downloaded because their hash no longer matches.
pub fn update_download_items(
    game_dir: &Path,
    contents: &PackContents,
    plan: &UpdatePlan,
) -> Result<Vec<DownloadItem>> {
    let mut items = Vec::new();
    for path in plan.add.iter().chain(&plan.replace) {
        let Some(PackFile {
            source: PackFileSource::Download(file),
            ..
        }) = contents.files.get(path)
        else {
            continue;
        };
        if file.downloads.is_empty() {
            return Err(Error::Invalid(format!(
                "mrpack file {} has no download URL",
                file.path
            )));
        }
        let checksum = match &file.hashes.sha512 {
            Some(sha512) if !sha512.is_empty() => Checksum::new(HashAlgo::Sha512, sha512),
            _ => Checksum::new(HashAlgo::Sha1, &file.hashes.sha1),
        };
        items.push(
            DownloadItem::with_urls(
                file.downloads.clone(),
                game_dir.join(validate_relative_path(path)?),
            )
            .checksum(Some(checksum))
            .size(Some(file.file_size)),
        );
    }
    Ok(items)
}

/// Writes the bundled files of a plan (added and replaced overrides) from the archive.
pub fn write_bundled(
    archive_path: &Path,
    game_dir: &Path,
    contents: &PackContents,
    plan: &UpdatePlan,
    cancel: &CancellationToken,
) -> Result<u64> {
    let wanted: BTreeMap<&str, &str> = plan
        .add
        .iter()
        .chain(&plan.replace)
        .filter_map(|path| match contents.files.get(path) {
            Some(PackFile {
                source: PackFileSource::Bundled { entry },
                ..
            }) => Some((entry.as_str(), path.as_str())),
            _ => None,
        })
        .collect();
    if wanted.is_empty() {
        return Ok(0);
    }
    let file = File::open(archive_path).map_err(|e| Error::io(archive_path, e))?;
    let mut archive =
        ZipArchive::new(BufReader::new(file)).map_err(|e| Error::archive(archive_path, e))?;
    let mut written = 0;
    for i in 0..archive.len() {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let mut entry = archive
            .by_index(i)
            .map_err(|e| Error::archive(archive_path, e))?;
        let Ok(name) = entry.name().map(|n| n.replace('\\', "/")) else {
            continue;
        };
        let Some(rel) = wanted.get(name.as_str()) else {
            continue;
        };
        let target = game_dir.join(validate_relative_path(rel)?);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
        }
        let tmp = crate::gamefiles::fsops::sibling_with_suffix(&target, ".part");
        let result = File::create(&tmp)
            .map_err(|e| Error::io(&tmp, e))
            .and_then(|out| {
                let mut out = BufWriter::new(out);
                std::io::copy(&mut entry, &mut out)
                    .map_err(|e| Error::archive(archive_path, format!("{name}: {e}")))?;
                out.flush().map_err(|e| Error::io(&tmp, e))
            })
            .and_then(|()| std::fs::rename(&tmp, &target).map_err(|e| Error::io(&target, e)));
        if let Err(error) = result {
            let _ = std::fs::remove_file(&tmp);
            return Err(error);
        }
        written += 1;
    }
    Ok(written)
}

/// Deletes the pack files the new version dropped (only the ones the player did not change: the
/// plan checked that). Returns how many went.
pub fn remove_dropped(game_dir: &Path, plan: &UpdatePlan) -> u64 {
    let mut removed = 0;
    for path in &plan.remove {
        let Ok(rel) = validate_relative_path(path) else {
            continue;
        };
        match std::fs::remove_file(game_dir.join(rel)) {
            Ok(()) => removed += 1,
            Err(error) => tracing::warn!(%path, %error, "could not remove a dropped pack file"),
        }
    }
    removed
}

/// The version to update an installed pack version to: the newest version for the same
/// Minecraft version and loader published after the installed one. Releases only, unless the
/// installed version is a beta (betas too) or an alpha (anything). `None` when the installed
/// version is the newest.
pub fn pack_update_candidate<'a>(
    versions: &'a [ModrinthVersion],
    installed: &ModrinthVersion,
    minecraft: &str,
    loader: LoaderKind,
) -> Option<&'a ModrinthVersion> {
    let loaders = compatible_loaders(loader, minecraft);
    let rank = |kind: ModrinthVersionType| match kind {
        ModrinthVersionType::Release => 0,
        ModrinthVersionType::Beta => 1,
        _ => 2,
    };
    let published = |v: &ModrinthVersion| v.date_published.parse::<jiff::Timestamp>().ok();
    let installed_at = published(installed)?;
    versions
        .iter()
        .filter(|v| v.id != installed.id && v.project_id == installed.project_id)
        .filter(|v| v.game_versions.iter().any(|g| g == minecraft))
        .filter(|v| {
            loaders.is_empty()
                || v.loaders
                    .iter()
                    .any(|l| loaders.iter().any(|ok| ok.eq_ignore_ascii_case(l)))
        })
        .filter(|v| rank(v.version_type) <= rank(installed.version_type))
        .filter_map(|v| Some((published(v)?, v)))
        .filter(|(at, _)| *at > installed_at)
        .max_by_key(|(at, _)| *at)
        .map(|(_, v)| v)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::test_support::zip_bytes;

    fn sha1_of(bytes: &[u8]) -> String {
        hex::encode(sha1::Sha1::digest(bytes))
    }

    fn index(files: serde_json::Value) -> String {
        serde_json::json!({
            "formatVersion": 1, "game": "minecraft", "versionId": "2.0.0", "name": "Pack",
            "files": files,
            "dependencies": {"minecraft": "1.21.1", "fabric-loader": "0.16.9"}
        })
        .to_string()
    }

    fn mod_file(path: &str, content: &[u8], project: &str, env: Option<&str>) -> serde_json::Value {
        let mut file = serde_json::json!({
            "path": path,
            "hashes": {"sha1": sha1_of(content)},
            "downloads": [format!("https://cdn.modrinth.com/data/{project}/versions/v/{}", path.rsplit('/').next().unwrap())],
            "fileSize": content.len(),
        });
        if let Some(client) = env {
            file["env"] = serde_json::json!({"client": client, "server": "required"});
        }
        file
    }

    fn write_pack(
        dir: &Path,
        name: &str,
        files: serde_json::Value,
        bundled: &[(&str, &[u8])],
    ) -> PathBuf {
        let index = index(files);
        let mut entries: Vec<(&str, &[u8])> = vec![(super::super::MRPACK_INDEX, index.as_bytes())];
        entries.extend_from_slice(bundled);
        let path = dir.join(name);
        std::fs::write(&path, zip_bytes(&entries)).unwrap();
        path
    }

    #[test]
    fn reads_downloads_and_bundled_files_with_hashes() {
        let dir = tempfile::tempdir().unwrap();
        let pack = write_pack(
            dir.path(),
            "p.mrpack",
            serde_json::json!([
                mod_file("mods/a.jar", b"a1", "AAAA1111", None),
                mod_file("mods/dh.jar", b"dh", "DHDH2222", Some("optional")),
                mod_file("mods/server.jar", b"s", "SSSS3333", Some("unsupported")),
            ]),
            &[
                ("overrides/config/a.toml", b"x=1"),
                ("overrides/options.txt", b"o"),
                ("client-overrides/config/a.toml", b"x=2"),
                ("overrides/../escape.txt", b"bad"),
            ],
        );
        let all = read_pack_contents(&pack, &BTreeSet::new()).unwrap();
        let paths: Vec<&str> = all.files.keys().map(String::as_str).collect();
        assert_eq!(
            paths,
            ["config/a.toml", "mods/a.jar", "mods/dh.jar", "options.txt"]
        );
        assert_eq!(
            all.files["config/a.toml"].sha1,
            sha1_of(b"x=2"),
            "client wins"
        );
        assert_eq!(all.files["mods/a.jar"].sha1, sha1_of(b"a1"));
        assert!(all.optional_off.is_empty());

        let off: BTreeSet<String> = ["DHDH2222".to_owned()].into();
        let without = read_pack_contents(&pack, &off).unwrap();
        assert!(!without.files.contains_key("mods/dh.jar"));
        assert_eq!(without.optional_off, off);

        let state = ModpackState::from_contents("proj", "ver1", Some("2.0.0".into()), &without);
        assert_eq!(state.minecraft, "1.21.1");
        assert_eq!(state.files.len(), 3);
        let path = dir.path().join(".launcher").join(MODPACK_STATE_FILE);
        state.save(&path).unwrap();
        assert_eq!(ModpackState::load(&path), Some(state));
    }

    /// Install version 1 the way an import does, then plan and apply version 2.
    #[test]
    fn updates_keep_what_the_player_changed() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        let v1 = write_pack(
            dir.path(),
            "v1.mrpack",
            serde_json::json!([
                mod_file("mods/keep.jar", b"k1", "KEEP0001", None),
                mod_file("mods/up.jar", b"u1", "UPUP0001", None),
                mod_file("mods/gone.jar", b"g1", "GONE0001", None),
                mod_file("mods/mine.jar", b"m1", "MINE0001", None),
                mod_file("mods/off.jar", b"f1", "OFFF0001", None),
                mod_file("mods/deleted.jar", b"d1", "DELE0001", None),
                mod_file("mods/extra.jar", b"e1", "EXTR0001", Some("optional")),
            ]),
            &[
                ("overrides/config/c.toml", b"c=1"),
                ("overrides/config/edited.toml", b"e=1"),
            ],
        );
        let off: BTreeSet<String> = ["EXTR0001".to_owned()].into();
        let v1_contents = read_pack_contents(&v1, &off).unwrap();
        let state = ModpackState::from_contents("proj", "v1", Some("1.0.0".into()), &v1_contents);
        // What the import left on disk, then the player's changes.
        for (path, file) in &v1_contents.files {
            let bytes: Vec<u8> = match &file.source {
                PackFileSource::Download(f) if f.path == "mods/keep.jar" => b"k1".to_vec(),
                PackFileSource::Download(f) if f.path == "mods/up.jar" => b"u1".to_vec(),
                PackFileSource::Download(f) if f.path == "mods/gone.jar" => b"g1".to_vec(),
                PackFileSource::Download(f) if f.path == "mods/mine.jar" => b"m1".to_vec(),
                PackFileSource::Download(f) if f.path == "mods/off.jar" => b"f1".to_vec(),
                PackFileSource::Download(f) if f.path == "mods/deleted.jar" => b"d1".to_vec(),
                PackFileSource::Bundled { entry } if entry.ends_with("c.toml") => b"c=1".to_vec(),
                _ => b"e=1".to_vec(),
            };
            let target = game.join(path);
            std::fs::create_dir_all(target.parent().unwrap()).unwrap();
            std::fs::write(target, bytes).unwrap();
        }
        std::fs::write(game.join("mods/mine.jar"), b"my build").unwrap();
        std::fs::rename(
            game.join("mods/off.jar"),
            game.join("mods/off.jar.disabled"),
        )
        .unwrap();
        std::fs::remove_file(game.join("mods/deleted.jar")).unwrap();
        std::fs::write(game.join("config/edited.toml"), b"e=mine").unwrap();
        std::fs::write(game.join("mods/own.jar"), b"own").unwrap();

        let v2 = write_pack(
            dir.path(),
            "v2.mrpack",
            serde_json::json!([
                mod_file("mods/keep.jar", b"k1", "KEEP0001", None),
                mod_file("mods/up.jar", b"u2", "UPUP0001", None),
                mod_file("mods/mine.jar", b"m2", "MINE0001", None),
                mod_file("mods/off.jar", b"f2", "OFFF0001", None),
                mod_file("mods/deleted.jar", b"d2", "DELE0001", None),
                mod_file("mods/new.jar", b"n1", "NEWW0001", None),
                mod_file("mods/own.jar", b"pack own", "OWNN0001", None),
                mod_file("mods/extra.jar", b"e2", "EXTR0001", Some("optional")),
                mod_file("mods/shiny.jar", b"s1", "SHIN0001", Some("optional")),
            ]),
            &[
                ("overrides/config/c.toml", b"c=2"),
                ("overrides/config/edited.toml", b"e=2"),
                ("overrides/config/fresh.toml", b"f=1"),
            ],
        );
        let v2_contents = read_pack_contents(&v2, &state.optional_off).unwrap();
        let no_cache = |_: &str, _: u64, _: Option<i64>| None;
        let mut disk = |rel: &str| disk_state(&game, rel, &no_cache);
        let plan = plan_update(&state, &v2_contents, &mut disk);
        let sorted = |mut v: Vec<String>| {
            v.sort();
            v
        };
        assert_eq!(
            sorted(plan.add.clone()),
            ["config/fresh.toml", "mods/new.jar", "mods/shiny.jar"]
        );
        assert_eq!(
            sorted(plan.replace.clone()),
            ["config/c.toml", "mods/up.jar"]
        );
        assert_eq!(plan.remove, ["mods/gone.jar"]);
        let mut kept = plan.kept.clone();
        kept.sort();
        assert_eq!(
            kept,
            [
                ("config/edited.toml".to_owned(), KeptReason::Changed),
                ("mods/deleted.jar".to_owned(), KeptReason::Removed),
                ("mods/mine.jar".to_owned(), KeptReason::Changed),
                ("mods/off.jar".to_owned(), KeptReason::Disabled),
                ("mods/own.jar".to_owned(), KeptReason::Theirs),
            ]
        );
        assert_eq!(plan.unchanged, 1, "keep.jar");
        assert!(
            !v2_contents.files.contains_key("mods/extra.jar"),
            "still left out"
        );

        // Applying: bundled files and removals here; downloads are the launcher's job.
        let items = update_download_items(&game, &v2_contents, &plan).unwrap();
        let mut targets: Vec<String> = items
            .iter()
            .map(|i| i.dest.file_name().unwrap().to_string_lossy().into_owned())
            .collect();
        targets.sort();
        assert_eq!(targets, ["new.jar", "shiny.jar", "up.jar"]);
        let written =
            write_bundled(&v2, &game, &v2_contents, &plan, &CancellationToken::new()).unwrap();
        assert_eq!(written, 2);
        assert_eq!(std::fs::read(game.join("config/c.toml")).unwrap(), b"c=2");
        assert_eq!(
            std::fs::read(game.join("config/fresh.toml")).unwrap(),
            b"f=1"
        );
        assert_eq!(
            std::fs::read(game.join("config/edited.toml")).unwrap(),
            b"e=mine"
        );
        assert_eq!(remove_dropped(&game, &plan), 1);
        assert!(!game.join("mods/gone.jar").exists());
        assert_eq!(std::fs::read(game.join("mods/own.jar")).unwrap(), b"own");
        assert!(game.join("mods/off.jar.disabled").is_file());

        // The next state remembers version 2's files and the player's choice.
        let next = ModpackState::from_contents("proj", "v2", Some("2.0.0".into()), &v2_contents);
        assert_eq!(next.optional_off, state.optional_off);
        assert_eq!(next.files["mods/up.jar"], sha1_of(b"u2"));
    }

    /// A mod's new version has another file name: matched by project, it is an update, and a mod
    /// the player turned off or deleted is not installed again.
    #[test]
    fn renamed_mod_files_follow_their_project() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        let v1 = write_pack(
            dir.path(),
            "v1.mrpack",
            serde_json::json!([
                mod_file("mods/sodium-0.5.jar", b"s1", "AANobbMI", None),
                mod_file("mods/zoomify-2.13.jar", b"z1", "w7ThoJFB", None),
                mod_file("mods/lithium-0.14.jar", b"l1", "gvQqBUqZ", None),
            ]),
            &[],
        );
        let v1_contents = read_pack_contents(&v1, &BTreeSet::new()).unwrap();
        let state = ModpackState::from_contents("p", "v1", None, &v1_contents);
        assert_eq!(state.projects["mods/sodium-0.5.jar"], "AANobbMI");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::write(game.join("mods/sodium-0.5.jar"), b"s1").unwrap();
        std::fs::write(game.join("mods/zoomify-2.13.jar.disabled"), b"z1").unwrap();
        // lithium was deleted by the player.

        let v2 = write_pack(
            dir.path(),
            "v2.mrpack",
            serde_json::json!([
                mod_file("mods/sodium-0.6.jar", b"s2", "AANobbMI", None),
                mod_file("mods/zoomify-2.14.jar", b"z2", "w7ThoJFB", None),
                mod_file("mods/lithium-0.15.jar", b"l2", "gvQqBUqZ", None),
            ]),
            &[],
        );
        let v2_contents = read_pack_contents(&v2, &BTreeSet::new()).unwrap();
        let no_cache = |_: &str, _: u64, _: Option<i64>| None;
        let mut disk = |rel: &str| disk_state(&game, rel, &no_cache);
        let plan = plan_update(&state, &v2_contents, &mut disk);
        assert_eq!(plan.add, ["mods/sodium-0.6.jar"]);
        assert_eq!(plan.remove, ["mods/sodium-0.5.jar"]);
        assert_eq!(
            plan.moved,
            [(
                "mods/sodium-0.5.jar".to_owned(),
                "mods/sodium-0.6.jar".to_owned()
            )]
        );
        let mut kept = plan.kept.clone();
        kept.sort();
        assert_eq!(
            kept,
            [
                ("mods/lithium-0.14.jar".to_owned(), KeptReason::Removed),
                ("mods/zoomify-2.13.jar".to_owned(), KeptReason::Disabled),
            ]
        );
        assert_eq!(plan.held.len(), 2);

        // The next state keeps the old entries of the held mods, so the next update holds them too.
        let next = state.after_update("v2", None, &v2_contents, &plan);
        assert!(next.files.contains_key("mods/sodium-0.6.jar"));
        assert!(next.files.contains_key("mods/zoomify-2.13.jar"));
        assert!(!next.files.contains_key("mods/zoomify-2.14.jar"));
        assert_eq!(next.projects["mods/zoomify-2.13.jar"], "w7ThoJFB");
    }

    #[test]
    fn nothing_to_do_for_the_same_version() {
        let dir = tempfile::tempdir().unwrap();
        let pack = write_pack(
            dir.path(),
            "p.mrpack",
            serde_json::json!([mod_file("mods/a.jar", b"a", "AAAA0001", None)]),
            &[],
        );
        let contents = read_pack_contents(&pack, &BTreeSet::new()).unwrap();
        let state = ModpackState::from_contents("p", "v", None, &contents);
        let mut disk = |_: &str| OnDisk::File(sha1_of(b"a"));
        let plan = plan_update(&state, &contents, &mut disk);
        assert!(plan.is_empty());
        assert_eq!(plan.unchanged, 1);
    }

    #[test]
    fn project_ids_come_from_modrinth_links() {
        let file: MrFile =
            serde_json::from_value(mod_file("mods/x.jar", b"x", "AbC123", None)).unwrap();
        assert_eq!(file.modrinth_project().as_deref(), Some("AbC123"));
        assert_eq!(file.optional_key(), "AbC123");
        let elsewhere = MrFile {
            downloads: vec!["https://github.com/x/y/releases/download/1/x.jar".into()],
            ..file
        };
        assert_eq!(elsewhere.modrinth_project(), None);
        assert_eq!(elsewhere.optional_key(), "mods/x.jar");
    }

    fn version(id: &str, date: &str, kind: &str, mc: &str, loader: &str) -> ModrinthVersion {
        serde_json::from_value(serde_json::json!({
            "id": id, "projectId": "pack", "authorId": null, "name": id, "versionNumber": id,
            "changelog": null, "dependencies": [], "gameVersions": [mc],
            "versionType": kind, "loaders": [loader], "featured": false, "status": null,
            "datePublished": date, "downloads": 0, "files": []
        }))
        .unwrap()
    }

    #[test]
    fn picks_the_newest_compatible_version() {
        let installed = version(
            "v1",
            "2026-09-01T10:00:00.000Z",
            "release",
            "1.21.1",
            "fabric",
        );
        let list = vec![
            version(
                "v3-beta",
                "2026-10-10T10:00:00Z",
                "beta",
                "1.21.1",
                "fabric",
            ),
            version("v2", "2026-10-01T10:00:00Z", "release", "1.21.1", "fabric"),
            version(
                "v4-other-mc",
                "2026-10-12T10:00:00Z",
                "release",
                "1.21.4",
                "fabric",
            ),
            version(
                "v5-forge",
                "2026-10-13T10:00:00Z",
                "release",
                "1.21.1",
                "neoforge",
            ),
            version("v0", "2026-08-01T10:00:00Z", "release", "1.21.1", "fabric"),
            installed.clone(),
        ];
        let pick = |installed: &ModrinthVersion| {
            pack_update_candidate(&list, installed, "1.21.1", LoaderKind::Fabric)
                .map(|v| v.id.as_str())
        };
        assert_eq!(pick(&installed), Some("v2"));
        let beta = version("v1b", "2026-09-01T10:00:00Z", "beta", "1.21.1", "fabric");
        assert_eq!(pick(&beta), Some("v3-beta"));
        let newest = version("v9", "2026-11-01T10:00:00Z", "release", "1.21.1", "fabric");
        assert_eq!(pick(&newest), None);
        // Quilt runs Fabric packs too.
        assert_eq!(
            pack_update_candidate(&list, &installed, "1.21.1", LoaderKind::Quilt)
                .map(|v| v.id.as_str()),
            Some("v2")
        );
    }
}
