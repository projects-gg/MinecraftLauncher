//! What a batch says about the computer: coarse enough that nobody can be told apart by it.
//!
//! The launcher's version, the operating system's family and major version (Windows 10 or 11,
//! macOS 14; Linux only as "linux"), the processor family, the interface language and the memory
//! rounded to a step. No computer name, no user name, no serial number, no install id, no IP
//! address (the service never stores the sender's).

use serde::Serialize;

/// The computer as a batch describes it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct AppFacts {
    /// The launcher's version without build metadata (`2.0.8`, `2.0.9-beta.1`).
    pub ver: String,
    /// `windows`, `macos` or `linux`.
    pub os: String,
    /// The system's major version (`10`, `11`, `14`); empty when unknown or on Linux.
    pub osv: String,
    /// `x86_64`, `aarch64` or `other`.
    pub arch: String,
    /// The interface language: `tr` or `en`.
    pub lang: String,
    /// Installed memory in GB, rounded up to 4, 8, 16, 32 or 64.
    pub ram: u32,
}

impl AppFacts {
    /// `version` as the app reports it; `os_version` the system's (major, minor, build) when
    /// known; `memory_mb` the installed memory.
    pub fn new(
        version: &str,
        os: &str,
        os_version: Option<(u64, u64, u64)>,
        arch: &str,
        lang: &str,
        memory_mb: u64,
    ) -> Self {
        Self {
            ver: clean_version(version),
            os: match os {
                "windows" | "macos" | "linux" => os.to_owned(),
                _ => "other".to_owned(),
            },
            osv: os_version_bucket(os, os_version),
            arch: match arch {
                "x86_64" | "aarch64" => arch.to_owned(),
                _ => "other".to_owned(),
            },
            lang: if lang == "en" { "en" } else { "tr" }.to_owned(),
            ram: ram_bucket(memory_mb),
        }
    }
}

/// `2.0.0+telemetry` → `2.0.0`: build metadata names a test build, not a release.
pub fn clean_version(version: &str) -> String {
    let version = version.split('+').next().unwrap_or_default();
    version
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || ".-".contains(*c))
        .take(32)
        .collect()
}

/// Windows 11 still says 10.0 (its build is 22000 or later); macOS its major; Linux nothing.
pub fn os_version_bucket(os: &str, version: Option<(u64, u64, u64)>) -> String {
    match (os, version) {
        ("windows", Some((10, _, build))) if build >= 22_000 => "11".to_owned(),
        ("windows", Some((major, _, _))) if major <= 10 => major.to_string(),
        ("macos", Some((major, _, _))) if (10..100).contains(&major) => major.to_string(),
        _ => String::new(),
    }
}

/// The memory step a computer is in: what matters for Minecraft, nothing finer.
pub fn ram_bucket(memory_mb: u64) -> u32 {
    let gb = memory_mb.div_ceil(1024);
    match gb {
        0..=4 => 4,
        5..=8 => 8,
        9..=16 => 16,
        17..=32 => 32,
        _ => 64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn facts_are_coarse() {
        let facts = AppFacts::new(
            "2.0.8+telemetry",
            "windows",
            Some((10, 0, 26_100)),
            "x86_64",
            "tr",
            15_800,
        );
        assert_eq!(
            facts,
            AppFacts {
                ver: "2.0.8".into(),
                os: "windows".into(),
                osv: "11".into(),
                arch: "x86_64".into(),
                lang: "tr".into(),
                ram: 16,
            }
        );
    }

    #[test]
    fn os_versions() {
        assert_eq!(os_version_bucket("windows", Some((10, 0, 19_045))), "10");
        assert_eq!(os_version_bucket("windows", Some((10, 0, 22_631))), "11");
        assert_eq!(os_version_bucket("macos", Some((14, 5, 0))), "14");
        assert_eq!(os_version_bucket("linux", Some((6, 8, 0))), "");
        assert_eq!(os_version_bucket("windows", None), "");
    }

    #[test]
    fn memory_steps() {
        assert_eq!(ram_bucket(0), 4);
        assert_eq!(ram_bucket(4096), 4);
        assert_eq!(ram_bucket(8000), 8);
        assert_eq!(ram_bucket(16_384), 16);
        assert_eq!(ram_bucket(32_768), 32);
        assert_eq!(ram_bucket(131_072), 64);
    }

    #[test]
    fn odd_values_fold_into_known_ones() {
        let facts = AppFacts::new("2.0.9-beta.1", "freebsd", None, "riscv64", "de", 2048);
        assert_eq!(facts.ver, "2.0.9-beta.1");
        assert_eq!(facts.os, "other");
        assert_eq!(facts.arch, "other");
        assert_eq!(facts.lang, "tr");
        assert_eq!(clean_version("2.0.0 <script>"), "2.0.0script");
    }
}
