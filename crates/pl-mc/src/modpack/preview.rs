//! Pack previews: what an import would create, read without extracting anything.

use std::fs::File;
use std::io::BufReader;
use std::path::Path;

use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use super::curseforge::describe_curseforge_archive;
use super::extract::entry_names;
use super::mrpack::{MrEnvSupport, MrFile, read_index};
use super::prism::describe_prism_archive;
use super::{ImportPendingFiles, ImportedInstance, ModpackFormat, detect_in};
use crate::error::{Error, Result};
use crate::gamefiles::fsops::blocking;
use crate::loaders::LoaderKind;

/// What importing a pack archive would create.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackPreview {
    pub format: ModpackFormat,
    pub name: String,
    pub pack_version: Option<String>,
    pub summary: Option<String>,
    pub author: Option<String>,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    /// Files downloaded during the import (Modrinth-hosted files of an `.mrpack`).
    pub download_files: u64,
    pub download_bytes: u64,
    /// Files bundled in the archive (overrides or the exported game folder).
    pub bundled_files: u64,
    /// Mods of a CurseForge pack that have to be downloaded by hand.
    pub manual_files: u64,
    /// Parts of the source instance the launcher cannot reproduce.
    pub unsupported: Vec<String>,
    /// Files of an `.mrpack` the pack marks optional on the client (counted in the downloads);
    /// the player may leave them out.
    pub optional: Vec<PackOptionalFile>,
}

/// A file of an `.mrpack` the pack marks optional on the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackOptionalFile {
    /// Where it goes in the game folder (`mods/x.jar`): what an import names to leave it out.
    pub path: String,
    pub file_name: String,
    /// `mods`, `resourcepacks`, `shaderpacks` or another folder.
    pub folder: String,
    pub size: u64,
    /// Its Modrinth project (from the download link), for a name and an icon.
    pub project_id: Option<String>,
}

impl PackOptionalFile {
    pub fn of(file: &MrFile) -> Self {
        let (folder, file_name) = match file.path.rsplit_once('/') {
            Some((folder, name)) => (folder.to_owned(), name.to_owned()),
            None => (String::new(), file.path.clone()),
        };
        Self {
            path: file.path.clone(),
            file_name,
            folder,
            size: file.file_size,
            project_id: file.modrinth_project(),
        }
    }
}

impl PackPreview {
    /// Files the new instance ends up with (bundled plus downloaded).
    pub fn total_files(&self) -> u64 {
        self.bundled_files + self.download_files
    }
}

fn count_below(names: &[String], prefixes: &[&str]) -> u64 {
    names
        .iter()
        .filter(|name| !name.ends_with('/'))
        .filter(|name| {
            prefixes
                .iter()
                .any(|p| name.len() > p.len() && name.starts_with(p))
        })
        .count() as u64
}

fn from_described(instance: ImportedInstance, bundled_files: u64) -> PackPreview {
    let manual_files = match &instance.pending {
        ImportPendingFiles::CurseForgeNeedsApiKey { files, .. } => files.len() as u64,
        _ => 0,
    };
    PackPreview {
        format: instance.format,
        name: instance.name,
        pack_version: instance.pack_version,
        summary: instance.summary,
        author: instance.author,
        mc_version: instance.mc_version,
        loader: instance.loader,
        loader_version: instance.loader_version,
        download_files: 0,
        download_bytes: 0,
        bundled_files,
        manual_files,
        unsupported: instance.unsupported,
        optional: Vec::new(),
    }
}

fn preview_blocking(path: &Path) -> Result<PackPreview> {
    let not_a_pack = || {
        Error::Unsupported(format!(
            "{} is not a Modrinth, Prism/MultiMC or CurseForge pack",
            path.display()
        ))
    };
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|_| not_a_pack())?;
    let format = detect_in(&mut archive).ok_or_else(not_a_pack)?;
    let names = entry_names(&archive);
    match format {
        ModpackFormat::Mrpack => {
            let index = read_index(&mut archive, path)?;
            let (loader, loader_version) = index.loader();
            let downloads: Vec<_> = index
                .files
                .iter()
                .filter(|f| f.client_support() != MrEnvSupport::Unsupported)
                .collect();
            Ok(PackPreview {
                format,
                name: index.name.clone(),
                pack_version: Some(index.version_id.clone()).filter(|v| !v.trim().is_empty()),
                summary: index.summary.clone().filter(|s| !s.trim().is_empty()),
                author: None,
                mc_version: index.minecraft_version().unwrap_or_default().to_owned(),
                loader,
                loader_version,
                download_files: downloads.len() as u64,
                download_bytes: downloads.iter().map(|f| f.file_size).sum(),
                bundled_files: count_below(&names, &["overrides/", "client-overrides/"]),
                manual_files: 0,
                unsupported: Vec::new(),
                optional: downloads
                    .iter()
                    .filter(|f| f.client_support() == MrEnvSupport::Optional)
                    .map(|f| PackOptionalFile::of(f))
                    .collect(),
            })
        }
        ModpackFormat::Prism => {
            let (instance, prefix) = describe_prism_archive(&mut archive, path)?;
            let bundled = count_below(&names, &[prefix.as_str()]);
            Ok(from_described(instance, bundled))
        }
        ModpackFormat::CurseForge => {
            let (instance, prefix) = describe_curseforge_archive(&mut archive, path)?;
            let bundled = count_below(&names, &[prefix.as_str()]);
            Ok(from_described(instance, bundled))
        }
    }
}

/// Describe a pack archive (format detected from its contents) without extracting it.
/// Archives that are not packs fail with [`Error::Unsupported`].
pub async fn preview_pack(path: &Path) -> Result<PackPreview> {
    let path = path.to_path_buf();
    blocking(move || preview_blocking(&path)).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::modpack::MRPACK_INDEX;
    use crate::mods::test_support::zip_bytes;

    const SHA1: &str = "a9993e364706816aba3e25717850c26c9cd0d89d";

    #[tokio::test]
    async fn previews_every_format_without_extracting() {
        let dir = tempfile::tempdir().unwrap();

        let index = serde_json::json!({
            "formatVersion": 1, "game": "minecraft", "versionId": "6.4.0", "name": "Optimized",
            "summary": "Fast",
            "files": [
                {"path": "mods/a.jar", "hashes": {"sha1": SHA1}, "downloads": ["https://cdn.modrinth.com/a.jar"], "fileSize": 100},
                {"path": "mods/s.jar", "hashes": {"sha1": SHA1}, "env": {"client": "unsupported", "server": "required"}, "downloads": ["https://cdn.modrinth.com/s.jar"], "fileSize": 7}
            ],
            "dependencies": {"minecraft": "1.21.1", "fabric-loader": "0.16.5"}
        })
        .to_string();
        let mrpack = dir.path().join("a.mrpack");
        std::fs::write(
            &mrpack,
            zip_bytes(&[
                (MRPACK_INDEX, index.as_bytes()),
                ("overrides/config/a.toml", b"a"),
                ("overrides/options.txt", b"o"),
                ("client-overrides/config/b.toml", b"b"),
                ("overrides/", b""),
            ]),
        )
        .unwrap();
        let preview = preview_pack(&mrpack).await.unwrap();
        assert_eq!(preview.format, ModpackFormat::Mrpack);
        assert_eq!(preview.name, "Optimized");
        assert_eq!(preview.pack_version.as_deref(), Some("6.4.0"));
        assert_eq!(preview.mc_version, "1.21.1");
        assert_eq!(preview.loader, LoaderKind::Fabric);
        assert_eq!(preview.loader_version.as_deref(), Some("0.16.5"));
        assert_eq!((preview.download_files, preview.download_bytes), (1, 100));
        assert_eq!(preview.bundled_files, 3);
        assert_eq!(preview.total_files(), 4);

        let prism = dir.path().join("b.zip");
        std::fs::write(
            &prism,
            zip_bytes(&[
                ("Inst/instance.cfg", b"name=My Pack\nInstanceType=OneSix"),
                (
                    "Inst/mmc-pack.json",
                    br#"{"components":[{"uid":"net.minecraft","version":"1.20.1"},{"uid":"net.minecraftforge","version":"47.2.0"}]}"#,
                ),
                ("Inst/.minecraft/mods/x.jar", b"x"),
                ("Inst/.minecraft/config/y.cfg", b"y"),
            ]),
        )
        .unwrap();
        let preview = preview_pack(&prism).await.unwrap();
        assert_eq!(preview.format, ModpackFormat::Prism);
        assert_eq!(preview.name, "My Pack");
        assert_eq!(preview.loader, LoaderKind::Forge);
        assert_eq!(preview.bundled_files, 2);
        assert!(!dir.path().join("mods").exists());

        let curse = dir.path().join("c.zip");
        std::fs::write(
            &curse,
            zip_bytes(&[
                (
                    "manifest.json",
                    br#"{"minecraft":{"version":"1.20.1","modLoaders":[{"id":"forge-47.2.0","primary":true}]},
                        "manifestType":"minecraftModpack","name":"Curse","version":"1.0",
                        "files":[{"projectID":1,"fileID":2,"required":true},{"projectID":3,"fileID":4}],
                        "overrides":"overrides"}"#,
                ),
                ("overrides/config/z.cfg", b"z"),
            ]),
        )
        .unwrap();
        let preview = preview_pack(&curse).await.unwrap();
        assert_eq!(preview.format, ModpackFormat::CurseForge);
        assert_eq!(preview.manual_files, 2);
        assert_eq!(preview.bundled_files, 1);

        let junk = dir.path().join("d.zip");
        std::fs::write(&junk, b"not a zip").unwrap();
        assert!(matches!(
            preview_pack(&junk).await,
            Err(Error::Unsupported(_))
        ));
    }
}
