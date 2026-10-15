//! Mojang version manifest (`version_manifest_v2.json`), cached under `meta/` for offline use.
//!
//! Versions are ordered by `releaseTime`, never by name: year-based versions (`26.1.2`) and old
//! ids (`1.21.10`, `b1.7.3`, `rd-132211`) do not compare meaningfully as semver.

use std::cmp::Ordering;

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::install::McContext;
use crate::paths::write_atomic;
use crate::version::model::parse_timestamp;

pub const VERSION_MANIFEST_URL: &str =
    "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json";

/// File name of the cached manifest inside `meta/`.
pub const MANIFEST_CACHE_FILE: &str = "version_manifest_v2.json";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "snake_case")]
pub enum VersionType {
    Release,
    Snapshot,
    OldBeta,
    OldAlpha,
    #[serde(other)]
    Other,
}

impl VersionType {
    pub fn parse(text: &str) -> Self {
        match text {
            "release" => Self::Release,
            "snapshot" => Self::Snapshot,
            "old_beta" => Self::OldBeta,
            "old_alpha" => Self::OldAlpha,
            _ => Self::Other,
        }
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::Release => "release",
            Self::Snapshot => "snapshot",
            Self::OldBeta => "old_beta",
            Self::OldAlpha => "old_alpha",
            Self::Other => "other",
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LatestVersions {
    pub release: String,
    pub snapshot: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ManifestVersion {
    pub id: String,
    #[serde(rename = "type")]
    pub kind: VersionType,
    /// URL of the version JSON.
    pub url: String,
    #[serde(default)]
    pub time: String,
    /// RFC 3339 release time; the only valid ordering key.
    pub release_time: String,
    /// SHA-1 of the version JSON.
    pub sha1: String,
    #[serde(default)]
    pub compliance_level: u32,
}

impl ManifestVersion {
    pub fn release_timestamp(&self) -> Option<jiff::Timestamp> {
        parse_timestamp(&self.release_time)
    }
}

/// Newest first, by release time (unparseable times sort last, then by id for stability).
pub fn compare_newest_first(a: &ManifestVersion, b: &ManifestVersion) -> Ordering {
    match (a.release_timestamp(), b.release_timestamp()) {
        (Some(x), Some(y)) => y.cmp(&x),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.id.cmp(&b.id),
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct VersionManifest {
    pub latest: LatestVersions,
    pub versions: Vec<ManifestVersion>,
}

impl VersionManifest {
    pub fn from_slice(bytes: &[u8]) -> Result<Self> {
        let mut manifest: Self =
            serde_json::from_slice(bytes).map_err(|e| Error::json("version manifest", e))?;
        manifest.sort_newest_first();
        Ok(manifest)
    }

    pub fn find(&self, id: &str) -> Option<&ManifestVersion> {
        self.versions.iter().find(|v| v.id == id)
    }

    pub fn latest_release(&self) -> Option<&ManifestVersion> {
        self.find(&self.latest.release)
    }

    pub fn latest_snapshot(&self) -> Option<&ManifestVersion> {
        self.find(&self.latest.snapshot)
    }

    /// Versions of one type, newest first.
    pub fn of_type(&self, kind: VersionType) -> impl Iterator<Item = &ManifestVersion> {
        self.versions.iter().filter(move |v| v.kind == kind)
    }

    /// Sort by release time, newest first (stable for equal times).
    pub fn sort_newest_first(&mut self) {
        self.versions.sort_by(compare_newest_first);
    }

    /// Whether `a` was released after `b` (both must be in the manifest).
    pub fn is_newer(&self, a: &str, b: &str) -> Option<bool> {
        let a = self.find(a)?.release_timestamp()?;
        let b = self.find(b)?.release_timestamp()?;
        Some(a > b)
    }
}

/// Fetch the manifest from Mojang and refresh the cache in `meta/`.
pub async fn fetch_manifest(ctx: &McContext) -> Result<VersionManifest> {
    let bytes = ctx.http.get_bytes(VERSION_MANIFEST_URL).await?;
    let manifest = VersionManifest::from_slice(&bytes)?;
    let cache = ctx.dirs.meta().join(MANIFEST_CACHE_FILE);
    if let Err(error) = write_atomic(&cache, &bytes).await {
        tracing::warn!(%error, "could not cache the version manifest");
    }
    Ok(manifest)
}

/// The cached manifest, if one was saved before.
pub async fn cached_manifest(ctx: &McContext) -> Result<Option<VersionManifest>> {
    let path = ctx.dirs.meta().join(MANIFEST_CACHE_FILE);
    match tokio::fs::read(&path).await {
        Ok(bytes) => VersionManifest::from_slice(&bytes).map(Some),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(Error::io(&path, e)),
    }
}

/// Online: fetch (falling back to the cache when Mojang is unreachable). Offline: cache only,
/// [`Error::Offline`] when nothing was cached yet.
pub async fn load_manifest(ctx: &McContext, offline: bool) -> Result<VersionManifest> {
    if offline {
        return cached_manifest(ctx).await?.ok_or(Error::Offline);
    }
    match fetch_manifest(ctx).await {
        Ok(manifest) => Ok(manifest),
        Err(error) => match cached_manifest(ctx).await {
            Ok(Some(cached)) => {
                tracing::warn!(%error, "version manifest fetch failed, using the cached copy");
                Ok(cached)
            }
            _ => Err(error),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const SAMPLE: &str = r#"{
        "latest": {"release": "26.1.2", "snapshot": "26.2-snapshot-1"},
        "versions": [
            {"id": "1.21.10", "type": "release", "url": "u", "time": "t", "releaseTime": "2025-10-07T09:17:23+00:00", "sha1": "a", "complianceLevel": 1},
            {"id": "26.2-snapshot-1", "type": "snapshot", "url": "u", "time": "t", "releaseTime": "2026-04-14T12:00:00+00:00", "sha1": "b", "complianceLevel": 1},
            {"id": "26.1.2", "type": "release", "url": "u", "time": "t", "releaseTime": "2026-04-01T10:00:00+00:00", "sha1": "c", "complianceLevel": 1},
            {"id": "b1.7.3", "type": "old_beta", "url": "u", "time": "t", "releaseTime": "2011-07-07T22:00:00+00:00", "sha1": "d"},
            {"id": "x", "type": "pending", "url": "u", "time": "t", "releaseTime": "2026-01-01T00:00:00+00:00", "sha1": "e"}
        ]
    }"#;

    #[test]
    fn orders_by_release_time_not_name() {
        let m = VersionManifest::from_slice(SAMPLE.as_bytes()).unwrap();
        let ids: Vec<&str> = m.versions.iter().map(|v| v.id.as_str()).collect();
        assert_eq!(ids, ["26.2-snapshot-1", "26.1.2", "x", "1.21.10", "b1.7.3"]);
        assert_eq!(m.latest_release().unwrap().id, "26.1.2");
        assert_eq!(m.is_newer("26.1.2", "1.21.10"), Some(true));
        assert_eq!(m.find("x").unwrap().kind, VersionType::Other);
        let releases: Vec<&str> = m
            .of_type(VersionType::Release)
            .map(|v| v.id.as_str())
            .collect();
        assert_eq!(releases, ["26.1.2", "1.21.10"]);
        assert_eq!(VersionType::parse("old_alpha"), VersionType::OldAlpha);
    }

    #[tokio::test]
    async fn offline_uses_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = McContext::new(
            pl_net::Http::new(pl_net::HttpConfig::new("pl-mc-test")).unwrap(),
            crate::paths::DataDirs::new(tmp.path()),
        );
        assert!(matches!(
            load_manifest(&ctx, true).await,
            Err(Error::Offline)
        ));
        write_atomic(
            &ctx.dirs.meta().join(MANIFEST_CACHE_FILE),
            SAMPLE.as_bytes(),
        )
        .await
        .unwrap();
        let m = load_manifest(&ctx, true).await.unwrap();
        assert_eq!(m.latest.release, "26.1.2");
    }
}
