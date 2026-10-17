//! The add-on index: what the launcher knows about each file in `mods/`, `resourcepacks/` and
//! `shaderpacks/`. It is only a cache: a missing or broken index is rebuilt by hashing the
//! files again and asking Modrinth.

use std::collections::BTreeMap;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths::write_atomic;

/// File name of the index. The launcher keeps it in `<instance>/.launcher/`.
pub const ADDON_INDEX_FILE: &str = "addons.json";
const SCHEMA_VERSION: u32 = 1;
pub(crate) const DISABLED_SUFFIX: &str = ".disabled";

/// The Modrinth project and version a file belongs to.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AddonSource {
    pub project_id: String,
    pub version_id: String,
    #[serde(default)]
    pub version_number: Option<String>,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub icon_url: Option<String>,
}

/// One add-on file as last seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddonRecord {
    pub size: u64,
    #[serde(default)]
    pub modified_ms: Option<i64>,
    /// Lowercase hex SHA-1 of the file.
    pub sha1: String,
    #[serde(default)]
    pub source: Option<AddonSource>,
    /// When Modrinth was last asked about this file (ms since the epoch).
    #[serde(default)]
    pub looked_up_ms: Option<i64>,
}

impl AddonRecord {
    /// True when the record still describes a file with this size and modification time.
    pub fn matches(&self, size: u64, modified_ms: Option<i64>) -> bool {
        self.size == size
            && match (self.modified_ms, modified_ms) {
                (Some(recorded), Some(current)) => recorded == current,
                _ => true,
            }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct AddonIndex {
    pub schema_version: u32,
    /// `<folder>/<file name without .disabled>` → record.
    pub files: BTreeMap<String, AddonRecord>,
    /// The Modrinth modpack the instance was created from.
    pub modpack: Option<AddonSource>,
}

impl Default for AddonIndex {
    fn default() -> Self {
        Self {
            schema_version: SCHEMA_VERSION,
            files: BTreeMap::new(),
            modpack: None,
        }
    }
}

/// Index key of a file: `mods/sodium.jar` for both `sodium.jar` and `sodium.jar.disabled`.
pub fn index_key(folder: &str, file_name: &str) -> String {
    let base = file_name.strip_suffix(DISABLED_SUFFIX).unwrap_or(file_name);
    format!("{folder}/{base}")
}

impl AddonIndex {
    fn parse(bytes: &[u8], path: &Path) -> Self {
        match serde_json::from_slice::<Self>(bytes) {
            Ok(index) => index,
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "ignoring a corrupt add-on index");
                Self::default()
            }
        }
    }

    /// Load the index; a missing or unreadable file gives an empty one.
    pub async fn load(path: &Path) -> Self {
        match tokio::fs::read(path).await {
            Ok(bytes) => Self::parse(&bytes, path),
            Err(_) => Self::default(),
        }
    }

    /// Blocking variant of [`AddonIndex::load`].
    pub fn load_blocking(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => Self::parse(&bytes, path),
            Err(_) => Self::default(),
        }
    }

    /// Write the index atomically.
    pub async fn save(&self, path: &Path) -> Result<()> {
        let bytes =
            serde_json::to_vec_pretty(self).map_err(|e| Error::json(ADDON_INDEX_FILE, e))?;
        write_atomic(path, &bytes).await
    }

    /// What the index knows about `folder/file_name`, if the file still has the recorded size
    /// and modification time. Cheap: no hashing, no network.
    pub fn source_for(
        &self,
        folder: &str,
        file_name: &str,
        size: u64,
        modified_ms: Option<i64>,
    ) -> Option<&AddonSource> {
        self.files
            .get(&index_key(folder, file_name))
            .filter(|record| record.matches(size, modified_ms))
            .and_then(|record| record.source.as_ref())
    }
}
