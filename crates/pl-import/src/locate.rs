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
    /// Per-user settings: `%APPDATA%`, `~/Library/Application Support`, `~/.config`.
    pub config_dir: Option<PathBuf>,
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
            config_dir: dirs::config_dir(),
            home: dirs::home_dir(),
            documents: dirs::document_dir(),
        }
    }

    /// The same layout under `root`, as a pretend home folder of `os`.
    pub fn under(root: &Path, os: Os) -> Self {
        let (data, local, config) = match os {
            Os::Windows => {
                let roaming = root.join("AppData").join("Roaming");
                (
                    roaming.clone(),
                    Some(root.join("AppData").join("Local")),
                    roaming,
                )
            }
            Os::MacOs => {
                let support = root.join("Library").join("Application Support");
                (support.clone(), None, support)
            }
            Os::Linux => (
                root.join(".local").join("share"),
                None,
                root.join(".config"),
            ),
        };
        Self {
            os,
            data_dir: Some(data),
            local_data_dir: local,
            config_dir: Some(config),
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

    /// The v1 Projects Launcher's folder, `%APPDATA%\.projects` (it ran on Windows only): its
    /// premium session, AFK accounts and client, and the game data it ran.
    pub fn projects_v1(&self) -> Option<PathBuf> {
        match self.os {
            Os::Windows => self.data_dir.as_ref().map(|d| d.join(".projects")),
            Os::MacOs | Os::Linux => None,
        }
    }

    /// Where the v1 launcher (a ClickOnce .NET app) kept its `user.config`, the offline nickname
    /// among its settings: ClickOnce's data folder and the app's own, under `%LOCALAPPDATA%`.
    pub fn projects_v1_settings(&self) -> Vec<PathBuf> {
        match (self.os, self.local_data_dir.as_ref()) {
            (Os::Windows, Some(local)) => vec![
                local.join("Apps").join("2.0").join("Data"),
                local.join("Projects"),
            ],
            _ => Vec::new(),
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

    /// CurseForge's instances in the app's default place (the player can move them: see
    /// [`Self::curseforge_instance_lists`]).
    pub(crate) fn curseforge_instances(&self) -> Option<PathBuf> {
        let base = match self.os {
            Os::Windows => self.home.as_ref()?,
            Os::MacOs | Os::Linux => self.documents.as_ref()?,
        };
        Some(base.join("curseforge").join("minecraft").join("Instances"))
    }

    /// The CurseForge app's own list of its Minecraft instances, each with its folder
    /// (`installPath`), wherever the player moved the app's Minecraft folder: the app's agent
    /// keeps it in `agent/GameInstances/MinecraftGameInstance.json` of the app's folder (the
    /// Overwolf version of the app in `Overwolf/Curse`).
    pub(crate) fn curseforge_instance_lists(&self) -> Vec<PathBuf> {
        let list = |base: PathBuf| {
            base.join("GameInstances")
                .join("MinecraftGameInstance.json")
        };
        let mut out = Vec::new();
        if let Some(config) = self.config_dir.as_ref() {
            let app = config.join("CurseForge");
            out.push(list(app.join("agent")));
            if self.os == Os::MacOs {
                // CurseForge's own help spells it this way on macOS.
                out.push(list(app.join("Agent")));
            }
        }
        if self.os == Os::Windows
            && let Some(local) = self.local_data_dir.as_ref()
        {
            out.push(list(local.join("Overwolf").join("Curse")));
        }
        out
    }

    /// The Modrinth App's folder: its settings and database (`app.db`), and its profiles unless
    /// the player moved them (the database says where).
    pub(crate) fn modrinth_app(&self) -> Option<PathBuf> {
        self.data_dir.as_ref().map(|d| d.join("ModrinthApp"))
    }

    /// The Modrinth App's folder before 2024, with a `profile.json` in each profile folder.
    pub(crate) fn modrinth_old_app(&self) -> Option<PathBuf> {
        self.config_dir
            .as_ref()
            .map(|d| d.join("com.modrinth.theseus"))
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
        assert_eq!(
            linux.curseforge_instances(),
            Some(root.join("Documents/curseforge/minecraft/Instances"))
        );
        assert_eq!(
            linux.curseforge_instance_lists(),
            [root.join(".config/CurseForge/agent/GameInstances/MinecraftGameInstance.json")]
        );
        assert_eq!(
            linux.modrinth_old_app(),
            Some(root.join(".config/com.modrinth.theseus"))
        );
        assert_eq!(
            windows.curseforge_instance_lists(),
            [
                root.join(
                    "AppData/Roaming/CurseForge/agent/GameInstances/MinecraftGameInstance.json"
                ),
                root.join("AppData/Local/Overwolf/Curse/GameInstances/MinecraftGameInstance.json"),
            ]
        );
        assert_eq!(
            windows.modrinth_app(),
            Some(root.join("AppData/Roaming/ModrinthApp"))
        );
        assert!(
            linux
                .multimc_family()
                .iter()
                .any(|(kind, path)| *kind == SourceKind::Prism && path.ends_with("PrismLauncher"))
        );
    }
}
