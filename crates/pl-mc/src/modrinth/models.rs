//! Modrinth API v2 models.
//!
//! The API speaks snake_case; these types are camelCase in both directions (one clean shape for
//! the UI bindings). [`super::ModrinthClient`] converts response keys before deserializing.
//! API enums accept unknown values as `Unknown` instead of failing the whole response.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::loaders::LoaderKind;

/// Enum over API string values with a lenient `Deserialize` (unknown → `Unknown`).
macro_rules! api_enum {
    ($(#[$meta:meta])* $name:ident { $($variant:ident => $text:literal),+ $(,)? }) => {
        $(#[$meta])*
        #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, specta::Type)]
        #[serde(rename_all = "camelCase")]
        pub enum $name {
            $($variant,)+
            /// A value this launcher does not know (yet).
            #[default]
            Unknown,
        }

        impl $name {
            /// The value as the Modrinth API spells it.
            pub fn as_str(self) -> &'static str {
                match self {
                    $(Self::$variant => $text,)+
                    Self::Unknown => "unknown",
                }
            }

            pub fn from_api(text: &str) -> Self {
                match text {
                    $($text => Self::$variant,)+
                    _ => Self::Unknown,
                }
            }
        }

        impl<'de> Deserialize<'de> for $name {
            fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
                let text = String::deserialize(deserializer)?;
                Ok(Self::from_api(&text))
            }
        }
    };
}

api_enum!(ModrinthProjectType {
    Mod => "mod",
    Modpack => "modpack",
    Resourcepack => "resourcepack",
    Shader => "shader",
    Datapack => "datapack",
    Plugin => "plugin",
});

api_enum!(
    /// Whether a project is needed on the client or server side.
    ModrinthSideSupport {
        Required => "required",
        Optional => "optional",
        Unsupported => "unsupported",
    }
);

api_enum!(ModrinthVersionType {
    Release => "release",
    Beta => "beta",
    Alpha => "alpha",
});

api_enum!(ModrinthDependencyType {
    Required => "required",
    Optional => "optional",
    Incompatible => "incompatible",
    Embedded => "embedded",
});

/// Search sort order (`index` parameter).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ModrinthSortIndex {
    #[default]
    Relevance,
    Downloads,
    Follows,
    Newest,
    Updated,
}

impl ModrinthSortIndex {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Relevance => "relevance",
            Self::Downloads => "downloads",
            Self::Follows => "follows",
            Self::Newest => "newest",
            Self::Updated => "updated",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ModrinthHashAlgorithm {
    #[default]
    Sha1,
    Sha512,
}

impl ModrinthHashAlgorithm {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Sha1 => "sha1",
            Self::Sha512 => "sha512",
        }
    }
}

/// Search parameters. Values inside `loaders` and `gameVersions` are OR-ed; every category
/// must match (AND).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthSearchQuery {
    pub query: Option<String>,
    pub project_type: Option<ModrinthProjectType>,
    /// Loader names as Modrinth spells them (`fabric`, `neoforge`, `iris`, …).
    pub loaders: Vec<String>,
    pub game_versions: Vec<String>,
    pub categories: Vec<String>,
    pub index: ModrinthSortIndex,
    pub offset: u32,
    /// 1–100.
    pub limit: u32,
}

impl Default for ModrinthSearchQuery {
    fn default() -> Self {
        Self {
            query: None,
            project_type: None,
            loaders: Vec::new(),
            game_versions: Vec::new(),
            categories: Vec::new(),
            index: ModrinthSortIndex::Relevance,
            offset: 0,
            limit: 20,
        }
    }
}

impl ModrinthSearchQuery {
    /// The `facets` parameter (JSON array of OR-groups), if any filter is set.
    pub fn facets(&self) -> Option<String> {
        let mut facets: Vec<Vec<String>> = Vec::new();
        if let Some(kind) = self.project_type {
            facets.push(vec![format!("project_type:{}", kind.as_str())]);
        }
        let group = |prefix: &str, values: &[String]| -> Vec<String> {
            values
                .iter()
                .filter(|v| !v.trim().is_empty())
                .map(|v| format!("{prefix}:{}", v.trim()))
                .collect()
        };
        let loaders = group("categories", &self.loaders);
        if !loaders.is_empty() {
            facets.push(loaders);
        }
        let versions = group("versions", &self.game_versions);
        if !versions.is_empty() {
            facets.push(versions);
        }
        for category in group("categories", &self.categories) {
            facets.push(vec![category]);
        }
        (!facets.is_empty()).then(|| serde_json::Value::from(facets).to_string())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthSearchResults {
    pub hits: Vec<ModrinthSearchHit>,
    pub offset: u32,
    pub limit: u32,
    pub total_hits: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthSearchHit {
    pub project_id: String,
    pub project_type: ModrinthProjectType,
    pub slug: Option<String>,
    pub author: String,
    pub title: String,
    pub description: String,
    pub categories: Vec<String>,
    pub display_categories: Option<Vec<String>>,
    /// Supported game versions.
    pub versions: Vec<String>,
    pub downloads: u64,
    pub follows: u64,
    pub icon_url: Option<String>,
    pub date_created: String,
    pub date_modified: String,
    pub latest_version: Option<String>,
    pub license: String,
    pub client_side: ModrinthSideSupport,
    pub server_side: ModrinthSideSupport,
    pub gallery: Option<Vec<String>>,
    pub featured_gallery: Option<String>,
    pub color: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthProject {
    pub id: String,
    pub slug: Option<String>,
    pub project_type: ModrinthProjectType,
    pub team: String,
    pub title: String,
    pub description: String,
    /// Long description (Markdown).
    pub body: String,
    pub published: String,
    pub updated: String,
    pub status: Option<String>,
    pub license: Option<ModrinthLicense>,
    pub client_side: ModrinthSideSupport,
    pub server_side: ModrinthSideSupport,
    pub downloads: u64,
    pub followers: u64,
    pub categories: Vec<String>,
    pub additional_categories: Option<Vec<String>>,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    /// Version ids.
    pub versions: Vec<String>,
    pub icon_url: Option<String>,
    pub issues_url: Option<String>,
    pub source_url: Option<String>,
    pub wiki_url: Option<String>,
    pub discord_url: Option<String>,
    pub donation_urls: Option<Vec<ModrinthDonationUrl>>,
    pub gallery: Option<Vec<ModrinthGalleryImage>>,
    pub color: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthLicense {
    pub id: String,
    pub name: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthDonationUrl {
    pub id: String,
    pub platform: String,
    pub url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthGalleryImage {
    pub url: String,
    pub featured: bool,
    pub title: Option<String>,
    pub description: Option<String>,
    pub created: Option<String>,
    pub ordering: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthVersion {
    pub id: String,
    pub project_id: String,
    pub author_id: Option<String>,
    pub name: String,
    pub version_number: String,
    pub changelog: Option<String>,
    pub dependencies: Vec<ModrinthDependency>,
    pub game_versions: Vec<String>,
    pub version_type: ModrinthVersionType,
    pub loaders: Vec<String>,
    pub featured: bool,
    pub status: Option<String>,
    pub date_published: String,
    pub downloads: u64,
    pub files: Vec<ModrinthFile>,
}

impl ModrinthVersion {
    /// The primary file, or the first one when none is flagged.
    pub fn primary_file(&self) -> Option<&ModrinthFile> {
        self.files
            .iter()
            .find(|f| f.primary)
            .or_else(|| self.files.first())
    }

    /// True if any file of this version has one of `hashes` (sha1 or sha512, lowercase hex).
    pub fn has_file_hash(&self, hashes: &HashSet<String>) -> bool {
        self.files.iter().any(|f| {
            [&f.hashes.sha1, &f.hashes.sha512]
                .into_iter()
                .flatten()
                .any(|h| hashes.contains(&h.to_ascii_lowercase()))
        })
    }

    pub fn required_dependencies(&self) -> impl Iterator<Item = &ModrinthDependency> {
        self.dependencies
            .iter()
            .filter(|d| d.dependency_type == ModrinthDependencyType::Required)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthDependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    /// Set for dependencies outside Modrinth.
    pub file_name: Option<String>,
    pub dependency_type: ModrinthDependencyType,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthFile {
    pub hashes: ModrinthHashes,
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
    /// `required-resource-pack`, `optional-resource-pack`, … (mostly unset).
    pub file_type: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthHashes {
    pub sha1: Option<String>,
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthCategory {
    /// SVG markup.
    pub icon: String,
    pub name: String,
    pub project_type: ModrinthProjectType,
    pub header: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthLoaderTag {
    /// SVG markup.
    pub icon: String,
    pub name: String,
    pub supported_project_types: Vec<ModrinthProjectType>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthGameVersionTag {
    pub version: String,
    /// `release`, `snapshot`, `alpha`, `beta`.
    pub version_type: String,
    pub date: String,
    pub major: bool,
}

/// Modrinth loader names whose mods run on `loader` for `game_version`: Quilt also loads Fabric
/// mods, NeoForge for 1.20.1 also loads Forge mods.
pub fn compatible_loaders(loader: LoaderKind, game_version: &str) -> Vec<String> {
    let names: &[&str] = match loader {
        LoaderKind::Vanilla => &[],
        LoaderKind::Fabric => &["fabric"],
        LoaderKind::Quilt => &["quilt", "fabric"],
        LoaderKind::Forge => &["forge"],
        LoaderKind::NeoForge if game_version == "1.20.1" => &["neoforge", "forge"],
        LoaderKind::NeoForge => &["neoforge"],
    };
    names.iter().map(|s| (*s).to_owned()).collect()
}

/// Which versions the dependency resolver may pick.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthResolveTarget {
    /// Accepted loader names (empty = any).
    pub loaders: Vec<String>,
    /// Accepted game versions (empty = any).
    pub game_versions: Vec<String>,
}

impl ModrinthResolveTarget {
    pub fn for_loader(loader: LoaderKind, game_version: impl Into<String>) -> Self {
        let game_version = game_version.into();
        Self {
            loaders: compatible_loaders(loader, &game_version),
            game_versions: vec![game_version],
        }
    }

    pub fn accepts(&self, version: &ModrinthVersion) -> bool {
        let loader_ok = self.loaders.is_empty()
            || version
                .loaders
                .iter()
                .any(|l| self.loaders.iter().any(|t| t.eq_ignore_ascii_case(l)));
        let game_ok = self.game_versions.is_empty()
            || version
                .game_versions
                .iter()
                .any(|g| self.game_versions.contains(g));
        loader_ok && game_ok
    }
}

/// What is already installed, to skip during dependency resolution.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub struct ModrinthInstalled {
    pub project_ids: HashSet<String>,
    /// Lowercase sha1/sha512 hex digests of installed files.
    pub hashes: HashSet<String>,
}

impl ModrinthInstalled {
    /// Build from installed file hashes and the result of
    /// [`super::ModrinthClient::version_files`] for them.
    pub fn from_lookup<'a>(
        hashes: impl IntoIterator<Item = &'a String>,
        found: &std::collections::HashMap<String, ModrinthVersion>,
    ) -> Self {
        Self {
            project_ids: found.values().map(|v| v.project_id.clone()).collect(),
            hashes: hashes.into_iter().map(|h| h.to_ascii_lowercase()).collect(),
        }
    }
}

/// Result of [`super::ModrinthClient::resolve_dependencies`].
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthDependencyResolution {
    /// Versions to install, in discovery order (closest dependencies first).
    pub to_install: Vec<ModrinthResolvedDependency>,
    /// Required projects that are already installed (project ids).
    pub already_installed: Vec<String>,
    pub unresolved: Vec<ModrinthUnresolvedDependency>,
    /// Installed projects that a version to install declares incompatible.
    pub conflicts: Vec<ModrinthDependencyConflict>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthResolvedDependency {
    pub project_id: String,
    pub title: Option<String>,
    pub icon_url: Option<String>,
    pub version: ModrinthVersion,
    /// Project id of the version that requires this one.
    pub required_by: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ModrinthUnresolvedReason {
    /// The project has no version for this loader/game version.
    NoCompatibleVersion,
    /// The project or version does not exist (anymore).
    NotFound,
    /// The dependency is a file outside Modrinth.
    External,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthUnresolvedDependency {
    pub project_id: Option<String>,
    pub version_id: Option<String>,
    pub file_name: Option<String>,
    pub title: Option<String>,
    pub required_by: String,
    pub reason: ModrinthUnresolvedReason,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthDependencyConflict {
    /// The installed project that is incompatible.
    pub project_id: String,
    pub title: Option<String>,
    /// Project id of the version that declares the incompatibility.
    pub declared_by: String,
}

/// An available update for an installed file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthUpdate {
    /// Hash of the installed file.
    pub hash: String,
    pub current: Option<ModrinthVersion>,
    pub latest: ModrinthVersion,
}
