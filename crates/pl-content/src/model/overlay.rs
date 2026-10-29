//! The in-game overlay's mod jars through the signed content (OVL-AUTO): a jar CI built and tested
//! for a Minecraft version, published by Studio, so a new game version needs no launcher release.
//! The index lists them as `overlay`; launchers read it only while `features.overlayJars` is on,
//! and only for the versions their own jars leave out. The signed index is the trust root for code
//! that runs in the game: a launcher places a jar only when its bytes match the entry's size,
//! SHA-256 and SHA-1, and only when it speaks the launcher's bridge protocol. See
//! `docs/content-format.md`, "In-game overlay jars".

use std::collections::HashSet;
use std::fmt;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{FileRef, is_http_url, is_valid_minecraft_version};
use crate::fsutil::normalize_hex;
use crate::path::check_mirror_path;

/// Most jars `overlay.jars` may list.
pub const MAX_OVERLAY_JARS: usize = 32;
/// Largest jar, in bytes.
pub const MAX_OVERLAY_JAR_BYTES: u64 = 32 << 20;
/// Longest `range`, in bytes.
pub const MAX_OVERLAY_RANGE_LEN: usize = 64;
/// Longest `version`, in bytes.
pub const MAX_OVERLAY_VERSION_LEN: usize = 64;
/// What a jar's file name starts with in `mods/` (`projects-overlay-<version>.jar`).
pub const OVERLAY_JAR_PREFIX: &str = "projects-overlay-";

/// `RootIndex.overlay`: the jars the team published.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OverlayRelease {
    #[serde(default)]
    pub jars: Vec<OverlayJar>,
}

/// One tested jar.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OverlayJar {
    /// The Minecraft version CI built and tested it for (`26.4`).
    pub minecraft: String,
    /// The versions it accepts, in Fabric's syntax, as its own `fabric.mod.json` says (`~26.4`).
    pub range: String,
    /// The mod's version (`0.1.0+26.4`); the jar is `projects-overlay-<version>.jar` in `mods/`.
    pub version: String,
    /// The bridge protocol it speaks (`custom."projects:bridge".proto` of its `fabric.mod.json`).
    pub proto: u32,
    /// The jar itself, with its SHA-1 (required here).
    pub file: FileRef,
    /// The Fabric API version it was built and tested with (`0.170.0+26.4`). Launchers do not
    /// read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub fabric_api: Option<String>,
    /// The open repository's commit it was built from (the GPL source). Launchers do not read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// The CI run that built and tested it, which Studio took it from
    /// (`https://github.com/projects-gg/MinecraftLauncher/actions/runs/<id>`). Launchers do not
    /// read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// When CI's tests of it passed. Launchers do not read it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tested_at: Option<Timestamp>,
}

impl OverlayJar {
    /// Its file name in `mods/`: `projects-overlay-<version>.jar`.
    pub fn file_name(&self) -> String {
        format!("{OVERLAY_JAR_PREFIX}{}.jar", self.version)
    }

    /// The file's SHA-1, lowercase (`None` when it is missing or malformed).
    pub fn sha1(&self) -> Option<String> {
        self.file
            .sha1
            .as_deref()
            .and_then(|sha1| normalize_hex(sha1, 40))
    }

    /// The file's SHA-256, lowercase (`None` when it is malformed).
    pub fn sha256(&self) -> Option<String> {
        normalize_hex(&self.file.sha256, 64)
    }
}

/// The rule one jar entry breaks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OverlayJarFault {
    /// `minecraft` is not a Minecraft version id.
    BadMinecraft,
    /// `range` is empty, longer than [`MAX_OVERLAY_RANGE_LEN`], has spaces around it or a
    /// control character.
    BadRange,
    /// `version` is not 1–[`MAX_OVERLAY_VERSION_LEN`] bytes of `[A-Za-z0-9.+_-]`.
    BadVersion,
    /// `proto` is 0.
    BadProto,
    /// The file has no valid SHA-256 or SHA-1, no size or one above [`MAX_OVERLAY_JAR_BYTES`],
    /// or neither a mirror path nor an `http(s)` URL to fetch it from.
    BadFile,
    /// An earlier jar has the same `minecraft` and `proto`.
    Duplicate,
}

impl fmt::Display for OverlayJarFault {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::BadMinecraft => "minecraft is not a Minecraft version",
            Self::BadRange => "range is not a Fabric version range",
            Self::BadVersion => "version is not 1-64 characters of A-Z, a-z, 0-9, ., +, _ and -",
            Self::BadProto => "proto is not a bridge protocol version",
            Self::BadFile => "file has no valid size, SHA-256, SHA-1 or download location",
            Self::Duplicate => "an earlier jar is for the same Minecraft version and protocol",
        })
    }
}

/// What is wrong with the published jars (Studio refuses to publish any of it).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum OverlayProblem {
    /// More than [`MAX_OVERLAY_JARS`]: launchers use the first ones.
    TooMany,
    /// The jar at `index` breaks `fault`: launchers leave it out.
    Jar {
        index: usize,
        fault: OverlayJarFault,
    },
}

impl fmt::Display for OverlayProblem {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::TooMany => write!(f, "more than {MAX_OVERLAY_JARS} overlay jars"),
            Self::Jar { index, fault } => write!(f, "overlay jar {}: {fault}", index + 1),
        }
    }
}

/// A mod version as the jar's file name carries it: 1–[`MAX_OVERLAY_VERSION_LEN`] bytes of
/// `[A-Za-z0-9.+_-]`, not starting with `.`.
pub fn is_overlay_version(version: &str) -> bool {
    (1..=MAX_OVERLAY_VERSION_LEN).contains(&version.len())
        && !version.starts_with('.')
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'+' | b'_' | b'-'))
}

fn is_overlay_range(range: &str) -> bool {
    !range.is_empty()
        && range.len() <= MAX_OVERLAY_RANGE_LEN
        && range.trim() == range
        && !range.chars().any(char::is_control)
}

fn is_overlay_file(file: &FileRef) -> bool {
    let reachable = file
        .path
        .as_deref()
        .is_some_and(|path| check_mirror_path(path).is_ok())
        || file.urls.iter().any(|url| is_http_url(url));
    reachable
        && normalize_hex(&file.sha256, 64).is_some()
        && file
            .sha1
            .as_deref()
            .is_some_and(|sha1| normalize_hex(sha1, 40).is_some())
        && (1..=MAX_OVERLAY_JAR_BYTES).contains(&file.size)
}

/// The rule `jar` breaks on its own (duplicates need the whole list: [`OverlayRelease::problems`]).
pub fn overlay_jar_fault(jar: &OverlayJar) -> Option<OverlayJarFault> {
    if !is_valid_minecraft_version(&jar.minecraft) {
        Some(OverlayJarFault::BadMinecraft)
    } else if !is_overlay_range(&jar.range) {
        Some(OverlayJarFault::BadRange)
    } else if !is_overlay_version(&jar.version) {
        Some(OverlayJarFault::BadVersion)
    } else if jar.proto == 0 {
        Some(OverlayJarFault::BadProto)
    } else if !is_overlay_file(&jar.file) {
        Some(OverlayJarFault::BadFile)
    } else {
        None
    }
}

impl OverlayRelease {
    /// Everything Studio refuses to publish.
    pub fn problems(&self) -> Vec<OverlayProblem> {
        let mut problems = Vec::new();
        if self.jars.len() > MAX_OVERLAY_JARS {
            problems.push(OverlayProblem::TooMany);
        }
        let mut seen = HashSet::new();
        for (index, jar) in self.jars.iter().enumerate() {
            let fault = overlay_jar_fault(jar).or_else(|| {
                (!seen.insert((jar.minecraft.as_str(), jar.proto)))
                    .then_some(OverlayJarFault::Duplicate)
            });
            if let Some(fault) = fault {
                problems.push(OverlayProblem::Jar { index, fault });
            }
        }
        problems
    }

    /// The jars a launcher that speaks bridge protocol `proto` may use, in the published order:
    /// those of another protocol, those that break a rule and those whose Minecraft version an
    /// earlier usable jar has are left out; at most [`MAX_OVERLAY_JARS`] are read.
    pub fn usable(&self, proto: u32) -> Vec<&OverlayJar> {
        let mut seen = HashSet::new();
        self.jars
            .iter()
            .take(MAX_OVERLAY_JARS)
            .filter(|jar| jar.proto == proto && overlay_jar_fault(jar).is_none())
            .filter(|jar| seen.insert(jar.minecraft.as_str()))
            .collect()
    }

    /// The jar for exactly `minecraft` and `proto`, if one is listed.
    pub fn jar(&self, minecraft: &str, proto: u32) -> Option<&OverlayJar> {
        self.jars
            .iter()
            .find(|jar| jar.minecraft == minecraft && jar.proto == proto)
    }
}

/// `overlay-release.json`: what the open repository's CI writes beside the jars it built and
/// tested (`scripts/release/overlay-auto.mjs report`), and what Studio imports. It is never
/// published as it is: Studio checks each jar itself and lists it in `RootIndex.overlay`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OverlayCiRelease {
    pub schema: u32,
    /// `projects_overlay`.
    pub mod_id: String,
    /// The commit the jars were built from (40 lowercase hex).
    pub commit: String,
    /// The CI run (`https://github.com/<repo>/actions/runs/<id>`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub run: Option<String>,
    /// When the last test passed.
    pub tested_at: Timestamp,
    pub jars: Vec<OverlayCiJar>,
}

/// One jar of [`OverlayCiRelease`]: the file beside the report and what CI ran against it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OverlayCiJar {
    pub minecraft: String,
    pub range: String,
    pub version: String,
    pub proto: u32,
    /// The jar's file name beside the report (`projects-overlay-0.1.0+26.4.jar`).
    pub file: String,
    pub sha1: String,
    pub sha256: String,
    pub size: u64,
    pub fabric_api: String,
    /// The Fabric Loader the tests ran with.
    pub loader: String,
    /// The compatibility class it was built with (`mc26_3`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub compat: Option<String>,
    pub tests: OverlayCiTests,
}

/// The test suites CI ran against one jar.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct OverlayCiTests {
    /// The mod's JUnit tests.
    pub junit: OverlayCiCount,
    /// Fabric's client game test, run against the jar itself.
    pub gametest: OverlayCiCount,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct OverlayCiCount {
    pub passed: u32,
    pub failed: u32,
    pub skipped: u32,
}

impl OverlayCiTests {
    /// Both suites ran something and nothing failed: the jar counts as tested.
    pub fn passed(&self) -> bool {
        [self.junit, self.gametest]
            .iter()
            .all(|count| count.passed > 0 && count.failed == 0)
    }
}

/// The report's version this build reads.
pub const OVERLAY_CI_SCHEMA: u32 = 1;

#[cfg(test)]
mod tests {
    use super::*;

    const SHA1: &str = "226ba27e9c2d1831485c88c8aa66b9036a758cfe";
    const SHA256: &str = "4c5ed0886b06a3f90744efa271b0a15709d8cb400993566fd2fe3f8c68ca6d5a";

    fn jar(minecraft: &str) -> OverlayJar {
        OverlayJar {
            minecraft: minecraft.into(),
            range: format!("~{minecraft}"),
            version: format!("0.1.0+{minecraft}"),
            proto: 1,
            file: FileRef {
                path: Some(format!("files/{SHA1}")),
                urls: Vec::new(),
                sha256: SHA256.into(),
                sha1: Some(SHA1.into()),
                size: 953_397,
            },
            fabric_api: Some(format!("0.170.0+{minecraft}")),
            commit: Some("6b892a6a228d36d16b41e7c61b69fe8c0aa6749b".into()),
            run: Some(
                "https://github.com/projects-gg/MinecraftLauncher/actions/runs/18877003214".into(),
            ),
            tested_at: Some("2026-11-02T10:00:00Z".parse().unwrap()),
        }
    }

    #[test]
    fn a_sound_jar_breaks_no_rule() {
        let good = jar("26.4");
        assert_eq!(overlay_jar_fault(&good), None);
        assert_eq!(good.file_name(), "projects-overlay-0.1.0+26.4.jar");
        assert_eq!(good.sha1().as_deref(), Some(SHA1));
        assert_eq!(good.sha256().as_deref(), Some(SHA256));
        // A download URL instead of a mirror path is fine too.
        let mut url = jar("26.4");
        url.file.path = None;
        url.file.urls = vec!["https://example.net/overlay.jar".into()];
        assert_eq!(overlay_jar_fault(&url), None);
        // Upper case hex is read as it is meant.
        let mut upper = jar("26.4");
        upper.file.sha1 = Some(SHA1.to_ascii_uppercase());
        assert_eq!(upper.sha1().as_deref(), Some(SHA1));
        assert_eq!(overlay_jar_fault(&upper), None);
    }

    #[test]
    fn every_rule_has_its_fault() {
        let broken = |edit: fn(&mut OverlayJar)| {
            let mut entry = jar("26.4");
            edit(&mut entry);
            overlay_jar_fault(&entry)
        };
        let bad_minecraft = Some(OverlayJarFault::BadMinecraft);
        assert_eq!(broken(|j| j.minecraft = String::new()), bad_minecraft);
        assert_eq!(broken(|j| j.minecraft = "26 4".into()), bad_minecraft);
        let bad_range = Some(OverlayJarFault::BadRange);
        assert_eq!(broken(|j| j.range = String::new()), bad_range);
        assert_eq!(broken(|j| j.range = " ~26.4".into()), bad_range);
        assert_eq!(broken(|j| j.range = "~26.4\n".into()), bad_range);
        assert_eq!(broken(|j| j.range = "x".repeat(65)), bad_range);
        let bad_version = Some(OverlayJarFault::BadVersion);
        assert_eq!(broken(|j| j.version = String::new()), bad_version);
        assert_eq!(broken(|j| j.version = "../../evil".into()), bad_version);
        assert_eq!(broken(|j| j.version = "0.1 0".into()), bad_version);
        assert_eq!(broken(|j| j.version = ".hidden".into()), bad_version);
        assert_eq!(broken(|j| j.version = "1".repeat(65)), bad_version);
        assert_eq!(broken(|j| j.proto = 0), Some(OverlayJarFault::BadProto));
        let bad_file = Some(OverlayJarFault::BadFile);
        assert_eq!(broken(|j| j.file.sha1 = None), bad_file);
        assert_eq!(broken(|j| j.file.sha1 = Some("abc".into())), bad_file);
        assert_eq!(broken(|j| j.file.sha256 = "abc".into()), bad_file);
        assert_eq!(broken(|j| j.file.size = 0), bad_file);
        assert_eq!(
            broken(|j| j.file.size = MAX_OVERLAY_JAR_BYTES + 1),
            bad_file
        );
        assert_eq!(broken(|j| j.file.path = None), bad_file);
        assert_eq!(
            broken(|j| j.file.path = Some("../files/x".into())),
            bad_file
        );
        assert_eq!(
            broken(|j| {
                j.file.path = None;
                j.file.urls = vec!["ftp://example.net/overlay.jar".into()];
            }),
            bad_file
        );
    }

    #[test]
    fn launchers_take_their_protocol_and_the_first_jar_of_a_version() {
        let mut other_proto = jar("26.5");
        other_proto.proto = 2;
        let mut broken = jar("26.6");
        broken.file.sha1 = None;
        let mut again = jar("26.4");
        again.version = "0.1.1+26.4".into();
        let release = OverlayRelease {
            jars: vec![jar("26.4"), other_proto, broken, again, jar("26.7")],
        };
        let usable: Vec<&str> = release
            .usable(1)
            .iter()
            .map(|jar| jar.version.as_str())
            .collect();
        assert_eq!(usable, ["0.1.0+26.4", "0.1.0+26.7"]);
        let newer: Vec<&str> = release
            .usable(2)
            .iter()
            .map(|jar| jar.minecraft.as_str())
            .collect();
        assert_eq!(newer, ["26.5"]);
        assert!(release.usable(3).is_empty());
        assert_eq!(
            release.problems(),
            [
                OverlayProblem::Jar {
                    index: 2,
                    fault: OverlayJarFault::BadFile
                },
                OverlayProblem::Jar {
                    index: 3,
                    fault: OverlayJarFault::Duplicate
                },
            ]
        );
        assert_eq!(
            release.jar("26.5", 2).map(|jar| jar.minecraft.as_str()),
            Some("26.5")
        );
        assert!(release.jar("26.5", 1).is_none());
    }

    #[test]
    fn at_most_the_first_jars_are_read() {
        let release = OverlayRelease {
            jars: (0..MAX_OVERLAY_JARS + 3)
                .map(|i| jar(&format!("27.{i}")))
                .collect(),
        };
        assert_eq!(release.usable(1).len(), MAX_OVERLAY_JARS);
        assert_eq!(release.problems(), [OverlayProblem::TooMany]);
    }

    #[test]
    fn the_wire_shape_is_compact() {
        let mut bare = jar("26.4");
        bare.fabric_api = None;
        bare.commit = None;
        bare.run = None;
        bare.tested_at = None;
        let written = serde_json::to_value(&bare).unwrap();
        assert_eq!(
            written,
            serde_json::json!({
                "minecraft": "26.4",
                "range": "~26.4",
                "version": "0.1.0+26.4",
                "proto": 1,
                "file": {
                    "path": format!("files/{SHA1}"),
                    "sha256": SHA256,
                    "sha1": SHA1,
                    "size": 953_397
                }
            })
        );
        let full = serde_json::to_value(jar("26.4")).unwrap();
        assert_eq!(full["fabricApi"], "0.170.0+26.4");
        assert_eq!(full["testedAt"], "2026-11-02T10:00:00Z");
        let back: OverlayJar = serde_json::from_value(full).unwrap();
        assert_eq!(back, jar("26.4"));
        let empty: OverlayRelease = serde_json::from_str("{}").unwrap();
        assert!(empty.jars.is_empty());
    }

    #[test]
    fn a_jar_counts_as_tested_only_when_both_suites_ran_green() {
        let green = OverlayCiCount {
            passed: 3,
            failed: 0,
            skipped: 1,
        };
        let tests = |junit, gametest| OverlayCiTests { junit, gametest };
        assert!(tests(green, green).passed());
        let red = OverlayCiCount { failed: 1, ..green };
        assert!(!tests(red, green).passed());
        assert!(!tests(green, red).passed());
        assert!(
            !tests(green, OverlayCiCount::default()).passed(),
            "nothing ran"
        );
        assert!(!tests(OverlayCiCount::default(), green).passed());
    }
}
