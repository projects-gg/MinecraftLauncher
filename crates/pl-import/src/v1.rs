//! The v1 Projects Launcher (a C# ClickOnce app, Windows only): where it kept its data
//! ([`Locations::projects_v1`]) and the offline nickname it remembered. The first-run step "Eski
//! launcher'ını bulduk" reads them (the launcher's `commands::migrate`); end-to-end runs point
//! [`Locations`] at a pretend home, so nothing of the real player's is read there.

use std::time::SystemTime;

use regex::Regex;
use walkdir::WalkDir;

use crate::locate::Locations;

/// How deep the settings folders are searched: ClickOnce nests each install's data a few
/// generated folder names down.
const SETTINGS_DEPTH: usize = 8;

/// The nickname the v1 launcher kept among its .NET settings (`user.config`, `NickNames`), the
/// most recently written one that `valid` accepts (a name the launcher can take as an offline
/// account), or nothing.
pub fn projects_v1_nickname(locations: &Locations, valid: impl Fn(&str) -> bool) -> Option<String> {
    let setting = Regex::new(r#"(?s)<setting name="NickNames"[^>]*>\s*<value>([^<]*)</value>"#)
        .expect("valid pattern");
    let mut best: Option<(SystemTime, String)> = None;
    for root in locations.projects_v1_settings() {
        for entry in WalkDir::new(&root)
            .max_depth(SETTINGS_DEPTH)
            .into_iter()
            .flatten()
        {
            if entry.file_name() != "user.config" {
                continue;
            }
            let Ok(text) = std::fs::read_to_string(entry.path()) else {
                continue;
            };
            let Some(name) = setting
                .captures(&text)
                .and_then(|c| c.get(1))
                .map(|m| m.as_str().trim().to_owned())
            else {
                continue;
            };
            if !valid(&name) {
                continue;
            }
            let modified = entry
                .metadata()
                .ok()
                .and_then(|m| m.modified().ok())
                .unwrap_or(SystemTime::UNIX_EPOCH);
            if best.as_ref().is_none_or(|(at, _)| modified > *at) {
                best = Some((modified, name));
            }
        }
    }
    best.map(|(_, name)| name)
}

#[cfg(test)]
mod tests {
    use std::fs::File;
    use std::path::Path;
    use std::time::Duration;

    use super::*;
    use crate::locate::Os;

    fn offline_name(name: &str) -> bool {
        (3..=16).contains(&name.len())
            && name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_')
    }

    fn user_config(path: &Path, nickname: &str, age_secs: u64) {
        std::fs::create_dir_all(path.parent().unwrap()).unwrap();
        std::fs::write(
            path,
            format!(
                "<configuration><userSettings><Projects.Properties.Settings>\n\
                 <setting name=\"NickNames\" serializeAs=\"String\">\n  <value> {nickname} </value>\n\
                 </setting></Projects.Properties.Settings></userSettings></configuration>"
            ),
        )
        .unwrap();
        let at = SystemTime::now() - Duration::from_secs(age_secs);
        File::options()
            .write(true)
            .open(path)
            .unwrap()
            .set_modified(at)
            .unwrap();
    }

    #[test]
    fn the_v1_folders_follow_the_pretend_home() {
        let home = Path::new("/home/oyuncu");
        let windows = Locations::under(home, Os::Windows);
        assert_eq!(
            windows.projects_v1(),
            Some(home.join("AppData").join("Roaming").join(".projects"))
        );
        assert_eq!(
            windows.projects_v1_settings(),
            [
                home.join("AppData/Local/Apps/2.0/Data"),
                home.join("AppData/Local/Projects")
            ]
        );
        // The v1 launcher ran on Windows only.
        for os in [Os::MacOs, Os::Linux] {
            let other = Locations::under(home, os);
            assert_eq!(other.projects_v1(), None);
            assert!(other.projects_v1_settings().is_empty());
        }
    }

    #[test]
    fn the_newest_valid_nickname_wins() {
        let home = tempfile::tempdir().unwrap();
        let locations = Locations::under(home.path(), Os::Windows);
        let local = home.path().join("AppData").join("Local");
        // ClickOnce's generated folders (shortened: the temporary folder is deep already).
        user_config(
            &local.join("Apps/2.0/Data/AB12CD34.EF5/GH67IJ89.KL0/proj..tion_0001/Data/user.config"),
            "Eski_Ad",
            3_600,
        );
        user_config(
            &local.join("Projects/Projects.exe_Url_x/1.0.0.0/user.config"),
            "Kaan_07",
            60,
        );
        // Newer, but not a name the launcher could take: skipped.
        user_config(
            &local.join("Projects/Projects.exe_Url_y/1.0.0.1/user.config"),
            "kötü ad",
            1,
        );
        assert_eq!(
            projects_v1_nickname(&locations, offline_name).as_deref(),
            Some("Kaan_07")
        );
    }

    #[test]
    fn an_empty_pretend_home_has_no_nickname() {
        let home = tempfile::tempdir().unwrap();
        let locations = Locations::under(home.path(), Os::Windows);
        assert_eq!(projects_v1_nickname(&locations, offline_name), None);
        assert!(!locations.projects_v1().unwrap().exists());
    }
}
