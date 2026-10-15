//! Operating system and architecture detection, and the Mojang-specific names derived from them
//! (rule OS names, `${arch}` natives substitution, Java runtime platform keys, natives classifiers).

use std::fmt;

use serde::{Deserialize, Serialize};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Os {
    Windows,
    #[serde(rename = "macos")]
    MacOs,
    Linux,
    Other,
}

impl Os {
    /// The OS this binary was compiled for.
    pub fn current() -> Self {
        if cfg!(target_os = "windows") {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else if cfg!(target_os = "linux") {
            Self::Linux
        } else {
            Self::Other
        }
    }

    /// Name used by `rules[].os.name` in Mojang version JSONs and as `natives` map key.
    pub fn mojang_name(self) -> Option<&'static str> {
        match self {
            Self::Windows => Some("windows"),
            Self::MacOs => Some("osx"),
            Self::Linux => Some("linux"),
            Self::Other => None,
        }
    }

    /// Whether a rule/`natives` OS name refers to this OS (`macos` is accepted as an alias).
    pub fn matches_name(self, name: &str) -> bool {
        match self {
            Self::Windows => name == "windows",
            Self::MacOs => name == "osx" || name == "macos",
            Self::Linux => name == "linux",
            Self::Other => false,
        }
    }

    /// Java classpath separator.
    pub fn classpath_separator(self) -> &'static str {
        if self == Self::Windows { ";" } else { ":" }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum Arch {
    /// 32-bit x86.
    X86,
    #[serde(rename = "x86_64")]
    X86_64,
    /// 32-bit ARM.
    Arm,
    Aarch64,
    Other,
}

impl Arch {
    /// Architecture of the running process. A launcher running under emulation (x64 on Windows
    /// ARM, Rosetta on macOS) reports the emulated architecture, which keeps the chosen Java and
    /// natives consistent with each other.
    pub fn current() -> Self {
        Self::from_name(std::env::consts::ARCH)
    }

    /// Parse the many spellings used by Rust, Java (`os.arch`), Mojang and LWJGL.
    pub fn from_name(name: &str) -> Self {
        match name.trim().to_ascii_lowercase().as_str() {
            "x86_64" | "amd64" | "x64" | "x86-64" => Self::X86_64,
            "x86" | "i386" | "i486" | "i586" | "i686" | "ia32" => Self::X86,
            "aarch64" | "arm64" => Self::Aarch64,
            "arm" | "arm32" | "armv7" | "armv7l" | "armhf" => Self::Arm,
            _ => Self::Other,
        }
    }

    pub fn is_64bit(self) -> bool {
        matches!(self, Self::X86_64 | Self::Aarch64)
    }

    /// Value of `${arch}` in legacy `natives` classifiers (`natives-windows-${arch}`).
    pub fn bits(self) -> &'static str {
        if self.is_64bit() { "64" } else { "32" }
    }

    /// Whether a `rules[].os.arch` value refers to this architecture. Mojang only uses `x86`
    /// (meaning a 32-bit JVM, e.g. for `-Xss1M`); the other spellings come from third-party JSONs.
    pub fn matches_rule(self, rule_arch: &str) -> bool {
        let wanted = Self::from_name(rule_arch);
        wanted != Self::Other && wanted == self
    }
}

/// The OS/architecture a game is installed and launched for.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Platform {
    pub os: Os,
    pub arch: Arch,
    /// OS version in the format Java reports as `os.version` (Windows 10/11: `10.0.<build>`,
    /// macOS: `14.5`, Linux: kernel release). Matched by `rules[].os.version` regexes.
    pub os_version: String,
}

impl Platform {
    pub fn new(os: Os, arch: Arch, os_version: impl Into<String>) -> Self {
        Self {
            os,
            arch,
            os_version: os_version.into(),
        }
    }

    /// Detect the platform of the running process.
    pub fn current() -> Self {
        let os = Os::current();
        Self::new(os, Arch::current(), detect_os_version(os))
    }

    /// Same OS with another architecture (e.g. an x64 Java running under emulation on ARM).
    pub fn with_arch(&self, arch: Arch) -> Self {
        Self {
            arch,
            ..self.clone()
        }
    }

    pub fn mojang_os_name(&self) -> Option<&'static str> {
        self.os.mojang_name()
    }

    pub fn classpath_separator(&self) -> &'static str {
        self.os.classpath_separator()
    }

    /// Value of `${arch}` in legacy natives classifiers: `32` or `64`.
    pub fn arch_bits(&self) -> &'static str {
        self.arch.bits()
    }

    /// Mojang Java runtime platform key for this exact OS/architecture, if Mojang publishes one.
    pub fn java_runtime_platform(&self) -> Option<&'static str> {
        match (self.os, self.arch) {
            (Os::Windows, Arch::X86_64) => Some("windows-x64"),
            (Os::Windows, Arch::X86) => Some("windows-x86"),
            (Os::Windows, Arch::Aarch64) => Some("windows-arm64"),
            (Os::MacOs, Arch::X86_64) => Some("mac-os"),
            (Os::MacOs, Arch::Aarch64) => Some("mac-os-arm64"),
            (Os::Linux, Arch::X86_64) => Some("linux"),
            (Os::Linux, Arch::X86) => Some("linux-i386"),
            _ => None,
        }
    }

    /// Runtime platform keys to try in order: the native one first, then platforms the OS can
    /// emulate (macOS arm64 → x64 via Rosetta 2, Windows arm64 → x64/x86 emulation).
    pub fn java_runtime_platforms(&self) -> Vec<&'static str> {
        let mut keys: Vec<&'static str> = self.java_runtime_platform().into_iter().collect();
        match (self.os, self.arch) {
            (Os::MacOs, Arch::Aarch64) => keys.push("mac-os"),
            (Os::Windows, Arch::Aarch64) => keys.extend(["windows-x64", "windows-x86"]),
            _ => {}
        }
        keys
    }

    /// Whether a 1.19+ style natives classifier (`natives-windows-arm64`, `natives-macos`,
    /// `natives-linux`...) is meant for this OS and architecture. Plain `natives-<os>` means x64
    /// in LWJGL 3.3+; `natives-macos-patch` is Mojang's patched x64 macOS build.
    pub fn natives_classifier_matches(&self, classifier: &str) -> bool {
        let Some(rest) = classifier.strip_prefix("natives-") else {
            return false;
        };
        let (os, suffix) = if let Some(s) = rest.strip_prefix("windows") {
            (Os::Windows, s)
        } else if let Some(s) = rest.strip_prefix("macos") {
            (Os::MacOs, s)
        } else if let Some(s) = rest.strip_prefix("osx") {
            (Os::MacOs, s)
        } else if let Some(s) = rest.strip_prefix("linux") {
            (Os::Linux, s)
        } else {
            return false;
        };
        let arch = match suffix {
            "" | "-patch" | "-x64" | "-x86_64" | "-amd64" => Arch::X86_64,
            other => match other.strip_prefix('-') {
                Some(name) => Arch::from_name(name),
                None => return false,
            },
        };
        os == self.os && arch != Arch::Other && arch == self.arch
    }
}

impl fmt::Display for Platform {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let os = match self.os {
            Os::Windows => "windows",
            Os::MacOs => "macos",
            Os::Linux => "linux",
            Os::Other => "other",
        };
        let arch = match self.arch {
            Arch::X86 => "x86",
            Arch::X86_64 => "x86_64",
            Arch::Arm => "arm",
            Arch::Aarch64 => "aarch64",
            Arch::Other => "other",
        };
        write!(f, "{os}-{arch} ({})", self.os_version)
    }
}

/// Architecture of the Java runtimes published under a Mojang platform key.
pub fn java_runtime_platform_arch(key: &str) -> Arch {
    match key {
        "windows-x64" | "mac-os" | "linux" => Arch::X86_64,
        "windows-x86" | "linux-i386" => Arch::X86,
        "windows-arm64" | "mac-os-arm64" => Arch::Aarch64,
        _ => Arch::Other,
    }
}

fn detect_os_version(os: Os) -> String {
    match os {
        Os::Windows => windows_version(),
        Os::MacOs => sysinfo::System::os_version().unwrap_or_default(),
        Os::Linux | Os::Other => sysinfo::System::kernel_version().unwrap_or_default(),
    }
}

/// Java reports Windows 10 and 11 as `10.0`; version JSON rules (`^10\.`) rely on that, so the
/// version is rebuilt from the build number instead of the marketing name.
fn windows_version() -> String {
    let build = sysinfo::System::kernel_version().and_then(|b| b.trim().parse::<u32>().ok());
    match build {
        Some(b) if b >= 10_240 => format!("10.0.{b}"),
        Some(b) if b >= 9_600 => format!("6.3.{b}"),
        Some(b) if b >= 9_200 => format!("6.2.{b}"),
        Some(b) if b >= 7_600 => format!("6.1.{b}"),
        Some(b) => format!("6.0.{b}"),
        None => sysinfo::System::os_version().unwrap_or_default(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn arch_spellings() {
        assert_eq!(Arch::from_name("amd64"), Arch::X86_64);
        assert_eq!(Arch::from_name("x86_64"), Arch::X86_64);
        assert_eq!(Arch::from_name("i686"), Arch::X86);
        assert_eq!(Arch::from_name("arm64"), Arch::Aarch64);
        assert_eq!(Arch::from_name("riscv64"), Arch::Other);
        assert_eq!(Arch::X86_64.bits(), "64");
        assert_eq!(Arch::X86.bits(), "32");
        assert!(Arch::X86.matches_rule("x86"));
        assert!(!Arch::X86_64.matches_rule("x86"));
        assert!(!Arch::Other.matches_rule("sparc"));
    }

    #[test]
    fn runtime_keys_and_fallbacks() {
        let mac_arm = Platform::new(Os::MacOs, Arch::Aarch64, "14.5");
        assert_eq!(mac_arm.java_runtime_platforms(), ["mac-os-arm64", "mac-os"]);
        let win = Platform::new(Os::Windows, Arch::X86_64, "10.0.26100");
        assert_eq!(win.java_runtime_platforms(), ["windows-x64"]);
        let win_arm = Platform::new(Os::Windows, Arch::Aarch64, "10.0.26100");
        assert_eq!(
            win_arm.java_runtime_platforms(),
            ["windows-arm64", "windows-x64", "windows-x86"]
        );
        let linux32 = Platform::new(Os::Linux, Arch::X86, "6.1.0");
        assert_eq!(linux32.java_runtime_platform(), Some("linux-i386"));
        let linux_arm = Platform::new(Os::Linux, Arch::Aarch64, "6.1.0");
        assert!(linux_arm.java_runtime_platforms().is_empty());
        assert_eq!(java_runtime_platform_arch("mac-os"), Arch::X86_64);
    }

    #[test]
    fn natives_classifiers() {
        let win = Platform::new(Os::Windows, Arch::X86_64, "10.0");
        assert!(win.natives_classifier_matches("natives-windows"));
        assert!(!win.natives_classifier_matches("natives-windows-arm64"));
        assert!(!win.natives_classifier_matches("natives-windows-x86"));
        assert!(!win.natives_classifier_matches("natives-linux"));
        assert!(!win.natives_classifier_matches("linux-x86_64"));
        let mac_arm = Platform::new(Os::MacOs, Arch::Aarch64, "14.5");
        assert!(mac_arm.natives_classifier_matches("natives-macos-arm64"));
        assert!(!mac_arm.natives_classifier_matches("natives-macos"));
        assert!(!mac_arm.natives_classifier_matches("natives-macos-patch"));
        let mac = mac_arm.with_arch(Arch::X86_64);
        assert!(mac.natives_classifier_matches("natives-macos-patch"));
        assert!(mac.natives_classifier_matches("natives-osx"));
        let linux_arm = Platform::new(Os::Linux, Arch::Arm, "6.1");
        assert!(linux_arm.natives_classifier_matches("natives-linux-arm32"));
    }

    #[test]
    fn current_platform_is_sane() {
        let p = Platform::current();
        assert_eq!(p.os, Os::current());
        if p.os == Os::Windows {
            // Windows 10 and 11 must look like Java's `os.version` ("10.0...").
            assert!(p.os_version.starts_with("10.0") || p.os_version.starts_with('6'));
            assert_eq!(p.classpath_separator(), ";");
        }
    }
}
