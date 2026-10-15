//! CurseForge modpack zips: `manifest.json` + overrides folder (+ `modlist.html`).
//!
//! The mod files themselves are only referenced by CurseForge project/file ids. Downloading them
//! needs the CurseForge API with an API key, which this launcher does not ship, so they are
//! reported as [`ImportPendingFiles::CurseForgeNeedsApiKey`] instead of being guessed.

use std::fs::File;
use std::io::{BufReader, Read, Seek};
use std::path::Path;
use std::sync::LazyLock;

use pl_net::CancellationToken;
use regex::Regex;
use serde::{Deserialize, Serialize};
use zip::ZipArchive;

use super::extract::{ExtractReport, entry_names, extract_prefix, read_entry_string};
use super::prism::find_root;
use super::safe_path::validate_relative_path;
use super::{ImportPendingFiles, ImportedInstance, ModpackFormat};
use crate::error::{Error, Result};
use crate::loaders::LoaderKind;

const MAX_MANIFEST_BYTES: u64 = 16 * 1024 * 1024;

/// A mod file referenced by a CurseForge manifest.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeFileRef {
    pub project_id: u64,
    pub file_id: u64,
    pub required: bool,
}

/// A mod name and page from `modlist.html`, to show the user what is missing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeModLink {
    pub name: String,
    pub url: String,
}

/// The parts of `manifest.json` the launcher uses.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CurseForgeManifest {
    pub name: String,
    pub version: Option<String>,
    pub author: Option<String>,
    pub mc_version: String,
    pub loader: LoaderKind,
    pub loader_version: Option<String>,
    pub files: Vec<CurseForgeFileRef>,
    /// Overrides folder inside the zip (usually `overrides`).
    pub overrides: String,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawManifest {
    minecraft: RawMinecraft,
    manifest_type: Option<String>,
    name: Option<String>,
    version: Option<String>,
    author: Option<String>,
    #[serde(default)]
    files: Vec<RawFile>,
    overrides: Option<String>,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawMinecraft {
    version: String,
    #[serde(default)]
    mod_loaders: Vec<RawLoader>,
}

#[derive(Deserialize)]
struct RawLoader {
    id: String,
    #[serde(default)]
    primary: bool,
}

#[derive(Deserialize)]
struct RawFile {
    #[serde(rename = "projectID")]
    project_id: u64,
    #[serde(rename = "fileID")]
    file_id: u64,
    #[serde(default = "default_required")]
    required: bool,
}

fn default_required() -> bool {
    true
}

/// `forge-47.2.0` / `neoforge-20.4.80` / `fabric-0.15.0` / `quilt-0.20.0` → loader + version.
/// CurseForge sometimes prefixes the Minecraft version (`neoforge-1.20.1-47.1.84`); that prefix
/// is dropped so versions read like Prism's and Modrinth's.
pub(crate) fn parse_loader_id(id: &str, mc_version: &str) -> Option<(LoaderKind, String)> {
    let (kind, version) = id.trim().split_once('-')?;
    let kind = match kind.to_ascii_lowercase().as_str() {
        "forge" => LoaderKind::Forge,
        "neoforge" => LoaderKind::NeoForge,
        "fabric" => LoaderKind::Fabric,
        "quilt" => LoaderKind::Quilt,
        _ => return None,
    };
    let version = version
        .strip_prefix(&format!("{mc_version}-"))
        .unwrap_or(version)
        .to_owned();
    (!version.is_empty()).then_some((kind, version))
}

/// Parse a CurseForge `manifest.json`.
pub fn parse_curseforge_manifest(text: &str) -> Result<CurseForgeManifest> {
    let raw: RawManifest =
        serde_json::from_str(text).map_err(|e| Error::json("CurseForge manifest.json", e))?;
    if raw
        .manifest_type
        .as_deref()
        .is_some_and(|t| t != "minecraftModpack")
    {
        return Err(Error::Unsupported(format!(
            "CurseForge manifest type {:?}",
            raw.manifest_type.unwrap_or_default()
        )));
    }
    let mc_version = raw.minecraft.version.trim().to_owned();
    if mc_version.is_empty() {
        return Err(Error::Invalid(
            "CurseForge manifest has no Minecraft version".into(),
        ));
    }
    let loader = raw
        .minecraft
        .mod_loaders
        .iter()
        .find(|l| l.primary)
        .or_else(|| raw.minecraft.mod_loaders.first())
        .map(|l| {
            parse_loader_id(&l.id, &mc_version)
                .ok_or_else(|| Error::Unsupported(format!("CurseForge mod loader {:?}", l.id)))
        })
        .transpose()?;
    let (loader, loader_version) = match loader {
        Some((kind, version)) => (kind, Some(version)),
        None => (LoaderKind::Vanilla, None),
    };
    Ok(CurseForgeManifest {
        name: raw
            .name
            .filter(|n| !n.trim().is_empty())
            .unwrap_or_else(|| "CurseForge pack".into()),
        version: raw.version.filter(|v| !v.trim().is_empty()),
        author: raw.author.filter(|a| !a.trim().is_empty()),
        mc_version,
        loader,
        loader_version,
        files: raw
            .files
            .into_iter()
            .map(|f| CurseForgeFileRef {
                project_id: f.project_id,
                file_id: f.file_id,
                required: f.required,
            })
            .collect(),
        overrides: raw
            .overrides
            .map(|o| o.trim_matches('/').to_owned())
            .filter(|o| !o.is_empty())
            .unwrap_or_else(|| "overrides".into()),
    })
}

static LINK: LazyLock<Option<Regex>> =
    LazyLock::new(|| Regex::new(r#"(?is)<a\s[^>]*href\s*=\s*"([^"]+)"[^>]*>(.*?)</a>"#).ok());

fn decode_entities(text: &str) -> String {
    text.replace("&lt;", "<")
        .replace("&gt;", ">")
        .replace("&quot;", "\"")
        .replace("&#39;", "'")
        .replace("&#039;", "'")
        .replace("&amp;", "&")
}

/// Links from `modlist.html` (`<li><a href="…">Name (by Author)</a></li>`).
pub(crate) fn parse_modlist(html: &str) -> Vec<CurseForgeModLink> {
    let Some(link) = LINK.as_ref() else {
        return Vec::new();
    };
    link.captures_iter(html)
        .filter_map(|caps| {
            let url = decode_entities(caps.get(1)?.as_str().trim());
            let name = decode_entities(caps.get(2)?.as_str().trim());
            let url_ok = url.starts_with("https://") || url.starts_with("http://");
            (url_ok && !name.is_empty()).then_some(CurseForgeModLink { name, url })
        })
        .collect()
}

pub(crate) fn import_curseforge_archive<R: Read + Seek>(
    archive: &mut ZipArchive<R>,
    archive_path: &Path,
    game_dir: &Path,
    cancel: &CancellationToken,
) -> Result<ImportedInstance> {
    let names = entry_names(archive);
    let root = find_root(&names, "manifest.json")
        .ok_or_else(|| Error::archive(archive_path, "manifest.json not found"))?;
    let text = read_entry_string(archive, &format!("{root}manifest.json"), MAX_MANIFEST_BYTES)
        .ok_or_else(|| Error::archive(archive_path, "manifest.json is unreadable"))?;
    let manifest = parse_curseforge_manifest(&text)?;
    validate_relative_path(&manifest.overrides)?;

    let mut report = ExtractReport::default();
    let prefix = format!("{root}{}/", manifest.overrides);
    extract_prefix(
        archive,
        archive_path,
        &prefix,
        game_dir,
        cancel,
        &mut report,
    )?;
    let links = read_entry_string(archive, &format!("{root}modlist.html"), MAX_MANIFEST_BYTES)
        .map(|html| parse_modlist(&html))
        .unwrap_or_default();

    let mut instance = ImportedInstance::new(
        ModpackFormat::CurseForge,
        manifest.name.clone(),
        manifest.mc_version.clone(),
    );
    instance.pack_version = manifest.version.clone();
    instance.author = manifest.author.clone();
    instance.loader = manifest.loader;
    instance.loader_version = manifest.loader_version.clone();
    instance.apply_report(report);
    instance.pending = if manifest.files.is_empty() {
        ImportPendingFiles::None
    } else {
        ImportPendingFiles::CurseForgeNeedsApiKey {
            files: manifest.files,
            links,
        }
    };
    Ok(instance)
}

pub(crate) fn import_curseforge_blocking(
    path: &Path,
    game_dir: &Path,
    cancel: &CancellationToken,
) -> Result<ImportedInstance> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let mut archive = ZipArchive::new(BufReader::new(file)).map_err(|e| Error::archive(path, e))?;
    import_curseforge_archive(&mut archive, path, game_dir, cancel)
}

/// Import a CurseForge modpack zip: extract the overrides into `game_dir` and report the mod
/// files that still need downloading (which requires a CurseForge API key).
pub async fn import_curseforge(
    path: &Path,
    game_dir: &Path,
    cancel: CancellationToken,
) -> Result<ImportedInstance> {
    let (path, game_dir) = (path.to_path_buf(), game_dir.to_path_buf());
    crate::gamefiles::fsops::blocking(move || import_curseforge_blocking(&path, &game_dir, &cancel))
        .await
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::test_support::zip_bytes;

    const MANIFEST: &str = r#"{
        "minecraft": {
            "version": "1.20.1",
            "modLoaders": [{"id": "neoforge-1.20.1-47.1.84", "primary": false}, {"id": "forge-47.2.0", "primary": true}]
        },
        "manifestType": "minecraftModpack",
        "manifestVersion": 1,
        "name": "All the Mods",
        "version": "1.0.5",
        "author": "ATM Team",
        "files": [
            {"projectID": 238222, "fileID": 4712866, "required": true},
            {"projectID": 306612, "fileID": 4614152, "required": false},
            {"projectID": 1, "fileID": 2}
        ],
        "overrides": "overrides"
    }"#;

    #[test]
    fn manifest_parsing() {
        let manifest = parse_curseforge_manifest(MANIFEST).unwrap();
        assert_eq!(manifest.mc_version, "1.20.1");
        assert_eq!(manifest.loader, LoaderKind::Forge);
        assert_eq!(manifest.loader_version.as_deref(), Some("47.2.0"));
        assert_eq!(manifest.files.len(), 3);
        assert!(!manifest.files[1].required);
        assert!(manifest.files[2].required);
        assert_eq!(manifest.author.as_deref(), Some("ATM Team"));
    }

    #[test]
    fn loader_ids() {
        assert_eq!(
            parse_loader_id("neoforge-1.20.1-47.1.84", "1.20.1"),
            Some((LoaderKind::NeoForge, "47.1.84".into()))
        );
        assert_eq!(
            parse_loader_id("neoforge-21.1.77", "1.21.1"),
            Some((LoaderKind::NeoForge, "21.1.77".into()))
        );
        assert_eq!(
            parse_loader_id("fabric-0.15.11", "1.20.1"),
            Some((LoaderKind::Fabric, "0.15.11".into()))
        );
        assert_eq!(
            parse_loader_id("quilt-0.26.0", "1.20.1"),
            Some((LoaderKind::Quilt, "0.26.0".into()))
        );
        assert_eq!(parse_loader_id("rift-1.0", "1.13"), None);
        assert_eq!(parse_loader_id("forge", "1.13"), None);
        let vanilla = parse_curseforge_manifest(r#"{"minecraft": {"version": "1.21.1"}}"#).unwrap();
        assert_eq!(vanilla.loader, LoaderKind::Vanilla);
        assert!(
            parse_curseforge_manifest(
                r#"{"minecraft": {"version": "1.21", "modLoaders": [{"id": "rift-1"}]}}"#
            )
            .is_err()
        );
        assert!(parse_curseforge_manifest("{}").is_err());
    }

    #[test]
    fn modlist_links() {
        let html = r#"<ul>
<li><a href="https://www.curseforge.com/minecraft/mc-mods/jei">Just Enough Items (JEI) (by mezz)</a></li>
<li><a href="https://www.curseforge.com/minecraft/mc-mods/x">Tom &amp; Jerry (by A)</a></li>
<li><a href="javascript:alert(1)">bad</a></li>
</ul>"#;
        let links = parse_modlist(html);
        assert_eq!(links.len(), 2);
        assert_eq!(links[0].name, "Just Enough Items (JEI) (by mezz)");
        assert_eq!(links[1].name, "Tom & Jerry (by A)");
    }

    #[tokio::test]
    async fn import_extracts_overrides_and_reports_files() {
        let dir = tempfile::tempdir().unwrap();
        let zip = dir.path().join("cf.zip");
        std::fs::write(
            &zip,
            zip_bytes(&[
                ("manifest.json", MANIFEST.as_bytes()),
                (
                    "modlist.html",
                    br#"<a href="https://www.curseforge.com/minecraft/mc-mods/jei">JEI</a>"#,
                ),
                ("overrides/config/jei.toml", b"x"),
                ("overrides/mods/bundled.jar", b"jar"),
                ("overrides/../../evil", b"x"),
            ]),
        )
        .unwrap();
        let game = dir.path().join("game");
        let instance = import_curseforge(&zip, &game, CancellationToken::new())
            .await
            .unwrap();
        assert_eq!(instance.format, ModpackFormat::CurseForge);
        assert_eq!(instance.name, "All the Mods");
        assert_eq!(instance.extracted_files, 2);
        assert_eq!(instance.skipped_entries.len(), 1);
        assert!(game.join("mods").join("bundled.jar").exists());
        match instance.pending {
            ImportPendingFiles::CurseForgeNeedsApiKey { files, links } => {
                assert_eq!(files.len(), 3);
                assert_eq!(links.len(), 1);
            }
            other => panic!("unexpected {other:?}"),
        }
    }
}
