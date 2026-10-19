//! Where other launchers keep their data on each system. Tests and end-to-end runs point the
//! same layout at a pretend home folder ([`Locations::under`]), so nothing real is read.

use std::path::{Path, PathBuf};

use crate::model::SourceKind;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Os {
    Windows,
    MacOs,
    Linux,
}

impl Os {
    pub fn current() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else if cfg!(target_os = "macos") {
            Self::MacOs
        } else {
            Self::Linux
        }
    }
}

/// The user folders the launchers' data hangs off.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Locations {
    pub os: Os,
    /// Roaming application data: `%APPDATA%`, `~/Library/Application Support`,
    /// `~/.local/share`.
    pub data_dir: Option<PathBuf>,
    /// `%LOCALAPPDATA%` on Windows.
    pub local_data_dir: Option<PathBuf>,
    pub home: Option<PathBuf>,
    pub documents: Option<PathBuf>,
}

impl Locations {
    /// This computer's folders.
    pub fn system() -> Self {
        Self {
            os: Os::current(),
            data_dir: dirs::data_dir(),
            local_data_dir: dirs::data_local_dir(),
            home: dirs::home_dir(),
            documents: dirs::document_dir(),
        }
    }

    /// The same layout under `root`, as a pretend home folder of `os`.
    pub fn under(root: &Path, os: Os) -> Self {
        let (data, local) = match os {
            Os::Windows => (
                root.join("AppData").join("Roaming"),
                Some(root.join("AppData").join("Local")),
            ),
            Os::MacOs => (root.join("Library").join("Application Support"), None),
            Os::Linux => (root.join(".local").join("share"), None),
        };
        Self {
            os,
            data_dir: Some(data),
            local_data_dir: local,
            home: Some(root.to_path_buf()),
            documents: Some(root.join("Documents")),
        }
    }

    /// The game folder Mojang's launcher (and TLauncher, SKlauncher) uses.
    pub(crate) fn minecraft(&self) -> Option<PathBuf> {
        match self.os {
            Os::Windows => self.data_dir.as_ref().map(|d| d.join(".minecraft")),
            Os::MacOs => self.data_dir.as_ref().map(|d| d.join("minecraft")),
            Os::Linux => self.home.as_ref().map(|h| h.join(".minecraft")),
        }
    }

    /// TLauncher's own folder (`tlauncher-2.0.properties`).
    pub(crate) fn tlauncher(&self) -> Option<PathBuf> {
        match self.os {
            Os::Windows => self.data_dir.as_ref().map(|d| d.join(".tlauncher")),
            Os::MacOs => self.data_dir.as_ref().map(|d| d.join("tlauncher")),
            Os::Linux => self.home.as_ref().map(|h| h.join(".tlauncher")),
        }
    }

    /// CurseForge's instances (the app's default place; there is no Linux app).
    pub(crate) fn curseforge_instances(&self) -> Option<PathBuf> {
        let base = match self.os {
            Os::Windows => self.home.as_ref()?,
            Os::MacOs => self.documents.as_ref()?,
            Os::Linux => return None,
        };
        Some(base.join("curseforge").join("minecraft").join("Instances"))
    }

    /// The Modrinth App's profile folders: its current one first, then the older app's.
    pub(crate) fn modrinth_profiles(&self) -> Vec<PathBuf> {
        let Some(data) = self.data_dir.as_ref() else {
            return Vec::new();
        };
        vec![
            data.join("ModrinthApp").join("profiles"),
            data.join("com.modrinth.theseus").join("profiles"),
        ]
    }

    /// Prism Launcher, PolyMC and MultiMC data folders worth a look. MultiMC is portable on
    /// Windows; only the places players usually unpack it are tried.
    pub(crate) fn multimc_family(&self) -> Vec<(SourceKind, PathBuf)> {
        let mut out = Vec::new();
        if let Some(data) = self.data_dir.as_ref() {
            out.push((SourceKind::Prism, data.join("PrismLauncher")));
            out.push((SourceKind::PolyMc, data.join("PolyMC")));
            if self.os == Os::Linux {
                out.push((SourceKind::MultiMc, data.join("multimc")));
            }
        }
        if let Some(home) = self.home.as_ref() {
            if self.os == Os::Linux {
                out.push((
                    SourceKind::Prism,
                    home.join(".var/app/org.prismlauncher.PrismLauncher/data/PrismLauncher"),
                ));
            }
            if self.os == Os::Windows {
                for place in ["Desktop", "Downloads", "Documents", ""] {
                    let base = if place.is_empty() {
                        home.clone()
                    } else {
                        home.join(place)
                    };
                    out.push((SourceKind::MultiMc, base.join("MultiMC")));
                }
            }
        }
        if self.os == Os::Windows
            && let Some(local) = self.local_data_dir.as_ref()
        {
            out.push((SourceKind::MultiMc, local.join("Programs").join("MultiMC")));
        }
        out
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pretend_homes_follow_each_systems_layout() {
        let root = Path::new("/home/oyuncu");
        let windows = Locations::under(root, Os::Windows);
        assert_eq!(
            windows.minecraft(),
            Some(root.join("AppData").join("Roaming").join(".minecraft"))
        );
        assert_eq!(
            windows.curseforge_instances(),
            Some(root.join("curseforge/minecraft/Instances"))
        );
        let mac = Locations::under(root, Os::MacOs);
        assert_eq!(
            mac.minecraft(),
            Some(root.join("Library/Application Support/minecraft"))
        );
        assert_eq!(
            mac.curseforge_instances(),
            Some(root.join("Documents/curseforge/minecraft/Instances"))
        );
        let linux = Locations::under(root, Os::Linux);
        assert_eq!(linux.minecraft(), Some(root.join(".minecraft")));
        assert_eq!(linux.curseforge_instances(), None);
        assert!(
            linux
                .multimc_family()
                .iter()
                .any(|(kind, path)| *kind == SourceKind::Prism && path.ends_with("PrismLauncher"))
        );
    }
}
