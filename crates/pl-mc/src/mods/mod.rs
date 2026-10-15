//! Mods of a game directory: metadata read from the jars (Fabric, Quilt, Forge, NeoForge and
//! legacy Forge descriptors, icons), enabling/disabling (`.jar` ↔ `.jar.disabled`) and removal.

pub(crate) mod lenient_json;
mod metadata;

pub(crate) use metadata::{read_entry_png as read_zip_png, read_entry_text as read_zip_text};

use std::io::{BufReader, Read, Seek};
use std::path::{Path, PathBuf};

use futures::StreamExt;
use serde::{Deserialize, Serialize};
use sha1::Digest;

use crate::error::{Error, Result};
use crate::gamefiles::fsops::{
    RemovalMethod, blocking, modified_ms, read_dir_or_empty, remove_path,
};
use crate::loaders::LoaderKind;

/// Metadata read from a mod's descriptor files. Every field is optional: broken descriptors
/// still yield whatever could be read.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModMetadata {
    pub id: Option<String>,
    pub name: Option<String>,
    pub version: Option<String>,
    pub description: Option<String>,
    pub authors: Vec<String>,
    /// Loaders the jar declares itself for (a multi-loader jar lists several).
    pub loaders: Vec<LoaderKind>,
    /// Icon PNG (at most 256 KB; larger icons are skipped).
    pub icon_png: Option<Vec<u8>>,
    pub homepage: Option<String>,
    pub sources: Option<String>,
    /// Required mod ids. Platform ids (`minecraft`, `java`, the loaders) are left out.
    pub depends: Vec<String>,
}

/// A mod file in a mods folder.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModInfo {
    /// File name including a `.disabled` suffix when disabled.
    pub file_name: String,
    pub path: PathBuf,
    pub enabled: bool,
    pub size: u64,
    pub modified_ms: Option<i64>,
    /// Lowercase hex SHA-1 (computed only when requested; used for Modrinth lookups).
    pub sha1: Option<String>,
    /// `None` when the file is not a readable jar or has no known descriptor.
    pub metadata: Option<ModMetadata>,
}

impl ModInfo {
    /// The name to show: the mod's own name, else the file name without extensions.
    pub fn display_name(&self) -> &str {
        self.metadata
            .as_ref()
            .and_then(|m| m.name.as_deref())
            .unwrap_or_else(|| {
                let name = self.file_name.as_str();
                let name = name.strip_suffix(".disabled").unwrap_or(name);
                name.rsplit_once('.').map_or(name, |(stem, _)| stem)
            })
    }
}

const DISABLED_SUFFIX: &str = ".disabled";
const MOD_EXTENSIONS: [&str; 3] = [".jar", ".zip", ".litemod"];

/// True for `*.jar`, `*.zip`, `*.litemod` and their `.disabled` variants.
pub fn is_mod_file_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    let base = lower.strip_suffix(DISABLED_SUFFIX).unwrap_or(&lower);
    MOD_EXTENSIONS.iter().any(|ext| base.ends_with(ext)) && !name.starts_with('.')
}

fn is_disabled_name(name: &str) -> bool {
    name.to_ascii_lowercase().ends_with(DISABLED_SUFFIX)
}

/// Read mod metadata from an in-memory or on-disk jar. Never panics; `None` for files that are
/// not zips or carry no known descriptor.
pub fn read_mod_metadata<R: Read + Seek>(reader: R) -> Option<ModMetadata> {
    let mut archive = zip::ZipArchive::new(reader).ok()?;
    metadata::read_metadata(&mut archive)
}

fn sha1_file(path: &Path) -> Result<String> {
    let mut file = std::fs::File::open(path).map_err(|e| Error::io(path, e))?;
    let mut hasher = sha1::Sha1::new();
    let mut buf = vec![0u8; 256 * 1024];
    loop {
        let n = file.read(&mut buf).map_err(|e| Error::io(path, e))?;
        if n == 0 {
            break;
        }
        hasher.update(&buf[..n]);
    }
    Ok(hex::encode(hasher.finalize()))
}

fn read_mod_blocking(path: &Path, with_hash: bool) -> Result<ModInfo> {
    let meta = std::fs::metadata(path).map_err(|e| Error::io(path, e))?;
    if !meta.is_file() {
        return Err(Error::Invalid(format!("{} is not a file", path.display())));
    }
    let file_name = path
        .file_name()
        .map(|n| n.to_string_lossy().into_owned())
        .unwrap_or_default();
    // Zip parsing of untrusted files: a panic inside the zip reader must not take the
    // listing down, so it degrades to "no metadata".
    let metadata = std::panic::catch_unwind(|| {
        let file = std::fs::File::open(path).ok()?;
        read_mod_metadata(BufReader::new(file))
    })
    .unwrap_or_else(|_| {
        tracing::warn!(path = %path.display(), "mod metadata reader panicked");
        None
    });
    let sha1 = if with_hash {
        match sha1_file(path) {
            Ok(hash) => Some(hash),
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "could not hash mod");
                None
            }
        }
    } else {
        None
    };
    Ok(ModInfo {
        enabled: !is_disabled_name(&file_name),
        file_name,
        path: path.to_path_buf(),
        size: meta.len(),
        modified_ms: modified_ms(&meta),
        sha1,
        metadata,
    })
}

/// Read one mod file.
pub async fn read_mod(path: &Path, with_hash: bool) -> Result<ModInfo> {
    let path = path.to_path_buf();
    blocking(move || read_mod_blocking(&path, with_hash)).await
}

/// List the mods in `dir` (missing folder = no mods), reading jars in parallel on the blocking
/// pool. Sorted by display name. Unreadable files are skipped, broken jars are listed without
/// metadata.
pub async fn list_mods(dir: &Path, with_hashes: bool) -> Result<Vec<ModInfo>> {
    let dir_owned = dir.to_path_buf();
    let paths: Vec<PathBuf> = blocking(move || {
        Ok(read_dir_or_empty(&dir_owned)?
            .into_iter()
            .filter(|entry| entry.file_type().is_ok_and(|t| t.is_file()))
            .filter(|entry| is_mod_file_name(&entry.file_name().to_string_lossy()))
            .map(|entry| entry.path())
            .collect())
    })
    .await?;

    let parallelism = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(4)
        .clamp(2, 16);
    let mut mods: Vec<ModInfo> = futures::stream::iter(paths)
        .map(|path| async move {
            let result = read_mod(&path, with_hashes).await;
            if let Err(error) = &result {
                tracing::debug!(path = %path.display(), %error, "skipping unreadable mod file");
            }
            result.ok()
        })
        .buffer_unordered(parallelism)
        .filter_map(|info| async move { info })
        .collect()
        .await;
    mods.sort_by_cached_key(|m| (m.display_name().to_lowercase(), m.file_name.clone()));
    Ok(mods)
}

/// Enable or disable a mod by renaming `x.jar` ↔ `x.jar.disabled`. Returns the new path (the
/// same path when it is already in the requested state). Never overwrites another file.
pub async fn set_enabled(path: &Path, enabled: bool) -> Result<PathBuf> {
    let path = path.to_path_buf();
    blocking(move || {
        let name = path
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .ok_or_else(|| Error::Invalid(format!("{} has no file name", path.display())))?;
        let disabled = is_disabled_name(&name);
        if disabled != enabled {
            std::fs::metadata(&path).map_err(|e| Error::io(&path, e))?;
            return Ok(path);
        }
        let target_name = if enabled {
            name[..name.len() - DISABLED_SUFFIX.len()].to_owned()
        } else {
            format!("{name}{DISABLED_SUFFIX}")
        };
        let target = path.with_file_name(target_name);
        if std::fs::symlink_metadata(&target).is_ok() {
            return Err(Error::AlreadyExists(target));
        }
        std::fs::rename(&path, &target).map_err(|e| Error::io(&path, e))?;
        Ok(target)
    })
    .await
}

/// Remove a mod: OS trash when available; permanent deletion only if `allow_permanent`.
pub async fn remove_mod(path: &Path, allow_permanent: bool) -> Result<RemovalMethod> {
    remove_path(path, allow_permanent).await
}

#[cfg(test)]
pub(crate) mod test_support {
    use std::io::{Cursor, Write};

    use zip::write::SimpleFileOptions;

    /// Build an in-memory zip from `(name, bytes)` pairs.
    pub fn zip_bytes(entries: &[(&str, &[u8])]) -> Vec<u8> {
        let mut writer = zip::ZipWriter::new(Cursor::new(Vec::new()));
        for (name, bytes) in entries {
            writer
                .start_file(*name, SimpleFileOptions::default())
                .unwrap();
            writer.write_all(bytes).unwrap();
        }
        writer.finish().unwrap().into_inner()
    }

    /// A tiny valid-looking PNG (signature + padding).
    pub fn png(len: usize) -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n".to_vec();
        bytes.resize(len.max(8), 0);
        bytes
    }
}

#[cfg(test)]
mod tests {
    use std::io::Cursor;

    use super::test_support::{png, zip_bytes};
    use super::*;

    #[test]
    fn fabric_jar_with_icon_map() {
        let fmj = br#"{"schemaVersion":1,"id":"fab","name":"Fab","version":"1.0","authors":["A"],
            "icon":{"32":"assets/fab/32.png","128":"assets/fab/128.png","512":"assets/fab/512.png"}}"#;
        let icon128 = png(100);
        let jar = zip_bytes(&[
            ("fabric.mod.json", fmj),
            ("assets/fab/32.png", &png(10)),
            ("assets/fab/128.png", &icon128),
            ("assets/fab/512.png", &png(300)),
        ]);
        let meta = read_mod_metadata(Cursor::new(jar)).unwrap();
        assert_eq!(meta.id.as_deref(), Some("fab"));
        assert_eq!(meta.icon_png.as_deref(), Some(icon128.as_slice()));
    }

    #[test]
    fn oversized_or_non_png_icons_fall_back() {
        let fmj = br#"{"id":"big","icon":{"128":"big.png","16":"small.png"}}"#;
        let small = png(20);
        let jar = zip_bytes(&[
            ("fabric.mod.json", fmj),
            ("big.png", &png(300 * 1024)),
            ("small.png", &small),
        ]);
        let meta = read_mod_metadata(Cursor::new(jar)).unwrap();
        assert_eq!(meta.icon_png.as_deref(), Some(small.as_slice()));

        let jar = zip_bytes(&[
            ("fabric.mod.json", br#"{"id":"x","icon":"i.png"}"#),
            ("i.png", b"GIF89a"),
        ]);
        assert_eq!(read_mod_metadata(Cursor::new(jar)).unwrap().icon_png, None);
    }

    #[test]
    fn neoforge_jar_resolves_jar_version_and_logo() {
        let toml = br#"
modLoader="javafml"
loaderVersion="[1,)"
license="MIT"
[[mods]]
modId="neo"
version="${file.jarVersion}"
displayName="Neo Mod"
logoFile="neo_logo.png"
authors=["X", "Y"]
[[dependencies.neo]]
modId="neoforge"
type="required"
"#;
        let manifest = b"Manifest-Version: 1.0\nImplementation-Version: 21.1.5\n";
        let logo = png(64);
        let jar = zip_bytes(&[
            ("META-INF/MANIFEST.MF", manifest),
            ("META-INF/neoforge.mods.toml", toml),
            ("neo_logo.png", &logo),
        ]);
        let meta = read_mod_metadata(Cursor::new(jar)).unwrap();
        assert_eq!(meta.version.as_deref(), Some("21.1.5"));
        assert_eq!(meta.name.as_deref(), Some("Neo Mod"));
        assert_eq!(meta.authors, vec!["X", "Y"]);
        assert_eq!(meta.loaders, vec![LoaderKind::NeoForge]);
        assert_eq!(meta.icon_png.as_deref(), Some(logo.as_slice()));
    }

    #[test]
    fn multi_loader_jar_merges_and_pack_png_fallback() {
        let fmj = br#"{"id":"multi","name":"Multi"}"#;
        let toml = b"[[mods]]\nmodId=\"multi\"\ndescription=\"From Forge\"\nversion=\"3.0\"\n";
        let pack = png(32);
        let jar = zip_bytes(&[
            ("fabric.mod.json", fmj),
            ("META-INF/mods.toml", toml),
            ("pack.png", &pack),
        ]);
        let meta = read_mod_metadata(Cursor::new(jar)).unwrap();
        assert_eq!(meta.loaders, vec![LoaderKind::Fabric, LoaderKind::Forge]);
        assert_eq!(meta.description.as_deref(), Some("From Forge"));
        assert_eq!(meta.version.as_deref(), Some("3.0"));
        assert_eq!(meta.icon_png.as_deref(), Some(pack.as_slice()));
    }

    #[test]
    fn garbage_never_panics() {
        assert!(read_mod_metadata(Cursor::new(b"not a zip".to_vec())).is_none());
        assert!(read_mod_metadata(Cursor::new(Vec::new())).is_none());
        let jar = zip_bytes(&[("fabric.mod.json", b"{{{{"), ("mcmod.info", b"[")]);
        assert!(read_mod_metadata(Cursor::new(jar)).is_none());
        let jar = zip_bytes(&[("README.txt", b"hello")]);
        assert!(read_mod_metadata(Cursor::new(jar)).is_none());
    }

    #[tokio::test]
    async fn list_toggle_and_hash() {
        let dir = tempfile::tempdir().unwrap();
        let mods = dir.path().join("mods");
        std::fs::create_dir_all(&mods).unwrap();
        std::fs::write(
            mods.join("b-mod.jar"),
            zip_bytes(&[("fabric.mod.json", br#"{"id":"b","name":"Beta"}"#)]),
        )
        .unwrap();
        std::fs::write(
            mods.join("a-mod.jar.disabled"),
            zip_bytes(&[("fabric.mod.json", br#"{"id":"a","name":"alpha"}"#)]),
        )
        .unwrap();
        std::fs::write(mods.join("broken.jar"), b"abc").unwrap();
        std::fs::write(mods.join("notes.txt"), b"ignored").unwrap();
        std::fs::create_dir_all(mods.join("folder.jar")).unwrap();

        let list = list_mods(&mods, true).await.unwrap();
        let names: Vec<&str> = list.iter().map(ModInfo::display_name).collect();
        assert_eq!(names, vec!["alpha", "Beta", "broken"]);
        assert!(!list[0].enabled);
        assert!(list[1].enabled);
        assert!(list[2].metadata.is_none());
        assert_eq!(
            list[2].sha1.as_deref(),
            Some("a9993e364706816aba3e25717850c26c9cd0d89d")
        );

        let enabled = set_enabled(&list[0].path, true).await.unwrap();
        assert_eq!(enabled, mods.join("a-mod.jar"));
        assert_eq!(set_enabled(&enabled, true).await.unwrap(), enabled);
        let disabled = set_enabled(&enabled, false).await.unwrap();
        assert_eq!(disabled, mods.join("a-mod.jar.disabled"));

        // Refuses to clobber an existing twin.
        std::fs::write(mods.join("a-mod.jar"), b"other").unwrap();
        assert!(matches!(
            set_enabled(&disabled, true).await,
            Err(Error::AlreadyExists(_))
        ));

        assert!(
            list_mods(&dir.path().join("missing"), false)
                .await
                .unwrap()
                .is_empty()
        );
    }
}
