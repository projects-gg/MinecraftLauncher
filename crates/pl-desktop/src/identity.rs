//! The identity a launcher process runs under: the app identifier (Tauri's single-instance lock,
//! window state, WebView2 profile and per-app folders follow it), the OS keychain service, the
//! default data folder and the names shown for it (window title, tray, login item).
//!
//! Release builds are the installed launcher: `gg.projects.launcher`, the keychain service of the
//! same name, `ProjectsLauncher` in the OS data folder and the product name everywhere.
//!
//! Debug builds (and measurement builds of the `e2e-release` profile, which keep debug assertions)
//! never share any of that with an installed launcher. They run as
//! `gg.projects.launcher.debug.<instance>` ([`INSTANCE_ENV`], default `dev`): the single-instance
//! lock never forwards to the installed launcher, the window state and the WebView2 profile stay
//! apart, the keychain service is the same name (the installed launcher's sessions, AFK login
//! commands and friends session are never read or written), the data folder is
//! `ProjectsLauncher-debug-<instance>` and the window title, tray tooltip and login item say which
//! instance it is. A data folder override may move the folder, but never onto a folder the
//! installed launcher uses ([`Identity::check_data_root`]). Several instances run side by side
//! under different names.

use std::path::{Path, PathBuf};

/// The installed launcher's app identifier and keychain service.
pub const PRODUCTION_IDENTIFIER: &str = "gg.projects.launcher";
/// The installed launcher's data folder name in the OS data folder (`pl_mc::paths::ROOT_DIR_NAME`).
pub const PRODUCTION_DATA_FOLDER: &str = "ProjectsLauncher";
/// Debug builds only: names this process's own identity (`[a-z0-9-]`, at most 32 characters).
pub const INSTANCE_ENV: &str = "PROJECTS_LAUNCHER_INSTANCE";
const DEFAULT_INSTANCE: &str = "dev";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Identity {
    /// Tauri's app identifier.
    pub identifier: String,
    /// OS keychain service of every secret this process keeps (accounts, AFK, friends).
    pub keychain_service: String,
    /// Name of the default data folder inside the OS data folder.
    pub data_folder: String,
    /// Debug builds: the instance name.
    pub instance: Option<String>,
}

impl Identity {
    /// The installed launcher's identity.
    pub fn production() -> Self {
        Self {
            identifier: PRODUCTION_IDENTIFIER.to_owned(),
            keychain_service: PRODUCTION_IDENTIFIER.to_owned(),
            data_folder: PRODUCTION_DATA_FOLDER.to_owned(),
            instance: None,
        }
    }

    /// The identity of a process. `isolated` is true for debug builds; `instance` is the value of
    /// [`INSTANCE_ENV`] (ignored unless isolated; blank means the default).
    pub fn resolve(isolated: bool, instance: Option<&str>) -> Result<Self, String> {
        if !isolated {
            return Ok(Self::production());
        }
        let name = instance
            .map(str::trim)
            .filter(|name| !name.is_empty())
            .unwrap_or(DEFAULT_INSTANCE);
        if !valid_instance(name) {
            return Err(format!(
                "{INSTANCE_ENV} must be 1-32 characters of a-z, 0-9 and '-' (got {name:?})"
            ));
        }
        let identifier = format!("{PRODUCTION_IDENTIFIER}.debug.{name}");
        Ok(Self {
            keychain_service: identifier.clone(),
            identifier,
            data_folder: format!("{PRODUCTION_DATA_FOLDER}-debug-{name}"),
            instance: Some(name.to_owned()),
        })
    }

    /// True for a debug build's identity of its own.
    pub fn is_isolated(&self) -> bool {
        self.instance.is_some()
    }

    /// `name` as this process shows it (window title, tray tooltip, login item): unchanged for the
    /// installed launcher, with the instance for a debug build ("Projects Launcher (debug dev)").
    pub fn label(&self, name: &str) -> String {
        match &self.instance {
            Some(instance) => format!("{name} (debug {instance})"),
            None => name.to_owned(),
        }
    }

    /// A debug build refuses a data folder the installed launcher uses: `production_roots` are its
    /// default folder and the folder its "Taşı" pointer names. The installed launcher's own
    /// identity accepts any folder.
    pub fn check_data_root(&self, root: &Path, production_roots: &[PathBuf]) -> Result<(), String> {
        if !self.is_isolated() {
            return Ok(());
        }
        match production_roots
            .iter()
            .find(|production| same_folder(root, production))
        {
            Some(production) => Err(format!(
                "a debug build never opens the installed launcher's data folder ({}); pick another \
                 PROJECTS_LAUNCHER_DATA_DIR",
                production.display()
            )),
            None => Ok(()),
        }
    }
}

fn valid_instance(name: &str) -> bool {
    (1..=32).contains(&name.len())
        && name
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        && !name.starts_with('-')
}

/// Two paths name the same folder: compared without trailing separators (case-insensitively on
/// Windows), and through links when both exist.
fn same_folder(a: &Path, b: &Path) -> bool {
    if let (Ok(a), Ok(b)) = (std::fs::canonicalize(a), std::fs::canonicalize(b))
        && a == b
    {
        return true;
    }
    let text = |path: &Path| {
        let text = path.to_string_lossy().replace('\\', "/");
        let text = text.trim_end_matches('/').to_owned();
        if cfg!(windows) {
            text.to_lowercase()
        } else {
            text
        }
    };
    text(a) == text(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn production_roots() -> Vec<PathBuf> {
        vec![
            PathBuf::from("C:/Users/test/AppData/Roaming/ProjectsLauncher"),
            PathBuf::from("E:/Oyunlar/ProjectsLauncher"),
        ]
    }

    #[test]
    fn release_builds_are_the_installed_launcher() {
        let identity = Identity::resolve(false, Some("owner")).unwrap();
        assert_eq!(identity, Identity::production());
        assert_eq!(identity.identifier, "gg.projects.launcher");
        assert_eq!(identity.keychain_service, "gg.projects.launcher");
        assert_eq!(identity.data_folder, "ProjectsLauncher");
        assert!(
            !identity.is_isolated(),
            "release builds ignore the instance"
        );
        assert_eq!(identity.label("Projects Launcher"), "Projects Launcher");
        for root in production_roots() {
            assert!(identity.check_data_root(&root, &production_roots()).is_ok());
        }
    }

    #[test]
    fn debug_builds_never_share_the_installed_identity() {
        let dev = Identity::resolve(true, None).unwrap();
        assert_eq!(dev.identifier, "gg.projects.launcher.debug.dev");
        assert_eq!(dev.keychain_service, dev.identifier);
        assert_eq!(dev.data_folder, "ProjectsLauncher-debug-dev");
        assert_eq!(dev.instance.as_deref(), Some("dev"));
        assert_eq!(
            dev.label("Projects Launcher"),
            "Projects Launcher (debug dev)"
        );

        let a = Identity::resolve(true, Some("opt1")).unwrap();
        let b = Identity::resolve(true, Some("e2e-2")).unwrap();
        assert_ne!(a.identifier, b.identifier, "two instances side by side");
        assert_ne!(a.keychain_service, b.keychain_service);
        assert_ne!(a.data_folder, b.data_folder);
        for identity in [&dev, &a, &b] {
            assert_ne!(identity.identifier, PRODUCTION_IDENTIFIER);
            assert_ne!(identity.keychain_service, PRODUCTION_IDENTIFIER);
            assert_ne!(identity.data_folder, PRODUCTION_DATA_FOLDER);
        }
    }

    #[test]
    fn debug_builds_refuse_the_installed_data_folders() {
        let dev = Identity::resolve(true, None).unwrap();
        for path in [
            "C:/Users/test/AppData/Roaming/ProjectsLauncher",
            "C:\\Users\\test\\AppData\\Roaming\\ProjectsLauncher\\",
            "E:/Oyunlar/ProjectsLauncher/",
        ] {
            assert!(
                dev.check_data_root(Path::new(path), &production_roots())
                    .is_err(),
                "{path}"
            );
        }
        if cfg!(windows) {
            assert!(
                dev.check_data_root(
                    Path::new("c:/users/TEST/appdata/roaming/projectslauncher"),
                    &production_roots()
                )
                .is_err()
            );
        }
        for path in [
            "D:/ProLauncher-build/mc-test",
            "C:/Users/test/AppData/Roaming/ProjectsLauncher-debug-dev",
        ] {
            assert!(
                dev.check_data_root(Path::new(path), &production_roots())
                    .is_ok(),
                "{path}"
            );
        }
    }

    #[test]
    fn instance_names_are_checked() {
        for bad in ["Owner", "a b", "../x", "-x", "x.y", &"x".repeat(33)] {
            assert!(Identity::resolve(true, Some(bad)).is_err(), "{bad}");
        }
        assert_eq!(
            Identity::resolve(true, Some(" "))
                .unwrap()
                .instance
                .as_deref(),
            Some("dev"),
            "blank: dev"
        );
        assert!(Identity::resolve(true, Some("e2e-owner-2")).is_ok());
        assert!(
            Identity::resolve(false, Some("../x")).is_ok(),
            "release ignores it"
        );
    }
}
