//! Ready-made mod sets ("Hazır setler"): curated lists of Modrinth mods, resource packs and
//! shader packs that the team keeps in Studio. A player starts a new instance from a set or adds
//! one to an existing instance; everything a set brings stays optional and follows the mod
//! center rules (see `docs/content-format.md`, "Mod sets").
//!
//! A set has one variant per Minecraft version (and mod loader). A variant is a small pack: its
//! `files` and `items` have exactly the shapes of a pack manifest's, so the launcher installs a
//! set with the pack sync engine ([`ModSetVariant::manifest`]).

use std::collections::{BTreeSet, HashMap, HashSet};

use serde::{Deserialize, Serialize};

use super::{
    ConfigPolicy, FilePolicy, MRPACK_FORMAT_VERSION, ManifestProblem, PackDependencies, PackFile,
    PackItem, PackLoader, PackManifest, ProjectsExtension, check_schema, default_schema,
    is_https_url, is_valid_id, is_valid_minecraft_version,
};
use crate::error::{Error, Result};
use crate::fsutil::sha256_hex;
use crate::items::resolve_items;

/// Highest [`ModSetCatalog::schema`] this build understands.
pub const MOD_SET_CATALOG_SCHEMA: u32 = 1;
/// Most sets a catalog may hold.
pub const MAX_MOD_SETS: usize = 32;
/// Most variants (Minecraft version and loader pairs) a set may have.
pub const MAX_MOD_SET_VARIANTS: usize = 16;
/// Most items a variant may have.
pub const MAX_MOD_SET_ITEMS: usize = 64;
/// Longest set name, in characters (it is a card title).
pub const MAX_MOD_SET_NAME_CHARS: usize = 32;
/// Longest set summary, in characters (two lines on a card).
pub const MAX_MOD_SET_SUMMARY_CHARS: usize = 140;

/// `modsets/modsets.<sha256>.json`: every ready-made set, in the order the launcher shows them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModSetCatalog {
    #[serde(default = "default_schema")]
    pub schema: u32,
    #[serde(default)]
    pub sets: Vec<ModSet>,
}

impl Default for ModSetCatalog {
    fn default() -> Self {
        Self {
            schema: MOD_SET_CATALOG_SCHEMA,
            sets: Vec::new(),
        }
    }
}

/// What a set is for. The launcher draws its card's mark from it.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum ModSetPurpose {
    Pvp,
    Performance,
    Visual,
    /// Also every purpose a newer Studio adds.
    #[default]
    #[serde(other)]
    Other,
}

/// The English wording of a set (Studio writes Turkish; English players fall back to it when a
/// field is missing).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ModSetText {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
}

/// One ready-made set.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModSet {
    /// Stable id, 1–64 characters from `[A-Za-z0-9._-]`. Instances remember a set by it.
    pub id: String,
    pub name: String,
    /// One sentence: what the set is for and what it changes.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    #[serde(default)]
    pub purpose: ModSetPurpose,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub en: Option<ModSetText>,
    /// One per Minecraft version and loader, newest first.
    #[serde(default)]
    pub variants: Vec<ModSetVariant>,
    /// Older launchers do not offer the set.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_launcher_version: Option<semver::Version>,
}

/// Kind of a set item, from the folder of its file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ModSetItemKind {
    /// `mods/<name>.jar`, per Minecraft version.
    Mod,
    /// `resourcepacks/<name>.zip`, shared by every version of the instance.
    ResourcePack,
    /// `shaderpacks/<name>.zip`, shared by every version of the instance.
    ShaderPack,
}

impl ModSetItemKind {
    pub const ALL: [Self; 3] = [Self::Mod, Self::ResourcePack, Self::ShaderPack];

    /// The game folder the kind's files go to.
    pub fn folder(self) -> &'static str {
        match self {
            Self::Mod => "mods",
            Self::ResourcePack => "resourcepacks",
            Self::ShaderPack => "shaderpacks",
        }
    }

    /// The file extension the kind's files have (lowercase, without the dot).
    pub fn extension(self) -> &'static str {
        match self {
            Self::Mod => "jar",
            Self::ResourcePack | Self::ShaderPack => "zip",
        }
    }

    /// The kind of a set file path: a single file directly inside one of the three folders.
    pub fn of_path(path: &str) -> Option<Self> {
        let (folder, name) = path.split_once('/')?;
        if name.is_empty() || name.contains('/') {
            return None;
        }
        Self::ALL.into_iter().find(|kind| kind.folder() == folder)
    }
}

/// One Minecraft version (and loader) of a set: a small pack. `files` and `items` follow the
/// pack manifest rules; every file is a Modrinth file inside `mods/`, `resourcepacks/` or
/// `shaderpacks/` and is installed by exactly one item.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModSetVariant {
    /// `minecraft` plus at most one loader (`.mrpack` spelling). Without a loader the variant
    /// may only hold resource and shader packs and fits every instance on that version.
    pub dependencies: PackDependencies,
    #[serde(default)]
    pub files: Vec<PackFile>,
    #[serde(default)]
    pub items: Vec<PackItem>,
}

/// A broken rule of a catalog, a set or a variant (see [`ModSetCatalog::problems`]). `Display`
/// gives the English text logs and errors carry; Studio words them in Turkish.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ModSetProblem {
    /// The catalog's `schema` is newer than this build (nothing else is checked then).
    UnsupportedSchema { found: u32 },
    /// More than [`MAX_MOD_SETS`] sets.
    TooManySets { count: usize },
    /// The id is not 1–64 characters from `[A-Za-z0-9._-]`.
    BadSetId { set: String },
    /// An earlier set has the same id.
    DuplicateSetId { set: String },
    /// The name (Turkish or English) is blank or longer than [`MAX_MOD_SET_NAME_CHARS`].
    BadName { set: String },
    /// A summary (Turkish or English) is longer than [`MAX_MOD_SET_SUMMARY_CHARS`] or holds a
    /// line break.
    BadSummary { set: String },
    /// The set has no variant, or more than [`MAX_MOD_SET_VARIANTS`].
    BadVariantCount { set: String, count: usize },
    /// `dependencies.minecraft` is not a valid version id.
    BadMinecraft { set: String, minecraft: String },
    /// The variant names more than one loader.
    TwoLoaders { set: String, minecraft: String },
    /// An earlier variant has the same Minecraft version and loader.
    DuplicateVariant { set: String, minecraft: String },
    /// The variant has no item, or more than [`MAX_MOD_SET_ITEMS`].
    BadItemCount {
        set: String,
        minecraft: String,
        count: usize,
    },
    /// The variant installs mods but names no loader.
    ModsWithoutLoader { set: String, minecraft: String },
    /// A file breaks the set file rules (folder, extension, HTTPS download, size).
    BadFile {
        set: String,
        minecraft: String,
        path: String,
        reason: &'static str,
    },
    /// No item installs this file.
    UncoveredFile {
        set: String,
        minecraft: String,
        path: String,
    },
    /// An item has no Modrinth reference (sets only carry Modrinth files).
    NotModrinth {
        set: String,
        minecraft: String,
        item: String,
    },
    /// A pack manifest rule the variant breaks (paths, hashes, items, requirements).
    Manifest {
        set: String,
        minecraft: String,
        problem: ManifestProblem,
    },
}

impl ModSetProblem {
    /// The id of the set the problem is about (empty for catalog-wide problems).
    pub fn set_id(&self) -> &str {
        match self {
            Self::UnsupportedSchema { .. } | Self::TooManySets { .. } => "",
            Self::BadSetId { set }
            | Self::DuplicateSetId { set }
            | Self::BadName { set }
            | Self::BadSummary { set }
            | Self::BadVariantCount { set, .. }
            | Self::BadMinecraft { set, .. }
            | Self::TwoLoaders { set, .. }
            | Self::DuplicateVariant { set, .. }
            | Self::BadItemCount { set, .. }
            | Self::ModsWithoutLoader { set, .. }
            | Self::BadFile { set, .. }
            | Self::UncoveredFile { set, .. }
            | Self::NotModrinth { set, .. }
            | Self::Manifest { set, .. } => set,
        }
    }

    /// The Minecraft version of the variant the problem is about, if it is about one.
    pub fn minecraft(&self) -> Option<&str> {
        match self {
            Self::BadMinecraft { minecraft, .. }
            | Self::TwoLoaders { minecraft, .. }
            | Self::DuplicateVariant { minecraft, .. }
            | Self::BadItemCount { minecraft, .. }
            | Self::ModsWithoutLoader { minecraft, .. }
            | Self::BadFile { minecraft, .. }
            | Self::UncoveredFile { minecraft, .. }
            | Self::NotModrinth { minecraft, .. }
            | Self::Manifest { minecraft, .. } => Some(minecraft),
            _ => None,
        }
    }
}

impl std::fmt::Display for ModSetProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnsupportedSchema { found } => write!(
                f,
                "unsupported mod set catalog schema {found} (supported {MOD_SET_CATALOG_SCHEMA})"
            ),
            Self::TooManySets { count } => {
                write!(f, "{count} mod sets (at most {MAX_MOD_SETS})")
            }
            Self::BadSetId { set } => write!(f, "bad mod set id {set:?}"),
            Self::DuplicateSetId { set } => write!(f, "mod set {set} is listed twice"),
            Self::BadName { set } => write!(f, "mod set {set} has a blank or long name"),
            Self::BadSummary { set } => write!(f, "mod set {set} has a long summary"),
            Self::BadVariantCount { set, count } => write!(
                f,
                "mod set {set} has {count} variants (1 to {MAX_MOD_SET_VARIANTS})"
            ),
            Self::BadMinecraft { set, minecraft } => {
                write!(f, "mod set {set} has a bad Minecraft version {minecraft:?}")
            }
            Self::TwoLoaders { set, minecraft } => {
                write!(f, "mod set {set} ({minecraft}) names two loaders")
            }
            Self::DuplicateVariant { set, minecraft } => write!(
                f,
                "mod set {set} has two variants for {minecraft} and the same loader"
            ),
            Self::BadItemCount {
                set,
                minecraft,
                count,
            } => write!(
                f,
                "mod set {set} ({minecraft}) has {count} items (1 to {MAX_MOD_SET_ITEMS})"
            ),
            Self::ModsWithoutLoader { set, minecraft } => {
                write!(f, "mod set {set} ({minecraft}) has mods but no loader")
            }
            Self::BadFile {
                set,
                minecraft,
                path,
                reason,
            } => write!(f, "mod set {set} ({minecraft}): {path}: {reason}"),
            Self::UncoveredFile {
                set,
                minecraft,
                path,
            } => write!(f, "mod set {set} ({minecraft}): no item installs {path}"),
            Self::NotModrinth {
                set,
                minecraft,
                item,
            } => write!(
                f,
                "mod set {set} ({minecraft}): item {item} has no Modrinth reference"
            ),
            Self::Manifest {
                set,
                minecraft,
                problem,
            } => write!(f, "mod set {set} ({minecraft}): {problem}"),
        }
    }
}

impl ModSetCatalog {
    /// Fails when the schema is newer than this build.
    pub fn check_schema(&self) -> Result<()> {
        check_schema("mod set catalog", self.schema, MOD_SET_CATALOG_SCHEMA)
    }

    pub fn set(&self, id: &str) -> Option<&ModSet> {
        self.sets.iter().find(|set| set.id == id)
    }

    /// Every broken rule, catalog-wide first, then per set in catalog order. Empty when the
    /// catalog is valid. Studio refuses to publish a catalog with problems.
    pub fn problems(&self) -> Vec<ModSetProblem> {
        if self.schema > MOD_SET_CATALOG_SCHEMA || self.schema == 0 {
            return vec![ModSetProblem::UnsupportedSchema { found: self.schema }];
        }
        let mut problems = Vec::new();
        if self.sets.len() > MAX_MOD_SETS {
            problems.push(ModSetProblem::TooManySets {
                count: self.sets.len(),
            });
        }
        let mut ids = HashSet::new();
        for set in &self.sets {
            if is_valid_id(&set.id) && !ids.insert(set.id.as_str()) {
                problems.push(ModSetProblem::DuplicateSetId {
                    set: set.id.clone(),
                });
            }
            problems.extend(set.problems());
        }
        problems
    }

    /// Fails with the first of [`Self::problems`].
    pub fn validate(&self) -> Result<()> {
        self.check_schema()?;
        match self.problems().into_iter().next() {
            Some(problem) => Err(Error::Invalid(problem.to_string())),
            None => Ok(()),
        }
    }

    /// The catalog without the sets that break a rule (and without later sets that repeat an
    /// id), plus what was dropped. Clients use it: one broken set never hides the others.
    pub fn without_broken_sets(mut self) -> (Self, Vec<ModSetProblem>) {
        let mut dropped = Vec::new();
        let mut ids = HashSet::new();
        self.sets.retain(|set| {
            let mut problems = set.problems();
            if is_valid_id(&set.id) && !ids.insert(set.id.clone()) {
                problems.push(ModSetProblem::DuplicateSetId {
                    set: set.id.clone(),
                });
            }
            let keep = problems.is_empty();
            dropped.extend(problems);
            keep
        });
        self.sets.truncate(MAX_MOD_SETS);
        (self, dropped)
    }
}

impl ModSet {
    /// Every broken rule of this set and its variants, in order.
    pub fn problems(&self) -> Vec<ModSetProblem> {
        let set = || self.id.clone();
        let mut problems = Vec::new();
        if !is_valid_id(&self.id) {
            problems.push(ModSetProblem::BadSetId { set: set() });
        }
        let english = self.en.as_ref();
        let names = [
            Some(self.name.as_str()),
            english.and_then(|en| en.name.as_deref()),
        ];
        if names.iter().enumerate().any(|(index, name)| match name {
            // The Turkish name is required; the English one may be left out.
            None => index == 0,
            Some(name) => {
                name.trim().is_empty()
                    || name.chars().count() > MAX_MOD_SET_NAME_CHARS
                    || name.chars().any(char::is_control)
            }
        }) {
            problems.push(ModSetProblem::BadName { set: set() });
        }
        let summaries = [
            self.summary.as_deref(),
            english.and_then(|en| en.summary.as_deref()),
        ];
        if summaries.into_iter().flatten().any(|summary| {
            summary.chars().count() > MAX_MOD_SET_SUMMARY_CHARS
                || summary.chars().any(char::is_control)
        }) {
            problems.push(ModSetProblem::BadSummary { set: set() });
        }
        if self.variants.is_empty() || self.variants.len() > MAX_MOD_SET_VARIANTS {
            problems.push(ModSetProblem::BadVariantCount {
                set: set(),
                count: self.variants.len(),
            });
        }
        let mut keys = HashSet::new();
        for variant in &self.variants {
            let minecraft = variant.minecraft().to_owned();
            if variant.loaders().count() <= 1 && !keys.insert((minecraft.clone(), variant.loader()))
            {
                problems.push(ModSetProblem::DuplicateVariant {
                    set: set(),
                    minecraft,
                });
            }
            problems.extend(variant.problems(&self.id));
        }
        problems
    }

    /// The set's name in the interface language (`english`: the English text when it has one).
    pub fn name_in(&self, english: bool) -> &str {
        english
            .then(|| self.en.as_ref().and_then(|en| en.name.as_deref()))
            .flatten()
            .filter(|name| !name.trim().is_empty())
            .unwrap_or(&self.name)
    }

    /// The set's summary in the interface language (falls back to the Turkish one).
    pub fn summary_in(&self, english: bool) -> Option<&str> {
        english
            .then(|| self.en.as_ref().and_then(|en| en.summary.as_deref()))
            .flatten()
            .filter(|summary| !summary.trim().is_empty())
            .or(self.summary.as_deref())
    }

    /// The variant an instance on `minecraft` with `loader` uses: the one for that loader, else
    /// the loader-less one (resource and shader packs only). Mods never cross loaders.
    pub fn variant_for(
        &self,
        minecraft: &str,
        loader: Option<PackLoader>,
    ) -> Option<&ModSetVariant> {
        let on_version = || {
            self.variants
                .iter()
                .filter(move |variant| variant.minecraft() == minecraft.trim())
        };
        on_version()
            .find(|variant| loader.is_some() && variant.loader() == loader)
            .or_else(|| on_version().find(|variant| variant.loader().is_none()))
    }

    /// Whether this launcher may offer the set (its `minLauncherVersion` is not newer).
    pub fn supported_by(&self, launcher: &semver::Version) -> bool {
        self.min_launcher_version
            .as_ref()
            .is_none_or(|minimum| launcher >= minimum)
    }
}

impl ModSetVariant {
    /// `dependencies.minecraft`, trimmed.
    pub fn minecraft(&self) -> &str {
        self.dependencies.minecraft.trim()
    }

    /// The loader the variant needs (`None`: packs only, any loader).
    pub fn loader(&self) -> Option<PackLoader> {
        self.dependencies.loader().map(|(loader, _)| loader)
    }

    /// The loader version Studio picked, for instances started from the set.
    pub fn loader_version(&self) -> Option<&str> {
        self.dependencies
            .loader()
            .map(|(_, version)| version.trim())
            .filter(|version| !version.is_empty())
    }

    fn loaders(&self) -> impl Iterator<Item = PackLoader> {
        let deps = &self.dependencies;
        [
            (PackLoader::Fabric, &deps.fabric_loader),
            (PackLoader::Quilt, &deps.quilt_loader),
            (PackLoader::Forge, &deps.forge),
            (PackLoader::NeoForge, &deps.neoforge),
        ]
        .into_iter()
        .filter(|(_, version)| version.is_some())
        .map(|(loader, _)| loader)
    }

    pub fn file(&self, path: &str) -> Option<&PackFile> {
        self.files.iter().find(|file| file.path == path)
    }

    /// The kind of an item (from its file's folder; mods when the path is not a set path).
    pub fn item_kind(item: &PackItem) -> ModSetItemKind {
        ModSetItemKind::of_path(&item.path).unwrap_or(ModSetItemKind::Mod)
    }

    /// The items a new install selects (`defaultEnabled`) plus everything they require, in
    /// item order. What a player gets when they add the set without changing anything.
    pub fn default_items(&self) -> Vec<&PackItem> {
        let manifest = self.raw_manifest("", "");
        let resolution = resolve_items(&manifest, None, None);
        self.items
            .iter()
            .filter(|item| resolution.is_enabled(&item.id))
            .collect()
    }

    /// Bytes a new install downloads: the files of [`Self::default_items`].
    pub fn default_download_size(&self) -> u64 {
        self.default_items()
            .iter()
            .filter_map(|item| self.file(&item.path))
            .map(|file| file.file_size)
            .sum()
    }

    /// A short, stable key of what the variant installs: the loader, and per item its id, file,
    /// file hash, requirements and library flag. Wording (titles, summaries, icons) is left
    /// out, so editing text is not an update. Pack states record it as their `versionId`.
    pub fn content_key(&self) -> String {
        let mut lines: Vec<String> = self
            .items
            .iter()
            .map(|item| {
                let sha1 = self
                    .file(&item.path)
                    .map(|file| file.hashes.sha1.trim().to_ascii_lowercase())
                    .unwrap_or_default();
                let mut requires: Vec<&str> = item.requires.iter().map(String::as_str).collect();
                requires.sort_unstable();
                format!(
                    "{}|{}|{}|{}|{}",
                    item.id,
                    item.path,
                    sha1,
                    item.library,
                    requires.join(",")
                )
            })
            .collect();
        lines.sort_unstable();
        lines.insert(
            0,
            format!("{}|{:?}", self.minecraft(), self.loader().map(loader_name)),
        );
        sha256_hex(lines.join("\n").as_bytes())[..16].to_owned()
    }

    /// The pack manifest the sync engine installs for `set` in a game directory. Items whose
    /// Modrinth project (or id) is in `provided` are left out: something else in the instance
    /// already brings that project (the official pack, the player's own mods), and two copies
    /// of one mod would stop the game. Requirements on left-out items are dropped with them.
    /// Set files never overwrite a file the player changed (`preserveUserChanges`).
    pub fn manifest(&self, set_id: &str, name: &str, provided: &BTreeSet<String>) -> PackManifest {
        let left_out: HashSet<&str> = self
            .items
            .iter()
            .filter(|item| {
                provided.contains(&item.id)
                    || item
                        .modrinth
                        .as_ref()
                        .is_some_and(|reference| provided.contains(&reference.project_id))
            })
            .map(|item| item.id.as_str())
            .collect();
        let mut manifest = self.raw_manifest(set_id, name);
        manifest.projects.items = self
            .items
            .iter()
            .filter(|item| !left_out.contains(item.id.as_str()))
            .map(|item| PackItem {
                requires: item
                    .requires
                    .iter()
                    .filter(|required| !left_out.contains(required.as_str()))
                    .cloned()
                    .collect(),
                ..item.clone()
            })
            .collect();
        let kept: HashSet<&str> = manifest
            .projects
            .items
            .iter()
            .map(|item| item.path.as_str())
            .collect();
        manifest
            .files
            .retain(|file| kept.contains(file.path.as_str()));
        manifest
    }

    fn raw_manifest(&self, set_id: &str, name: &str) -> PackManifest {
        PackManifest {
            format_version: MRPACK_FORMAT_VERSION,
            game: "minecraft".to_owned(),
            version_id: self.content_key(),
            name: name.to_owned(),
            summary: None,
            files: self.files.clone(),
            dependencies: self.dependencies.clone(),
            projects: ProjectsExtension {
                pack_id: set_id.to_owned(),
                items: self.items.clone(),
                config_policy: ConfigPolicy {
                    default: FilePolicy::PreserveUserChanges,
                    rules: Vec::new(),
                },
                ..ProjectsExtension::default()
            },
        }
    }

    /// Every broken rule of this variant (`set` names it in the problems).
    pub fn problems(&self, set: &str) -> Vec<ModSetProblem> {
        let minecraft = self.minecraft().to_owned();
        let set = || set.to_owned();
        let mc = || minecraft.clone();
        let mut problems = Vec::new();
        if !is_valid_minecraft_version(&minecraft) {
            problems.push(ModSetProblem::BadMinecraft {
                set: set(),
                minecraft: mc(),
            });
        }
        if self.loaders().count() > 1 {
            problems.push(ModSetProblem::TwoLoaders {
                set: set(),
                minecraft: mc(),
            });
        }
        if self.items.is_empty() || self.items.len() > MAX_MOD_SET_ITEMS {
            problems.push(ModSetProblem::BadItemCount {
                set: set(),
                minecraft: mc(),
                count: self.items.len(),
            });
        }
        let mut has_mods = false;
        for file in &self.files {
            let bad = |reason| ModSetProblem::BadFile {
                set: set(),
                minecraft: mc(),
                path: file.path.clone(),
                reason,
            };
            match ModSetItemKind::of_path(&file.path) {
                None => problems.push(bad("not a file in mods/, resourcepacks/ or shaderpacks/")),
                Some(kind) => {
                    has_mods |= kind == ModSetItemKind::Mod;
                    let extension = file
                        .path
                        .rsplit_once('.')
                        .map(|(_, extension)| extension.to_ascii_lowercase());
                    if extension.as_deref() != Some(kind.extension()) {
                        problems.push(bad("wrong file extension"));
                    }
                }
            }
            if !file.downloads.iter().all(|url| is_https_url(url)) {
                problems.push(bad("download URLs must be HTTPS"));
            }
            if file.file_size == 0 {
                problems.push(bad("no file size"));
            }
            if !self.items.iter().any(|item| item.path == file.path) {
                problems.push(ModSetProblem::UncoveredFile {
                    set: set(),
                    minecraft: mc(),
                    path: file.path.clone(),
                });
            }
        }
        if has_mods && self.loader().is_none() {
            problems.push(ModSetProblem::ModsWithoutLoader {
                set: set(),
                minecraft: mc(),
            });
        }
        for item in &self.items {
            if item.modrinth.is_none() {
                problems.push(ModSetProblem::NotModrinth {
                    set: set(),
                    minecraft: mc(),
                    item: item.id.clone(),
                });
            }
        }
        problems.extend(
            self.raw_manifest(&set(), "")
                .problems()
                .into_iter()
                .map(|problem| ModSetProblem::Manifest {
                    set: set(),
                    minecraft: mc(),
                    problem,
                }),
        );
        problems
    }

    /// Item id → item, for quick lookups.
    pub fn items_by_id(&self) -> HashMap<&str, &PackItem> {
        self.items
            .iter()
            .map(|item| (item.id.as_str(), item))
            .collect()
    }
}

/// The `.mrpack` dependency key of a loader, for keys and logs.
pub fn loader_name(loader: PackLoader) -> &'static str {
    match loader {
        PackLoader::Fabric => "fabric-loader",
        PackLoader::Quilt => "quilt-loader",
        PackLoader::Forge => "forge",
        PackLoader::NeoForge => "neoforge",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{ItemCategory, ModrinthRef, PackHashes};

    fn file(path: &str, sha: char, size: u64) -> PackFile {
        PackFile {
            path: path.into(),
            hashes: PackHashes {
                sha1: sha.to_string().repeat(40),
                sha512: Some(sha.to_string().repeat(128)),
            },
            env: None,
            downloads: vec![format!("https://cdn.modrinth.com/data/x/versions/y/{path}")],
            file_size: size,
        }
    }

    fn item(id: &str, path: &str) -> PackItem {
        PackItem {
            id: id.into(),
            path: path.into(),
            title: id.to_uppercase(),
            summary: None,
            category: ItemCategory::Other,
            icon_url: None,
            page_url: None,
            modrinth: Some(ModrinthRef {
                project_id: id.into(),
                version_id: format!("{id}-v1"),
            }),
            default_enabled: true,
            requires: Vec::new(),
            library: false,
        }
    }

    fn fabric(minecraft: &str) -> PackDependencies {
        PackDependencies {
            minecraft: minecraft.into(),
            fabric_loader: Some("0.19.5".into()),
            ..PackDependencies::default()
        }
    }

    fn variant() -> ModSetVariant {
        let mut zoom = item("zoom", "mods/zoom.jar");
        zoom.requires = vec!["api".into()];
        let mut api = item("api", "mods/api.jar");
        api.library = true;
        let mut fire = item("fire", "resourcepacks/LowFire.zip");
        fire.default_enabled = false;
        ModSetVariant {
            dependencies: fabric("26.3"),
            files: vec![
                file("mods/zoom.jar", 'a', 100),
                file("mods/api.jar", 'b', 1000),
                file("resourcepacks/LowFire.zip", 'c', 10),
            ],
            items: vec![zoom, api, fire],
        }
    }

    fn set(variants: Vec<ModSetVariant>) -> ModSet {
        ModSet {
            id: "pvp".into(),
            name: "PvP".into(),
            summary: Some("Net görüntü, sabit FPS.".into()),
            purpose: ModSetPurpose::Pvp,
            en: None,
            variants,
            min_launcher_version: None,
        }
    }

    #[test]
    fn a_valid_set_has_no_problems() {
        let catalog = ModSetCatalog {
            schema: 1,
            sets: vec![set(vec![variant()])],
        };
        assert_eq!(catalog.problems(), []);
        catalog.validate().unwrap();
        let json = serde_json::to_value(&catalog).unwrap();
        assert_eq!(
            serde_json::from_value::<ModSetCatalog>(json).unwrap(),
            catalog
        );
    }

    #[test]
    fn kinds_follow_the_folder() {
        assert_eq!(
            ModSetItemKind::of_path("mods/a.jar"),
            Some(ModSetItemKind::Mod)
        );
        assert_eq!(
            ModSetItemKind::of_path("resourcepacks/a.zip"),
            Some(ModSetItemKind::ResourcePack)
        );
        assert_eq!(
            ModSetItemKind::of_path("shaderpacks/a.zip"),
            Some(ModSetItemKind::ShaderPack)
        );
        assert_eq!(ModSetItemKind::of_path("mods/sub/a.jar"), None);
        assert_eq!(ModSetItemKind::of_path("config/a.json"), None);
        assert_eq!(ModSetItemKind::of_path("mods/"), None);
    }

    #[test]
    fn defaults_and_sizes() {
        let variant = variant();
        let ids: Vec<&str> = variant
            .default_items()
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        // The library comes with the zoom mod; the default-off pack stays out.
        assert_eq!(ids, ["zoom", "api"]);
        assert_eq!(variant.default_download_size(), 1100);
    }

    #[test]
    fn variants_are_picked_by_version_and_loader() {
        let mut packs_only = variant();
        packs_only.dependencies = PackDependencies {
            minecraft: "26.2".into(),
            ..PackDependencies::default()
        };
        packs_only
            .files
            .retain(|file| !file.path.starts_with("mods/"));
        packs_only
            .items
            .retain(|item| !item.path.starts_with("mods/"));
        let set = set(vec![variant(), packs_only]);
        assert_eq!(set.problems(), []);
        let fabric_26_3 = set.variant_for("26.3", Some(PackLoader::Fabric)).unwrap();
        assert_eq!(fabric_26_3.loader(), Some(PackLoader::Fabric));
        assert!(
            set.variant_for("26.3", Some(PackLoader::NeoForge))
                .is_none()
        );
        assert!(set.variant_for("26.3", None).is_none(), "mods need Fabric");
        // A packs-only variant fits every loader and vanilla.
        for loader in [None, Some(PackLoader::Fabric), Some(PackLoader::NeoForge)] {
            assert_eq!(set.variant_for("26.2", loader).unwrap().minecraft(), "26.2");
        }
        assert!(
            set.variant_for("26.1.2", Some(PackLoader::Fabric))
                .is_none()
        );
    }

    #[test]
    fn content_key_ignores_wording() {
        let base = variant();
        let mut reworded = base.clone();
        reworded.items[0].title = "Yakınlaştırma".into();
        reworded.items[0].summary = Some("C ile yakınlaştır.".into());
        reworded.items.reverse();
        assert_eq!(base.content_key(), reworded.content_key());
        let mut updated = base.clone();
        updated.files[0].hashes.sha1 = "d".repeat(40);
        assert_ne!(base.content_key(), updated.content_key());
        let mut added = base.clone();
        added.files.push(file("mods/hud.jar", 'e', 5));
        added.items.push(item("hud", "mods/hud.jar"));
        assert_ne!(base.content_key(), added.content_key());
        assert_eq!(base.content_key().len(), 16);
    }

    #[test]
    fn provided_projects_are_left_out() {
        let variant = variant();
        let provided: BTreeSet<String> = ["api".to_owned()].into();
        let manifest = variant.manifest("pvp", "PvP", &provided);
        manifest.validate().unwrap();
        let ids: Vec<&str> = manifest
            .projects
            .items
            .iter()
            .map(|item| item.id.as_str())
            .collect();
        assert_eq!(ids, ["zoom", "fire"]);
        assert!(manifest.projects.items[0].requires.is_empty());
        assert!(manifest.file("mods/api.jar").is_none());
        assert_eq!(manifest.projects.pack_id, "pvp");
        assert_eq!(manifest.version_id, variant.content_key());
        assert_eq!(
            manifest.projects.config_policy.policy_for("mods/zoom.jar"),
            FilePolicy::PreserveUserChanges
        );
        // By Modrinth project too.
        let mut renamed = variant.clone();
        renamed.items[1].id = "fabric-api".into();
        renamed.items[0].requires = vec!["fabric-api".into()];
        let manifest = renamed.manifest("pvp", "PvP", &provided);
        assert!(manifest.projects.item("fabric-api").is_none());
    }

    #[test]
    fn broken_rules_are_listed() {
        let mut broken = variant();
        broken.files.push(file("config/x.json", 'f', 1));
        broken.files.push(file("mods/readme.txt", '1', 1));
        broken.files.push(PackFile {
            downloads: vec!["http://example.com/a.zip".into()],
            ..file("shaderpacks/a.zip", '2', 0)
        });
        broken.items.push(item("txt", "mods/readme.txt"));
        broken.items.push(PackItem {
            modrinth: None,
            ..item("shader", "shaderpacks/a.zip")
        });
        let problems = broken.problems("pvp");
        let has = |needle: &str| {
            problems
                .iter()
                .any(|problem| problem.to_string().contains(needle))
        };
        assert!(has("config/x.json: not a file in mods/"), "{problems:?}");
        assert!(has("no item installs config/x.json"), "{problems:?}");
        assert!(has("mods/readme.txt: wrong file extension"), "{problems:?}");
        assert!(has("shaderpacks/a.zip: download URLs must be HTTPS"));
        assert!(has("shaderpacks/a.zip: no file size"));
        assert!(has("item shader has no Modrinth reference"));
        assert!(problems.iter().all(|problem| problem.set_id() == "pvp"));
        assert!(
            problems
                .iter()
                .all(|problem| problem.minecraft() == Some("26.3"))
        );

        let mut no_loader = variant();
        no_loader.dependencies.fabric_loader = None;
        assert!(
            no_loader
                .problems("pvp")
                .contains(&ModSetProblem::ModsWithoutLoader {
                    set: "pvp".into(),
                    minecraft: "26.3".into()
                })
        );
        let mut two_loaders = variant();
        two_loaders.dependencies.neoforge = Some("21.0".into());
        assert!(
            two_loaders
                .problems("pvp")
                .iter()
                .any(|p| matches!(p, ModSetProblem::TwoLoaders { .. }))
        );
        let mut empty = variant();
        empty.files.clear();
        empty.items.clear();
        assert!(
            empty
                .problems("pvp")
                .iter()
                .any(|p| matches!(p, ModSetProblem::BadItemCount { count: 0, .. }))
        );
        let mut requires_unknown = variant();
        requires_unknown.items[0].requires.push("ghost".into());
        assert!(
            requires_unknown
                .problems("pvp")
                .iter()
                .any(|p| p.to_string().contains("requires the unknown item ghost"))
        );
    }

    #[test]
    fn set_rules() {
        let mut long_name = set(vec![variant()]);
        long_name.name = "Çok uzun bir set adı ki kartın başlığına sığmaz".into();
        assert!(
            long_name
                .problems()
                .contains(&ModSetProblem::BadName { set: "pvp".into() })
        );
        let mut blank_english = set(vec![variant()]);
        blank_english.en = Some(ModSetText {
            name: Some(" ".into()),
            summary: None,
        });
        assert!(
            blank_english
                .problems()
                .contains(&ModSetProblem::BadName { set: "pvp".into() })
        );
        let mut long_summary = set(vec![variant()]);
        long_summary.summary = Some("a".repeat(MAX_MOD_SET_SUMMARY_CHARS + 1));
        assert!(
            long_summary
                .problems()
                .contains(&ModSetProblem::BadSummary { set: "pvp".into() })
        );
        let twice = set(vec![variant(), variant()]);
        assert!(twice.problems().contains(&ModSetProblem::DuplicateVariant {
            set: "pvp".into(),
            minecraft: "26.3".into()
        }));
        let none = set(Vec::new());
        assert!(none.problems().contains(&ModSetProblem::BadVariantCount {
            set: "pvp".into(),
            count: 0
        }));
        let mut bad_id = set(vec![variant()]);
        bad_id.id = "pvp set".into();
        assert!(bad_id.problems().contains(&ModSetProblem::BadSetId {
            set: "pvp set".into()
        }));
    }

    #[test]
    fn catalog_rules_and_broken_sets() {
        let good = set(vec![variant()]);
        let mut broken = set(vec![variant()]);
        broken.id = "broken".into();
        broken.variants[0].items[0].title = String::new();
        let catalog = ModSetCatalog {
            schema: 1,
            sets: vec![good.clone(), broken, good.clone()],
        };
        let problems = catalog.problems();
        assert!(
            problems.contains(&ModSetProblem::DuplicateSetId { set: "pvp".into() }),
            "{problems:?}"
        );
        assert!(problems.iter().any(|p| p.set_id() == "broken"));
        assert!(catalog.validate().is_err());
        let (kept, dropped) = catalog.without_broken_sets();
        assert_eq!(kept.sets, [good]);
        assert_eq!(dropped.len(), 2, "{dropped:?}");

        let newer = ModSetCatalog {
            schema: 2,
            sets: Vec::new(),
        };
        assert_eq!(
            newer.problems(),
            [ModSetProblem::UnsupportedSchema { found: 2 }]
        );
        assert!(matches!(
            newer.validate(),
            Err(Error::UnsupportedSchema { found: 2, .. })
        ));
    }

    #[test]
    fn wording_follows_the_language() {
        let mut set = set(vec![variant()]);
        assert_eq!(set.name_in(true), "PvP");
        assert_eq!(set.summary_in(true), Some("Net görüntü, sabit FPS."));
        set.en = Some(ModSetText {
            name: Some("PvP".into()),
            summary: Some("Clear visuals, steady FPS.".into()),
        });
        assert_eq!(set.summary_in(true), Some("Clear visuals, steady FPS."));
        assert_eq!(set.summary_in(false), Some("Net görüntü, sabit FPS."));
        let launcher: semver::Version = "2.0.0".parse().unwrap();
        assert!(set.supported_by(&launcher));
        set.min_launcher_version = Some("2.1.0".parse().unwrap());
        assert!(!set.supported_by(&launcher));
    }

    #[test]
    fn unknown_purposes_read_as_other() {
        let purpose: ModSetPurpose = serde_json::from_str("\"cinematic\"").unwrap();
        assert_eq!(purpose, ModSetPurpose::Other);
        let purpose: ModSetPurpose = serde_json::from_str("\"pvp\"").unwrap();
        assert_eq!(purpose, ModSetPurpose::Pvp);
    }
}
