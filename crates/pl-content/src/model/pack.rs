use std::collections::{HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{default_true, is_http_url, is_https_url, is_valid_id};
use crate::error::{Error, Result};
use crate::fsutil::normalize_hex;
use crate::glob::glob_match;
use crate::path::check_relative_path;

/// The only `.mrpack` index format version.
pub const MRPACK_FORMAT_VERSION: u32 = 1;

/// `packs/<id>/<version>.json`: a Modrinth `.mrpack` index (`modrinth.index.json`) plus the
/// `projects` extension object. Modrinth App, Prism and Amethyst ignore the extension.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackManifest {
    pub format_version: u32,
    #[serde(default = "default_game")]
    pub game: String,
    pub version_id: String,
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub files: Vec<PackFile>,
    pub dependencies: PackDependencies,
    #[serde(default)]
    pub projects: ProjectsExtension,
}

fn default_game() -> String {
    "minecraft".to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackFile {
    /// `/`-separated path relative to the game directory.
    pub path: String,
    pub hashes: PackHashes,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub env: Option<PackEnv>,
    /// Absolute HTTPS URLs, tried in order (Modrinth CDN, our R2 copy, the GitHub mirror).
    #[serde(default)]
    pub downloads: Vec<String>,
    #[serde(default)]
    pub file_size: u64,
}

impl PackFile {
    /// Client-side requirement (`required` when `env` is absent).
    pub fn client_support(&self) -> EnvSupport {
        self.env.map_or(EnvSupport::Required, |env| env.client)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackHashes {
    pub sha1: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackEnv {
    #[serde(default)]
    pub client: EnvSupport,
    #[serde(default)]
    pub server: EnvSupport,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum EnvSupport {
    #[default]
    Required,
    Optional,
    Unsupported,
    /// Treated like `required`.
    #[serde(other)]
    Unknown,
}

/// `.mrpack` `dependencies` (game and loader versions).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PackDependencies {
    pub minecraft: String,
    #[serde(
        rename = "fabric-loader",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub fabric_loader: Option<String>,
    #[serde(
        rename = "quilt-loader",
        default,
        skip_serializing_if = "Option::is_none"
    )]
    pub quilt_loader: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub forge: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub neoforge: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum PackLoader {
    Fabric,
    Quilt,
    Forge,
    NeoForge,
}

impl PackDependencies {
    /// The mod loader and its version, if the pack needs one.
    pub fn loader(&self) -> Option<(PackLoader, &str)> {
        [
            (PackLoader::Fabric, &self.fabric_loader),
            (PackLoader::Quilt, &self.quilt_loader),
            (PackLoader::Forge, &self.forge),
            (PackLoader::NeoForge, &self.neoforge),
        ]
        .into_iter()
        .find_map(|(loader, version)| version.as_deref().map(|v| (loader, v)))
    }
}

/// The `projects` extension of a pack manifest.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ProjectsExtension {
    pub pack_id: String,
    /// The family this pack belongs to: one pack published for several Minecraft versions,
    /// one member (pack id) per version. `None`: the pack is its own family (`packId`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// Markdown shown after an update.
    pub changelog: String,
    /// Legacy switchable file sets. Files covered by [`Self::items`] ignore them.
    pub optional_groups: Vec<OptionalGroup>,
    /// The player-facing catalog (mod center): every item is optional and installed only when
    /// the player selected it or an enabled item requires it. Files no item covers behave as
    /// before (installed unless server-only or in a disabled group).
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub items: Vec<PackItem>,
    pub config_policy: ConfigPolicy,
    /// Servers added to `servers.dat` for new installs.
    pub servers: Vec<PackServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_memory_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_launcher_version: Option<semver::Version>,
}

impl ProjectsExtension {
    /// The family id: `family`, else the pack id.
    pub fn family_id(&self) -> &str {
        self.family
            .as_deref()
            .filter(|family| !family.is_empty())
            .unwrap_or(&self.pack_id)
    }

    pub fn item(&self, id: &str) -> Option<&PackItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// The item that installs `path`, if any.
    pub fn item_for_path(&self, path: &str) -> Option<&PackItem> {
        self.items.iter().find(|item| item.path == path)
    }
}

/// One entry of the mod center: a `files[]` entry the player can switch on or off.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackItem {
    /// Stable id (Studio uses the Modrinth project id or a slug). Choices are keyed by it, so a
    /// new file name for the same mod keeps the player's choice.
    pub id: String,
    /// The `files[].path` this item installs.
    pub path: String,
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub category: ItemCategory,
    /// `https://` icon shown next to the title.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_url: Option<String>,
    /// The mod's official page (`https://`), opened in the browser from the mod center.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub page_url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub modrinth: Option<ModrinthRef>,
    /// Selected on a brand-new install. Items added later start switched off for existing
    /// players, whatever this says.
    #[serde(default = "default_true")]
    pub default_enabled: bool,
    /// Ids of items that must be installed with this one.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub requires: Vec<String>,
    /// Dependency only: installed exactly when an enabled item requires it, never offered as a
    /// choice.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub library: bool,
}

impl PackItem {
    /// The page to open in the browser: [`Self::page_url`] when it is an `https://` URL. Valid
    /// manifests only carry those; this guards links that come from anywhere else.
    pub fn page(&self) -> Option<&str> {
        self.page_url.as_deref().filter(|url| is_https_url(url))
    }
}

/// Mod center section of an item (display only; behaviour follows [`PackItem::library`]).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum ItemCategory {
    Performance,
    Visual,
    Interface,
    Gameplay,
    Library,
    /// Also every category a newer Studio adds.
    #[default]
    #[serde(other)]
    Other,
}

/// Where an item comes from on Modrinth.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModrinthRef {
    pub project_id: String,
    pub version_id: String,
}

/// Files the player can switch on or off as a unit. A file listed in several groups is
/// installed when any of them is enabled. Legacy: files covered by an item ignore groups.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OptionalGroup {
    pub id: String,
    pub name: String,
    #[serde(default)]
    pub description: String,
    #[serde(default)]
    pub default_enabled: bool,
    /// Paths of `files[]` entries.
    #[serde(default)]
    pub files: Vec<String>,
}

/// What the sync engine may do to a file that already exists.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum FilePolicy {
    /// Always make the file match the pack.
    #[default]
    Overwrite,
    /// Install only when the file does not exist.
    IfMissing,
    /// Update only while the player has not changed the file since we installed it.
    /// Unknown policies from a newer Studio map here (the safest choice).
    #[serde(other)]
    PreserveUserChanges,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ConfigPolicy {
    /// Policy for files no rule matches.
    pub default: FilePolicy,
    /// Checked top to bottom; the first matching glob wins.
    pub rules: Vec<PolicyRule>,
}

impl ConfigPolicy {
    pub fn policy_for(&self, path: &str) -> FilePolicy {
        self.rules
            .iter()
            .find(|rule| glob_match(&rule.glob, path))
            .map_or(self.default, |rule| rule.policy)
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PolicyRule {
    pub glob: String,
    pub policy: FilePolicy,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackServer {
    pub name: String,
    pub address: String,
}

impl PackManifest {
    /// Structural validation before anything touches the disk: format version, game, safe
    /// and unique paths (case-insensitively, no file/folder clashes), hash formats, HTTP(S)
    /// download URLs, unique group ids, the family id and the item catalog (see
    /// [`Self::validate_items`]). Fails with the first of [`Self::problems`].
    pub fn validate(&self) -> Result<()> {
        match self.problems().into_iter().next() {
            Some(problem) => Err(problem.into_error()),
            None => Ok(()),
        }
    }

    /// Every rule [`Self::validate`] checks that the manifest breaks: an unsupported format
    /// (alone; nothing else is checked then), the game, each file in manifest order (path,
    /// duplicate, hashes, download URLs), file/folder clashes, group ids, the family id, then
    /// the item rules ([`Self::item_problems`]). Empty when the manifest is valid. For tools
    /// that list all problems (Studio).
    pub fn problems(&self) -> Vec<ManifestProblem> {
        if self.format_version != MRPACK_FORMAT_VERSION {
            return vec![ManifestProblem::UnsupportedFormat {
                found: self.format_version,
            }];
        }
        let mut problems = Vec::new();
        if self.game != "minecraft" {
            problems.push(ManifestProblem::UnsupportedGame {
                game: self.game.clone(),
            });
        }
        let mut seen = HashSet::new();
        for file in &self.files {
            let path = || file.path.clone();
            match check_relative_path(&file.path) {
                Ok(()) => {}
                Err(Error::UnsafePath { reason, .. }) => {
                    problems.push(ManifestProblem::UnsafePath {
                        path: path(),
                        reason,
                    });
                }
                Err(_) => problems.push(ManifestProblem::UnsafePath {
                    path: path(),
                    reason: "invalid path",
                }),
            }
            if !seen.insert(file.path.to_lowercase()) {
                problems.push(ManifestProblem::DuplicatePath { path: path() });
            }
            if normalize_hex(&file.hashes.sha1, 40).is_none() {
                problems.push(ManifestProblem::BadSha1 { path: path() });
            }
            if let Some(sha512) = &file.hashes.sha512
                && normalize_hex(sha512, 128).is_none()
            {
                problems.push(ManifestProblem::BadSha512 { path: path() });
            }
            if file.downloads.is_empty() || !file.downloads.iter().all(|url| is_http_url(url)) {
                problems.push(ManifestProblem::BadDownloads { path: path() });
            }
        }
        // A file whose path is also the folder of another file, once per file.
        let mut clashes: Vec<&str> = Vec::new();
        for file in &self.files {
            let lower = file.path.to_lowercase();
            let Some(folder) = lower
                .match_indices('/')
                .map(|(index, _)| &lower[..index])
                .find(|folder| seen.contains(*folder))
            else {
                continue;
            };
            let clash = self
                .files
                .iter()
                .find(|other| other.path.to_lowercase() == folder)
                .map_or(file.path.as_str(), |other| other.path.as_str());
            if !clashes.contains(&clash) {
                clashes.push(clash);
                problems.push(ManifestProblem::FileAndFolder {
                    path: clash.to_owned(),
                });
            }
        }
        let mut groups = HashSet::new();
        for group in &self.projects.optional_groups {
            if group.id.is_empty() || !groups.insert(group.id.as_str()) {
                problems.push(ManifestProblem::BadGroupId {
                    id: group.id.clone(),
                });
            }
        }
        if let Some(family) = &self.projects.family
            && !is_valid_id(family)
        {
            problems.push(ManifestProblem::BadFamilyId {
                family: family.clone(),
            });
        }
        problems.extend(self.item_problems().into_iter().map(ManifestProblem::Item));
        problems
    }

    /// Item rules: valid and unique ids, each item points at a different, client-side
    /// `files[]` entry, has a title and an `https://` icon (if any), `requires` names existing
    /// items and has no cycles. Fails with the first of [`Self::item_problems`].
    pub fn validate_items(&self) -> Result<()> {
        match self.item_problems().into_iter().next() {
            Some(problem) => Err(Error::Invalid(problem.to_string())),
            None => Ok(()),
        }
    }

    /// Every broken item rule (see [`Self::validate_items`]), in manifest order: per-item
    /// problems first, then unknown requirements, then a `requires` cycle. Empty when the
    /// catalog is valid. For tools that list all problems (Studio).
    pub fn item_problems(&self) -> Vec<ItemProblem> {
        let items = &self.projects.items;
        let mut problems = Vec::new();
        let mut ids = HashSet::new();
        let mut paths = HashSet::new();
        for item in items {
            let id = || item.id.clone();
            if !is_valid_id(&item.id) {
                problems.push(ItemProblem::BadId { id: id() });
            } else if !ids.insert(item.id.as_str()) {
                problems.push(ItemProblem::DuplicateId { id: id() });
            }
            match self.file(&item.path) {
                None => problems.push(ItemProblem::MissingFile {
                    id: id(),
                    path: item.path.clone(),
                }),
                Some(file) => {
                    if !paths.insert(item.path.as_str()) {
                        problems.push(ItemProblem::SharedFile {
                            id: id(),
                            path: item.path.clone(),
                        });
                    }
                    if file.client_support() == EnvSupport::Unsupported {
                        problems.push(ItemProblem::ServerOnly {
                            id: id(),
                            path: item.path.clone(),
                        });
                    }
                }
            }
            if item.title.trim().is_empty() {
                problems.push(ItemProblem::NoTitle { id: id() });
            }
            if let Some(url) = &item.icon_url
                && !is_https_url(url)
            {
                problems.push(ItemProblem::InsecureIcon { id: id() });
            }
            if let Some(url) = &item.page_url
                && !is_https_url(url)
            {
                problems.push(ItemProblem::InsecurePage { id: id() });
            }
        }
        let known: HashSet<&str> = items.iter().map(|item| item.id.as_str()).collect();
        for item in items {
            for missing in item
                .requires
                .iter()
                .filter(|required| !known.contains(required.as_str()))
            {
                problems.push(ItemProblem::UnknownRequirement {
                    id: item.id.clone(),
                    missing: missing.clone(),
                });
            }
        }
        if let Some(id) = requires_cycle(items) {
            problems.push(ItemProblem::Cycle { id: id.to_owned() });
        }
        problems
    }

    pub fn file(&self, path: &str) -> Option<&PackFile> {
        self.files.iter().find(|file| file.path == path)
    }
}

/// A broken item rule (see [`PackManifest::item_problems`]). `Display` gives the text that
/// [`PackManifest::validate_items`] reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ItemProblem {
    /// The id is empty, longer than 64 bytes or uses characters outside `[A-Za-z0-9._-]`.
    BadId { id: String },
    /// An earlier item has the same id.
    DuplicateId { id: String },
    /// `path` is not a `files[]` entry.
    MissingFile { id: String, path: String },
    /// An earlier item installs the same file.
    SharedFile { id: String, path: String },
    /// The file is server-only (`env.client: unsupported`).
    ServerOnly { id: String, path: String },
    /// The title is blank.
    NoTitle { id: String },
    /// `iconUrl` is not an `https://` URL.
    InsecureIcon { id: String },
    /// `pageUrl` is not an `https://` URL.
    InsecurePage { id: String },
    /// `requires` names an id no item has.
    UnknownRequirement { id: String, missing: String },
    /// The item is part of a `requires` cycle.
    Cycle { id: String },
}

impl ItemProblem {
    /// The id of the item the problem is about.
    pub fn item_id(&self) -> &str {
        match self {
            Self::BadId { id }
            | Self::DuplicateId { id }
            | Self::MissingFile { id, .. }
            | Self::SharedFile { id, .. }
            | Self::ServerOnly { id, .. }
            | Self::NoTitle { id }
            | Self::InsecureIcon { id }
            | Self::InsecurePage { id }
            | Self::UnknownRequirement { id, .. }
            | Self::Cycle { id } => id,
        }
    }
}

impl std::fmt::Display for ItemProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadId { id } | Self::DuplicateId { id } => {
                write!(f, "bad or duplicate item id {id:?}")
            }
            Self::MissingFile { id, path } => {
                write!(f, "item {id} points at {path}, which is not a pack file")
            }
            Self::SharedFile { path, .. } => write!(f, "two items install {path}"),
            Self::ServerOnly { id, .. } => write!(f, "item {id} installs a server-only file"),
            Self::NoTitle { id } => write!(f, "item {id} has no title"),
            Self::InsecureIcon { id } => write!(f, "item {id} has a non-HTTPS icon"),
            Self::InsecurePage { id } => write!(f, "item {id} has a non-HTTPS page"),
            Self::UnknownRequirement { id, missing } => {
                write!(f, "item {id} requires the unknown item {missing}")
            }
            Self::Cycle { id } => write!(f, "item {id} is part of a requires cycle"),
        }
    }
}

/// A broken manifest rule (see [`PackManifest::problems`]). `Display` gives the text that
/// [`PackManifest::validate`] reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ManifestProblem {
    /// `formatVersion` is not [`MRPACK_FORMAT_VERSION`].
    UnsupportedFormat { found: u32 },
    /// `game` is not `minecraft`.
    UnsupportedGame { game: String },
    /// `path` breaks the path rules (see [`check_relative_path`]).
    UnsafePath { path: String, reason: &'static str },
    /// An earlier file has the same path (compared case-insensitively).
    DuplicatePath { path: String },
    /// `hashes.sha1` is not 40 hex digits.
    BadSha1 { path: String },
    /// `hashes.sha512` is not 128 hex digits.
    BadSha512 { path: String },
    /// No download URL, or one that is not HTTP(S).
    BadDownloads { path: String },
    /// The file at `path` is also the folder of another file.
    FileAndFolder { path: String },
    /// An optional group id is empty or used twice.
    BadGroupId { id: String },
    /// `projects.family` is not 1–64 characters from `[A-Za-z0-9._-]`.
    BadFamilyId { family: String },
    /// A broken item rule.
    Item(ItemProblem),
}

impl ManifestProblem {
    /// The `files[]` path the problem is about, if it is about one file.
    pub fn path(&self) -> Option<&str> {
        match self {
            Self::UnsafePath { path, .. }
            | Self::DuplicatePath { path }
            | Self::BadSha1 { path }
            | Self::BadSha512 { path }
            | Self::BadDownloads { path }
            | Self::FileAndFolder { path }
            | Self::Item(
                ItemProblem::MissingFile { path, .. }
                | ItemProblem::SharedFile { path, .. }
                | ItemProblem::ServerOnly { path, .. },
            ) => Some(path.as_str()),
            _ => None,
        }
    }

    /// The error [`PackManifest::validate`] fails with.
    fn into_error(self) -> Error {
        match self {
            Self::UnsupportedFormat { found } => Error::UnsupportedSchema {
                what: "pack manifest".into(),
                found,
                supported: MRPACK_FORMAT_VERSION,
            },
            Self::UnsafePath { path, reason } => Error::UnsafePath { path, reason },
            other => Error::Invalid(other.to_string()),
        }
    }
}

impl std::fmt::Display for ManifestProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedFormat { found } => {
                write!(f, "unsupported pack manifest format {found}")
            }
            Self::UnsupportedGame { game } => write!(f, "unsupported game {game:?}"),
            Self::UnsafePath { path, reason } => write!(f, "unsafe path {path:?}: {reason}"),
            Self::DuplicatePath { path } => write!(f, "duplicate pack path {path}"),
            Self::BadSha1 { path } => write!(f, "bad sha1 for {path}"),
            Self::BadSha512 { path } => write!(f, "bad sha512 for {path}"),
            Self::BadDownloads { path } => write!(f, "bad download URLs for {path}"),
            Self::FileAndFolder { path } => write!(f, "{path} is both a file and a folder"),
            Self::BadGroupId { id } => write!(f, "bad optional group id {id:?}"),
            Self::BadFamilyId { family } => write!(f, "bad pack family id {family:?}"),
            Self::Item(problem) => problem.fmt(f),
        }
    }
}

/// An item on a `requires` cycle (depth-first search), if there is one.
fn requires_cycle(items: &[PackItem]) -> Option<&str> {
    let by_id: HashMap<&str, &PackItem> =
        items.iter().map(|item| (item.id.as_str(), item)).collect();
    let mut done: HashSet<&str> = HashSet::new();
    for start in items {
        if done.contains(start.id.as_str()) {
            continue;
        }
        // The current path: (item id, index of the next requirement to visit).
        let mut path: Vec<(&str, usize)> = vec![(start.id.as_str(), 0)];
        let mut on_path: HashSet<&str> = HashSet::from([start.id.as_str()]);
        while let Some(&(id, next)) = path.last() {
            let requires = by_id
                .get(id)
                .map_or(&[][..], |item| item.requires.as_slice());
            let Some(required) = requires.get(next) else {
                path.pop();
                on_path.remove(id);
                done.insert(id);
                continue;
            };
            if let Some(top) = path.last_mut() {
                top.1 += 1;
            }
            let required = required.as_str();
            if on_path.contains(required) {
                return Some(required);
            }
            if !done.contains(required) && by_id.contains_key(required) {
                on_path.insert(required);
                path.push((required, 0));
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn file(path: &str) -> PackFile {
        PackFile {
            path: path.into(),
            hashes: PackHashes {
                sha1: "a".repeat(40),
                sha512: None,
            },
            env: None,
            downloads: vec!["https://cdn.modrinth.com/x.jar".into()],
            file_size: 1,
        }
    }

    fn manifest(files: Vec<PackFile>) -> PackManifest {
        PackManifest {
            format_version: 1,
            game: "minecraft".into(),
            version_id: "1.0.0".into(),
            name: "Test".into(),
            summary: None,
            files,
            dependencies: PackDependencies {
                minecraft: "1.21.1".into(),
                fabric_loader: Some("0.16.9".into()),
                ..PackDependencies::default()
            },
            projects: ProjectsExtension::default(),
        }
    }

    #[test]
    fn policy_rules_first_match_wins() {
        let policy = ConfigPolicy {
            default: FilePolicy::Overwrite,
            rules: vec![
                PolicyRule {
                    glob: "config/keep/**".into(),
                    policy: FilePolicy::IfMissing,
                },
                PolicyRule {
                    glob: "config/**".into(),
                    policy: FilePolicy::PreserveUserChanges,
                },
                PolicyRule {
                    glob: "options.txt".into(),
                    policy: FilePolicy::IfMissing,
                },
            ],
        };
        assert_eq!(policy.policy_for("mods/a.jar"), FilePolicy::Overwrite);
        assert_eq!(
            policy.policy_for("config/a.json"),
            FilePolicy::PreserveUserChanges
        );
        assert_eq!(
            policy.policy_for("config/keep/a.json"),
            FilePolicy::IfMissing
        );
        assert_eq!(policy.policy_for("options.txt"), FilePolicy::IfMissing);
    }

    #[test]
    fn unknown_policy_is_conservative() {
        let policy: FilePolicy = serde_json::from_str("\"mergeJson\"").unwrap();
        assert_eq!(policy, FilePolicy::PreserveUserChanges);
        let policy: FilePolicy = serde_json::from_str("\"ifMissing\"").unwrap();
        assert_eq!(policy, FilePolicy::IfMissing);
    }

    #[test]
    fn validation() {
        manifest(vec![file("mods/a.jar"), file("config/a.json")])
            .validate()
            .unwrap();
        assert!(matches!(
            manifest(vec![file("../a.jar")]).validate(),
            Err(Error::UnsafePath { .. })
        ));
        assert!(
            manifest(vec![file("mods/a.jar"), file("MODS/A.jar")])
                .validate()
                .is_err()
        );
        assert!(
            manifest(vec![file("config"), file("config/x.json")])
                .validate()
                .is_err()
        );
        let mut bad_hash = file("mods/a.jar");
        bad_hash.hashes.sha1 = "xyz".into();
        assert!(manifest(vec![bad_hash]).validate().is_err());
        let mut bad_url = file("mods/a.jar");
        bad_url.downloads = vec!["file:///etc/passwd".into()];
        assert!(manifest(vec![bad_url]).validate().is_err());
        let mut wrong_version = manifest(vec![]);
        wrong_version.format_version = 2;
        assert!(matches!(
            wrong_version.validate(),
            Err(Error::UnsupportedSchema { .. })
        ));
    }

    fn group(id: &str) -> OptionalGroup {
        OptionalGroup {
            id: id.into(),
            name: id.into(),
            description: String::new(),
            default_enabled: false,
            files: Vec::new(),
        }
    }

    #[test]
    fn every_manifest_problem_is_listed() {
        let mut bad_hashes = file("mods/b.jar");
        bad_hashes.hashes.sha1 = "xyz".into();
        bad_hashes.hashes.sha512 = Some("short".into());
        let mut no_downloads = file("mods/c.jar");
        no_downloads.downloads.clear();
        let mut m = manifest(vec![
            file("mods/a.jar"),
            file("../escape.jar"),
            file("MODS/A.jar"),
            bad_hashes,
            no_downloads,
            file("config"),
            file("config/x.json"),
            file("config/y.json"),
        ]);
        m.game = "terraria".into();
        m.projects.optional_groups = vec![group(""), group("g"), group("g")];
        m.projects.family = Some("bad family".into());
        m.projects.items = vec![PackItem {
            title: String::new(),
            ..item("a", "mods/a.jar")
        }];
        let problems = m.problems();
        assert_eq!(
            problems,
            [
                ManifestProblem::UnsupportedGame {
                    game: "terraria".into()
                },
                ManifestProblem::UnsafePath {
                    path: "../escape.jar".into(),
                    reason: "relative path component"
                },
                ManifestProblem::DuplicatePath {
                    path: "MODS/A.jar".into()
                },
                ManifestProblem::BadSha1 {
                    path: "mods/b.jar".into()
                },
                ManifestProblem::BadSha512 {
                    path: "mods/b.jar".into()
                },
                ManifestProblem::BadDownloads {
                    path: "mods/c.jar".into()
                },
                ManifestProblem::FileAndFolder {
                    path: "config".into()
                },
                ManifestProblem::BadGroupId { id: String::new() },
                ManifestProblem::BadGroupId { id: "g".into() },
                ManifestProblem::BadFamilyId {
                    family: "bad family".into()
                },
                ManifestProblem::Item(ItemProblem::NoTitle { id: "a".into() }),
            ]
        );
        assert_eq!(problems[0].path(), None);
        assert_eq!(problems[1].path(), Some("../escape.jar"));
        assert_eq!(problems[6].path(), Some("config"));

        // `validate` fails with the first problem, with the same error as before.
        assert_eq!(
            m.validate().unwrap_err().to_string(),
            "invalid data: unsupported game \"terraria\""
        );
        m.game = "minecraft".into();
        assert!(matches!(m.validate(), Err(Error::UnsafePath { .. })));
        m.files.remove(1);
        assert_eq!(
            m.validate().unwrap_err().to_string(),
            "invalid data: duplicate pack path MODS/A.jar"
        );

        // An unsupported format is the only problem reported.
        m.format_version = 2;
        assert_eq!(
            m.problems(),
            [ManifestProblem::UnsupportedFormat { found: 2 }]
        );
        assert!(
            manifest(vec![file("mods/a.jar"), file("config/a.json")])
                .problems()
                .is_empty()
        );
    }

    fn item(id: &str, path: &str) -> PackItem {
        PackItem {
            id: id.into(),
            path: path.into(),
            title: id.to_uppercase(),
            summary: None,
            category: ItemCategory::Performance,
            icon_url: None,
            page_url: None,
            modrinth: None,
            default_enabled: true,
            requires: Vec::new(),
            library: false,
        }
    }

    fn requiring(id: &str, path: &str, requires: &[&str]) -> PackItem {
        PackItem {
            requires: requires.iter().map(|r| (*r).to_owned()).collect(),
            ..item(id, path)
        }
    }

    #[test]
    fn page_url_is_optional_and_round_trips() {
        let json = r#"{"id":"wi-zoom","path":"mods/a.jar","title":"WI Zoom","pageUrl":"https://www.wimods.net/wi-zoom/"}"#;
        let item: PackItem = serde_json::from_str(json).unwrap();
        assert_eq!(
            item.page_url.as_deref(),
            Some("https://www.wimods.net/wi-zoom/")
        );
        assert!(
            serde_json::to_string(&item)
                .unwrap()
                .contains(r#""pageUrl":"https://www.wimods.net/wi-zoom/""#)
        );
        // Written before the field existed: no page, and none is written back.
        let old: PackItem =
            serde_json::from_str(r#"{"id":"a","path":"mods/a.jar","title":"A"}"#).unwrap();
        assert_eq!(old.page_url, None);
        assert!(!serde_json::to_string(&old).unwrap().contains("pageUrl"));
        assert_eq!(old.page(), None);
        // Only https pages are ever opened.
        assert_eq!(item.page(), Some("https://www.wimods.net/wi-zoom/"));
        for unsafe_page in [
            "http://wimods.net/",
            "file:///C:/Windows",
            "javascript:x",
            "wimods",
        ] {
            let item = PackItem {
                page_url: Some(unsafe_page.into()),
                ..old.clone()
            };
            assert_eq!(item.page(), None, "{unsafe_page}");
        }
    }

    fn with_items(items: Vec<PackItem>) -> PackManifest {
        let mut m = manifest(vec![
            file("mods/a.jar"),
            file("mods/b.jar"),
            file("mods/c.jar"),
            file("options.txt"),
        ]);
        m.projects.items = items;
        m
    }

    #[test]
    fn item_validation() {
        let invalid = |items: Vec<PackItem>| with_items(items).validate().unwrap_err().to_string();
        // A diamond (a → b, a → c, b → c) is fine.
        with_items(vec![
            requiring("a", "mods/a.jar", &["b", "c"]),
            requiring("b", "mods/b.jar", &["c"]),
            item("c", "mods/c.jar"),
        ])
        .validate()
        .unwrap();

        assert!(
            invalid(vec![item("a", "mods/a.jar"), item("a", "mods/b.jar")])
                .contains("duplicate item id")
        );
        assert!(invalid(vec![item("bad id", "mods/a.jar")]).contains("item id"));
        assert!(invalid(vec![item("", "mods/a.jar")]).contains("item id"));
        assert!(invalid(vec![item("a", "mods/missing.jar")]).contains("not a pack file"));
        assert!(
            invalid(vec![item("a", "mods/a.jar"), item("b", "mods/a.jar")]).contains("two items")
        );
        let untitled = PackItem {
            title: " ".into(),
            ..item("a", "mods/a.jar")
        };
        assert!(invalid(vec![untitled]).contains("no title"));
        let insecure_icon = PackItem {
            icon_url: Some("http://cdn.example/icon.png".into()),
            ..item("a", "mods/a.jar")
        };
        assert!(invalid(vec![insecure_icon]).contains("HTTPS"));
        for page in [
            "http://wimods.net/wi-zoom/",
            "javascript:alert(1)",
            "wimods.net",
        ] {
            let insecure_page = PackItem {
                page_url: Some(page.into()),
                ..item("a", "mods/a.jar")
            };
            assert!(
                invalid(vec![insecure_page]).contains("non-HTTPS page"),
                "{page}"
            );
        }
        with_items(vec![PackItem {
            page_url: Some("https://www.wimods.net/wi-zoom/".into()),
            ..item("a", "mods/a.jar")
        }])
        .validate()
        .unwrap();
        assert!(invalid(vec![requiring("a", "mods/a.jar", &["zzz"])]).contains("unknown item zzz"));
        assert!(
            invalid(vec![
                requiring("a", "mods/a.jar", &["b"]),
                requiring("b", "mods/b.jar", &["c"]),
                requiring("c", "mods/c.jar", &["a"]),
            ])
            .contains("cycle")
        );
        assert!(invalid(vec![requiring("a", "mods/a.jar", &["a"])]).contains("cycle"));

        let mut server_only = with_items(vec![item("a", "mods/a.jar")]);
        server_only.files[0].env = Some(PackEnv {
            client: EnvSupport::Unsupported,
            server: EnvSupport::Required,
        });
        assert!(
            server_only
                .validate()
                .unwrap_err()
                .to_string()
                .contains("server-only")
        );

        // Every problem is listed, not only the first one.
        let untitled_insecure = PackItem {
            title: String::new(),
            icon_url: Some("http://cdn.example/icon.png".into()),
            page_url: Some("http://cdn.example/".into()),
            ..item("b", "mods/b.jar")
        };
        let problems = with_items(vec![
            requiring("a", "mods/a.jar", &["zzz"]),
            untitled_insecure,
            item("a", "mods/c.jar"),
            item("d", "mods/missing.jar"),
        ])
        .item_problems();
        assert_eq!(
            problems,
            [
                ItemProblem::NoTitle { id: "b".into() },
                ItemProblem::InsecureIcon { id: "b".into() },
                ItemProblem::InsecurePage { id: "b".into() },
                ItemProblem::DuplicateId { id: "a".into() },
                ItemProblem::MissingFile {
                    id: "d".into(),
                    path: "mods/missing.jar".into()
                },
                ItemProblem::UnknownRequirement {
                    id: "a".into(),
                    missing: "zzz".into()
                },
            ]
        );
        assert_eq!(problems[3].item_id(), "a");
        assert_eq!(
            invalid(vec![
                requiring("a", "mods/a.jar", &["zzz"]),
                item("a", "mods/b.jar")
            ]),
            "invalid data: bad or duplicate item id \"a\"",
            "validate_items reports the first problem"
        );
        assert!(
            with_items(vec![item("a", "mods/a.jar")])
                .item_problems()
                .is_empty()
        );

        let mut family = with_items(Vec::new());
        assert_eq!(family.projects.family_id(), "");
        family.projects.pack_id = "projects-1.21.4".into();
        assert_eq!(family.projects.family_id(), "projects-1.21.4");
        family.projects.family = Some("projects".into());
        family.validate().unwrap();
        assert_eq!(family.projects.family_id(), "projects");
        family.projects.family = Some("bad/family".into());
        assert!(family.validate().is_err());
    }

    #[test]
    fn item_defaults_and_old_manifests() {
        let parsed: PackItem = serde_json::from_str(
            r#"{ "id": "x", "path": "mods/x.jar", "title": "X", "category": "cosmetics" }"#,
        )
        .unwrap();
        assert!(parsed.default_enabled, "items default to enabled");
        assert!(!parsed.library && parsed.requires.is_empty());
        assert_eq!(parsed.category, ItemCategory::Other, "unknown categories");
        let json = serde_json::to_value(&parsed).unwrap();
        assert!(json.get("library").is_none() && json.get("requires").is_none());
        assert_eq!(json["category"], "other");

        // A manifest without the new fields serializes exactly as before.
        let old = manifest(vec![file("mods/a.jar")]);
        let json = serde_json::to_value(&old).unwrap();
        assert!(json["projects"].get("items").is_none());
        assert!(json["projects"].get("family").is_none());
        assert_eq!(old.projects.item_for_path("mods/a.jar"), None);
    }

    #[test]
    fn loader_detection_and_mrpack_names() {
        let m = manifest(vec![]);
        assert_eq!(
            m.dependencies.loader(),
            Some((PackLoader::Fabric, "0.16.9"))
        );
        let json = serde_json::to_value(&m).unwrap();
        assert_eq!(json["formatVersion"], 1);
        assert_eq!(json["versionId"], "1.0.0");
        assert_eq!(json["dependencies"]["fabric-loader"], "0.16.9");
        assert!(json["dependencies"].get("quilt-loader").is_none());
    }
}
