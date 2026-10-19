//! What came from where: `instances/<id>/import.json` (next to `instance.json`, outside the game
//! folder). It makes a second run copy only what is missing, and tells which other launcher's
//! profile an instance was made from. It goes with its instance (deleted, trashed, moved), so a
//! profile whose instance was deleted can be brought over again.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::Result;

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
    /// Copied item (path relative to the game folder: `saves/New World`, `options.txt`) → the
    /// other launcher's file or folder it came from.
    #[serde(default)]
    pub items: BTreeMap<String, String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct JournalFrom {
    /// The source's folder ([`path_key`]).
    pub source: String,
    pub profile: String,
}

impl Journal {
    pub(crate) fn read(instance_dir: &Path) -> Self {
        std::fs::read(instance_dir.join(JOURNAL_FILE))
            .ok()
            .and_then(|bytes| serde_json::from_slice(&bytes).ok())
            .unwrap_or_default()
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
    let Ok(entries) = std::fs::read_dir(instances_root) else {
        return Vec::new();
    };
    entries
        .flatten()
        .filter(|e| e.file_type().is_ok_and(|t| t.is_dir()))
        .filter(|e| e.path().join(pl_mc::instance::INSTANCE_FILE).is_file())
        .filter_map(|e| {
            let journal = Journal::read(&e.path());
            let from = journal.from?;
            Some(ImportedFrom {
                instance_id: e.file_name().to_string_lossy().into_owned(),
                source: from.source,
                profile: from.profile,
            })
        })
        .collect()
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
        std::fs::create_dir_all(game.join("saves/New World")).unwrap();
        let mut journal = Journal::default();
        let source = Path::new("C:/Users/x/AppData/Roaming/.minecraft/saves/New World");
        journal.record("saves/New World", source);
        journal.write(dir.path()).unwrap();
        let back = Journal::read(dir.path());
        assert_eq!(back, journal);
        assert_eq!(
            back.copied_to(source, &game).as_deref(),
            Some("saves/New World")
        );
        std::fs::remove_dir_all(game.join("saves/New World")).unwrap();
        assert_eq!(back.copied_to(source, &game), None);
    }
}
