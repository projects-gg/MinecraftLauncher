use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::is_http_url;
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
    /// Markdown shown after an update.
    pub changelog: String,
    pub optional_groups: Vec<OptionalGroup>,
    pub config_policy: ConfigPolicy,
    /// Servers added to `servers.dat` for new installs.
    pub servers: Vec<PackServer>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_memory_mb: Option<u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_launcher_version: Option<semver::Version>,
}

/// Files the player can switch on or off as a unit. A file listed in several groups is
/// installed when any of them is enabled.
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
    /// download URLs and unique group ids.
    pub fn validate(&self) -> Result<()> {
        if self.format_version != MRPACK_FORMAT_VERSION {
            return Err(Error::UnsupportedSchema {
                what: "pack manifest".into(),
                found: self.format_version,
                supported: MRPACK_FORMAT_VERSION,
            });
        }
        if self.game != "minecraft" {
            return Err(Error::Invalid(format!("unsupported game {:?}", self.game)));
        }
        let mut seen = HashSet::new();
        for file in &self.files {
            check_relative_path(&file.path)?;
            if !seen.insert(file.path.to_lowercase()) {
                return Err(Error::Invalid(format!("duplicate pack path {}", file.path)));
            }
            if normalize_hex(&file.hashes.sha1, 40).is_none() {
                return Err(Error::Invalid(format!("bad sha1 for {}", file.path)));
            }
            if let Some(sha512) = &file.hashes.sha512
                && normalize_hex(sha512, 128).is_none()
            {
                return Err(Error::Invalid(format!("bad sha512 for {}", file.path)));
            }
            if file.downloads.is_empty() || !file.downloads.iter().all(|url| is_http_url(url)) {
                return Err(Error::Invalid(format!(
                    "bad download URLs for {}",
                    file.path
                )));
            }
        }
        for path in &seen {
            if let Some((index, _)) = path
                .match_indices('/')
                .find(|(index, _)| seen.contains(&path[..*index]))
            {
                return Err(Error::Invalid(format!(
                    "{} is both a file and a folder",
                    &path[..index]
                )));
            }
        }
        let mut groups = HashSet::new();
        for group in &self.projects.optional_groups {
            if group.id.is_empty() || !groups.insert(group.id.as_str()) {
                return Err(Error::Invalid(format!(
                    "bad optional group id {:?}",
                    group.id
                )));
            }
        }
        Ok(())
    }

    pub fn file(&self, path: &str) -> Option<&PackFile> {
        self.files.iter().find(|file| file.path == path)
    }
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
