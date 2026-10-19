//! Worlds of a game folder: which Minecraft version last saved each one and with which loader
//! (`level.dat`: `Data.Version.Name`, `Data.ServerBrands`), so a world can follow the instance it
//! was played with, and what tells it apart from other worlds (its seed and its own name).

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
    /// The loader it was last played with: the last loader brand that saved it, Forge for the
    /// worlds older Forge versions saved without brands, vanilla for a world never modded. `None`
    /// when it cannot be told (a modded world of an unknown loader, an unreadable `level.dat`).
    pub loader: Option<LoaderKind>,
    /// Its seed and its own name: together they tell it apart when its folder is renamed.
    pub seed: Option<i64>,
    pub name: Option<String>,
    /// Its `level.dat` could not be read.
    pub unreadable: bool,
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
            let read = read_level(&level);
            let unreadable = read.is_none();
            let level = read.unwrap_or_default();
            Some(WorldDir {
                folder: e.file_name().to_string_lossy().into_owned(),
                path,
                version: level.version,
                loader: level.loader,
                seed: level.seed,
                name: level.name,
                unreadable,
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

/// What `level.dat` says about its world.
#[derive(Debug, Default)]
struct Level {
    version: Option<String>,
    loader: Option<LoaderKind>,
    seed: Option<i64>,
    name: Option<String>,
}

fn read_level(path: &Path) -> Option<Level> {
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
    let root = compound(&root)?;
    let data = root.get("Data").and_then(compound)?;
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
    // Forge before 1.17 kept its mod list beside `Data` (`FML`, later `fml`), brands or not.
    let forge_tag = root.keys().any(|key| key.eq_ignore_ascii_case("fml"));
    let seed = data
        .get("WorldGenSettings")
        .and_then(compound)
        .and_then(|w| w.get("seed"))
        .or_else(|| data.get("RandomSeed"))
        .and_then(long);
    let name = data.get("LevelName").and_then(string).map(str::to_owned);
    Some(Level {
        version,
        loader: loader_of(&brands, modded, forge_tag),
        seed,
        name,
    })
}

/// The loader a world was played with: the last brand that is one (a world opened in Forge
/// after Fabric counts as Forge); Forge when Forge's own tag is there; else vanilla when it never
/// was modded (worlds before 1.13 have no brands at all).
fn loader_of(brands: &[String], modded: bool, forge_tag: bool) -> Option<LoaderKind> {
    let found = brands.iter().rev().find_map(|brand| match brand.as_str() {
        "fabric" => Some(LoaderKind::Fabric),
        "quilt" => Some(LoaderKind::Quilt),
        "forge" | "fml,forge" => Some(LoaderKind::Forge),
        "neoforge" => Some(LoaderKind::NeoForge),
        _ => None,
    });
    match found {
        Some(loader) => Some(loader),
        None if forge_tag => Some(LoaderKind::Forge),
        None if !modded && brands.iter().all(|b| b == "vanilla") => Some(LoaderKind::Vanilla),
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

fn long(value: &Value) -> Option<i64> {
    match value {
        Value::Long(n) => Some(*n),
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

    /// A gzip `level.dat` saved by `version` with `brands`; `extra` adds tags beside `Data`.
    pub(crate) fn level_dat_bytes(
        version: Option<&str>,
        brands: Option<&[&str]>,
        modded: bool,
        seed: i64,
        extra: &[&str],
    ) -> Vec<u8> {
        let mut data = HashMap::new();
        if let Some(version) = version {
            let mut version_tag = HashMap::new();
            version_tag.insert("Name".to_owned(), Value::String(version.to_owned()));
            data.insert("Version".to_owned(), Value::Compound(version_tag));
        }
        if let Some(brands) = brands {
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
        }
        let mut gen_settings = HashMap::new();
        gen_settings.insert("seed".to_owned(), Value::Long(seed));
        data.insert("WorldGenSettings".to_owned(), Value::Compound(gen_settings));
        data.insert("LevelName".to_owned(), Value::String("World".to_owned()));
        let mut root = HashMap::new();
        root.insert("Data".to_owned(), Value::Compound(data));
        for tag in extra {
            root.insert((*tag).to_owned(), Value::Compound(HashMap::new()));
        }
        let nbt = fastnbt::to_bytes(&Value::Compound(root)).unwrap();
        let mut gz = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::fast());
        gz.write_all(&nbt).unwrap();
        gz.finish().unwrap()
    }

    pub(crate) fn make_world(saves: &Path, folder: &str, version: &str, brands: &[&str]) {
        let modded = brands.iter().any(|b| *b != "vanilla");
        write_world(
            saves,
            folder,
            level_dat_bytes(Some(version), Some(brands), modded, 42, &[]),
        );
    }

    fn write_world(saves: &Path, folder: &str, level: Vec<u8>) {
        let dir = saves.join(folder);
        std::fs::create_dir_all(dir.join("region")).unwrap();
        std::fs::write(dir.join("level.dat"), level).unwrap();
        std::fs::write(dir.join("region").join("r.0.0.mca"), vec![7u8; 4096]).unwrap();
    }

    #[test]
    fn worlds_tell_their_version_and_loader() {
        let dir = tempfile::tempdir().unwrap();
        let saves = dir.path();
        make_world(saves, "Survival", "1.21.4", &["vanilla"]);
        make_world(saves, "Create", "1.20.1", &["forge"]);
        make_world(saves, "Opened in both", "1.20.1", &["fabric", "forge"]);
        // Forge 1.12.2 wrote no brands, only its own tag beside `Data`.
        write_world(
            saves,
            "RLCraft",
            level_dat_bytes(Some("1.12.2"), None, false, 7, &["FML", "Forge"]),
        );
        // Before 1.13 a vanilla world has no brands either.
        write_world(
            saves,
            "Old",
            level_dat_bytes(Some("1.12.2"), None, false, 8, &[]),
        );
        // Modded by something this launcher does not know.
        write_world(
            saves,
            "Odd",
            level_dat_bytes(Some("1.21.4"), Some(&["vanilla", "odd"]), true, 9, &[]),
        );
        std::fs::create_dir_all(saves.join("Broken")).unwrap();
        std::fs::write(saves.join("Broken/level.dat"), b"not nbt").unwrap();
        std::fs::create_dir_all(saves.join("not a world")).unwrap();
        assert_eq!(count_worlds(saves), 7);
        let worlds = list_worlds(saves);
        let by_name: Vec<_> = worlds
            .iter()
            .map(|w| (w.folder.as_str(), w.version.as_deref(), w.loader))
            .collect();
        assert_eq!(
            by_name,
            [
                ("Broken", None, None),
                ("Create", Some("1.20.1"), Some(LoaderKind::Forge)),
                ("Odd", Some("1.21.4"), None),
                ("Old", Some("1.12.2"), Some(LoaderKind::Vanilla)),
                ("Opened in both", Some("1.20.1"), Some(LoaderKind::Forge)),
                ("RLCraft", Some("1.12.2"), Some(LoaderKind::Forge)),
                ("Survival", Some("1.21.4"), Some(LoaderKind::Vanilla)),
            ]
        );
        assert!(worlds[0].unreadable);
        let survival = worlds.last().unwrap();
        assert!(!survival.unreadable);
        assert_eq!(
            (survival.seed, survival.name.as_deref()),
            (Some(42), Some("World"))
        );
        assert!(!in_use(&saves.join("Survival")));
    }
}
