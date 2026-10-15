//! Tolerant serde model of Mojang version JSONs and loader profiles (Fabric, Quilt, Forge,
//! NeoForge, OptiFine...). Unknown fields are ignored and malformed library/argument entries are
//! skipped with a warning instead of failing the whole file.

use std::collections::BTreeMap;

use pl_net::Checksum;
use serde::de::DeserializeOwned;
use serde::{Deserialize, Deserializer, Serialize};

use crate::error::Result;
use crate::platform::Platform;
use crate::version::maven::{MAVEN_CENTRAL_URL, MOJANG_LIBRARIES_URL, MavenCoord};
use crate::version::rules::{Features, rules_allow};

/// One `versions/<id>/<id>.json` file, before `inheritsFrom` resolution.
#[derive(Debug, Clone, Default, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionJson {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub inherits_from: Option<String>,
    /// `release`, `snapshot`, `old_beta`, `old_alpha` (free text in loader profiles).
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub main_class: Option<String>,
    /// Pre-1.13 game arguments as one space-separated string.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minecraft_arguments: Option<String>,
    /// 1.13+ structured arguments.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arguments: Option<Arguments>,
    #[serde(
        default,
        deserialize_with = "lenient_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub libraries: Vec<Library>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub asset_index: Option<AssetIndexInfo>,
    /// Asset index id for very old JSONs without `assetIndex`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub assets: Option<String>,
    /// `client`, `server`, `client_mappings`, ...
    #[serde(
        default,
        deserialize_with = "lenient_map",
        skip_serializing_if = "BTreeMap::is_empty"
    )]
    pub downloads: BTreeMap<String, Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub java_version: Option<JavaVersionInfo>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub logging: Option<LoggingInfo>,
    /// Kept as text: loader profiles use odd formats; see [`parse_timestamp`].
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub release_time: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub time: Option<String>,
    /// Version whose client jar should be used (legacy Forge/OptiFine profiles).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub jar: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compliance_level: Option<u32>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub minimum_launcher_version: Option<u32>,
}

impl VersionJson {
    pub fn from_slice(bytes: &[u8], what: &str) -> Result<Self> {
        serde_json::from_slice(bytes).map_err(|e| crate::Error::json(what, e))
    }
}

/// A file download (`downloads.client`, `downloads.artifact`, classifiers...).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Artifact {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub path: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
}

impl Artifact {
    pub fn checksum(&self) -> Option<Checksum> {
        self.sha1
            .as_deref()
            .filter(|s| !s.trim().is_empty())
            .map(Checksum::sha1)
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AssetIndexInfo {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub total_size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JavaVersionInfo {
    /// Mojang runtime component (`java-runtime-delta`, `jre-legacy`...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub component: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub major_version: Option<u32>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingInfo {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub client: Option<LoggingConfig>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingConfig {
    /// JVM argument template, e.g. `-Dlog4j.configurationFile=${path}`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub argument: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub file: Option<LoggingFile>,
    #[serde(rename = "type", default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LoggingFile {
    pub id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

/// `rules[]` entry used by libraries and arguments.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub action: RuleAction,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub os: Option<OsRule>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub features: Option<BTreeMap<String, bool>>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum RuleAction {
    Allow,
    Disallow,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct OsRule {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Regex matched against the OS version (`^10\.` for Windows 10/11).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub version: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub arch: Option<String>,
}

/// 1.13+ `arguments` object.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct Arguments {
    #[serde(
        default,
        deserialize_with = "lenient_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub game: Vec<Argument>,
    #[serde(
        default,
        deserialize_with = "lenient_vec",
        skip_serializing_if = "Vec::is_empty"
    )]
    pub jvm: Vec<Argument>,
}

/// One argument: a plain string or a rule-guarded value.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum Argument {
    Plain(String),
    Conditional {
        #[serde(default)]
        rules: Vec<Rule>,
        value: ArgumentValue,
    },
}

impl Argument {
    pub fn plain(value: impl Into<String>) -> Self {
        Self::Plain(value.into())
    }

    /// The raw (unsubstituted) values regardless of rules.
    pub fn values(&self) -> Vec<&str> {
        match self {
            Self::Plain(s) => vec![s.as_str()],
            Self::Conditional { value, .. } => value.values(),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(untagged)]
pub enum ArgumentValue {
    Single(String),
    Many(Vec<String>),
}

impl ArgumentValue {
    pub fn values(&self) -> Vec<&str> {
        match self {
            Self::Single(s) => vec![s.as_str()],
            Self::Many(v) => v.iter().map(String::as_str).collect(),
        }
    }
}

/// A library entry. Covers Mojang (`downloads.artifact` / `classifiers` / `natives`), Fabric and
/// Quilt (`name` + `url` + `sha1`/`size`) and Forge (`clientreq`, empty URLs for generated files).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Library {
    /// Maven coordinate.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub downloads: Option<LibraryDownloads>,
    /// Maven repository base for name-only libraries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha1: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha256: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sha512: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub size: Option<u64>,
    /// Legacy natives: OS name → classifier (may contain `${arch}`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub natives: Option<BTreeMap<String, String>>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub extract: Option<ExtractRules>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rules: Option<Vec<Rule>>,
    /// Old Forge profiles: `false` marks server-only libraries.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub clientreq: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub serverreq: Option<bool>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct LibraryDownloads {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub artifact: Option<Artifact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub classifiers: Option<BTreeMap<String, Artifact>>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExtractRules {
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude: Vec<String>,
}

/// A concrete file to fetch for a library.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LibraryArtifact {
    /// Maven-layout path relative to `libraries/`, `/`-separated.
    pub path: String,
    /// Candidate URLs, primary first. Empty when the file is generated locally (Forge processors).
    pub urls: Vec<String>,
    pub checksum: Option<Checksum>,
    pub size: Option<u64>,
}

impl Library {
    pub fn coord(&self) -> Result<MavenCoord> {
        MavenCoord::parse(&self.name)
    }

    /// Key for child-over-parent deduplication: `group:artifact[:classifier]`.
    pub fn dedup_key(&self) -> String {
        self.coord()
            .map(|c| c.dedup_key())
            .unwrap_or_else(|_| self.name.clone())
    }

    /// Whether the library is used on `platform` (rules + Forge's `clientreq: false`).
    pub fn applies_to(&self, platform: &Platform) -> bool {
        if self.clientreq == Some(false) {
            return false;
        }
        match &self.rules {
            Some(rules) => rules_allow(rules, platform, &Features::default()),
            None => true,
        }
    }

    /// 1.19+ natives jar (`org.lwjgl:lwjgl:3.3.3:natives-windows`): a regular classpath library
    /// whose classifier names an OS/architecture.
    pub fn natives_classifier_in_name(&self) -> Option<String> {
        self.coord()
            .ok()
            .and_then(|c| c.classifier)
            .filter(|c| c.starts_with("natives-"))
    }

    fn checksum(&self) -> Option<Checksum> {
        fn pick(value: &Option<String>) -> Option<&str> {
            value.as_deref().filter(|v| !v.trim().is_empty())
        }
        pick(&self.sha1)
            .map(Checksum::sha1)
            .or_else(|| pick(&self.sha256).map(Checksum::sha256))
            .or_else(|| pick(&self.sha512).map(Checksum::sha512))
    }

    /// The jar that goes on the classpath, if any. Legacy natives-only entries have none.
    pub fn artifact(&self) -> Option<LibraryArtifact> {
        let downloads = self.downloads.as_ref();
        if let Some(artifact) = downloads.and_then(|d| d.artifact.as_ref()) {
            let path = match &artifact.path {
                Some(p) if !p.trim().is_empty() => p.clone(),
                _ => self.coord().ok()?.path(),
            };
            let urls = match artifact.url.as_deref() {
                Some(u) if !u.trim().is_empty() => vec![u.to_owned()],
                // An explicitly empty URL marks a file generated by a loader installer.
                Some(_) => Vec::new(),
                None => maven_urls(self.url.as_deref(), &path),
            };
            return Some(LibraryArtifact {
                path,
                urls,
                checksum: artifact.checksum().or_else(|| self.checksum()),
                size: artifact.size.or(self.size),
            });
        }
        if self.natives.is_some() || downloads.is_some_and(|d| d.classifiers.is_some()) {
            return None;
        }
        let path = self.coord().ok()?.path();
        Some(LibraryArtifact {
            urls: maven_urls(self.url.as_deref(), &path),
            path,
            checksum: self.checksum(),
            size: self.size,
        })
    }

    /// Legacy natives classifier for `platform` (`natives-windows-64` after `${arch}`).
    pub fn native_classifier(&self, platform: &Platform) -> Option<String> {
        let natives = self.natives.as_ref()?;
        let classifier = natives
            .iter()
            .find_map(|(os, classifier)| platform.os.matches_name(os).then_some(classifier))?;
        Some(classifier.replace("${arch}", platform.arch_bits()))
    }

    /// Legacy natives jar to extract for `platform`.
    pub fn native_artifact(&self, platform: &Platform) -> Option<LibraryArtifact> {
        let classifier = self.native_classifier(platform)?;
        let listed = self
            .downloads
            .as_ref()
            .and_then(|d| d.classifiers.as_ref())
            .and_then(|c| c.get(&classifier));
        let coord = self.coord().ok().map(|c| c.with_classifier(&classifier));
        match listed {
            Some(artifact) => {
                let path = match &artifact.path {
                    Some(p) if !p.trim().is_empty() => p.clone(),
                    _ => coord?.path(),
                };
                let urls = match artifact.url.as_deref() {
                    Some(u) if !u.trim().is_empty() => vec![u.to_owned()],
                    _ => maven_urls(self.url.as_deref(), &path),
                };
                Some(LibraryArtifact {
                    path,
                    urls,
                    checksum: artifact.checksum(),
                    size: artifact.size,
                })
            }
            None => {
                let path = coord?.path();
                Some(LibraryArtifact {
                    urls: maven_urls(self.url.as_deref(), &path),
                    path,
                    checksum: None,
                    size: None,
                })
            }
        }
    }

    /// `extract.exclude` prefixes (always including `META-INF/`).
    pub fn extract_excludes(&self) -> Vec<String> {
        let mut excludes: Vec<String> = self
            .extract
            .as_ref()
            .map(|e| e.exclude.clone())
            .unwrap_or_default();
        if !excludes.iter().any(|e| e == "META-INF/") {
            excludes.push("META-INF/".to_owned());
        }
        excludes
    }
}

/// Repository URLs for a name-only library: its own repository, then Mojang's, then Maven Central.
pub fn maven_urls(base: Option<&str>, path: &str) -> Vec<String> {
    let mut urls = Vec::with_capacity(3);
    let bases = base
        .filter(|b| !b.trim().is_empty())
        .into_iter()
        .chain([MOJANG_LIBRARIES_URL, MAVEN_CENTRAL_URL]);
    for base in bases {
        let url = format!("{}/{}", base.trim().trim_end_matches('/'), path);
        if !urls.contains(&url) {
            urls.push(url);
        }
    }
    urls
}

/// Parse Mojang / loader timestamps (`2024-08-08T12:24:45+00:00`, also `-0700` style offsets).
pub fn parse_timestamp(text: &str) -> Option<jiff::Timestamp> {
    let text = text.trim();
    if let Ok(ts) = text.parse::<jiff::Timestamp>() {
        return Some(ts);
    }
    // `+0000` / `-0700` offsets without a colon.
    if text.is_ascii() && text.len() > 5 {
        let (head, tail) = text.split_at(text.len() - 5);
        let tb = tail.as_bytes();
        if (tb[0] == b'+' || tb[0] == b'-') && tb[1..].iter().all(u8::is_ascii_digit) {
            let fixed = format!("{head}{}:{}", &tail[..3], &tail[3..]);
            if let Ok(ts) = fixed.parse::<jiff::Timestamp>() {
                return Some(ts);
            }
        }
    }
    // No offset at all: treat as UTC.
    text.parse::<jiff::civil::DateTime>()
        .ok()
        .and_then(|dt| dt.to_zoned(jiff::tz::TimeZone::UTC).ok())
        .map(|z| z.timestamp())
}

/// Deserialize a list, skipping entries that do not match `T` (with a warning).
fn lenient_vec<'de, D, T>(deserializer: D) -> std::result::Result<Vec<T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let raw: Option<Vec<serde_json::Value>> = Option::deserialize(deserializer)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .filter_map(|value| match serde_json::from_value::<T>(value) {
            Ok(item) => Some(item),
            Err(error) => {
                tracing::warn!(%error, "skipping malformed entry in version JSON");
                None
            }
        })
        .collect())
}

/// Deserialize a map, skipping values that do not match `T` (with a warning).
fn lenient_map<'de, D, T>(deserializer: D) -> std::result::Result<BTreeMap<String, T>, D::Error>
where
    D: Deserializer<'de>,
    T: DeserializeOwned,
{
    let raw: Option<BTreeMap<String, serde_json::Value>> = Option::deserialize(deserializer)?;
    Ok(raw
        .unwrap_or_default()
        .into_iter()
        .filter_map(|(key, value)| match serde_json::from_value::<T>(value) {
            Ok(item) => Some((key, item)),
            Err(error) => {
                tracing::warn!(%key, %error, "skipping malformed download in version JSON");
                None
            }
        })
        .collect())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::{Arch, Os};

    fn win64() -> Platform {
        Platform::new(Os::Windows, Arch::X86_64, "10.0.26100")
    }

    #[test]
    fn tolerant_parsing() {
        let json = r#"{
            "id": "x",
            "libraries": [
                {"name": "a:b:1"},
                {"oops": true},
                {"name": "c:d:2", "url": "https://maven.example/"}
            ],
            "arguments": {"game": ["--a", 5, {"rules": [], "value": ["--b", "c"]}], "jvm": null},
            "downloads": {"client": {"sha1": "ab", "size": 1, "url": "https://x/c.jar"}, "weird": 3},
            "unknownField": {"anything": [1, 2, 3]}
        }"#;
        let v = VersionJson::from_slice(json.as_bytes(), "test").unwrap();
        assert_eq!(v.libraries.len(), 2);
        let args = v.arguments.unwrap();
        assert_eq!(args.game.len(), 2);
        assert!(args.jvm.is_empty());
        assert_eq!(v.downloads.len(), 1);
    }

    #[test]
    fn fabric_style_library() {
        let lib: Library = serde_json::from_str(
            r#"{"name": "net.fabricmc:fabric-loader:0.16.5", "url": "https://maven.fabricmc.net/",
                "sha1": "AB12", "size": 10}"#,
        )
        .unwrap();
        let a = lib.artifact().unwrap();
        assert_eq!(
            a.path,
            "net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar"
        );
        assert_eq!(
            a.urls,
            [
                "https://maven.fabricmc.net/net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar",
                "https://libraries.minecraft.net/net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar",
                "https://repo1.maven.org/maven2/net/fabricmc/fabric-loader/0.16.5/fabric-loader-0.16.5.jar",
            ]
        );
        assert_eq!(a.checksum, Some(Checksum::sha1("ab12")));
        assert_eq!(a.size, Some(10));
    }

    #[test]
    fn forge_generated_library_has_no_urls() {
        let lib: Library = serde_json::from_str(
            r#"{"name": "net.minecraftforge:forge:1.20.1-47.2.0:client",
                "downloads": {"artifact": {"path": "net/minecraftforge/forge/1.20.1-47.2.0/forge-1.20.1-47.2.0-client.jar",
                "url": "", "sha1": "aa", "size": 5}}}"#,
        )
        .unwrap();
        let a = lib.artifact().unwrap();
        assert!(a.urls.is_empty());
        let server_only: Library =
            serde_json::from_str(r#"{"name": "a:b:1", "clientreq": false}"#).unwrap();
        assert!(!server_only.applies_to(&win64()));
    }

    #[test]
    fn legacy_natives() {
        let lib: Library = serde_json::from_str(
            r#"{"name": "tv.twitch:twitch-platform:5.16",
                "rules": [{"action": "allow"}, {"action": "disallow", "os": {"name": "linux"}}],
                "natives": {"linux": "natives-linux", "osx": "natives-osx", "windows": "natives-windows-${arch}"},
                "extract": {"exclude": ["META-INF/"]},
                "downloads": {"classifiers": {
                    "natives-windows-64": {"path": "tv/twitch/twitch-platform/5.16/twitch-platform-5.16-natives-windows-64.jar",
                        "sha1": "b7", "size": 7, "url": "https://libraries.minecraft.net/tv/twitch/twitch-platform/5.16/twitch-platform-5.16-natives-windows-64.jar"}}}}"#,
        )
        .unwrap();
        let p = win64();
        assert!(lib.applies_to(&p));
        assert!(
            lib.artifact().is_none(),
            "natives-only entries have no classpath jar"
        );
        assert_eq!(
            lib.native_classifier(&p).as_deref(),
            Some("natives-windows-64")
        );
        let n = lib.native_artifact(&p).unwrap();
        assert!(n.path.ends_with("natives-windows-64.jar"));
        assert_eq!(n.size, Some(7));
        let linux = Platform::new(Os::Linux, Arch::X86_64, "6.1");
        assert!(!lib.applies_to(&linux));
        assert_eq!(lib.extract_excludes(), ["META-INF/"]);
    }

    #[test]
    fn new_style_natives_name() {
        let lib: Library =
            serde_json::from_str(r#"{"name": "org.lwjgl:lwjgl:3.3.3:natives-windows-arm64"}"#)
                .unwrap();
        assert_eq!(
            lib.natives_classifier_in_name().as_deref(),
            Some("natives-windows-arm64")
        );
        assert!(lib.artifact().is_some());
    }

    #[test]
    fn timestamps() {
        assert!(parse_timestamp("2024-08-08T12:24:45+00:00").is_some());
        assert!(parse_timestamp("1960-01-01T00:00:00-0700").is_some());
        assert!(parse_timestamp("2013-04-04T16:30:00").is_some());
        assert!(parse_timestamp("garbage").is_none());
        let a = parse_timestamp("2009-05-13T20:11:00+00:00").unwrap();
        let b = parse_timestamp("2026-09-15T11:23:02+00:00").unwrap();
        assert!(a < b);
    }
}
