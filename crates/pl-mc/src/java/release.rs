//! What a Java installation says about itself without being started: the `release` file at its
//! root (`JAVA_VERSION="21.0.7"`, `IMPLEMENTOR="Microsoft"`, `OS_ARCH="amd64"`), the Java home
//! of an executable, a familiar vendor name, and where a typed custom path really points.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use crate::java::detect::parse_major;
use crate::platform::{Arch, Os};

/// What a Java installation's `release` file says.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct ReleaseInfo {
    /// `JAVA_VERSION` (`21.0.7`, `1.8.0_51`).
    pub version: String,
    /// `JAVA_RUNTIME_VERSION`, the exact build (`21.0.7+6-LTS`), when present.
    pub build: Option<String>,
    /// `IMPLEMENTOR` (`Eclipse Adoptium`, `Microsoft`), when present.
    pub vendor: Option<String>,
    /// `OS_ARCH` (`amd64`, `aarch64`), when present and known.
    pub arch: Option<Arch>,
}

impl ReleaseInfo {
    pub fn major(&self) -> Option<u32> {
        parse_major(&self.version)
    }
}

/// Parse a `release` file. `None` without a usable `JAVA_VERSION`.
pub fn parse_release(text: &str) -> Option<ReleaseInfo> {
    let mut values: HashMap<&str, &str> = HashMap::new();
    for line in text.lines() {
        let Some((key, value)) = line.trim().split_once('=') else {
            continue;
        };
        let value = value.trim().trim_matches('"').trim();
        values.entry(key.trim()).or_insert(value);
    }
    let known = |key: &str| {
        values
            .get(key)
            .copied()
            .filter(|v| !v.is_empty() && !v.eq_ignore_ascii_case("N/A"))
            .map(str::to_owned)
    };
    let version = known("JAVA_VERSION")?;
    parse_major(&version)?;
    Some(ReleaseInfo {
        version,
        build: known("JAVA_RUNTIME_VERSION"),
        vendor: known("IMPLEMENTOR"),
        arch: known("OS_ARCH")
            .map(|arch| Arch::from_name(&arch))
            .filter(|arch| *arch != Arch::Other),
    })
}

/// The Java home of an executable: `<home>/bin/java(w)(.exe)` → `<home>` (macOS bundles end in
/// `Contents/Home`).
pub fn java_home(executable: &Path) -> Option<PathBuf> {
    let bin = executable.parent()?;
    if !bin
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("bin"))
    {
        return None;
    }
    bin.parent().map(Path::to_path_buf)
}

/// The `release` file of the installation `executable` belongs to (a JDK 8's `jre/bin/java`
/// finds the JDK's file). Blocking (one small file).
pub fn read_release(executable: &Path) -> Option<ReleaseInfo> {
    let home = java_home(executable)?;
    let mut candidates = vec![home.join("release")];
    if home
        .file_name()
        .is_some_and(|name| name.eq_ignore_ascii_case("jre"))
        && let Some(jdk) = home.parent()
    {
        candidates.push(jdk.join("release"));
    }
    candidates
        .into_iter()
        .find_map(|file| std::fs::read_to_string(file).ok())
        .and_then(|text| parse_release(&text))
}

/// A short, familiar name for a vendor string (`IMPLEMENTOR` or `java.vendor`).
pub fn vendor_label(raw: &str) -> Option<String> {
    let raw = raw.trim();
    if raw.is_empty() || raw.eq_ignore_ascii_case("N/A") {
        return None;
    }
    let lower = raw.to_lowercase();
    let words: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric())
        .filter(|w| !w.is_empty())
        .collect();
    let has = |word: &str| words.contains(&word);
    let label = if lower.contains("adoptium") || lower.contains("temurin") {
        "Eclipse Temurin"
    } else if lower.contains("adoptopenjdk") {
        "AdoptOpenJDK"
    } else if has("microsoft") {
        "Microsoft"
    } else if has("azul") {
        "Azul Zulu"
    } else if has("amazon") || lower.contains("amazon.com") {
        "Amazon Corretto"
    } else if has("bellsoft") {
        "BellSoft Liberica"
    } else if lower.contains("graalvm") {
        "GraalVM"
    } else if has("ibm") || lower.contains("business machines") {
        "IBM Semeru"
    } else if has("red") && has("hat") {
        "Red Hat"
    } else if has("jetbrains") {
        "JetBrains"
    } else if has("sap") {
        "SapMachine"
    } else if has("oracle") {
        "Oracle"
    } else if has("mojang") {
        "Mojang"
    } else {
        let trimmed = [", Inc.", " Inc.", ", Inc", " Inc", " Corporation", " Corp."]
            .iter()
            .fold(raw, |name, suffix| {
                name.strip_suffix(suffix).unwrap_or(name)
            });
        return Some(trimmed.trim().to_owned());
    };
    Some(label.to_owned())
}

/// The executable a custom Java path stands for. Players often paste the Java folder or its
/// `bin` folder instead of the program: those lead to `bin/javaw.exe` (Windows, no console) or
/// `bin/java`. A file (or anything that does not exist) is kept as it is.
pub fn resolve_custom_java(path: &Path, os: Os) -> PathBuf {
    if !path.is_dir() {
        return path.to_path_buf();
    }
    let names: &[&str] = if os == Os::Windows {
        &["javaw.exe", "java.exe"]
    } else {
        &["java"]
    };
    let bins = [
        path.to_path_buf(),
        path.join("bin"),
        path.join("Contents").join("Home").join("bin"),
        path.join("jre").join("bin"),
    ];
    bins.iter()
        .flat_map(|bin| names.iter().map(move |name| bin.join(name)))
        .find(|candidate| candidate.is_file())
        .unwrap_or_else(|| path.to_path_buf())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_release_files_of_common_builds() {
        let temurin = "IMPLEMENTOR=\"Eclipse Adoptium\"\nIMPLEMENTOR_VERSION=\"Temurin-21.0.5+11\"\nJAVA_RUNTIME_VERSION=\"21.0.5+11-LTS\"\nJAVA_VERSION=\"21.0.5\"\nJAVA_VERSION_DATE=\"2024-10-15\"\nLIBC=\"default\"\nMODULES=\"java.base java.compiler\"\nOS_ARCH=\"x86_64\"\nOS_NAME=\"Windows\"\n";
        let info = parse_release(temurin).unwrap();
        assert_eq!(info.version, "21.0.5");
        assert_eq!(info.major(), Some(21));
        assert_eq!(info.build.as_deref(), Some("21.0.5+11-LTS"));
        assert_eq!(info.vendor.as_deref(), Some("Eclipse Adoptium"));
        assert_eq!(info.arch, Some(Arch::X86_64));

        // Mojang's runtimes: only the version (and the module list).
        let mojang = "JAVA_VERSION=\"21.0.7\"\nMODULES=\"java.base java.compiler\"\n";
        let info = parse_release(mojang).unwrap();
        assert_eq!(
            (info.major(), info.vendor, info.arch),
            (Some(21), None, None)
        );

        // Oracle's Java 8 (and Mojang's jre-legacy).
        let java8 = "JAVA_VERSION=\"1.8.0_51\"\nOS_NAME=\"Windows\"\nOS_VERSION=\"5.2\"\nOS_ARCH=\"amd64\"\nBUILD_TYPE=\"commercial\"\n";
        let info = parse_release(java8).unwrap();
        assert_eq!(info.major(), Some(8));
        assert_eq!(info.arch, Some(Arch::X86_64));

        // Unquoted values, Windows line ends, unknown values.
        let plain = "JAVA_VERSION=17.0.2\r\nIMPLEMENTOR=N/A\r\nOS_ARCH=aarch64\r\n";
        let info = parse_release(plain).unwrap();
        assert_eq!(info.major(), Some(17));
        assert_eq!(info.vendor, None);
        assert_eq!(info.arch, Some(Arch::Aarch64));

        assert!(parse_release("IMPLEMENTOR=\"Microsoft\"\n").is_none());
        assert!(parse_release("JAVA_VERSION=\"\"\n").is_none());
        assert!(parse_release("JAVA_VERSION=\"abc\"\n").is_none());
    }

    #[test]
    fn finds_the_java_home() {
        assert_eq!(
            java_home(Path::new("/jdk-21/bin/java")),
            Some(PathBuf::from("/jdk-21"))
        );
        assert_eq!(
            java_home(Path::new("/x/jre.bundle/Contents/Home/bin/java")),
            Some(PathBuf::from("/x/jre.bundle/Contents/Home"))
        );
        assert_eq!(java_home(Path::new("/opt/java")), None);

        let tmp = tempfile::tempdir().unwrap();
        // A JDK 8: the JRE inside has no release file of its own.
        let jdk = tmp.path().join("jdk1.8.0_431");
        std::fs::create_dir_all(jdk.join("jre").join("bin")).unwrap();
        std::fs::write(
            jdk.join("release"),
            "JAVA_VERSION=\"1.8.0_431\"\nOS_ARCH=\"amd64\"\n",
        )
        .unwrap();
        let info = read_release(&jdk.join("jre").join("bin").join("java.exe")).unwrap();
        assert_eq!(info.version, "1.8.0_431");
        assert!(read_release(&tmp.path().join("bin").join("java")).is_none());
    }

    #[test]
    fn vendor_names() {
        let label = |raw: &str| vendor_label(raw).unwrap_or_default();
        assert_eq!(label("Eclipse Adoptium"), "Eclipse Temurin");
        assert_eq!(label("AdoptOpenJDK"), "AdoptOpenJDK");
        assert_eq!(label("Microsoft"), "Microsoft");
        assert_eq!(label("Azul Systems, Inc."), "Azul Zulu");
        assert_eq!(label("Amazon.com Inc."), "Amazon Corretto");
        assert_eq!(label("BellSoft"), "BellSoft Liberica");
        assert_eq!(label("GraalVM Community"), "GraalVM");
        assert_eq!(
            label("International Business Machines Corporation"),
            "IBM Semeru"
        );
        assert_eq!(label("Red Hat, Inc."), "Red Hat");
        assert_eq!(label("SAP SE"), "SapMachine");
        assert_eq!(label("Oracle Corporation"), "Oracle");
        assert_eq!(label("Acme Java Corp."), "Acme Java");
        assert_eq!(label("Tiny Builds, Inc."), "Tiny Builds");
        assert_eq!(vendor_label("N/A"), None);
        assert_eq!(vendor_label("  "), None);
    }

    #[test]
    fn custom_paths_lead_to_the_program() {
        let tmp = tempfile::tempdir().unwrap();
        let home = tmp.path().join("jdk-21");
        let bin = home.join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        std::fs::write(bin.join("java.exe"), b"").unwrap();
        std::fs::write(bin.join("javaw.exe"), b"").unwrap();
        std::fs::write(bin.join("java"), b"").unwrap();

        // The Java folder, its bin folder, the program itself.
        assert_eq!(
            resolve_custom_java(&home, Os::Windows),
            bin.join("javaw.exe")
        );
        assert_eq!(
            resolve_custom_java(&bin, Os::Windows),
            bin.join("javaw.exe")
        );
        assert_eq!(resolve_custom_java(&home, Os::Linux), bin.join("java"));
        let exe = bin.join("java.exe");
        assert_eq!(resolve_custom_java(&exe, Os::Windows), exe);
        // Missing paths and folders without Java stay as typed (the check reports them).
        let missing = tmp.path().join("gone").join("java.exe");
        assert_eq!(resolve_custom_java(&missing, Os::Windows), missing);
        let empty = tmp.path().join("empty");
        std::fs::create_dir_all(&empty).unwrap();
        assert_eq!(resolve_custom_java(&empty, Os::Windows), empty);
    }
}
