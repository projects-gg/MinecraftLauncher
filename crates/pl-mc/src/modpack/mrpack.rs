//! Modrinth `.mrpack`: `modrinth.index.json` + `overrides/` (+ `client-overrides/`).
//! <https://support.modrinth.com/en/articles/8802351-modrinth-modpack-format-mrpack>

use std::collections::{BTreeMap, HashSet};
use std::fs::File;
use std::io::{BufReader, BufWriter, Write};
use std::path::{Path, PathBuf};

use pl_net::{CancellationToken, Checksum, DownloadItem, HashAlgo};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use zip::ZipArchive;
use zip::write::SimpleFileOptions;

use super::extract::{ExtractReport, extract_prefix, read_entry_string};
use super::safe_path::validate_relative_path;
use super::{ImportPendingFiles, ImportedInstance, ModpackFormat};
use crate::error::{Error, Result};
use crate::gamefiles::fsops::{blocking, sibling_with_suffix};
use crate::loaders::LoaderKind;
use crate::modrinth::{ModrinthClient, ModrinthHashAlgorithm};

pub const MRPACK_INDEX: &str = "modrinth.index.json";
const MAX_INDEX_BYTES: u64 = 64 * 1024 * 1024;
/// Never bundled into an export.
const JUNK_FILES: [&str; 3] = [".DS_Store", "Thumbs.db", "desktop.ini"];

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MrIndex {
    pub format_version: u32,
    pub game: String,
    pub version_id: String,
    pub name: String,
    pub summary: Option<String>,
    pub files: Vec<MrFile>,
    /// `minecraft`, `fabric-loader`, `quilt-loader`, `forge`, `neoforge` → version.
    pub dependencies: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MrFile {
    /// Destination relative to the game directory (`/`-separated, validated).
    pub path: String,
    pub hashes: MrHashes,
    pub env: Option<MrEnv>,
    pub downloads: Vec<String>,
    pub file_size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MrHashes {
    pub sha1: String,
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MrEnv {
    pub client: MrEnvSupport,
    pub server: MrEnvSupport,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MrEnvSupport {
    Required,
    Optional,
    Unsupported,
}

impl MrFile {
    pub fn client_support(&self) -> MrEnvSupport {
        self.env.map_or(MrEnvSupport::Required, |env| env.client)
    }

    /// The Modrinth project of the file, read from its download link
    /// (`https://cdn.modrinth.com/data/<project>/versions/<version>/<file>`).
    pub fn modrinth_project(&self) -> Option<String> {
        self.downloads.iter().find_map(|url| {
            let id = url
                .strip_prefix("https://cdn.modrinth.com/data/")?
                .split('/')
                .next()?;
            (!id.is_empty() && id.len() <= 64 && id.chars().all(|c| c.is_ascii_alphanumeric()))
                .then(|| id.to_owned())
        })
    }

    /// What names an optional file across versions of a pack (a player's choice to leave it out
    /// is kept by it): its Modrinth project, else its path.
    pub fn optional_key(&self) -> String {
        self.modrinth_project().unwrap_or_else(|| self.path.clone())
    }
}

impl MrIndex {
    pub fn minecraft_version(&self) -> Option<&str> {
        self.dependencies.get("minecraft").map(String::as_str)
    }

    /// The mod loader and its version (`Vanilla` when the pack has none).
    pub fn loader(&self) -> (LoaderKind, Option<String>) {
        // NeoForge/Forge first: a pack never lists two loaders, but be deterministic.
        for kind in [
            LoaderKind::NeoForge,
            LoaderKind::Forge,
            LoaderKind::Quilt,
            LoaderKind::Fabric,
        ] {
            if let Some(version) = kind
                .mrpack_dependency_key()
                .and_then(|key| self.dependencies.get(key))
            {
                return (kind, Some(version.clone()));
            }
        }
        (LoaderKind::Vanilla, None)
    }

    /// Check the format and that every file path is safe. Packs with unsafe paths are
    /// rejected as a whole.
    pub fn validate(&self) -> Result<()> {
        if self.format_version != 1 {
            return Err(Error::Unsupported(format!(
                "mrpack format version {}",
                self.format_version
            )));
        }
        if self.game != "minecraft" {
            return Err(Error::Unsupported(format!(
                "mrpack for game {:?}",
                self.game
            )));
        }
        if self.minecraft_version().is_none_or(|v| v.trim().is_empty()) {
            return Err(Error::Invalid("mrpack has no Minecraft version".into()));
        }
        for file in &self.files {
            validate_relative_path(&file.path)?;
            if file.hashes.sha1.len() != 40
                || !file.hashes.sha1.chars().all(|c| c.is_ascii_hexdigit())
            {
                return Err(Error::Invalid(format!(
                    "mrpack file {} has a bad sha1",
                    file.path
                )));
            }
        }
        Ok(())
    }

    pub fn parse(json: &str) -> Result<Self> {
        let index: Self = serde_json::from_str(json).map_err(|e| Error::json(MRPACK_INDEX, e))?;
        index.validate()?;
        Ok(index)
    }

    /// Index JSON as written into packs: pretty-printed, without `null` fields.
    pub fn to_json(&self) -> Result<String> {
        let mut value = serde_json::to_value(self).map_err(|e| Error::json(MRPACK_INDEX, e))?;
        strip_nulls(&mut value);
        serde_json::to_string_pretty(&value).map_err(|e| Error::json(MRPACK_INDEX, e))
    }
}

fn strip_nulls(value: &mut Value) {
    match value {
        Value::Object(map) => {
            map.retain(|_, v| !v.is_null());
            map.values_mut().for_each(strip_nulls);
        }
        Value::Array(items) => items.iter_mut().for_each(strip_nulls),
        _ => {}
    }
}

/// Download list for the client: files marked unsupported on the client are left out, optional
/// ones only when `include_optional`. Checksums prefer SHA-512.
pub fn mrpack_download_items(
    game_dir: &Path,
    files: &[MrFile],
    include_optional: bool,
) -> Result<Vec<DownloadItem>> {
    let mut items = Vec::new();
    for file in files {
        match file.client_support() {
            MrEnvSupport::Unsupported => continue,
            MrEnvSupport::Optional if !include_optional => continue,
            _ => {}
        }
        let rel = validate_relative_path(&file.path)?;
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
            DownloadItem::with_urls(file.downloads.clone(), game_dir.join(rel))
                .checksum(Some(checksum))
                .size(Some(file.file_size)),
        );
    }
    Ok(items)
}

fn open_archive(path: &Path) -> Result<ZipArchive<BufReader<File>>> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    ZipArchive::new(BufReader::new(file)).map_err(|e| Error::archive(path, e))
}

pub(crate) fn read_index(
    archive: &mut ZipArchive<BufReader<File>>,
    path: &Path,
) -> Result<MrIndex> {
    let json = read_entry_string(archive, MRPACK_INDEX, MAX_INDEX_BYTES)
        .ok_or_else(|| Error::archive(path, format!("{MRPACK_INDEX} is missing")))?;
    MrIndex::parse(&json)
}

/// Read and validate the index of an `.mrpack` (for previews; nothing is extracted).
pub async fn read_mrpack_index(path: &Path) -> Result<MrIndex> {
    let path = path.to_path_buf();
    blocking(move || {
        let mut archive = open_archive(&path)?;
        read_index(&mut archive, &path)
    })
    .await
}

pub(crate) fn import_mrpack_blocking(
    path: &Path,
    game_dir: &Path,
    cancel: &CancellationToken,
) -> Result<ImportedInstance> {
    let mut archive = open_archive(path)?;
    let index = read_index(&mut archive, path)?;
    let mut report = ExtractReport::default();
    for prefix in ["overrides/", "client-overrides/"] {
        extract_prefix(&mut archive, path, prefix, game_dir, cancel, &mut report)?;
    }
    let (loader, loader_version) = index.loader();
    let files: Vec<MrFile> = index
        .files
        .iter()
        .filter(|f| f.client_support() != MrEnvSupport::Unsupported)
        .cloned()
        .collect();
    let mut instance = ImportedInstance::new(
        ModpackFormat::Mrpack,
        index.name.clone(),
        index.minecraft_version().unwrap_or_default().to_owned(),
    );
    instance.pack_version = Some(index.version_id.clone()).filter(|v| !v.is_empty());
    instance.summary = index.summary.clone();
    instance.loader = loader;
    instance.loader_version = loader_version;
    instance.apply_report(report);
    instance.pending = if files.is_empty() {
        ImportPendingFiles::None
    } else {
        ImportPendingFiles::Modrinth { files }
    };
    Ok(instance)
}

/// Import an `.mrpack`: validate the index, extract `overrides/` then `client-overrides/` into
/// `game_dir`. The Modrinth-hosted files are returned in `pending` for
/// [`mrpack_download_items`].
pub async fn import_mrpack(
    path: &Path,
    game_dir: &Path,
    cancel: CancellationToken,
) -> Result<ImportedInstance> {
    let (path, game_dir) = (path.to_path_buf(), game_dir.to_path_buf());
    blocking(move || import_mrpack_blocking(&path, &game_dir, &cancel)).await
}

/// What to put into an exported `.mrpack`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MrpackExport {
    pub name: String,
    pub version_id: String,
    pub summary: Option<String>,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    /// Modrinth-hosted files (paths relative to the game directory): listed in the index,
    /// not bundled. See [`identify_hosted_files`].
    pub files: Vec<MrFile>,
    /// Files or folders (relative to the game directory) bundled under `overrides/`. Paths
    /// that are also in `files` are not bundled twice.
    pub overrides: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MrpackWriteSummary {
    pub path: PathBuf,
    pub index_files: u64,
    pub override_files: u64,
    pub override_bytes: u64,
    /// Requested override paths that did not exist or had unsafe names.
    pub skipped: Vec<String>,
}

impl MrpackExport {
    fn index(&self) -> Result<MrIndex> {
        if self.mc_version.trim().is_empty() {
            return Err(Error::Invalid(
                "mrpack export needs a Minecraft version".into(),
            ));
        }
        let mut dependencies = BTreeMap::new();
        dependencies.insert("minecraft".to_owned(), self.mc_version.clone());
        if let Some(key) = self.loader.mrpack_dependency_key() {
            let version = self
                .loader_version
                .clone()
                .filter(|v| !v.trim().is_empty())
                .ok_or_else(|| {
                    Error::Invalid(format!(
                        "{} needs a loader version",
                        self.loader.display_name()
                    ))
                })?;
            dependencies.insert(key.to_owned(), version);
        }
        let index = MrIndex {
            format_version: 1,
            game: "minecraft".into(),
            version_id: self.version_id.clone(),
            name: self.name.clone(),
            summary: self.summary.clone().filter(|s| !s.trim().is_empty()),
            files: self.files.clone(),
            dependencies,
        };
        index.validate()?;
        Ok(index)
    }
}

/// Collect `(absolute path, archive-relative name)` for override files.
fn collect_overrides(
    game_dir: &Path,
    overrides: &[String],
    hosted: &HashSet<String>,
    skipped: &mut Vec<String>,
) -> Vec<(PathBuf, String)> {
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for requested in overrides {
        let requested = requested.trim_matches('/');
        let Ok(rel) = validate_relative_path(requested) else {
            skipped.push(requested.to_owned());
            continue;
        };
        let full = game_dir.join(rel);
        let Ok(meta) = std::fs::symlink_metadata(&full) else {
            skipped.push(requested.to_owned());
            continue;
        };
        let mut push = |path: PathBuf, name: String| {
            let base = name.rsplit('/').next().unwrap_or(&name);
            if JUNK_FILES.contains(&base) || hosted.contains(&name) || !seen.insert(name.clone()) {
                return;
            }
            out.push((path, name));
        };
        if meta.is_file() {
            push(full, requested.to_owned());
        } else if meta.is_dir() {
            for entry in walkdir::WalkDir::new(&full).sort_by_file_name() {
                let Ok(entry) = entry else { continue };
                if !entry.file_type().is_file() {
                    continue;
                }
                let Ok(rel) = entry.path().strip_prefix(game_dir) else {
                    continue;
                };
                let name = rel
                    .components()
                    .map(|c| c.as_os_str().to_string_lossy())
                    .collect::<Vec<_>>()
                    .join("/");
                if validate_relative_path(&name).is_err() {
                    skipped.push(name);
                    continue;
                }
                push(entry.path().to_path_buf(), name);
            }
        }
    }
    out
}

pub(crate) fn write_mrpack_blocking(
    game_dir: &Path,
    export: &MrpackExport,
    dest: &Path,
    cancel: &CancellationToken,
) -> Result<MrpackWriteSummary> {
    let index = export.index()?;
    let hosted: HashSet<String> = index.files.iter().map(|f| f.path.clone()).collect();
    let mut summary = MrpackWriteSummary {
        path: dest.to_path_buf(),
        index_files: index.files.len() as u64,
        override_files: 0,
        override_bytes: 0,
        skipped: Vec::new(),
    };
    let overrides = collect_overrides(game_dir, &export.overrides, &hosted, &mut summary.skipped);

    if let Some(parent) = dest.parent() {
        std::fs::create_dir_all(parent).map_err(|e| Error::io(parent, e))?;
    }
    let tmp = sibling_with_suffix(dest, ".part");
    if let Err(error) = write_mrpack_zip(&tmp, &index, &overrides, cancel, &mut summary) {
        let _ = std::fs::remove_file(&tmp);
        return Err(error);
    }
    std::fs::rename(&tmp, dest).map_err(|e| Error::io(dest, e))?;
    Ok(summary)
}

fn write_mrpack_zip(
    tmp: &Path,
    index: &MrIndex,
    overrides: &[(PathBuf, String)],
    cancel: &CancellationToken,
    summary: &mut MrpackWriteSummary,
) -> Result<()> {
    let file = File::create(tmp).map_err(|e| Error::io(tmp, e))?;
    let mut zip = zip::ZipWriter::new(BufWriter::new(file));
    let options = SimpleFileOptions::default().compression_method(zip::CompressionMethod::Deflated);
    zip.start_file(MRPACK_INDEX, options)
        .map_err(|e| Error::archive(tmp, e))?;
    zip.write_all(index.to_json()?.as_bytes())
        .map_err(|e| Error::io(tmp, e))?;
    for (path, name) in overrides {
        if cancel.is_cancelled() {
            return Err(Error::Cancelled);
        }
        let mut source = File::open(path).map_err(|e| Error::io(path, e))?;
        let size = source.metadata().map(|m| m.len()).unwrap_or(0);
        zip.start_file(
            format!("overrides/{name}"),
            options.large_file(size >= 0xffff_ffff),
        )
        .map_err(|e| Error::archive(tmp, e))?;
        summary.override_bytes +=
            std::io::copy(&mut source, &mut zip).map_err(|e| Error::io(path, e))?;
        summary.override_files += 1;
    }
    let mut writer = zip.finish().map_err(|e| Error::archive(tmp, e))?;
    writer.flush().map_err(|e| Error::io(tmp, e))
}

/// Write an `.mrpack` for `game_dir` to `dest` (temporary file + rename; an existing `dest` is
/// replaced, as confirmed by the save dialog).
pub async fn write_mrpack(
    game_dir: &Path,
    export: &MrpackExport,
    dest: &Path,
    cancel: CancellationToken,
) -> Result<MrpackWriteSummary> {
    let (game_dir, export, dest) = (game_dir.to_path_buf(), export.clone(), dest.to_path_buf());
    blocking(move || write_mrpack_blocking(&game_dir, &export, &dest, &cancel)).await
}

/// Result of [`identify_hosted_files`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MrpackHostedFiles {
    /// Files Modrinth hosts, ready for [`MrpackExport::files`].
    pub files: Vec<MrFile>,
    /// Paths Modrinth does not know; bundle them as overrides.
    pub unmatched: Vec<String>,
}

/// Look up local files (paths relative to `game_dir`, e.g. everything in `mods/`) on Modrinth by
/// SHA-1 and turn the known ones into index entries with download URLs.
pub async fn identify_hosted_files(
    client: &ModrinthClient,
    game_dir: &Path,
    candidates: &[String],
) -> Result<MrpackHostedFiles> {
    let mut hashed: Vec<(String, String, u64)> = Vec::new();
    let mut result = MrpackHostedFiles::default();
    for rel in candidates {
        let Ok(rel_path) = validate_relative_path(rel) else {
            result.unmatched.push(rel.clone());
            continue;
        };
        let full = game_dir.join(rel_path);
        let size = match tokio::fs::metadata(&full).await {
            Ok(meta) if meta.is_file() => meta.len(),
            _ => {
                result.unmatched.push(rel.clone());
                continue;
            }
        };
        let sha1 = pl_net::hash_file(&full, HashAlgo::Sha1).await?;
        hashed.push((rel.clone(), sha1, size));
    }
    let sha1s: Vec<String> = hashed.iter().map(|(_, h, _)| h.clone()).collect();
    let found = client
        .version_files(&sha1s, ModrinthHashAlgorithm::Sha1)
        .await?;
    for (rel, sha1, size) in hashed {
        let remote = found.get(&sha1).and_then(|version| {
            version
                .files
                .iter()
                .find(|f| f.hashes.sha1.as_deref() == Some(sha1.as_str()))
        });
        match remote {
            Some(remote) => {
                let sha512 = match remote.hashes.sha512.clone() {
                    Some(hash) => hash,
                    None => pl_net::hash_file(&game_dir.join(&rel), HashAlgo::Sha512).await?,
                };
                result.files.push(MrFile {
                    path: rel,
                    hashes: MrHashes {
                        sha1,
                        sha512: Some(sha512),
                    },
                    env: None,
                    downloads: vec![remote.url.clone()],
                    file_size: size,
                });
            }
            None => result.unmatched.push(rel),
        }
    }
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::io::{Cursor, Read};

    use super::*;
    use crate::mods::test_support::zip_bytes;

    const SHA1: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";

    fn index_json(path: &str) -> String {
        serde_json::json!({
            "formatVersion": 1, "game": "minecraft", "versionId": "1.2.0", "name": "Test Pack",
            "summary": "A pack",
            "files": [
                {"path": path, "hashes": {"sha1": SHA1, "sha512": "abc"},
                 "env": {"client": "required", "server": "required"},
                 "downloads": ["https://cdn.modrinth.com/data/x/versions/y/mod.jar"], "fileSize": 3},
                {"path": "mods/server-only.jar", "hashes": {"sha1": SHA1},
                 "env": {"client": "unsupported", "server": "required"},
                 "downloads": ["https://cdn.modrinth.com/s.jar"], "fileSize": 3},
                {"path": "mods/optional.jar", "hashes": {"sha1": SHA1},
                 "env": {"client": "optional", "server": "optional"},
                 "downloads": ["https://cdn.modrinth.com/o.jar"], "fileSize": 3}
            ],
            "dependencies": {"minecraft": "1.20.1", "fabric-loader": "0.15.11"}
        })
        .to_string()
    }

    #[test]
    fn index_parsing_and_loader() {
        let index = MrIndex::parse(&index_json("mods/a.jar")).unwrap();
        assert_eq!(index.minecraft_version(), Some("1.20.1"));
        assert_eq!(index.loader(), (LoaderKind::Fabric, Some("0.15.11".into())));
        assert_eq!(index.files[1].client_support(), MrEnvSupport::Unsupported);
        assert_eq!(index.files[0].hashes.sha512.as_deref(), Some("abc"));
    }

    #[test]
    fn unsafe_index_paths_reject_the_pack() {
        for bad in [
            "../evil.jar",
            "/abs/evil.jar",
            "C:/evil.jar",
            "mods\\..\\evil.jar",
            "mods/aux.jar",
            "mods/../../evil.jar",
        ] {
            assert!(MrIndex::parse(&index_json(bad)).is_err(), "{bad} accepted");
        }
        let mut wrong = serde_json::from_str::<Value>(&index_json("mods/a.jar")).unwrap();
        wrong["formatVersion"] = 2.into();
        assert!(matches!(
            MrIndex::parse(&wrong.to_string()),
            Err(Error::Unsupported(_))
        ));
    }

    #[test]
    fn download_items_respect_env_and_prefer_sha512() {
        let index = MrIndex::parse(&index_json("mods/a.jar")).unwrap();
        let game = Path::new("/game");
        let items = mrpack_download_items(game, &index.files, false).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].dest, game.join("mods").join("a.jar"));
        assert_eq!(items[0].checksum.as_ref().unwrap().algo, HashAlgo::Sha512);
        assert_eq!(items[0].size, Some(3));
        let with_optional = mrpack_download_items(game, &index.files, true).unwrap();
        assert_eq!(with_optional.len(), 2);
        assert_eq!(
            with_optional[1].checksum.as_ref().unwrap().algo,
            HashAlgo::Sha1
        );
    }

    #[tokio::test]
    async fn import_extracts_overrides_in_order() {
        let dir = tempfile::tempdir().unwrap();
        let pack = dir.path().join("pack.mrpack");
        std::fs::write(
            &pack,
            zip_bytes(&[
                (MRPACK_INDEX, index_json("mods/a.jar").as_bytes()),
                ("overrides/config/a.cfg", b"common"),
                ("overrides/options.txt", b"opts"),
                ("client-overrides/config/a.cfg", b"client"),
                ("server-overrides/server.properties", b"server"),
                ("overrides/../../escape.txt", b"x"),
            ]),
        )
        .unwrap();
        let game = dir.path().join("game");
        let instance = import_mrpack(&pack, &game, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(instance.format, ModpackFormat::Mrpack);
        assert_eq!(instance.name, "Test Pack");
        assert_eq!(instance.mc_version, "1.20.1");
        assert_eq!(instance.loader, LoaderKind::Fabric);
        assert_eq!(instance.pack_version.as_deref(), Some("1.2.0"));
        assert_eq!(
            std::fs::read(game.join("config").join("a.cfg")).unwrap(),
            b"client"
        );
        assert!(game.join("options.txt").exists());
        assert!(!game.join("server.properties").exists());
        assert_eq!(instance.extracted_files, 3);
        assert_eq!(instance.skipped_entries, vec!["overrides/../../escape.txt"]);
        match instance.pending {
            ImportPendingFiles::Modrinth { files } => assert_eq!(files.len(), 2),
            other => panic!("unexpected pending {other:?}"),
        }
        let preview = read_mrpack_index(&pack).await.unwrap();
        assert_eq!(preview.files.len(), 3);
    }

    #[tokio::test]
    async fn export_round_trips_and_skips_hosted_files() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("game");
        std::fs::create_dir_all(game.join("mods")).unwrap();
        std::fs::create_dir_all(game.join("config").join("sub")).unwrap();
        std::fs::write(game.join("mods").join("hosted.jar"), b"abc").unwrap();
        std::fs::write(game.join("mods").join("custom.jar"), b"custom").unwrap();
        std::fs::write(game.join("config").join("sub").join("x.toml"), b"x=1").unwrap();
        std::fs::write(game.join("config").join(".DS_Store"), b"junk").unwrap();
        std::fs::write(game.join("options.txt"), b"fov:1").unwrap();

        let export = MrpackExport {
            name: "Projects".into(),
            version_id: "2.0.0".into(),
            summary: None,
            mc_version: "1.21.1".into(),
            loader: LoaderKind::NeoForge,
            loader_version: Some("21.1.77".into()),
            files: vec![MrFile {
                path: "mods/hosted.jar".into(),
                hashes: MrHashes {
                    sha1: SHA1.into(),
                    sha512: Some("ddaf".into()),
                },
                env: None,
                downloads: vec!["https://cdn.modrinth.com/hosted.jar".into()],
                file_size: 3,
            }],
            overrides: vec![
                "mods".into(),
                "config/".into(),
                "options.txt".into(),
                "missing".into(),
                "../outside".into(),
            ],
        };
        let dest = dir.path().join("out").join("Projects.mrpack");
        let summary = write_mrpack(&game, &export, &dest, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(summary.index_files, 1);
        assert_eq!(summary.override_files, 3); // custom.jar, x.toml, options.txt
        assert_eq!(summary.skipped, vec!["missing", "../outside"]);

        let mut archive = ZipArchive::new(Cursor::new(std::fs::read(&dest).unwrap())).unwrap();
        let mut index_text = String::new();
        archive
            .by_name(MRPACK_INDEX)
            .unwrap()
            .read_to_string(&mut index_text)
            .unwrap();
        assert!(
            !index_text.contains("null"),
            "nulls must be stripped: {index_text}"
        );
        let index = MrIndex::parse(&index_text).unwrap();
        assert_eq!(
            index.loader(),
            (LoaderKind::NeoForge, Some("21.1.77".into()))
        );
        let names: Vec<String> = archive
            .file_names()
            .map(|n| n.unwrap().into_owned())
            .collect();
        assert!(names.contains(&"overrides/mods/custom.jar".to_owned()));
        assert!(names.contains(&"overrides/config/sub/x.toml".to_owned()));
        assert!(
            !names
                .iter()
                .any(|n| n.contains("hosted.jar") || n.contains(".DS_Store"))
        );

        // Importing the export reproduces the bundled files.
        let restored = dir.path().join("restored");
        let instance = import_mrpack(&dest, &restored, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(instance.loader, LoaderKind::NeoForge);
        assert_eq!(
            std::fs::read(restored.join("options.txt")).unwrap(),
            b"fov:1"
        );

        let mut no_loader_version = export.clone();
        no_loader_version.loader_version = None;
        assert!(
            write_mrpack(&game, &no_loader_version, &dest, CancellationToken::new())
                .await
                .is_err()
        );
    }
}
