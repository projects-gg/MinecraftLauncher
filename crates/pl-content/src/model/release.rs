use std::collections::BTreeMap;

use jiff::Timestamp;
use pl_lang::Lang;
use serde::{Deserialize, Serialize};

use crate::fsutil::sha256_hex;

/// `releases/<channel>.json`: Tauri's static updater JSON (`version`, `notes`, `pub_date`,
/// `platforms`) plus staged-rollout fields that Tauri ignores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseChannel {
    /// Semver, optionally prefixed with `v`.
    pub version: String,
    /// The release notes (Markdown): the one text Tauri and launchers before per-language notes
    /// read, whatever the player's language. Writers put the Turkish notes here (the English
    /// ones when there are none).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
    /// The notes per language, written when the release has English notes; what a player sees is
    /// [`ReleaseChannel::notes_for`]. Display text never costs the file: an odd value reads as
    /// none.
    #[serde(
        default,
        deserialize_with = "lenient",
        skip_serializing_if = "Option::is_none"
    )]
    #[specta(type = Option<LocalizedNotes>)]
    pub localized_notes: Option<LocalizedNotes>,
    #[serde(rename = "pub_date", default, skip_serializing_if = "Option::is_none")]
    pub pub_date: Option<Timestamp>,
    /// Tauri target key (`windows-x86_64`, `darwin-aarch64`, `linux-x86_64`, …) → bundle.
    #[serde(default)]
    pub platforms: BTreeMap<String, ReleasePlatform>,
    /// Percentage of installs (0–100) that are offered this release.
    #[serde(default = "full_rollout")]
    pub rollout: u8,
    /// Every eligible install must update (lock screen).
    #[serde(default)]
    pub mandatory: bool,
    /// Installs older than this must update regardless of `rollout`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub min_version: Option<String>,
    /// `stable`, `beta`, …
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
}

fn full_rollout() -> u8 {
    100
}

/// A field that does not read is absent (the same type it writes, which specta is told).
fn lenient<'de, D, T>(deserializer: D) -> Result<Option<T>, D::Error>
where
    D: serde::Deserializer<'de>,
    T: serde::de::DeserializeOwned,
{
    let value = serde_json::Value::deserialize(deserializer)?;
    Ok(serde_json::from_value(value).ok())
}

/// A release's notes per language (`localizedNotes` of a channel file); either may be missing.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct LocalizedNotes {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tr: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub en: Option<String>,
}

impl LocalizedNotes {
    /// The notes in `lang`, else the other language's. A blank text counts as missing.
    pub fn get(&self, lang: Lang) -> Option<&str> {
        let (first, second) = match lang {
            Lang::Tr => (&self.tr, &self.en),
            Lang::En => (&self.en, &self.tr),
        };
        [first, second]
            .into_iter()
            .filter_map(Option::as_deref)
            .find(|text| !text.trim().is_empty())
    }
}

/// The release notes a player reading `lang` sees: that language's, else the other language's
/// ([`LocalizedNotes::get`]), else `notes`, the one text launchers before per-language notes show
/// in both languages. Blank texts count as missing.
pub fn release_notes<'a>(
    localized: Option<&'a LocalizedNotes>,
    notes: Option<&'a str>,
    lang: Lang,
) -> Option<&'a str> {
    localized
        .and_then(|localized| localized.get(lang))
        .or_else(|| notes.filter(|text| !text.trim().is_empty()))
}

/// [`release_notes`] of a channel document as JSON, e.g. the file an update came from as Tauri's
/// updater keeps it. Only `localizedNotes` and `notes` are read; odd values count as missing.
pub fn release_notes_in(document: &serde_json::Value, lang: Lang) -> Option<String> {
    let localized = document
        .get("localizedNotes")
        .and_then(|value| LocalizedNotes::deserialize(value).ok());
    let notes = document.get("notes").and_then(serde_json::Value::as_str);
    release_notes(localized.as_ref(), notes, lang).map(str::to_owned)
}

/// One updater bundle: minisign signature (content of the `.sig` file) and download URL.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReleasePlatform {
    pub signature: String,
    pub url: String,
}

/// What the launcher should do with a channel file.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UpdateDecision {
    /// The installed version is the same or newer.
    UpToDate,
    /// Newer, but this install is outside the rollout percentage.
    HeldBack,
    /// Offer the update.
    Available,
    /// Install before playing.
    Mandatory,
    /// The channel file has an unparseable version.
    Invalid,
}

impl ReleaseChannel {
    pub fn target_version(&self) -> Option<semver::Version> {
        parse_version(&self.version)
    }

    pub fn platform(&self, target: &str) -> Option<&ReleasePlatform> {
        self.platforms.get(target)
    }

    /// The notes a player reading `lang` sees ([`release_notes`]).
    pub fn notes_for(&self, lang: Lang) -> Option<&str> {
        release_notes(self.localized_notes.as_ref(), self.notes.as_deref(), lang)
    }

    /// Decides for an install running `current` whose rollout bucket is `bucket`
    /// (see [`rollout_bucket`]). Installs below `minVersion` always get a mandatory update;
    /// otherwise the install must fall inside `rollout` percent.
    pub fn decide(&self, current: &semver::Version, bucket: u8) -> UpdateDecision {
        let Some(target) = self.target_version() else {
            return UpdateDecision::Invalid;
        };
        if target <= *current {
            return UpdateDecision::UpToDate;
        }
        let forced = self
            .min_version
            .as_deref()
            .and_then(parse_version)
            .is_some_and(|min| *current < min);
        if forced {
            return UpdateDecision::Mandatory;
        }
        if bucket >= self.rollout.min(100) {
            return UpdateDecision::HeldBack;
        }
        if self.mandatory {
            UpdateDecision::Mandatory
        } else {
            UpdateDecision::Available
        }
    }
}

/// Stable rollout bucket in `0..100` for an install id: the first four bytes of
/// SHA-256(install id), big endian, modulo 100.
pub fn rollout_bucket(install_id: &str) -> u8 {
    let digest = sha256_hex(install_id.as_bytes());
    let prefix = u32::from_str_radix(&digest[..8], 16).unwrap_or(0);
    // `% 100` keeps the value below 100, so the narrowing cannot truncate.
    (prefix % 100) as u8
}

/// The release channel files a launcher reads for its own updates, in order:
/// `<mirror>releases/<channel>.json` (`beta`, or `stable` for anything else). The installed
/// launcher reads its content mirrors (`production`). A test launcher (`isolated`: debug builds,
/// every `PROJECTS_LAUNCHER_INSTANCE`) reads only the test mirrors it was given (`test`, the debug
/// override of the content mirrors): without one it gets no channel at all, so it never offers,
/// downloads or runs a real release's installer over the installed launcher.
pub fn release_channel_urls(
    isolated: bool,
    production: &[&str],
    test: Option<&[String]>,
    channel: &str,
) -> Vec<String> {
    let channel = if channel == "beta" { "beta" } else { "stable" };
    let bases: Vec<&str> = if isolated {
        test.unwrap_or_default()
            .iter()
            .map(String::as_str)
            .collect()
    } else {
        production.to_vec()
    };
    bases
        .into_iter()
        .map(|base| format!("{base}releases/{channel}.json"))
        .collect()
}

/// Whether a launcher may download the update bundle at `url`. The installed launcher takes what
/// its signed channel names. A test launcher takes a bundle only from one of its test mirrors
/// ([`release_channel_urls`]), so a test channel that points at a real installer is refused too.
pub fn release_bundle_allowed(isolated: bool, test: Option<&[String]>, url: &str) -> bool {
    !isolated
        || test
            .unwrap_or_default()
            .iter()
            .any(|base| !base.is_empty() && url.starts_with(base.as_str()))
}

fn parse_version(text: &str) -> Option<semver::Version> {
    let text = text.trim();
    semver::Version::parse(text.strip_prefix('v').unwrap_or(text)).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn channel(version: &str, rollout: u8) -> ReleaseChannel {
        ReleaseChannel {
            version: version.into(),
            notes: None,
            localized_notes: None,
            pub_date: None,
            platforms: BTreeMap::new(),
            rollout,
            mandatory: false,
            min_version: None,
            channel: Some("stable".into()),
        }
    }

    #[test]
    fn rollout_buckets_are_stable_and_spread() {
        assert_eq!(rollout_bucket("install-1"), rollout_bucket("install-1"));
        let mut counts = [0u32; 100];
        for i in 0..10_000 {
            let bucket = rollout_bucket(&format!("install-{i}"));
            assert!(bucket < 100);
            counts[usize::from(bucket)] += 1;
        }
        // Every bucket receives roughly 1 %: 100 expected, allow wide noise.
        assert!(
            counts.iter().all(|&c| (50..=160).contains(&c)),
            "{counts:?}"
        );
        let below_half = counts[..50].iter().sum::<u32>();
        assert!((4_500..=5_500).contains(&below_half), "{below_half}");
    }

    #[test]
    fn eligibility() {
        let current = semver::Version::new(2, 0, 0);
        assert_eq!(
            channel("2.0.0", 100).decide(&current, 0),
            UpdateDecision::UpToDate
        );
        assert_eq!(
            channel("1.9.0", 100).decide(&current, 0),
            UpdateDecision::UpToDate
        );
        assert_eq!(
            channel("v2.1.0", 100).decide(&current, 99),
            UpdateDecision::Available
        );
        assert_eq!(
            channel("2.1.0", 0).decide(&current, 0),
            UpdateDecision::HeldBack
        );
        assert_eq!(
            channel("2.1.0", 25).decide(&current, 24),
            UpdateDecision::Available
        );
        assert_eq!(
            channel("2.1.0", 25).decide(&current, 25),
            UpdateDecision::HeldBack
        );
        assert_eq!(
            channel("2.1.0", 250).decide(&current, 99),
            UpdateDecision::Available
        );
        assert_eq!(
            channel("nonsense", 100).decide(&current, 0),
            UpdateDecision::Invalid
        );

        let mut mandatory = channel("2.1.0", 10);
        mandatory.mandatory = true;
        assert_eq!(mandatory.decide(&current, 5), UpdateDecision::Mandatory);
        assert_eq!(mandatory.decide(&current, 50), UpdateDecision::HeldBack);

        let mut forced = channel("2.1.0", 0);
        forced.min_version = Some("2.0.5".into());
        assert_eq!(forced.decide(&current, 99), UpdateDecision::Mandatory);
        assert_eq!(
            forced.decide(&semver::Version::new(2, 0, 5), 99),
            UpdateDecision::HeldBack
        );
    }

    const PRODUCTION: [&str; 2] = [
        "https://cdn.projects.gg/v1/",
        "https://raw.githubusercontent.com/projects-gg/launcher-content/main/v1/",
    ];

    #[test]
    fn the_installed_launcher_reads_its_content_mirrors() {
        assert_eq!(
            release_channel_urls(false, &PRODUCTION, None, "stable"),
            [
                "https://cdn.projects.gg/v1/releases/stable.json",
                "https://raw.githubusercontent.com/projects-gg/launcher-content/main/v1/releases/stable.json"
            ]
        );
        assert_eq!(
            release_channel_urls(false, &PRODUCTION, None, "beta")[0],
            "https://cdn.projects.gg/v1/releases/beta.json"
        );
        // Any other name is the stable channel (nothing from outside reaches the path).
        assert_eq!(
            release_channel_urls(false, &PRODUCTION, None, "../nightly")[0],
            "https://cdn.projects.gg/v1/releases/stable.json"
        );
        // Test mirrors never redirect the installed launcher.
        let test = vec!["http://127.0.0.1:8811/v1/".to_owned()];
        assert_eq!(
            release_channel_urls(false, &PRODUCTION, Some(test.as_slice()), "stable")[0],
            "https://cdn.projects.gg/v1/releases/stable.json"
        );
        assert!(release_bundle_allowed(
            false,
            None,
            "https://cdn.projects.gg/v1/releases/2.1.0/x.nsis.zip"
        ));
    }

    #[test]
    fn a_test_launcher_never_reaches_a_real_release() {
        for test in [None, Some(&[][..])] {
            assert!(
                release_channel_urls(true, &PRODUCTION, test, "stable").is_empty(),
                "no test mirror: no updater"
            );
            assert!(release_channel_urls(true, &PRODUCTION, test, "beta").is_empty());
            assert!(!release_bundle_allowed(
                true,
                test,
                "https://cdn.projects.gg/v1/releases/2.1.0/x.nsis.zip"
            ));
        }
        let test = vec!["http://127.0.0.1:8811/v1/".to_owned()];
        assert_eq!(
            release_channel_urls(true, &PRODUCTION, Some(test.as_slice()), "beta"),
            ["http://127.0.0.1:8811/v1/releases/beta.json"]
        );
        assert!(release_bundle_allowed(
            true,
            Some(test.as_slice()),
            "http://127.0.0.1:8811/v1/releases/2.1.0/x.nsis.zip"
        ));
        // A test channel that names the real installer is refused.
        assert!(!release_bundle_allowed(
            true,
            Some(test.as_slice()),
            "https://cdn.projects.gg/v1/releases/2.1.0/x.nsis.zip"
        ));
        assert!(!release_bundle_allowed(
            true,
            Some(&[String::new()][..]),
            "https://cdn.projects.gg/v1/releases/2.1.0/x.nsis.zip"
        ));
    }

    #[test]
    fn tauri_compatible_shape() {
        let json = r#"{
            "version": "2.1.0",
            "notes": "Yenilikler",
            "pub_date": "2026-10-15T12:00:00Z",
            "platforms": {
                "windows-x86_64": {"signature": "c2ln", "url": "https://cdn.projects.gg/v1/releases/2.1.0/x.nsis.zip"}
            }
        }"#;
        let parsed: ReleaseChannel = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.rollout, 100);
        assert!(!parsed.mandatory);
        assert!(parsed.platform("windows-x86_64").is_some());
        let out = serde_json::to_value(&parsed).unwrap();
        assert_eq!(out["pub_date"], "2026-10-15T12:00:00Z");
        assert!(out.get("minVersion").is_none());
        assert!(out.get("localizedNotes").is_none());
        assert_eq!(out["rollout"], 100);
    }

    fn localized(tr: Option<&str>, en: Option<&str>) -> LocalizedNotes {
        LocalizedNotes {
            tr: tr.map(Into::into),
            en: en.map(Into::into),
        }
    }

    #[test]
    fn notes_show_in_the_players_language_then_the_other_then_the_one_text() {
        let both = localized(Some("- Yeni"), Some("- New"));
        let old = Some("- Eski tek metin");
        assert_eq!(release_notes(Some(&both), old, Lang::Tr), Some("- Yeni"));
        assert_eq!(release_notes(Some(&both), old, Lang::En), Some("- New"));
        // A language without notes reads the other language's.
        let turkish = localized(Some("- Yeni"), None);
        assert_eq!(
            release_notes(Some(&turkish), None, Lang::En),
            Some("- Yeni")
        );
        let english = localized(None, Some("- New"));
        assert_eq!(release_notes(Some(&english), old, Lang::Tr), Some("- New"));
        // Blank counts as missing.
        let blank = localized(Some("- Yeni"), Some(" \n "));
        assert_eq!(release_notes(Some(&blank), old, Lang::En), Some("- Yeni"));
        // No language has notes: the one text every launcher reads, in both languages.
        for lang in [Lang::Tr, Lang::En] {
            assert_eq!(release_notes(None, old, lang), old);
            assert_eq!(
                release_notes(Some(&localized(Some(""), None)), old, lang),
                old
            );
            assert_eq!(release_notes(None, Some("  "), lang), None);
            assert_eq!(release_notes(None, None, lang), None);
        }
    }

    #[test]
    fn a_channel_with_notes_per_language() {
        let json = r#"{
            "version": "2.1.0",
            "notes": "Yeni ana ekran",
            "localizedNotes": {"tr": "Yeni ana ekran", "en": "A new home screen"},
            "platforms": {}
        }"#;
        let parsed: ReleaseChannel = serde_json::from_str(json).unwrap();
        assert_eq!(parsed.notes_for(Lang::Tr), Some("Yeni ana ekran"));
        assert_eq!(parsed.notes_for(Lang::En), Some("A new home screen"));
        let out = serde_json::to_value(&parsed).unwrap();
        assert_eq!(out["localizedNotes"]["en"], "A new home screen");

        // An older writer's file: the one text in both languages.
        let old: ReleaseChannel =
            serde_json::from_str(r#"{"version": "2.0.1", "notes": "Yeni"}"#).unwrap();
        assert_eq!(old.notes_for(Lang::En), Some("Yeni"));

        // Odd notes per language never cost the release: the file reads, `notes` shows.
        for odd in [r#""text""#, "null", r#"{"tr": 5}"#, r#"{"en": ["New"]}"#] {
            let json =
                format!(r#"{{"version": "2.1.0", "notes": "Yeni", "localizedNotes": {odd}}}"#);
            let parsed: ReleaseChannel = serde_json::from_str(&json).unwrap();
            assert_eq!(parsed.localized_notes, None, "{odd}");
            assert_eq!(parsed.notes_for(Lang::En), Some("Yeni"), "{odd}");
        }
    }

    #[test]
    fn notes_of_the_document_an_update_came_from() {
        let document = serde_json::json!({
            "version": "2.1.0",
            "notes": "Yeni ana ekran",
            "localizedNotes": {"tr": "Yeni ana ekran", "en": "A new home screen"},
            "rollout": 300
        });
        // Read even where the whole file would not parse (a rollout above 255).
        assert_eq!(
            release_notes_in(&document, Lang::En).as_deref(),
            Some("A new home screen")
        );
        assert_eq!(
            release_notes_in(&document, Lang::Tr).as_deref(),
            Some("Yeni ana ekran")
        );
        let old = serde_json::json!({"version": "2.0.1", "notes": "Yeni"});
        assert_eq!(release_notes_in(&old, Lang::En).as_deref(), Some("Yeni"));
        let odd = serde_json::json!({"notes": 5, "localizedNotes": "x"});
        assert_eq!(release_notes_in(&odd, Lang::En), None);
        assert_eq!(release_notes_in(&serde_json::json!(null), Lang::Tr), None);
    }
}
