//! What came from where: `instances/<id>/import.json` (next to `instance.json`, outside the game
//! folder). It makes a second run copy only what is missing, and tells which other launcher's
//! profile an instance was made from. It goes with its instance (deleted, trashed, moved), so a
//! profile whose instance was deleted can be brought over again.
//!
//! Worlds are recorded with what identifies them ([`WorldRecord`]); [`WorldIndex`] reads every
//! instance's records, so a world that came over once is never copied again, whichever instance
//! it went into.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use crate::error::Result;
use crate::model::SourceKind;
use crate::worlds::WorldDir;

pub(crate) const JOURNAL_FILE: &str = "import.json";
const SCHEMA: u32 = 1;

fn schema() -> u32 {
    SCHEMA
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct Journal {
    #[serde(default = "schema")]
    pub schema: u32,
    /// The profile this instance was made from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<JournalFrom>,
    /// Copied item (path relative to the game folder: `options.txt`, `resourcepacks/x.zip`) → the
    /// other launcher's file or folder it came from. Worlds copied before IMPORT2 are here too
    /// (`saves/New World`).
    #[serde(default)]
    pub items: BTreeMap<String, String>,
    /// Copied world (`saves/New World`) → where it came from and what identifies it.
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub worlds: BTreeMap<String, WorldRecord>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JournalFrom {
    /// The source's folder ([`path_key`]).
    pub source: String,
    pub profile: String,
}

/// A world an import copied: the launcher and folder it came from, and what identifies it there.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct WorldRecord {
    pub launcher: SourceKind,
    /// The game folder it was in ([`path_key`]): the source's, or a profile's own.
    pub source: String,
    /// Its own folder there.
    pub path: String,
    /// Its seed and its own name: a world whose folder was renamed there is still this one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub seed: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl WorldRecord {
    pub(crate) fn of(launcher: SourceKind, game_dir: &Path, world: &WorldDir) -> Self {
        Self {
            launcher,
            source: path_key(game_dir),
            path: world.path.display().to_string(),
            seed: world.seed,
            name: world.name.clone(),
        }
    }
}

impl Journal {
    pub(crate) fn read(instance_dir: &Path) -> Self {
        let Ok(bytes) = std::fs::read(instance_dir.join(JOURNAL_FILE)) else {
            return Self::default();
        };
        if let Ok(journal) = serde_json::from_slice(&bytes) {
            return journal;
        }
        // A part this version cannot read (a newer one wrote it) must not cost the rest: without
        // `from`, the instance's profile would be brought over again.
        let Ok(value) = serde_json::from_slice::<serde_json::Value>(&bytes) else {
            return Self::default();
        };
        fn part<T: serde::de::DeserializeOwned>(value: &serde_json::Value, key: &str) -> Option<T> {
            serde_json::from_value(value.get(key)?.clone()).ok()
        }
        let worlds = value
            .get("worlds")
            .and_then(serde_json::Value::as_object)
            .map(|records| {
                records
                    .iter()
                    .filter_map(|(dest, record)| {
                        Some((dest.clone(), serde_json::from_value(record.clone()).ok()?))
                    })
                    .collect()
            })
            .unwrap_or_default();
        Self {
            schema: SCHEMA,
            from: part(&value, "from"),
            items: part(&value, "items").unwrap_or_default(),
            worlds,
        }
    }

    pub(crate) fn write(&self, instance_dir: &Path) -> Result<()> {
        let bytes = serde_json::to_vec_pretty(self)
            .map_err(|e| crate::Error::Invalid(format!("import journal: {e}")))?;
        pl_mc::paths::write_durable_blocking(&instance_dir.join(JOURNAL_FILE), &bytes)?;
        Ok(())
    }

    /// The game-folder path an earlier run copied `source` to, when it is still there.
    pub(crate) fn copied_to(&self, source: &Path, game_dir: &Path) -> Option<String> {
        let key = path_key(source);
        self.items
            .iter()
            .find(|(dest, from)| {
                path_key(Path::new(from.as_str())) == key && game_dir.join(dest).exists()
            })
            .map(|(dest, _)| dest.clone())
    }

    pub(crate) fn record(&mut self, dest: &str, source: &Path) {
        self.items
            .insert(dest.replace('\\', "/"), source.display().to_string());
    }

    pub(crate) fn record_world(&mut self, dest: &str, record: WorldRecord) {
        self.worlds.insert(dest.replace('\\', "/"), record);
    }
}

/// An instance an earlier import made from another launcher's profile.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ImportedFrom {
    pub instance_id: String,
    /// The source's folder ([`path_key`]).
    pub source: String,
    pub profile: String,
}

/// Instances under `instances_root` that were made from profiles (blocking; one small file per
/// instance).
pub fn imported_profiles(instances_root: &Path) -> Vec<ImportedFrom> {
    instance_journals(instances_root)
        .into_iter()
        .filter_map(|(id, _, journal)| {
            let from = journal.from?;
            Some(ImportedFrom {
                instance_id: id,
                source: from.source,
                profile: from.profile,
            })
        })
        .collect()
}

/// Every instance's id, folder and journal (blocking).
fn instance_journals(instances_root: &Path) -> Vec<(String, PathBuf, Journal)> {
    let Ok(entries) = std::fs::read_dir(instances_root) else {
        return Vec::new();
    };
    let mut out: Vec<(String, PathBuf, Journal)> = entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| e.path().join(pl_mc::instance::INSTANCE_FILE).is_file())
        .filter(|e| e.path().join(JOURNAL_FILE).is_file())
        .map(|e| {
            let dir = e.path();
            let journal = Journal::read(&dir);
            (e.file_name().to_string_lossy().into_owned(), dir, journal)
        })
        .collect();
    out.sort_by(|a, b| a.0.cmp(&b.0));
    out
}

/// The worlds earlier imports brought, read from every instance's journal: a world comes over
/// once, whichever instance it went into.
#[derive(Debug, Clone, Default)]
pub(crate) struct WorldIndex {
    copies: Vec<WorldCopy>,
}

#[derive(Debug, Clone)]
struct WorldCopy {
    /// The copy (`<game folder>/saves/<name>`).
    at: PathBuf,
    /// The other launcher's world folder it came from, as written and as [`path_key`].
    from: PathBuf,
    from_key: String,
    /// The game folder that world was in ([`path_key`]); unknown for copies made before IMPORT2.
    source: Option<String>,
    seed: Option<i64>,
    name: Option<String>,
}

impl WorldIndex {
    /// Reads every instance's journal under `instances_root` (blocking; one small file each).
    pub(crate) fn read(instances_root: &Path) -> Self {
        let mut index = Self::default();
        for (_, dir, journal) in instance_journals(instances_root) {
            let game_dir = dir.join(pl_mc::instance::GAME_DIR_NAME);
            for (dest, record) in &journal.worlds {
                index.add(game_dir.join(dest), record);
            }
            // Worlds an import recorded before they had records of their own.
            for (dest, from) in &journal.items {
                if dest.starts_with("saves/") && !journal.worlds.contains_key(dest) {
                    let from = PathBuf::from(from);
                    index.copies.push(WorldCopy {
                        at: game_dir.join(dest),
                        from_key: path_key(&from),
                        from,
                        source: None,
                        seed: None,
                        name: None,
                    });
                }
            }
        }
        index
    }

    /// Remembers a copy made now.
    pub(crate) fn add(&mut self, at: PathBuf, record: &WorldRecord) {
        let from = PathBuf::from(&record.path);
        self.copies.push(WorldCopy {
            at,
            from_key: path_key(&from),
            from,
            source: Some(record.source.clone()),
            seed: record.seed,
            name: record.name.clone(),
        });
    }

    /// The copy an earlier import made of `world` (in the game folder `game_dir`), while it is
    /// still there: the copy of the same folder, or of a world with the same seed and name from
    /// the same game folder whose own folder is gone there (renamed).
    pub(crate) fn find(&self, world: &WorldDir, game_dir: &Path) -> Option<&Path> {
        let key = path_key(&world.path);
        let source = path_key(game_dir);
        let identified = world.seed.is_some() && world.name.is_some();
        self.copies
            .iter()
            .filter(|copy| copy.at.is_dir())
            .find(|copy| {
                copy.from_key == key
                    || (identified
                        && copy.source.as_deref() == Some(source.as_str())
                        && copy.seed == world.seed
                        && copy.name == world.name
                        && !copy.from.exists())
            })
            .map(|copy| copy.at.as_path())
    }
}

/// A folder's path for comparisons: separators unified, no trailing separator, and on Windows
/// without case.
pub(crate) fn path_key(path: &Path) -> String {
    let text = path.display().to_string().replace('\\', "/");
    let text = text.trim_end_matches('/').to_owned();
    if cfg!(windows) {
        text.to_lowercase()
    } else {
        text
    }
}

pub(crate) fn same_path(a: &Path, b: &Path) -> bool {
    path_key(a) == path_key(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journals_find_their_copies() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("minecraft");
        std::fs::create_dir_all(game.join("resourcepacks/Pack")).unwrap();
        let mut journal = Journal::default();
        let source = Path::new("C:/Users/x/AppData/Roaming/.minecraft/resourcepacks/Pack");
        journal.record("resourcepacks/Pack", source);
        journal.write(dir.path()).unwrap();
        let back = Journal::read(dir.path());
        assert_eq!(back, journal);
        assert_eq!(
            back.copied_to(source, &game).as_deref(),
            Some("resourcepacks/Pack")
        );
        std::fs::remove_dir_all(game.join("resourcepacks/Pack")).unwrap();
        assert_eq!(back.copied_to(source, &game), None);
    }

    #[test]
    fn a_record_a_newer_version_wrote_does_not_cost_the_rest() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join(JOURNAL_FILE),
            r#"{"schema": 1,
                "from": {"source": "c:/x/.minecraft", "profile": "version:1.21.4"},
                "items": {"options.txt": "C:/x/.minecraft/options.txt"},
                "worlds": {
                    "saves/A": {"launcher": "aNewLauncher", "source": "c:/x", "path": "C:/x/saves/A"},
                    "saves/B": {"launcher": "prism", "source": "c:/y", "path": "C:/y/saves/B", "seed": 3}
                }}"#,
        )
        .unwrap();
        let journal = Journal::read(dir.path());
        assert_eq!(
            journal.from.map(|f| f.profile).as_deref(),
            Some("version:1.21.4")
        );
        assert_eq!(journal.items.len(), 1);
        assert_eq!(journal.worlds.keys().collect::<Vec<_>>(), ["saves/B"]);
    }

    fn world(path: &Path, seed: Option<i64>, name: Option<&str>) -> WorldDir {
        WorldDir {
            folder: path.file_name().unwrap().to_string_lossy().into_owned(),
            path: path.to_path_buf(),
            version: Some("1.21.4".into()),
            loader: Some(pl_mc::loaders::LoaderKind::Vanilla),
            seed,
            name: name.map(str::to_owned),
            unreadable: false,
        }
    }

    /// An instance folder with `instance.json` and a journal.
    fn instance(root: &Path, id: &str, journal: &Journal) -> PathBuf {
        let dir = root.join(id);
        std::fs::create_dir_all(dir.join("minecraft/saves")).unwrap();
        std::fs::write(dir.join(pl_mc::instance::INSTANCE_FILE), "{}").unwrap();
        journal.write(&dir).unwrap();
        dir
    }

    #[test]
    fn a_world_is_known_wherever_it_went() {
        let dir = tempfile::tempdir().unwrap();
        let other = dir.path().join("other/.minecraft");
        let survival = other.join("saves/Survival");
        let old = other.join("saves/Old");
        std::fs::create_dir_all(&survival).unwrap();
        let instances = dir.path().join("instances");

        let mut forge = Journal::default();
        forge.record_world(
            "saves/Survival",
            WorldRecord::of(
                SourceKind::TLauncher,
                &other,
                &world(&survival, Some(5), Some("Survival")),
            ),
        );
        let forge_dir = instance(&instances, "forge", &forge);
        std::fs::create_dir_all(forge_dir.join("minecraft/saves/Survival")).unwrap();
        // A journal written before worlds had records: only where the copy came from.
        let mut projects = Journal::default();
        projects.record("saves/Old", &old);
        let projects_dir = instance(&instances, "projects", &projects);
        std::fs::create_dir_all(projects_dir.join("minecraft/saves/Old")).unwrap();

        let index = WorldIndex::read(&instances);
        let found = index.find(&world(&survival, Some(5), Some("Survival")), &other);
        assert_eq!(
            found,
            Some(forge_dir.join("minecraft/saves/Survival").as_path())
        );
        assert!(index.find(&world(&old, None, None), &other).is_some());

        // Renamed there: the same seed and name, its old folder gone.
        let renamed = other.join("saves/Survival 2");
        let moved = world(&renamed, Some(5), Some("Survival"));
        assert!(
            index.find(&moved, &other).is_none(),
            "the old folder is still there"
        );
        std::fs::remove_dir_all(&survival).unwrap();
        assert!(index.find(&moved, &other).is_some());
        // Another world of the same seed but another name is another world.
        assert!(
            index
                .find(&world(&renamed, Some(5), Some("Copy")), &other)
                .is_none()
        );
        // From another game folder it is another world too.
        let elsewhere = dir.path().join("elsewhere");
        assert!(index.find(&moved, &elsewhere).is_none());

        // A copy the player deleted can come again.
        std::fs::remove_dir_all(forge_dir.join("minecraft/saves/Survival")).unwrap();
        assert!(index.find(&moved, &other).is_none());
    }
}
