//! Installable loader versions per Minecraft version.
//!
//! Sources: Fabric and Quilt meta (`/versions/loader/<mc>`), Forge `maven-metadata.xml` plus
//! `promotions_slim.json`, NeoForge `maven-metadata.xml` (and the `net.neoforged:forge` artifact
//! for 1.20.1). The normalized list is cached per loader and Minecraft version in
//! `meta/loaders/<loader>/<mc>.json`, which is used whenever the network fails.

use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;

use pl_net::NetError;
use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::install::McContext;
use crate::loaders::LoaderKind;
use crate::loaders::order::sort_newest_first;
use crate::paths::{safe_name, write_atomic};

pub(crate) const FABRIC_META_URL: &str = "https://meta.fabricmc.net/v2";
pub(crate) const QUILT_META_URL: &str = "https://meta.quiltmc.org/v3";
pub(crate) const FORGE_MAVEN_URL: &str = "https://maven.minecraftforge.net/";
pub(crate) const FORGE_METADATA_URL: &str =
    "https://maven.minecraftforge.net/net/minecraftforge/forge/maven-metadata.xml";
pub(crate) const FORGE_PROMOTIONS_URL: &str =
    "https://files.minecraftforge.net/net/minecraftforge/forge/promotions_slim.json";
pub(crate) const NEOFORGE_MAVEN_URL: &str = "https://maven.neoforged.net/releases/";
pub(crate) const NEOFORGE_METADATA_URL: &str =
    "https://maven.neoforged.net/releases/net/neoforged/neoforge/maven-metadata.xml";
pub(crate) const NEOFORGE_LEGACY_METADATA_URL: &str =
    "https://maven.neoforged.net/releases/net/neoforged/forge/maven-metadata.xml";

/// The only Minecraft version NeoForge published under the old `net.neoforged:forge` artifact.
pub(crate) const NEOFORGE_LEGACY_MC: &str = "1.20.1";

/// Cache format version (bump when [`CachedList`] changes incompatibly).
const CACHE_FORMAT: u32 = 1;

/// One installable loader version for a Minecraft version.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LoaderVersion {
    /// Version as shown to the user and passed to [`crate::loaders::install_loader`]: `0.16.10`,
    /// `47.4.10`, `21.1.262`. Forge versions are the loader part only (`10.13.4.1614`, not the
    /// Maven version `1.7.10-10.13.4.1614-1.7.10`).
    pub version: String,
    /// Not a beta/alpha/pre-release or experimental build.
    pub stable: bool,
    /// The version to preselect: Forge's promoted "recommended" build; Fabric, Quilt and NeoForge
    /// publish no recommendation, so their newest stable version. At most one entry is marked and
    /// there may be none (Forge before the first recommended build of a Minecraft version).
    pub recommended: bool,
}

/// A listed version plus what the installer needs internally.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct VersionEntry {
    pub version: String,
    pub stable: bool,
    pub recommended: bool,
    /// Full Maven version of the loader artifact when it differs from `version`
    /// (Forge: `1.7.10-10.13.4.1614-1.7.10`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub maven: Option<String>,
}

impl VersionEntry {
    fn public(&self) -> LoaderVersion {
        LoaderVersion {
            version: self.version.clone(),
            stable: self.stable,
            recommended: self.recommended,
        }
    }
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CachedList {
    format: u32,
    versions: Vec<VersionEntry>,
}

/// Where the version lists come from (tests point these at unreachable hosts).
#[derive(Debug, Clone)]
pub(crate) struct Endpoints {
    pub fabric: String,
    pub quilt: String,
    pub forge_metadata: String,
    pub forge_promotions: String,
    pub neoforge_metadata: String,
    pub neoforge_legacy_metadata: String,
}

impl Default for Endpoints {
    fn default() -> Self {
        Self {
            fabric: FABRIC_META_URL.to_owned(),
            quilt: QUILT_META_URL.to_owned(),
            forge_metadata: FORGE_METADATA_URL.to_owned(),
            forge_promotions: FORGE_PROMOTIONS_URL.to_owned(),
            neoforge_metadata: NEOFORGE_METADATA_URL.to_owned(),
            neoforge_legacy_metadata: NEOFORGE_LEGACY_METADATA_URL.to_owned(),
        }
    }
}

/// Installable versions of `kind` for Minecraft `mc_version`, newest first. Fetched online and
/// cached in `meta/loaders/`; when the network fails the cached list is returned
/// ([`Error::Offline`] when there is none). An empty list means the loader does not support that
/// Minecraft version.
pub async fn list_loader_versions(
    ctx: &McContext,
    kind: LoaderKind,
    mc_version: &str,
) -> Result<Vec<LoaderVersion>> {
    let entries = loader_entries(ctx, &Endpoints::default(), kind, mc_version.trim()).await?;
    Ok(entries.iter().map(VersionEntry::public).collect())
}

/// The Minecraft versions among `candidates` that `kind` has builds for, in the order given
/// (vanilla supports all of them). One request per loader, cached in
/// `meta/loaders/<loader>/_game-versions.json` and used whenever the network fails
/// ([`Error::Offline`] when there is no cache).
pub async fn supported_minecraft_versions(
    ctx: &McContext,
    kind: LoaderKind,
    candidates: &[String],
) -> Result<Vec<String>> {
    if kind == LoaderKind::Vanilla {
        return Ok(candidates.to_vec());
    }
    let source = game_version_source(ctx, &Endpoints::default(), kind).await?;
    Ok(candidates
        .iter()
        .filter(|mc| source.supports(kind, mc.trim()))
        .cloned()
        .collect())
}

/// What a loader publishes about the Minecraft versions it supports.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub(crate) struct GameVersionSource {
    format: u32,
    /// Fabric and Quilt: Minecraft versions. Forge and NeoForge: loader Maven versions.
    versions: Vec<String>,
    /// NeoForge builds for 1.20.1 exist (published under the legacy artifact).
    #[serde(default)]
    legacy: bool,
}

impl GameVersionSource {
    pub(crate) fn supports(&self, kind: LoaderKind, mc: &str) -> bool {
        match kind {
            LoaderKind::Vanilla => true,
            LoaderKind::Fabric | LoaderKind::Quilt => self.versions.iter().any(|v| v == mc),
            LoaderKind::Forge => self
                .versions
                .iter()
                .any(|maven| split_forge_version(mc, maven).is_some()),
            LoaderKind::NeoForge if mc == NEOFORGE_LEGACY_MC => self.legacy,
            LoaderKind::NeoForge => NeoForgeFilter::for_minecraft(mc)
                .is_some_and(|filter| self.versions.iter().any(|v| filter.matches(v))),
        }
    }
}

async fn game_version_source(
    ctx: &McContext,
    endpoints: &Endpoints,
    kind: LoaderKind,
) -> Result<GameVersionSource> {
    let path = ctx
        .dirs
        .meta()
        .join("loaders")
        .join(kind_dir(kind))
        .join("_game-versions.json");
    match fetch_game_versions(ctx, endpoints, kind).await {
        Ok(source) => {
            match serde_json::to_vec(&source) {
                Ok(bytes) => {
                    if let Err(error) = write_atomic(&path, &bytes).await {
                        tracing::warn!(%error, ?kind, "could not cache the supported game versions");
                    }
                }
                Err(error) => tracing::warn!(%error, "could not serialize the game versions"),
            }
            Ok(source)
        }
        Err(error) => {
            let cached = tokio::fs::read(&path)
                .await
                .ok()
                .and_then(|bytes| serde_json::from_slice::<GameVersionSource>(&bytes).ok())
                .filter(|cached| cached.format == CACHE_FORMAT);
            match cached {
                Some(cached) => {
                    tracing::warn!(%error, ?kind, "game version list fetch failed, using the cached copy");
                    Ok(cached)
                }
                None if error.is_offline() => Err(Error::Offline),
                None => Err(error),
            }
        }
    }
}

async fn fetch_game_versions(
    ctx: &McContext,
    endpoints: &Endpoints,
    kind: LoaderKind,
) -> Result<GameVersionSource> {
    let versions = match kind {
        LoaderKind::Vanilla => Vec::new(),
        LoaderKind::Fabric | LoaderKind::Quilt => {
            let base = if kind == LoaderKind::Fabric {
                &endpoints.fabric
            } else {
                &endpoints.quilt
            };
            let url = meta_url(base, &["versions", "game"])?;
            parse_meta_game_versions(&ctx.http.get_bytes(&url).await?)?
        }
        LoaderKind::Forge => {
            parse_maven_versions(&ctx.http.get_text(&endpoints.forge_metadata).await?)?
        }
        LoaderKind::NeoForge => {
            parse_maven_versions(&ctx.http.get_text(&endpoints.neoforge_metadata).await?)?
        }
    };
    let legacy = if kind == LoaderKind::NeoForge {
        match ctx.http.get_text(&endpoints.neoforge_legacy_metadata).await {
            Ok(xml) => parse_maven_versions(&xml)?
                .iter()
                .any(|v| v.starts_with(&format!("{NEOFORGE_LEGACY_MC}-"))),
            Err(error) => {
                tracing::warn!(%error, "NeoForge 1.20.1 builds could not be listed");
                false
            }
        }
    } else {
        false
    };
    Ok(GameVersionSource {
        format: CACHE_FORMAT,
        versions,
        legacy,
    })
}

/// Fabric/Quilt `/versions/game` answers: the Minecraft versions they support.
pub(crate) fn parse_meta_game_versions(bytes: &[u8]) -> Result<Vec<String>> {
    #[derive(Deserialize)]
    struct GameVersion {
        version: String,
    }
    let raw: Vec<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| Error::json("game version list", e))?;
    Ok(raw
        .into_iter()
        .filter_map(|value| serde_json::from_value::<GameVersion>(value).ok())
        .map(|entry| entry.version.trim().to_owned())
        .filter(|version| !version.is_empty())
        .collect())
}

/// Online list with cache fallback.
pub(crate) async fn loader_entries(
    ctx: &McContext,
    endpoints: &Endpoints,
    kind: LoaderKind,
    mc: &str,
) -> Result<Vec<VersionEntry>> {
    if kind == LoaderKind::Vanilla {
        return Ok(Vec::new());
    }
    safe_name(mc)?;
    match fetch_entries(ctx, endpoints, kind, mc).await {
        Ok(entries) => {
            if let Err(error) = write_cache(ctx, kind, mc, &entries).await {
                tracing::warn!(%error, ?kind, %mc, "could not cache the loader version list");
            }
            Ok(entries)
        }
        Err(error) => match read_cache(ctx, kind, mc).await {
            Some(cached) => {
                tracing::warn!(%error, ?kind, %mc, "loader version list fetch failed, using the cached copy");
                Ok(cached)
            }
            None if error.is_offline() => Err(Error::Offline),
            None => Err(error),
        },
    }
}

/// The cached list only (no network).
pub(crate) async fn read_cache(
    ctx: &McContext,
    kind: LoaderKind,
    mc: &str,
) -> Option<Vec<VersionEntry>> {
    let path = cache_path(ctx, kind, mc).ok()?;
    let bytes = tokio::fs::read(&path).await.ok()?;
    let cached: CachedList = serde_json::from_slice(&bytes).ok()?;
    (cached.format == CACHE_FORMAT).then_some(cached.versions)
}

async fn write_cache(
    ctx: &McContext,
    kind: LoaderKind,
    mc: &str,
    entries: &[VersionEntry],
) -> Result<()> {
    let path = cache_path(ctx, kind, mc)?;
    let cached = CachedList {
        format: CACHE_FORMAT,
        versions: entries.to_vec(),
    };
    let bytes = serde_json::to_vec(&cached).map_err(|e| Error::json("loader version cache", e))?;
    write_atomic(&path, &bytes).await
}

fn cache_path(ctx: &McContext, kind: LoaderKind, mc: &str) -> Result<PathBuf> {
    Ok(ctx
        .dirs
        .meta()
        .join("loaders")
        .join(kind_dir(kind))
        .join(format!("{}.json", safe_name(mc)?)))
}

pub(crate) fn kind_dir(kind: LoaderKind) -> &'static str {
    match kind {
        LoaderKind::Vanilla => "vanilla",
        LoaderKind::Fabric => "fabric",
        LoaderKind::Quilt => "quilt",
        LoaderKind::Forge => "forge",
        LoaderKind::NeoForge => "neoforge",
    }
}

async fn fetch_entries(
    ctx: &McContext,
    endpoints: &Endpoints,
    kind: LoaderKind,
    mc: &str,
) -> Result<Vec<VersionEntry>> {
    match kind {
        LoaderKind::Vanilla => Ok(Vec::new()),
        LoaderKind::Fabric | LoaderKind::Quilt => {
            let base = if kind == LoaderKind::Fabric {
                &endpoints.fabric
            } else {
                &endpoints.quilt
            };
            let url = meta_url(base, &["versions", "loader", mc])?;
            match ctx.http.get_bytes(&url).await {
                Ok(bytes) => parse_meta_loaders(&bytes),
                // Fabric answers 400 and Quilt 404 for Minecraft versions they do not know.
                Err(NetError::Status {
                    status: 400 | 404, ..
                }) => Ok(Vec::new()),
                Err(error) => Err(error.into()),
            }
        }
        LoaderKind::Forge => {
            let (metadata, promotions) = tokio::join!(
                ctx.http.get_text(&endpoints.forge_metadata),
                ctx.http.get_bytes(&endpoints.forge_promotions)
            );
            let versions = parse_maven_versions(&metadata?)?;
            let promotions = match promotions {
                Ok(bytes) => parse_promotions(&bytes).unwrap_or_else(|error| {
                    tracing::warn!(%error, "ignoring unreadable Forge promotions");
                    BTreeMap::new()
                }),
                Err(error) => {
                    tracing::warn!(%error, "Forge promotions unavailable; no recommended build");
                    BTreeMap::new()
                }
            };
            Ok(forge_entries(&versions, &promotions, mc))
        }
        LoaderKind::NeoForge if mc == NEOFORGE_LEGACY_MC => {
            let metadata = ctx
                .http
                .get_text(&endpoints.neoforge_legacy_metadata)
                .await?;
            Ok(neoforge_legacy_entries(
                &parse_maven_versions(&metadata)?,
                mc,
            ))
        }
        LoaderKind::NeoForge => {
            if NeoForgeFilter::for_minecraft(mc).is_none() {
                return Ok(Vec::new());
            }
            let metadata = ctx.http.get_text(&endpoints.neoforge_metadata).await?;
            Ok(neoforge_entries(&parse_maven_versions(&metadata)?, mc))
        }
    }
}

/// `base` + percent-encoded path segments.
pub(crate) fn meta_url(base: &str, segments: &[&str]) -> Result<String> {
    let mut url =
        url::Url::parse(base).map_err(|e| Error::Invalid(format!("meta URL `{base}`: {e}")))?;
    url.path_segments_mut()
        .map_err(|()| Error::Invalid(format!("meta URL `{base}` cannot have a path")))?
        .pop_if_empty()
        .extend(segments);
    Ok(url.into())
}

/// Sort newest first and, when `recommend_newest_stable`, mark the newest stable version.
fn finish(mut entries: Vec<VersionEntry>, recommend_newest_stable: bool) -> Vec<VersionEntry> {
    let mut seen = HashSet::new();
    entries.retain(|e| seen.insert(e.version.clone()));
    sort_newest_first(&mut entries, |e| e.version.as_str());
    if recommend_newest_stable && let Some(first) = entries.iter_mut().find(|e| e.stable) {
        first.recommended = true;
    }
    entries
}

#[derive(Deserialize)]
struct MetaEntry {
    loader: MetaLoader,
}

#[derive(Deserialize)]
struct MetaLoader {
    version: String,
    /// Fabric only; Quilt marks pre-releases in the version (`0.20.0-beta.9`).
    #[serde(default)]
    stable: Option<bool>,
}

/// Fabric/Quilt `/versions/loader/<mc>` answers.
pub(crate) fn parse_meta_loaders(bytes: &[u8]) -> Result<Vec<VersionEntry>> {
    let raw: Vec<serde_json::Value> =
        serde_json::from_slice(bytes).map_err(|e| Error::json("loader version list", e))?;
    let entries = raw
        .into_iter()
        .filter_map(|value| serde_json::from_value::<MetaEntry>(value).ok())
        .filter(|entry| !entry.loader.version.trim().is_empty())
        .map(|entry| {
            let version = entry.loader.version.trim().to_owned();
            VersionEntry {
                stable: entry
                    .loader
                    .stable
                    .unwrap_or_else(|| !version.contains('-')),
                version,
                recommended: false,
                maven: None,
            }
        })
        .collect();
    Ok(finish(entries, true))
}

/// `<versioning><versions><version>…` of a `maven-metadata.xml`, in document order.
pub(crate) fn parse_maven_versions(xml: &str) -> Result<Vec<String>> {
    use quick_xml::events::Event;

    let mut reader = quick_xml::Reader::from_str(xml);
    let mut stack: Vec<String> = Vec::new();
    let mut text = String::new();
    let mut versions = Vec::new();
    loop {
        match reader.read_event() {
            Ok(Event::Start(start)) => {
                stack.push(start.local_name().into_inner().to_owned());
                text.clear();
            }
            Ok(Event::Text(chunk)) => text.push_str(&chunk.xml10_content()),
            Ok(Event::CData(chunk)) => text.push_str(&chunk),
            Ok(Event::GeneralRef(reference)) => {
                let raw = format!("&{};", reference.xml10_content());
                match quick_xml::escape::unescape(&raw) {
                    Ok(resolved) => text.push_str(&resolved),
                    Err(_) => text.push_str(&raw),
                }
            }
            Ok(Event::End(_)) => {
                let len = stack.len();
                if len >= 2 && stack[len - 1] == "version" && stack[len - 2] == "versions" {
                    let version = text.trim();
                    if !version.is_empty() {
                        versions.push(version.to_owned());
                    }
                }
                stack.pop();
                text.clear();
            }
            Ok(Event::Eof) => break,
            Ok(_) => {}
            Err(error) => {
                return Err(Error::Invalid(format!("maven-metadata.xml: {error}")));
            }
        }
    }
    Ok(versions)
}

#[derive(Deserialize)]
struct Promotions {
    #[serde(default)]
    promos: BTreeMap<String, String>,
}

/// `promotions_slim.json`: `"<mc>-recommended"` / `"<mc>-latest"` → loader version.
pub(crate) fn parse_promotions(bytes: &[u8]) -> Result<BTreeMap<String, String>> {
    let parsed: Promotions =
        serde_json::from_slice(bytes).map_err(|e| Error::json("Forge promotions", e))?;
    Ok(parsed.promos)
}

/// Split a Forge Maven version of `mc` into its loader part and branch suffix:
/// `1.7.10-10.13.4.1614-1.7.10` → (`10.13.4.1614`, `Some("1.7.10")`).
pub(crate) fn split_forge_version<'a>(
    mc: &str,
    maven: &'a str,
) -> Option<(&'a str, Option<&'a str>)> {
    let rest = maven.strip_prefix(mc)?.strip_prefix('-')?;
    let (loader, branch) = match rest.split_once('-') {
        Some((loader, branch)) => (loader, Some(branch)),
        None => (rest, None),
    };
    (!loader.is_empty()).then_some((loader, branch))
}

/// Forge builds of `mc`: loader part as the version, Maven version kept internally. Builds from
/// experimental branches (`-failtests`, `-new`, pull request numbers) are unstable.
pub(crate) fn forge_entries(
    versions: &[String],
    promotions: &BTreeMap<String, String>,
    mc: &str,
) -> Vec<VersionEntry> {
    let recommended = promotions.get(&format!("{mc}-recommended"));
    let mut entries: Vec<VersionEntry> = Vec::new();
    for maven in versions {
        let Some((loader, branch)) = split_forge_version(mc, maven) else {
            continue;
        };
        let stable = match branch {
            None => true,
            Some(branch) => {
                branch == mc
                    || (branch.starts_with(|c: char| c.is_ascii_digit()) && branch.contains('.'))
            }
        };
        let entry = VersionEntry {
            version: loader.to_owned(),
            stable,
            recommended: recommended.is_some_and(|r| r == loader),
            maven: Some(maven.clone()),
        };
        // Prefer the build without an experimental branch when a loader version appears twice.
        match entries.iter_mut().find(|e| e.version == entry.version) {
            Some(existing) if !existing.stable && entry.stable => *existing = entry,
            Some(_) => {}
            None => entries.push(entry),
        }
    }
    finish(entries, false)
}

/// Which NeoForge versions belong to a Minecraft version.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NeoForgeFilter {
    /// Version prefix including the trailing dot (`21.1.`, `26.1.0.`).
    prefix: String,
    /// Required build metadata for snapshot builds (`snapshot-1` in `…-alpha.3+snapshot-1`);
    /// release versions only take versions without build metadata.
    build: Option<String>,
}

impl NeoForgeFilter {
    /// `1.X.Y` → `X.Y.`, `1.X` → `X.0.` (from 1.20.2 on); year-based `A.B.C` → `A.B.C.`, `A.B` →
    /// `A.B.0.`; year-based snapshots (`26.1-snapshot-1`, `26.1-pre-3`) → that prefix plus
    /// matching build metadata. 1.20.1 uses the legacy artifact and is not covered here.
    pub(crate) fn for_minecraft(mc: &str) -> Option<Self> {
        let (base, tag) = match mc.split_once('-') {
            Some((base, tag)) => (base, Some(tag)),
            None => (mc, None),
        };
        let parts: Vec<u32> = base
            .split('.')
            .map(|p| p.parse::<u32>().ok())
            .collect::<Option<Vec<_>>>()?;
        match parts.as_slice() {
            [1, minor, rest @ ..] if rest.len() <= 1 && tag.is_none() => {
                let patch = rest.first().copied().unwrap_or(0);
                if (*minor, patch) < (20, 2) {
                    return None;
                }
                Some(Self {
                    prefix: format!("{minor}.{patch}."),
                    build: None,
                })
            }
            [year, minor, rest @ ..] if *year >= 24 && rest.len() <= 1 => {
                let patch = rest.first().copied().unwrap_or(0);
                Some(Self {
                    prefix: format!("{year}.{minor}.{patch}."),
                    build: tag.map(str::to_owned),
                })
            }
            _ => None,
        }
    }

    pub(crate) fn matches(&self, version: &str) -> bool {
        let (main, build) = match version.split_once('+') {
            Some((main, build)) => (main, Some(build)),
            None => (version, None),
        };
        main.starts_with(&self.prefix) && build == self.build.as_deref()
    }
}

/// NeoForge builds of `mc` (`-beta`/`-alpha` builds are unstable).
pub(crate) fn neoforge_entries(versions: &[String], mc: &str) -> Vec<VersionEntry> {
    let Some(filter) = NeoForgeFilter::for_minecraft(mc) else {
        return Vec::new();
    };
    let entries = versions
        .iter()
        .filter(|v| filter.matches(v))
        .map(|v| VersionEntry {
            version: v.clone(),
            stable: !v.contains('-'),
            recommended: false,
            maven: None,
        })
        .collect();
    finish(entries, true)
}

/// NeoForge for 1.20.1 (`net.neoforged:forge:1.20.1-47.1.106`): loader part as the version.
pub(crate) fn neoforge_legacy_entries(versions: &[String], mc: &str) -> Vec<VersionEntry> {
    let entries = versions
        .iter()
        .filter_map(|maven| {
            let loader = maven.strip_prefix(mc)?.strip_prefix('-')?;
            (!loader.is_empty()).then(|| VersionEntry {
                version: loader.to_owned(),
                stable: !loader.contains('-'),
                recommended: false,
                maven: Some(maven.clone()),
            })
        })
        .collect();
    finish(entries, true)
}

#[cfg(test)]
mod tests {
    use pl_net::{Http, HttpConfig};

    use super::*;
    use crate::paths::DataDirs;

    const FORGE_XML: &str = r#"<?xml version="1.0" encoding="UTF-8"?>
<metadata>
  <groupId>net.minecraftforge</groupId>
  <artifactId>forge</artifactId>
  <versioning>
    <latest>1.20.4-49.2.13</latest>
    <release>1.20.4-49.2.13</release>
    <versions>
      <version>1.21-51.0.33</version>
      <version>1.20.1-47.4.9</version>
      <version>1.20.1-47.4.10</version>
      <version>1.20.1-47.4.28</version>
      <version>1.12.2-14.23.4.2720-4627</version>
      <version>1.12.2-14.23.5.2859</version>
      <version>1.10.2-12.18.1.2016-failtests</version>
      <version>1.7.10-10.13.4.1614-1.7.10</version>
      <version>1.7.10-10.13.0.1150</version>
      <version>1.7.10_pre4-10.12.2.1149-prerelease</version>
      <version>26.1.2-64.1.0</version>
    </versions>
    <lastUpdated>20261015000000</lastUpdated>
  </versioning>
</metadata>"#;

    fn versions() -> Vec<String> {
        parse_maven_versions(FORGE_XML).unwrap()
    }

    #[test]
    fn maven_metadata_versions_in_document_order() {
        let v = versions();
        assert_eq!(v.len(), 11);
        assert_eq!(v[0], "1.21-51.0.33");
        assert_eq!(v[10], "26.1.2-64.1.0");
        assert!(
            !v.iter().any(|x| x == "1.20.4-49.2.13"),
            "<latest> is not a version entry"
        );
        assert!(
            parse_maven_versions("<metadata><versioning><versions><version>1</version>").is_ok()
        );
        assert!(parse_maven_versions("<a></b>").is_err());
        let escaped = parse_maven_versions(
            "<m><versioning><versions><version>a&amp;b</version></versions></versioning></m>",
        )
        .unwrap();
        assert_eq!(escaped, ["a&b"]);
    }

    #[test]
    fn promotions_and_forge_entries() {
        let promos = parse_promotions(
            br#"{"homepage": "https://files.minecraftforge.net/", "promos": {
                "1.7.10-recommended": "10.13.4.1614", "1.7.10-latest": "10.13.4.1614",
                "1.20.1-recommended": "47.4.10", "1.20.1-latest": "47.4.28",
                "26.1.2-latest": "64.1.0"}}"#,
        )
        .unwrap();
        assert_eq!(promos["1.20.1-latest"], "47.4.28");

        let forge = forge_entries(&versions(), &promos, "1.20.1");
        let names: Vec<&str> = forge.iter().map(|e| e.version.as_str()).collect();
        assert_eq!(names, ["47.4.28", "47.4.10", "47.4.9"]);
        assert!(forge.iter().all(|e| e.stable));
        let recommended: Vec<&str> = forge
            .iter()
            .filter(|e| e.recommended)
            .map(|e| e.version.as_str())
            .collect();
        assert_eq!(recommended, ["47.4.10"]);
        assert_eq!(forge[1].maven.as_deref(), Some("1.20.1-47.4.10"));

        let old = forge_entries(&versions(), &promos, "1.7.10");
        assert_eq!(old.len(), 2, "1.7.10_pre4 is another Minecraft version");
        assert_eq!(old[0].version, "10.13.4.1614");
        assert_eq!(old[0].maven.as_deref(), Some("1.7.10-10.13.4.1614-1.7.10"));
        assert!(old[0].recommended && old[0].stable);
        assert_eq!(old[1].maven.as_deref(), Some("1.7.10-10.13.0.1150"));

        let pr = forge_entries(&versions(), &promos, "1.12.2");
        assert_eq!(pr[0].version, "14.23.5.2859");
        assert!(pr[0].stable);
        assert_eq!(pr[1].version, "14.23.4.2720");
        assert!(!pr[1].stable, "pull request build");
        assert!(!forge_entries(&versions(), &promos, "1.10.2")[0].stable);

        let year = forge_entries(&versions(), &promos, "26.1.2");
        assert_eq!(year.len(), 1);
        assert!(!year[0].recommended, "only `latest` is promoted");
        assert!(forge_entries(&versions(), &promos, "1.21.1").is_empty());
        assert_eq!(forge_entries(&versions(), &promos, "1.21").len(), 1);

        assert_eq!(
            split_forge_version("1.7.10", "1.7.10-10.13.4.1614-1.7.10"),
            Some(("10.13.4.1614", Some("1.7.10")))
        );
        assert_eq!(split_forge_version("1.7.1", "1.7.10-10.13.4.1614"), None);
    }

    fn neo_versions() -> Vec<String> {
        [
            "26.1.0.0-alpha.1+snapshot-1",
            "26.1.0.0-alpha.15+pre-3",
            "26.1.0.1-beta",
            "26.1.0.19-beta",
            "26.1.1.15-beta",
            "26.1.2.120",
            "26.1.2.121",
            "26.1.2.0-beta",
            "26.3.0.83",
            "26.3.0.75-beta",
            "0.25w14craftmine.3-beta",
            "20.2.3-beta",
            "20.2.93",
            "21.0.167",
            "21.1.1",
            "21.1.262",
            "21.1.259",
            "21.10.64",
            "21.11.48",
        ]
        .map(str::to_owned)
        .to_vec()
    }

    fn names(entries: &[VersionEntry]) -> Vec<&str> {
        entries.iter().map(|e| e.version.as_str()).collect()
    }

    #[test]
    fn neoforge_minecraft_mapping() {
        let all = neo_versions();
        let v1211 = neoforge_entries(&all, "1.21.1");
        assert_eq!(names(&v1211), ["21.1.262", "21.1.259", "21.1.1"]);
        assert!(v1211[0].recommended && v1211[0].stable);
        assert_eq!(names(&neoforge_entries(&all, "1.21")), ["21.0.167"]);
        assert_eq!(names(&neoforge_entries(&all, "1.21.10")), ["21.10.64"]);
        assert_eq!(
            names(&neoforge_entries(&all, "1.20.2")),
            ["20.2.93", "20.2.3-beta"]
        );
        assert!(
            neoforge_entries(&all, "1.20.1").is_empty(),
            "legacy artifact"
        );
        assert!(neoforge_entries(&all, "1.19.2").is_empty());

        // Year-based releases: `26.1` → `26.1.0.*` without snapshot builds.
        let v261 = neoforge_entries(&all, "26.1");
        assert_eq!(names(&v261), ["26.1.0.19-beta", "26.1.0.1-beta"]);
        assert!(v261.iter().all(|e| !e.stable && !e.recommended));
        assert_eq!(names(&neoforge_entries(&all, "26.1.1")), ["26.1.1.15-beta"]);
        let v2612 = neoforge_entries(&all, "26.1.2");
        assert_eq!(names(&v2612), ["26.1.2.121", "26.1.2.120", "26.1.2.0-beta"]);
        assert!(v2612[0].recommended);
        assert_eq!(
            names(&neoforge_entries(&all, "26.3")),
            ["26.3.0.83", "26.3.0.75-beta"]
        );
        // Snapshots map through build metadata.
        assert_eq!(
            names(&neoforge_entries(&all, "26.1-snapshot-1")),
            ["26.1.0.0-alpha.1+snapshot-1"]
        );
        assert_eq!(
            names(&neoforge_entries(&all, "26.1-pre-3")),
            ["26.1.0.0-alpha.15+pre-3"]
        );
        assert!(neoforge_entries(&all, "26.2").is_empty());
        assert!(NeoForgeFilter::for_minecraft("b1.7.3").is_none());
        assert!(NeoForgeFilter::for_minecraft("1.21-pre1").is_none());

        let legacy = neoforge_legacy_entries(
            &["1.20.1-47.1.7", "1.20.1-47.1.106", "1.20.1-47.1.11"].map(str::to_owned),
            "1.20.1",
        );
        assert_eq!(names(&legacy), ["47.1.106", "47.1.11", "47.1.7"]);
        assert_eq!(legacy[0].maven.as_deref(), Some("1.20.1-47.1.106"));
        assert!(legacy[0].recommended);
    }

    #[test]
    fn fabric_and_quilt_meta() {
        let fabric = parse_meta_loaders(
            br#"[{"loader": {"separator": ".", "build": 0, "maven": "net.fabricmc:fabric-loader:0.20.0", "version": "0.20.0", "stable": false}, "intermediary": {}},
                {"loader": {"version": "0.19.5", "stable": true}},
                {"loader": {"version": "0.19.4", "stable": false}},
                {"broken": true}]"#,
        )
        .unwrap();
        assert_eq!(names(&fabric), ["0.20.0", "0.19.5", "0.19.4"]);
        assert!(!fabric[0].stable && !fabric[0].recommended);
        assert!(fabric[1].stable && fabric[1].recommended);

        // Quilt has no `stable` flag and is not sorted.
        let quilt = parse_meta_loaders(
            br#"[{"loader": {"version": "0.20.0-beta.9"}}, {"loader": {"version": "0.24.0"}},
                {"loader": {"version": "0.30.1"}}, {"loader": {"version": "0.31.0-beta.1"}}]"#,
        )
        .unwrap();
        assert_eq!(
            names(&quilt),
            ["0.31.0-beta.1", "0.30.1", "0.24.0", "0.20.0-beta.9"]
        );
        assert!(!quilt[0].stable);
        assert!(quilt[1].recommended);
        assert_eq!(quilt.iter().filter(|e| e.recommended).count(), 1);
    }

    #[test]
    fn meta_urls_are_encoded() {
        assert_eq!(
            meta_url(FABRIC_META_URL, &["versions", "loader", "1.21.1"]).unwrap(),
            "https://meta.fabricmc.net/v2/versions/loader/1.21.1"
        );
        assert_eq!(
            meta_url(
                QUILT_META_URL,
                &["versions", "loader", "1.14 Pre-Release 1", "0.1+b/2"]
            )
            .unwrap(),
            "https://meta.quiltmc.org/v3/versions/loader/1.14%20Pre-Release%201/0.1+b%2F2"
        );
    }

    #[test]
    fn supported_minecraft_versions_per_loader() {
        let fabric = GameVersionSource {
            format: CACHE_FORMAT,
            versions: parse_meta_game_versions(
                br#"[{"version": "1.21.4", "stable": true}, {"version": "25w14a", "stable": false},
                    {"version": "1.20.1", "stable": true}, {"broken": 1}]"#,
            )
            .unwrap(),
            legacy: false,
        };
        assert_eq!(fabric.versions, ["1.21.4", "25w14a", "1.20.1"]);
        assert!(fabric.supports(LoaderKind::Fabric, "1.21.4"));
        assert!(!fabric.supports(LoaderKind::Fabric, "1.21.5"));

        let forge = GameVersionSource {
            format: CACHE_FORMAT,
            versions: versions(),
            legacy: false,
        };
        assert!(forge.supports(LoaderKind::Forge, "1.20.1"));
        assert!(forge.supports(LoaderKind::Forge, "1.7.10"));
        assert!(
            !forge.supports(LoaderKind::Forge, "1.7.1"),
            "not a prefix match"
        );
        assert!(!forge.supports(LoaderKind::Forge, "1.21.1"));

        let neo = GameVersionSource {
            format: CACHE_FORMAT,
            versions: neo_versions(),
            legacy: true,
        };
        assert!(neo.supports(LoaderKind::NeoForge, "1.21.1"));
        assert!(neo.supports(LoaderKind::NeoForge, "26.1.2"));
        assert!(
            neo.supports(LoaderKind::NeoForge, "1.20.1"),
            "legacy artifact"
        );
        assert!(!neo.supports(LoaderKind::NeoForge, "1.19.2"));
        assert!(!neo.supports(LoaderKind::NeoForge, "26.2"));
        assert!(neo.supports(LoaderKind::Vanilla, "b1.7.3"));
    }

    /// With every endpoint unreachable the cached list is used; without a cache the error is
    /// `Offline`.
    #[tokio::test]
    async fn network_failure_falls_back_to_cache() {
        let tmp = tempfile::tempdir().unwrap();
        let ctx = McContext::new(
            Http::new(HttpConfig {
                retries: 0,
                ..HttpConfig::new("pl-mc-test")
            })
            .unwrap(),
            DataDirs::new(tmp.path()),
        );
        let dead = "http://127.0.0.1:9/meta";
        let endpoints = Endpoints {
            fabric: dead.to_owned(),
            quilt: dead.to_owned(),
            forge_metadata: dead.to_owned(),
            forge_promotions: dead.to_owned(),
            neoforge_metadata: dead.to_owned(),
            neoforge_legacy_metadata: dead.to_owned(),
        };
        let missing = loader_entries(&ctx, &endpoints, LoaderKind::Fabric, "1.21.1").await;
        assert!(matches!(missing, Err(Error::Offline)), "{missing:?}");

        let cached = vec![VersionEntry {
            version: "0.16.10".into(),
            stable: true,
            recommended: true,
            maven: None,
        }];
        write_cache(&ctx, LoaderKind::Fabric, "1.21.1", &cached)
            .await
            .unwrap();
        let entries = loader_entries(&ctx, &endpoints, LoaderKind::Fabric, "1.21.1")
            .await
            .unwrap();
        assert_eq!(entries, cached);
        assert!(ctx.dirs.meta().join("loaders/fabric/1.21.1.json").is_file());
        // Minecraft versions without NeoForge need no request at all.
        let none = loader_entries(&ctx, &endpoints, LoaderKind::NeoForge, "1.16.5")
            .await
            .unwrap();
        assert!(none.is_empty());
        assert!(
            loader_entries(&ctx, &endpoints, LoaderKind::Vanilla, "1.21.1")
                .await
                .unwrap()
                .is_empty()
        );

        // The supported Minecraft versions fall back to their cache the same way.
        let missing = game_version_source(&ctx, &endpoints, LoaderKind::Quilt).await;
        assert!(matches!(missing, Err(Error::Offline)), "{missing:?}");
        let cached = GameVersionSource {
            format: CACHE_FORMAT,
            versions: vec!["1.21.1".into()],
            legacy: false,
        };
        write_atomic(
            &ctx.dirs.meta().join("loaders/quilt/_game-versions.json"),
            &serde_json::to_vec(&cached).unwrap(),
        )
        .await
        .unwrap();
        assert_eq!(
            game_version_source(&ctx, &endpoints, LoaderKind::Quilt)
                .await
                .unwrap(),
            cached
        );
    }
}
