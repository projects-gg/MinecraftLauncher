use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{DocRef, FileRef, check_schema, default_schema};
use crate::error::Result;

/// Highest `RootIndex.schema` this build understands.
pub const ROOT_INDEX_SCHEMA: u32 = 1;

/// Platform keys used by AFK client builds (`<os>-<arch>`).
pub const PLATFORM_KEYS: [&str; 5] = [
    "windows-x86_64",
    "linux-x86_64",
    "linux-aarch64",
    "macos-aarch64",
    "macos-x86_64",
];

/// `v1/index.json`: the single signed root. Everything else is reached through it and
/// verified by the SHA-256 recorded here.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RootIndex {
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// Strictly increasing publish counter; clients refuse anything lower than what they have
    /// (rollback protection).
    pub sequence: u64,
    pub generated_at: Timestamp,
    /// `config/launcher.<sha256>.json` ([`crate::LauncherConfig`]).
    pub config: DocRef,
    /// `news/news.<sha256>.json` ([`crate::NewsFeed`]).
    pub news: DocRef,
    #[serde(default)]
    pub packs: Vec<PackRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub afk: Option<AfkRelease>,
}

impl RootIndex {
    pub fn check_schema(&self) -> Result<()> {
        check_schema("root index", self.schema, ROOT_INDEX_SCHEMA)
    }

    /// Schema plus every document reference.
    pub fn validate(&self) -> Result<()> {
        self.check_schema()?;
        self.config.validate()?;
        self.news.validate()?;
        for pack in &self.packs {
            pack.manifest.validate()?;
        }
        Ok(())
    }

    pub fn pack(&self, id: &str) -> Option<&PackRef> {
        self.packs.iter().find(|pack| pack.id == id)
    }

    /// Every document this index references (config, news, pack manifests).
    pub fn documents(&self) -> impl Iterator<Item = &DocRef> {
        [&self.config, &self.news]
            .into_iter()
            .chain(self.packs.iter().map(|pack| &pack.manifest))
    }
}

/// A modpack and the manifest of its latest version (`packs/<id>/<version>.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackRef {
    pub id: String,
    pub name: String,
    /// `versionId` of the referenced manifest.
    pub latest: String,
    pub manifest: DocRef,
}

/// The current AFK client release: one zip per platform key.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkRelease {
    pub version: String,
    /// Platform key (see [`PLATFORM_KEYS`]) → build.
    #[serde(default)]
    pub builds: BTreeMap<String, FileRef>,
}

impl AfkRelease {
    pub fn build_for(&self, platform: &str) -> Option<&FileRef> {
        self.builds.get(platform)
    }

    /// The build for the platform this binary runs on.
    pub fn current_build(&self) -> Option<&FileRef> {
        self.build_for(&current_platform_key())
    }
}

/// `<os>-<arch>` of the running binary, e.g. `windows-x86_64` or `macos-aarch64`.
pub fn current_platform_key() -> String {
    format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn platform_key_has_known_shape() {
        let key = current_platform_key();
        if cfg!(all(target_os = "windows", target_arch = "x86_64")) {
            assert_eq!(key, "windows-x86_64");
        }
        assert!(key.contains('-'));
    }

    #[test]
    fn unsupported_schema_is_rejected() {
        let doc = |schema: u32| DocRef {
            path: "config/x.json".into(),
            sha256: "0".repeat(64),
            size: schema.into(),
        };
        let mut index = RootIndex {
            schema: 1,
            sequence: 1,
            generated_at: Timestamp::UNIX_EPOCH,
            config: doc(1),
            news: doc(2),
            packs: Vec::new(),
            afk: None,
        };
        index.validate().unwrap();
        assert_eq!(index.documents().count(), 2);
        index.schema = 2;
        assert!(matches!(
            index.validate(),
            Err(crate::Error::UnsupportedSchema { found: 2, .. })
        ));
    }
}
