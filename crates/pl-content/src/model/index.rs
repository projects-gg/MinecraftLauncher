use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{DocRef, FileRef, check_schema, default_schema, is_valid_id};
use crate::error::{Error, Result};

/// Highest `RootIndex.schema` this build understands.
pub const ROOT_INDEX_SCHEMA: u32 = 1;

/// Platform keys used by AFK client builds (`<os>-<arch>`, as [`current_platform_key`] spells
/// them). New keys are additive: clients look up their own key and ignore the rest.
pub const PLATFORM_KEYS: [&str; 6] = [
    "windows-x86_64",
    "windows-aarch64",
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
    /// `modsets/modsets.<sha256>.json` ([`crate::ModSetCatalog`]): the ready-made mod sets.
    /// Absent when the team publishes none.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mod_sets: Option<DocRef>,
}

impl RootIndex {
    pub fn check_schema(&self) -> Result<()> {
        check_schema("root index", self.schema, ROOT_INDEX_SCHEMA)
    }

    /// Schema, every document reference and the family rules: valid family ids, at most one
    /// `default` member per family, each Minecraft version at most once per family (the first
    /// of [`family_problems`]).
    pub fn validate(&self) -> Result<()> {
        self.check_schema()?;
        self.config.validate()?;
        self.news.validate()?;
        for pack in &self.packs {
            pack.manifest.validate()?;
        }
        if let Some(mod_sets) = &self.mod_sets {
            mod_sets.validate()?;
        }
        match family_problems(&self.packs).into_iter().next() {
            Some(problem) => Err(Error::Invalid(problem.to_string())),
            None => Ok(()),
        }
    }

    pub fn pack(&self, id: &str) -> Option<&PackRef> {
        self.packs.iter().find(|pack| pack.id == id)
    }

    /// Every document this index references (config, news, pack manifests, mod sets).
    pub fn documents(&self) -> impl Iterator<Item = &DocRef> {
        [&self.config, &self.news]
            .into_iter()
            .chain(self.packs.iter().map(|pack| &pack.manifest))
            .chain(self.mod_sets.iter())
    }

    /// Distinct family ids, in index order.
    pub fn families(&self) -> Vec<&str> {
        let mut families: Vec<&str> = Vec::new();
        for pack in &self.packs {
            if !families.contains(&pack.family_id()) {
                families.push(pack.family_id());
            }
        }
        families
    }

    /// The members of a family, newest first: by `minecraft` ([`compare_minecraft_versions`],
    /// members without it last), then by `latest` (semver), then in index order.
    pub fn family_members(&self, family: &str) -> Vec<&PackRef> {
        let mut members: Vec<&PackRef> = self
            .packs
            .iter()
            .filter(|pack| pack.family_id() == family)
            .collect();
        // Stable sort: equal members keep their index order.
        members.sort_by(|a, b| compare_members(b, a));
        members
    }

    /// The member a new install uses: the one marked `default`, else the newest
    /// ([`Self::family_members`] order).
    pub fn family_default(&self, family: &str) -> Option<&PackRef> {
        let members = self.family_members(family);
        members
            .iter()
            .find(|pack| pack.default)
            .or_else(|| members.first())
            .copied()
    }

    /// The member of `family` published for a Minecraft version (needs `minecraft` on the
    /// pack references; Studio always writes it).
    pub fn family_member(&self, family: &str, minecraft: &str) -> Option<&PackRef> {
        self.packs
            .iter()
            .find(|pack| pack.family_id() == family && pack.minecraft() == Some(minecraft))
    }

    /// The pack a new install follows. `recommended` is the launcher config's
    /// `recommendedPack`: a pack id (its family's default member is returned) or a family id.
    /// Without it, or when it names nothing published, the first pack's family is used.
    pub fn recommended_pack(&self, recommended: Option<&str>) -> Option<&PackRef> {
        let named = recommended.and_then(|id| match self.pack(id) {
            Some(pack) => self.family_default(pack.family_id()),
            None => self.family_default(id),
        });
        named.or_else(|| {
            self.packs
                .first()
                .and_then(|pack| self.family_default(pack.family_id()))
        })
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
    /// Family id: the same pack published for several Minecraft versions, one pack id per
    /// version. `None` means the pack is its own family (`id`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub family: Option<String>,
    /// `dependencies.minecraft` of the `latest` manifest, so clients can list a family's
    /// versions without downloading manifests.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft: Option<String>,
    /// The family member new installs use (at most one per family).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub default: bool,
}

impl PackRef {
    /// The family id: `family`, else the pack id.
    pub fn family_id(&self) -> &str {
        self.family
            .as_deref()
            .filter(|family| !family.is_empty())
            .unwrap_or(&self.id)
    }

    /// The Minecraft version, when the index records it.
    pub fn minecraft(&self) -> Option<&str> {
        self.minecraft
            .as_deref()
            .filter(|version| !version.is_empty())
    }
}

/// Every pack family rule `packs` (an index's `packs[]`, in order) breaks: a bad family id, a
/// second `default` member, a Minecraft version listed twice in a family. Empty when the rules
/// hold. [`RootIndex::validate`] fails with the first one; tools that list every problem
/// (Studio, before it builds an index) use this directly.
pub fn family_problems(packs: &[PackRef]) -> Vec<FamilyProblem> {
    let mut problems = Vec::new();
    let mut defaults: HashMap<&str, &str> = HashMap::new();
    let mut versions: HashMap<(&str, &str), &str> = HashMap::new();
    for pack in packs {
        if let Some(family) = &pack.family
            && !is_valid_id(family)
        {
            problems.push(FamilyProblem::BadFamilyId {
                pack: pack.id.clone(),
                family: family.clone(),
            });
        }
        let family = pack.family_id();
        if pack.default
            && let Some(other) = defaults.insert(family, &pack.id)
        {
            problems.push(FamilyProblem::TwoDefaults {
                family: family.to_owned(),
                first: other.to_owned(),
                second: pack.id.clone(),
            });
        }
        if let Some(minecraft) = pack.minecraft()
            && let Some(other) = versions.insert((family, minecraft), &pack.id)
        {
            problems.push(FamilyProblem::DuplicateMinecraft {
                family: family.to_owned(),
                minecraft: minecraft.to_owned(),
                first: other.to_owned(),
                second: pack.id.clone(),
            });
        }
    }
    problems
}

/// A broken pack family rule (see [`family_problems`]). `Display` gives the text that
/// [`RootIndex::validate`] reports.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FamilyProblem {
    /// `pack`'s `family` is not 1–64 characters from `[A-Za-z0-9._-]`.
    BadFamilyId { pack: String, family: String },
    /// `first` and `second` are both the `default` member of `family`.
    TwoDefaults {
        family: String,
        first: String,
        second: String,
    },
    /// `first` and `second` both publish `family` for the same Minecraft version.
    DuplicateMinecraft {
        family: String,
        minecraft: String,
        first: String,
        second: String,
    },
}

impl std::fmt::Display for FamilyProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadFamilyId { pack, family } => {
                write!(f, "pack {pack} has a bad family id {family:?}")
            }
            Self::TwoDefaults {
                family,
                first,
                second,
            } => write!(
                f,
                "family {family} has two default packs ({first}, {second})"
            ),
            Self::DuplicateMinecraft {
                family,
                minecraft,
                first,
                second,
            } => write!(
                f,
                "family {family} lists Minecraft {minecraft} twice ({first}, {second})"
            ),
        }
    }
}

/// Older first: by Minecraft version, then by pack version.
fn compare_members(a: &PackRef, b: &PackRef) -> Ordering {
    let by_minecraft = match (a.minecraft(), b.minecraft()) {
        (Some(x), Some(y)) => compare_minecraft_versions(x, y),
        (Some(_), None) => Ordering::Greater,
        (None, Some(_)) => Ordering::Less,
        (None, None) => Ordering::Equal,
    };
    by_minecraft.then_with(|| {
        match (
            semver::Version::parse(a.latest.trim()),
            semver::Version::parse(b.latest.trim()),
        ) {
            (Ok(x), Ok(y)) => x.cmp(&y),
            _ => Ordering::Equal,
        }
    })
}

/// Orders Minecraft version ids, older first. The leading run of digits and dots is compared
/// numerically, component by component (`1.21.10` > `1.21.9`, `26.1` > `1.21.10`, missing
/// components count as 0); for equal numbers the plain release is newer than anything with a
/// suffix (`1.21.4` > `1.21.4-rc1` > `1.21.4-pre1`, suffixes compare as text). Ids without a
/// leading number sort before all others, as text.
pub fn compare_minecraft_versions(a: &str, b: &str) -> Ordering {
    let (a_numbers, a_suffix) = split_minecraft_version(a.trim());
    let (b_numbers, b_suffix) = split_minecraft_version(b.trim());
    match (a_numbers.is_empty(), b_numbers.is_empty()) {
        (true, true) => return a.trim().cmp(b.trim()),
        (true, false) => return Ordering::Less,
        (false, true) => return Ordering::Greater,
        (false, false) => {}
    }
    for index in 0..a_numbers.len().max(b_numbers.len()) {
        let x = a_numbers.get(index).copied().unwrap_or(0);
        let y = b_numbers.get(index).copied().unwrap_or(0);
        if x != y {
            return x.cmp(&y);
        }
    }
    match (a_suffix.is_empty(), b_suffix.is_empty()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => a_suffix.cmp(b_suffix),
    }
}

/// `1.21.4-pre1` → (`[1, 21, 4]`, `-pre1`).
fn split_minecraft_version(version: &str) -> (Vec<u64>, &str) {
    let end = version
        .find(|c: char| !(c.is_ascii_digit() || c == '.'))
        .unwrap_or(version.len());
    let (release, suffix) = version.split_at(end);
    let numbers = release
        .split('.')
        .filter(|part| !part.is_empty())
        .map(|part| part.parse().unwrap_or(u64::MAX))
        .collect();
    (numbers, suffix)
}

/// The current AFK client release: one zip per platform key, and the Minecraft versions that
/// client supports (the AFK accounts' version picker).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkRelease {
    pub version: String,
    /// Platform key (see [`PLATFORM_KEYS`]) → build.
    #[serde(default)]
    pub builds: BTreeMap<String, FileRef>,
    /// Minecraft versions the client supports, newest first (written by the client's CI).
    /// Empty: clients use their built-in list.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minecraft_versions: Vec<AfkMinecraftVersion>,
    /// The version the Projects server needs (chosen in Studio): one of the listed values, or
    /// [`AFK_AUTO_VERSION`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub recommended_minecraft_version: Option<String>,
}

/// One entry of [`AfkRelease::minecraft_versions`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AfkMinecraftVersion {
    /// What the client's `MinecraftVersion` setting takes (`1.21.9`).
    pub value: String,
    /// What the picker shows (`1.21.9 – 1.21.10`).
    pub label: String,
}

/// [`AfkRelease::recommended_minecraft_version`] value that lets the client detect the
/// server's version.
pub const AFK_AUTO_VERSION: &str = "auto";
/// Most entries [`AfkRelease::minecraft_versions`] may have.
pub const MAX_AFK_MINECRAFT_VERSIONS: usize = 256;
/// Longest version label, in characters.
const MAX_AFK_VERSION_LABEL: usize = 64;

/// A broken rule of the AFK Minecraft version list (see
/// [`AfkRelease::minecraft_version_problems`]).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AfkVersionProblem {
    /// A value is empty, longer than 32 bytes, contains whitespace or control characters or is
    /// [`AFK_AUTO_VERSION`] (clients offer that themselves), or its label is empty, longer than
    /// 64 characters or contains control characters.
    BadEntry { value: String },
    /// An earlier entry has the same value.
    Duplicate { value: String },
    /// More than [`MAX_AFK_MINECRAFT_VERSIONS`] entries.
    TooMany { count: usize },
    /// The recommendation is neither [`AFK_AUTO_VERSION`] nor a listed value.
    UnknownRecommended { value: String },
}

impl std::fmt::Display for AfkVersionProblem {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::BadEntry { value } => write!(f, "bad AFK Minecraft version entry {value:?}"),
            Self::Duplicate { value } => {
                write!(f, "AFK Minecraft version {value:?} is listed twice")
            }
            Self::TooMany { count } => write!(
                f,
                "{count} AFK Minecraft versions (at most {MAX_AFK_MINECRAFT_VERSIONS})"
            ),
            Self::UnknownRecommended { value } => write!(
                f,
                "recommended AFK Minecraft version {value:?} is not in the list"
            ),
        }
    }
}

impl AfkRelease {
    pub fn build_for(&self, platform: &str) -> Option<&FileRef> {
        self.builds.get(platform)
    }

    /// The build for the platform this binary runs on.
    pub fn current_build(&self) -> Option<&FileRef> {
        self.build_for(&current_platform_key())
    }

    /// Every broken rule of the Minecraft version list and its recommendation: non-empty
    /// values (at most 32 bytes, no whitespace or control characters, never
    /// [`AFK_AUTO_VERSION`]) and labels (at most 64 characters, no control characters), no
    /// duplicate values, at most [`MAX_AFK_MINECRAFT_VERSIONS`] entries, a recommendation that
    /// is [`AFK_AUTO_VERSION`] or a listed value. Studio refuses to publish a list with
    /// problems; a client that finds one should ignore the list (and the recommendation) and
    /// use its built-in list.
    pub fn minecraft_version_problems(&self) -> Vec<AfkVersionProblem> {
        let mut problems = Vec::new();
        let versions = &self.minecraft_versions;
        if versions.len() > MAX_AFK_MINECRAFT_VERSIONS {
            problems.push(AfkVersionProblem::TooMany {
                count: versions.len(),
            });
        }
        let mut seen = std::collections::HashSet::new();
        for entry in versions {
            let value_ok = super::is_valid_minecraft_version(&entry.value)
                && !entry.value.eq_ignore_ascii_case(AFK_AUTO_VERSION);
            let label = entry.label.trim();
            let label_ok = !label.is_empty()
                && entry.label.chars().count() <= MAX_AFK_VERSION_LABEL
                && !entry.label.chars().any(char::is_control);
            if !value_ok || !label_ok {
                problems.push(AfkVersionProblem::BadEntry {
                    value: entry.value.clone(),
                });
            } else if !seen.insert(entry.value.as_str()) {
                problems.push(AfkVersionProblem::Duplicate {
                    value: entry.value.clone(),
                });
            }
        }
        if let Some(value) = &self.recommended_minecraft_version
            && value != AFK_AUTO_VERSION
            && !versions.iter().any(|entry| entry.value == *value)
        {
            problems.push(AfkVersionProblem::UnknownRecommended {
                value: value.clone(),
            });
        }
        problems
    }

    /// Fails with the first of [`Self::minecraft_version_problems`].
    pub fn validate_minecraft_versions(&self) -> Result<()> {
        match self.minecraft_version_problems().into_iter().next() {
            Some(problem) => Err(Error::Invalid(problem.to_string())),
            None => Ok(()),
        }
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
        // Every desktop the AFK client is built for, Windows on ARM included; the running
        // binary's key is one of them on those systems.
        let mut keys = PLATFORM_KEYS.to_vec();
        keys.sort_unstable();
        keys.dedup();
        assert_eq!(keys.len(), 6, "unique keys");
        for os in ["windows", "linux", "macos"] {
            for arch in ["x86_64", "aarch64"] {
                assert!(PLATFORM_KEYS.contains(&format!("{os}-{arch}").as_str()));
            }
        }
        if cfg!(any(
            target_os = "windows",
            target_os = "linux",
            target_os = "macos"
        )) && cfg!(any(target_arch = "x86_64", target_arch = "aarch64"))
        {
            assert!(PLATFORM_KEYS.contains(&key.as_str()), "{key}");
        }
        let release = AfkRelease {
            version: "1.4.0".into(),
            minecraft_versions: Vec::new(),
            recommended_minecraft_version: None,
            builds: PLATFORM_KEYS
                .iter()
                .map(|platform| {
                    (
                        (*platform).to_owned(),
                        FileRef {
                            path: Some(format!("afk/1.4.0/{platform}.zip")),
                            urls: Vec::new(),
                            sha256: "0".repeat(64),
                            sha1: None,
                            size: 1,
                        },
                    )
                })
                .collect(),
        };
        assert_eq!(
            release
                .build_for("windows-aarch64")
                .unwrap()
                .path
                .as_deref(),
            Some("afk/1.4.0/windows-aarch64.zip")
        );
        assert!(release.current_build().is_some() || !PLATFORM_KEYS.contains(&key.as_str()));
    }

    #[test]
    fn afk_minecraft_versions() {
        let json = r#"{
            "version": "4.0.0",
            "builds": {},
            "minecraftVersions": [
                { "value": "1.21.9", "label": "1.21.9 – 1.21.10" },
                { "value": "1.21.5", "label": "1.21.5 – 1.21.8" }
            ],
            "recommendedMinecraftVersion": "1.21.9"
        }"#;
        let mut release: AfkRelease = serde_json::from_str(json).unwrap();
        assert_eq!(release.minecraft_versions[0].label, "1.21.9 – 1.21.10");
        release.validate_minecraft_versions().unwrap();
        release.recommended_minecraft_version = Some(AFK_AUTO_VERSION.into());
        assert!(release.minecraft_version_problems().is_empty());

        let entry = |value: &str, label: &str| AfkMinecraftVersion {
            value: value.into(),
            label: label.into(),
        };
        release.minecraft_versions.extend([
            entry("1.21.9", "again"),
            entry("", "empty"),
            entry("1.21 4", "space"),
            entry("1.20.1", " "),
            entry(&"9".repeat(33), "long"),
            // Clients offer "auto" themselves; listing it would show it twice.
            entry("Auto", "Otomatik"),
        ]);
        release.recommended_minecraft_version = Some("1.19.4".into());
        assert_eq!(
            release.minecraft_version_problems(),
            [
                AfkVersionProblem::Duplicate {
                    value: "1.21.9".into()
                },
                AfkVersionProblem::BadEntry { value: "".into() },
                AfkVersionProblem::BadEntry {
                    value: "1.21 4".into()
                },
                AfkVersionProblem::BadEntry {
                    value: "1.20.1".into()
                },
                AfkVersionProblem::BadEntry {
                    value: "9".repeat(33)
                },
                AfkVersionProblem::BadEntry {
                    value: "Auto".into()
                },
                AfkVersionProblem::UnknownRecommended {
                    value: "1.19.4".into()
                },
            ]
        );
        assert!(
            release
                .validate_minecraft_versions()
                .unwrap_err()
                .to_string()
                .contains("listed twice")
        );
        release.minecraft_versions = (0..=MAX_AFK_MINECRAFT_VERSIONS)
            .map(|i| entry(&format!("1.{i}"), "x"))
            .collect();
        release.recommended_minecraft_version = None;
        assert_eq!(
            release.minecraft_version_problems(),
            [AfkVersionProblem::TooMany {
                count: MAX_AFK_MINECRAFT_VERSIONS + 1
            }]
        );

        // Releases without the new fields keep their exact shape.
        let old: AfkRelease = serde_json::from_str(r#"{ "version": "1.4.0" }"#).unwrap();
        let back = serde_json::to_value(&old).unwrap();
        assert!(back.get("minecraftVersions").is_none());
        assert!(back.get("recommendedMinecraftVersion").is_none());
        old.validate_minecraft_versions().unwrap();
    }

    fn doc(size: u64) -> DocRef {
        DocRef {
            path: "config/x.json".into(),
            sha256: "0".repeat(64),
            size,
        }
    }

    fn index(packs: Vec<PackRef>) -> RootIndex {
        RootIndex {
            schema: 1,
            sequence: 1,
            generated_at: Timestamp::UNIX_EPOCH,
            config: doc(1),
            news: doc(2),
            packs,
            afk: None,
            mod_sets: None,
        }
    }

    fn pack(id: &str, family: Option<&str>, minecraft: Option<&str>, latest: &str) -> PackRef {
        PackRef {
            id: id.into(),
            name: "Projects".into(),
            latest: latest.into(),
            manifest: DocRef {
                path: format!("packs/{id}/{latest}.json"),
                sha256: "1".repeat(64),
                size: 10,
            },
            family: family.map(str::to_owned),
            minecraft: minecraft.map(str::to_owned),
            default: false,
        }
    }

    #[test]
    fn mod_sets_are_an_optional_document() {
        let mut index = index(Vec::new());
        let json = serde_json::to_value(&index).unwrap();
        assert!(json.get("modSets").is_none(), "old shape kept");
        index.mod_sets = Some(DocRef {
            path: format!("modsets/modsets.{}.json", "a".repeat(64)),
            sha256: "a".repeat(64),
            size: 10,
        });
        index.validate().unwrap();
        assert_eq!(index.documents().count(), 3);
        let back: RootIndex =
            serde_json::from_value(serde_json::to_value(&index).unwrap()).unwrap();
        assert_eq!(back, index);
        index.mod_sets.as_mut().unwrap().sha256 = "xyz".into();
        assert!(index.validate().is_err());
    }

    #[test]
    fn unsupported_schema_is_rejected() {
        let mut index = index(Vec::new());
        index.validate().unwrap();
        assert_eq!(index.documents().count(), 2);
        index.schema = 2;
        assert!(matches!(
            index.validate(),
            Err(crate::Error::UnsupportedSchema { found: 2, .. })
        ));
    }

    #[test]
    fn minecraft_version_order() {
        let mut versions = vec![
            "1.21.10",
            "1.21.4",
            "26.1",
            "1.21.4-pre1",
            "1.21",
            "1.21.4-rc1",
            "1.20.6",
            "1.21.9",
            "unknown",
            "1.21.1",
        ];
        versions.sort_by(|a, b| compare_minecraft_versions(a, b));
        assert_eq!(
            versions,
            [
                "unknown",
                "1.20.6",
                "1.21",
                "1.21.1",
                "1.21.4-pre1",
                "1.21.4-rc1",
                "1.21.4",
                "1.21.9",
                "1.21.10",
                "26.1",
            ]
        );
        assert_eq!(
            compare_minecraft_versions("1.21", "1.21.0"),
            Ordering::Equal
        );
    }

    #[test]
    fn family_selection() {
        // `projects` stands alone in its own family name; the 1.21.4 member names it.
        let mut packs = vec![
            pack("projects", None, Some("1.21.1"), "2.3.0"),
            pack("projects-1.21.4", Some("projects"), Some("1.21.4"), "1.0.0"),
            pack("projects-1.20.6", Some("projects"), Some("1.20.6"), "3.0.0"),
            pack("lobby", None, Some("1.21.4"), "1.0.0"),
        ];
        fn ids(members: Vec<&PackRef>) -> Vec<&str> {
            members.into_iter().map(|pack| pack.id.as_str()).collect()
        }
        let plain = index(packs.clone());
        plain.validate().unwrap();
        assert_eq!(plain.families(), ["projects", "lobby"]);
        assert_eq!(
            ids(plain.family_members("projects")),
            ["projects-1.21.4", "projects", "projects-1.20.6"],
            "newest Minecraft first"
        );
        // Without a default, new installs get the newest member.
        assert_eq!(
            plain.family_default("projects").unwrap().id,
            "projects-1.21.4"
        );
        assert_eq!(
            plain.family_member("projects", "1.20.6").unwrap().id,
            "projects-1.20.6"
        );
        assert!(plain.family_member("projects", "1.19.2").is_none());
        assert!(plain.family_member("lobby", "1.21.1").is_none());

        // An explicit default wins over the newest.
        packs[0].default = true;
        let with_default = index(packs.clone());
        with_default.validate().unwrap();
        assert_eq!(
            with_default.family_default("projects").unwrap().id,
            "projects"
        );
        // `recommendedPack` may name any member or the family; both give the default.
        for recommended in ["projects", "projects-1.20.6"] {
            assert_eq!(
                with_default.recommended_pack(Some(recommended)).unwrap().id,
                "projects"
            );
        }
        assert_eq!(
            with_default.recommended_pack(Some("lobby")).unwrap().id,
            "lobby"
        );
        assert_eq!(
            with_default.recommended_pack(Some("missing")).unwrap().id,
            "projects"
        );
        assert_eq!(with_default.recommended_pack(None).unwrap().id, "projects");

        // Members without `minecraft` (older Studio) sort last; `latest` breaks ties.
        let legacy = index(vec![
            pack("a", Some("f"), None, "1.0.0"),
            pack("b", Some("f"), None, "1.10.0"),
            pack("c", Some("f"), Some("1.21.1"), "0.1.0"),
        ]);
        assert_eq!(ids(legacy.family_members("f")), ["c", "b", "a"]);
    }

    #[test]
    fn family_rules_are_validated() {
        let mut two_defaults = vec![
            pack("projects", None, Some("1.21.1"), "2.3.0"),
            pack("projects-1.21.4", Some("projects"), Some("1.21.4"), "1.0.0"),
        ];
        two_defaults[0].default = true;
        two_defaults[1].default = true;
        let error = index(two_defaults).validate().unwrap_err().to_string();
        assert!(error.contains("two default packs"), "{error}");

        let same_version = vec![
            pack("projects", None, Some("1.21.1"), "2.3.0"),
            pack("projects-b", Some("projects"), Some("1.21.1"), "1.0.0"),
        ];
        let error = index(same_version).validate().unwrap_err().to_string();
        assert!(error.contains("Minecraft 1.21.1 twice"), "{error}");

        let bad_family = vec![pack("projects", Some("../x"), Some("1.21.1"), "2.3.0")];
        assert!(index(bad_family).validate().is_err());

        // `family_problems` lists every broken rule; `validate` reports the first.
        let mut broken = vec![
            pack("projects", None, Some("1.21.1"), "2.3.0"),
            pack("projects-b", Some("projects"), Some("1.21.1"), "1.0.0"),
            pack("projects-c", Some("projects"), Some("1.21.4"), "1.0.0"),
            pack("lobby", Some("bad/family"), None, "1.0.0"),
        ];
        broken[1].default = true;
        broken[2].default = true;
        let problems = family_problems(&broken);
        assert_eq!(
            problems,
            [
                FamilyProblem::DuplicateMinecraft {
                    family: "projects".into(),
                    minecraft: "1.21.1".into(),
                    first: "projects".into(),
                    second: "projects-b".into(),
                },
                FamilyProblem::TwoDefaults {
                    family: "projects".into(),
                    first: "projects-b".into(),
                    second: "projects-c".into(),
                },
                FamilyProblem::BadFamilyId {
                    pack: "lobby".into(),
                    family: "bad/family".into(),
                },
            ]
        );
        assert_eq!(
            index(broken).validate().unwrap_err().to_string(),
            format!("invalid data: {}", problems[0])
        );
        assert!(family_problems(&[pack("projects", None, Some("1.21.1"), "2.3.0")]).is_empty());

        // Defaults in different families are fine; so is an old index without the fields.
        let mut separate = vec![
            pack("projects", None, Some("1.21.1"), "2.3.0"),
            pack("lobby", None, Some("1.21.1"), "1.0.0"),
        ];
        separate[0].default = true;
        separate[1].default = true;
        index(separate).validate().unwrap();
        index(vec![pack("projects", None, None, "2.3.0")])
            .validate()
            .unwrap();

        // The new fields are omitted when unset, so old indexes keep their exact shape.
        let json = serde_json::to_value(pack("projects", None, None, "2.3.0")).unwrap();
        assert!(json.get("family").is_none());
        assert!(json.get("minecraft").is_none());
        assert!(json.get("default").is_none());
    }
}
