use std::collections::BTreeMap;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use crate::fsutil::sha256_hex;

/// `releases/<channel>.json`: Tauri's static updater JSON (`version`, `notes`, `pub_date`,
/// `platforms`) plus staged-rollout fields that Tauri ignores.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ReleaseChannel {
    /// Semver, optionally prefixed with `v`.
    pub version: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub notes: Option<String>,
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
        assert_eq!(out["rollout"], 100);
    }
}
