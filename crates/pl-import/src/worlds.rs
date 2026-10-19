//! Worlds of a game folder: which Minecraft version last saved each one and with which loader
//! (`level.dat`: `Data.Version.Name`, `Data.ServerBrands`), so a world can follow the instance it
//! was played with.

use std::collections::HashMap;
use std::io::Read;
use std::path::{Path, PathBuf};

use fastnbt::Value;
use pl_mc::loaders::LoaderKind;

/// `level.dat` is a few kilobytes; a modded one with many registries stays well under this.
const MAX_LEVEL_DAT: u64 = 32 * 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct WorldDir {
    pub folder: String,
    pub path: PathBuf,
    /// Game version that last saved it (missing before 1.9 or when unreadable).
    pub version: Option<String>,
    /// The loader it was last played with, read from the brands that saved it.
    pub loader: Option<LoaderKind>,
}

/// World folders (with a `level.dat`) in `saves`, by folder name.
pub(crate) fn list_worlds(saves: &Path) -> Vec<WorldDir> {
    let Ok(entries) = std::fs::read_dir(saves) else {
        return Vec::new();
    };
    let mut worlds: Vec<WorldDir> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter_map(|e| {
            let path = e.path();
            let level = level_dat(&path)?;
            let (version, loader) = read_level(&level).unwrap_or((None, None));
            Some(WorldDir {
                folder: e.file_name().to_string_lossy().into_owned(),
                path,
                version,
                loader,
            })
        })
        .collect();
    worlds.sort_by_key(|w| w.folder.to_lowercase());
    worlds
}

/// Counts world folders without reading them.
pub(crate) fn count_worlds(saves: &Path) -> u32 {
    std::fs::read_dir(saves)
        .map(|entries| {
            entries
                .flatten()
                .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
                .filter(|e| level_dat(&e.path()).is_some())
                .count() as u32
        })
        .unwrap_or(0)
}

fn level_dat(world: &Path) -> Option<PathBuf> {
    ["level.dat", "level.dat_old"]
        .into_iter()
        .map(|name| world.join(name))
        .find(|p| p.is_file())
}

fn read_level(path: &Path) -> Option<(Option<String>, Option<LoaderKind>)> {
    let file = std::fs::File::open(path).ok()?;
    let mut raw = Vec::new();
    file.take(MAX_LEVEL_DAT).read_to_end(&mut raw).ok()?;
    let bytes = if raw.starts_with(&[0x1f, 0x8b]) {
        let mut out = Vec::new();
        flate2::read::GzDecoder::new(raw.as_slice())
            .take(MAX_LEVEL_DAT)
            .read_to_end(&mut out)
            .ok()?;
        out
    } else {
        raw
    };
    let root: Value = fastnbt::from_bytes(&bytes).ok()?;
    let data = compound(&root)?.get("Data").and_then(compound)?;
    let version = data
        .get("Version")
        .and_then(compound)
        .and_then(|v| v.get("Name"))
        .and_then(string)
        .map(str::to_owned);
    let brands: Vec<String> = match data.get("ServerBrands") {
        Some(Value::List(items)) => items
            .iter()
            .filter_map(string)
            .map(str::to_ascii_lowercase)
            .collect(),
        _ => Vec::new(),
    };
    let modded = matches!(data.get("WasModded"), Some(Value::Byte(1)));
    Some((version, loader_of(&brands, modded)))
}

/// The loader a world was played with: the last brand that is one (a world opened in Forge
/// after Fabric counts as Forge), else vanilla when it never was modded.
fn loader_of(brands: &[String], modded: bool) -> Option<LoaderKind> {
    let found = brands.iter().rev().find_map(|brand| match brand.as_str() {
        "fabric" => Some(LoaderKind::Fabric),
        "quilt" => Some(LoaderKind::Quilt),
        "forge" | "fml,forge" => Some(LoaderKind::Forge),
        "neoforge" => Some(LoaderKind::NeoForge),
        _ => None,
    });
    match found {
        Some(loader) => Some(loader),
        None if !modded && brands.iter().all(|b| b == "vanilla") && !brands.is_empty() => {
            Some(LoaderKind::Vanilla)
        }
        None => None,
    }
}

fn compound(value: &Value) -> Option<&HashMap<String, Value>> {
    match value {
        Value::Compound(map) => Some(map),
        _ => None,
    }
}

fn string(value: &Value) -> Option<&str> {
    match value {
        Value::String(s) => Some(s.as_str()),
        _ => None,
    }
}

/// Whether a running game holds the world open: the game keeps `session.lock` locked while it
/// plays it (Windows refuses to open such a file). Elsewhere the lock is advisory and this
/// reports false.
pub(crate) fn in_use(world: &Path) -> bool {
    let lock = world.join("session.lock");
    if !lock.is_file() {
        return false;
    }
    match std::fs::File::open(&lock) {
        Ok(mut file) => {
            let mut buf = [0u8; 1];
            // Windows lets the file open but refuses reads of a locked region.
            matches!(file.read(&mut buf), Err(e) if cfg!(windows) && e.raw_os_error() == Some(33))
        }
        Err(e) => cfg!(windows) && matches!(e.raw_os_error(), Some(32 | 33)),
    }
}

#[cfg(test)]
pub(crate) mod tests {
    use std::io::Write;

    use super::*;

    /// A gzip `level.dat` saved by `version` with `brands`.
    pub(crate) fn level_dat_bytes(version: &str, brands: &[&str], modded: bool) -> Vec<u8> {
        let mut version_tag = HashMap::new();
        version_tag.insert("Name".to_owned(), Value::String(version.to_owned()));
        let mut data = HashMap::new();
        data.insert("Version".to_owned(), Value::Compound(version_tag));
        data.insert(
            "ServerBrands".to_owned(),
            Value::List(
                brands
                    .iter()
                    .map(|b| Value::String((*b).to_owned()))
                    .collect(),
            ),
        );
        data.insert("WasModded".to_owned(), Value::Byte(i8::from(modded)));
        data.insert("LevelName".to_owned(), Value::String("World".to_owned()));
        let mut root = HashMap::new();
        root.insert("Data".to_owned(), Value::Compound(data));
        let nbt = fastnbt::to_bytes(&Value::Compound(root)).unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&nbt).unwrap();
        gz.finish().unwrap()
    }

    pub(crate) fn make_world(saves: &Path, folder: &str, version: &str, brands: &[&str]) {
        let dir = saves.join(folder);
        std::fs::create_dir_all(dir.join("region")).unwrap();
        let modded = brands.iter().any(|b| *b != "vanilla");
        std::fs::write(
            dir.join("level.dat"),
            level_dat_bytes(version, brands, modded),
        )
        .unwrap();
        std::fs::write(dir.join("region").join("r.0.0.mca"), vec![7u8; 4096]).unwrap();
    }

    #[test]
    fn worlds_tell_their_version_and_loader() {
        let dir = tempfile::tempdir().unwrap();
        let saves = dir.path();
        make_world(saves, "Survival", "1.21.4", &["vanilla"]);
        make_world(saves, "Create", "1.20.1", &["forge"]);
        make_world(saves, "Opened in both", "1.20.1", &["fabric", "forge"]);
        std::fs::create_dir_all(saves.join("not a world")).unwrap();
        assert_eq!(count_worlds(saves), 3);
        let worlds = list_worlds(saves);
        let by_name: Vec<_> = worlds
            .iter()
            .map(|w| (w.folder.as_str(), w.version.as_deref(), w.loader))
            .collect();
        assert_eq!(
            by_name,
            [
                ("Create", Some("1.20.1"), Some(LoaderKind::Forge)),
                ("Opened in both", Some("1.20.1"), Some(LoaderKind::Forge)),
                ("Survival", Some("1.21.4"), Some(LoaderKind::Vanilla)),
            ]
        );
        assert!(!in_use(&saves.join("Survival")));
    }
}
