//! Settings files a mod gets when a ready-made set puts it in place (MODHUB2).
//!
//! Some mods ship defaults that do not suit the official pack's instances or the Projects
//! server: Litematica opens its menu with M, Simple Voice Chat's microphone mute in the pack,
//! MaLiLib opens its settings with A then C, the pack's zoom key, and Chest Tracker draws the title
//! of a menu opened at a block above that block, which on the Projects server is the menu's
//! picture (resource pack glyphs). `data/mod-configs.json` lists a
//! settings file per mod holding only the values that matter. [`seed_mod_configs`] writes it
//! while no file is at its path and never touches one that is: the player's later changes and
//! the mod's own file always win. The mod keeps its own defaults for the rest and writes its full
//! file itself.
//!
//! The caller holds the instance's lock and the game is not running (a set is never installed
//! while it runs), so nothing else writes these files meanwhile.

use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::Deserialize;

use crate::error::{Error, Result};
use crate::paths::write_durable;

const TABLE_JSON: &str = include_str!("../../data/mod-configs.json");
const TABLE_SCHEMA: u32 = 1;
/// The folder every file of the table lives in (the active set's, per Minecraft version).
const CONFIG_DIR: &str = "config";

/// One settings file of `data/mod-configs.json`.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct ModConfig {
    /// The Modrinth projects it belongs to (a set item's id or its Modrinth project).
    pub projects: Vec<String>,
    /// The mod's name, for logs.
    #[serde(rename = "mod")]
    pub mod_name: String,
    /// The file, relative to the game folder: `config/…`, `/`-separated.
    pub path: String,
    /// What the file holds.
    pub json: serde_json::Value,
}

#[derive(Deserialize)]
struct Table {
    schema: u32,
    files: Vec<ModConfig>,
}

static TABLE: LazyLock<Vec<ModConfig>> =
    LazyLock::new(|| match serde_json::from_str::<Table>(TABLE_JSON) {
        Ok(table) if table.schema == TABLE_SCHEMA => table
            .files
            .into_iter()
            .filter(|file| {
                let ok = settings_path(&file.path).is_some() && file.json.is_object();
                if !ok {
                    tracing::error!(path = %file.path, "mod settings table entry left out");
                }
                ok
            })
            .collect(),
        Ok(table) => {
            tracing::error!(
                schema = table.schema,
                "mod settings table has an unknown schema"
            );
            Vec::new()
        }
        Err(error) => {
            tracing::error!(%error, "mod settings table is unreadable");
            Vec::new()
        }
    });

/// Every settings file of the table (`data/mod-configs.json`).
pub fn mod_configs() -> &'static [ModConfig] {
    &TABLE
}

/// The settings files of `projects`, in the table's order.
pub fn configs_for<'a>(projects: impl IntoIterator<Item = &'a str>) -> Vec<&'static ModConfig> {
    let wanted: Vec<&str> = projects.into_iter().collect();
    mod_configs()
        .iter()
        .filter(|file| file.projects.iter().any(|p| wanted.contains(&p.as_str())))
        .collect()
}

/// `path` as a path under the game folder's `config/`, or `None` for anything else (a path
/// outside it, a `..` or `.` part, an empty part, a backslash or a drive).
fn settings_path(path: &str) -> Option<PathBuf> {
    let rest = path.strip_prefix(CONFIG_DIR)?.strip_prefix('/')?;
    if rest.is_empty() || path.contains(['\\', ':']) {
        return None;
    }
    let mut out = PathBuf::from(CONFIG_DIR);
    for part in rest.split('/') {
        if part.is_empty() || part == "." || part == ".." {
            return None;
        }
        out.push(part);
    }
    Some(out)
}

/// Writes the settings files of `projects` (the Modrinth projects of the mods a set just put in
/// place) that do not exist yet in `game_dir`, and returns the table paths it wrote. Whatever is
/// already at a path (the player's file, the mod's own, even a folder) is left alone.
pub async fn seed_mod_configs(game_dir: &Path, projects: &[String]) -> Result<Vec<String>> {
    let mut written = Vec::new();
    for file in configs_for(projects.iter().map(String::as_str)) {
        let Some(relative) = settings_path(&file.path) else {
            continue;
        };
        let path = game_dir.join(relative);
        match tokio::fs::symlink_metadata(&path).await {
            Ok(_) => continue,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
            Err(error) => return Err(Error::io(&path, error)),
        }
        let mut bytes =
            serde_json::to_vec_pretty(&file.json).map_err(|e| Error::json(&file.path, e))?;
        bytes.push(b'\n');
        write_durable(&path, &bytes).await?;
        tracing::info!(mod_name = %file.mod_name, path = %file.path, "wrote a mod's first settings");
        written.push(file.path.clone());
    }
    Ok(written)
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;

    use serde_json::{Value, json};

    use super::*;

    const LITEMATICA: &str = "bEpr0Arc";
    const MALILIB: &str = "GcWjdA9I";
    /// Chest Tracker (Unofficial port), the only Chest Tracker with files for 26.x.
    const CHEST_TRACKER: &str = "VC2NohMN";

    /// The official pack's default key bindings (as in `keys.rs`): name, key, when it acts.
    const PACK_KEYS: &str = include_str!("../../testdata/key-bindings/projects-26.1.2.tsv");

    fn file_of(project: &str) -> &'static ModConfig {
        let files = configs_for([project]);
        assert_eq!(files.len(), 1, "{project}");
        files[0]
    }

    /// The hotkeys (MaLiLib's spelling, comma-separated keys) a table file sets.
    fn hotkeys(file: &ModConfig) -> Vec<(&str, &str)> {
        file.json["Hotkeys"]
            .as_object()
            .map(|map| {
                map.iter()
                    .map(|(name, value)| (name.as_str(), value.as_str().expect("a hotkey is text")))
                    .collect()
            })
            .unwrap_or_default()
    }

    /// The keys the pack's mappings use in normal play (`game` context), `options.txt` names.
    fn keys_in_play() -> BTreeSet<&'static str> {
        PACK_KEYS
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .filter_map(|line| {
                let cols: Vec<&str> = line.split('\t').collect();
                (cols.get(2) == Some(&"game")).then(|| cols[1])
            })
            .collect()
    }

    #[test]
    fn every_table_file_is_a_settings_file() {
        let table: Table = serde_json::from_str(TABLE_JSON).unwrap();
        assert_eq!(table.schema, TABLE_SCHEMA);
        assert_eq!(mod_configs().len(), table.files.len(), "no entry left out");
        let mut paths = BTreeSet::new();
        for file in mod_configs() {
            assert!(settings_path(&file.path).is_some(), "{}", file.path);
            // Written as plain JSON, which is JSON5 too (YACL's files).
            assert!(
                file.path.ends_with(".json") || file.path.ends_with(".json5"),
                "{}",
                file.path
            );
            assert!(file.json.is_object(), "{}", file.path);
            assert!(
                paths.insert(file.path.to_ascii_lowercase()),
                "{} twice",
                file.path
            );
            assert!(!file.projects.is_empty(), "{}", file.path);
            for project in &file.projects {
                assert!(
                    !project.is_empty()
                        && project.len() <= 64
                        && project
                            .bytes()
                            .all(|c| c.is_ascii_alphanumeric() || b"._-".contains(&c)),
                    "{project}"
                );
            }
        }
    }

    #[test]
    fn only_paths_under_config_are_written() {
        assert!(settings_path("config/litematica.json").is_some());
        assert!(settings_path("config/sub/a.json").is_some());
        for bad in [
            "config/",
            "config",
            "configs/a.json",
            "options.txt",
            "config/../options.txt",
            "config/./a.json",
            "config//a.json",
            "config\\a.json",
            "config/C:/a.json",
            "/config/a.json",
            "mods/a.jar",
        ] {
            assert!(settings_path(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn litematica_keeps_easy_placement_off_and_unbound() {
        let file = file_of(LITEMATICA);
        assert_eq!(file.path, "config/litematica.json");
        assert_eq!(file.json["Generic"]["easyPlaceMode"], json!(false));
        assert_eq!(file.json["Hotkeys"]["easyPlaceToggle"], json!(""));
    }

    #[test]
    fn litematica_leaves_the_voice_chat_mute_key_alone() {
        // Simple Voice Chat mutes the microphone on M in the official pack.
        assert!(keys_in_play().contains("key.keyboard.m"));
        let keys = hotkeys(file_of(LITEMATICA));
        assert!(keys.iter().any(|(name, _)| *name == "openGuiMainMenu"));
        for (name, value) in keys {
            let parts: Vec<&str> = value.split(',').filter(|part| !part.is_empty()).collect();
            assert!(!parts.contains(&"M"), "{name} = {value}");
            // A combination's first key reaches the game before the combination is complete:
            // it is J (free in play) or Ctrl (sprint, the game's own modifier).
            if let Some(first) = parts.first() {
                assert!(
                    *first == "J" || (*first == "LEFT_CONTROL" && parts.get(1) == Some(&"J")),
                    "{name} = {value}"
                );
            }
        }
    }

    #[test]
    fn j_does_nothing_in_play_in_the_official_pack() {
        assert!(!keys_in_play().contains("key.keyboard.j"));
    }

    #[test]
    fn malilib_settings_need_no_walking_and_zooming() {
        let file = file_of(MALILIB);
        assert_eq!(file.path, "config/malilib.json");
        assert_eq!(file.json["Generic"]["openGuiConfigs"], json!(""));
    }

    #[test]
    fn chest_tracker_draws_no_container_names() {
        // The Projects server's menus draw their pictures with resource pack glyphs in the title:
        // with names on, a menu opened at a block left its picture hanging above that block.
        let file = file_of(CHEST_TRACKER);
        assert_eq!(file.path, "config/chesttracker.json5");
        assert_eq!(
            file.json,
            json!({ "debug": { "disableContainerNames": true } })
        );
    }

    #[test]
    fn chest_tracker_menu_key_is_free_in_play() {
        // Chest Tracker opens its menu with the key below Esc (GLFW's grave accent) and keeps it;
        // its bundled Where Is It searches with Y only in inventory screens.
        assert!(!keys_in_play().contains("key.keyboard.grave.accent"));
    }

    #[tokio::test]
    async fn chest_tracker_gets_a_file_its_json5_reader_takes() {
        let dir = tempfile::tempdir().unwrap();
        let written = seed_mod_configs(dir.path(), &[CHEST_TRACKER.to_owned()])
            .await
            .unwrap();
        assert_eq!(written, ["config/chesttracker.json5"]);
        let bytes = std::fs::read(dir.path().join("config").join("chesttracker.json5")).unwrap();
        // Plain JSON (no comments, no trailing commas) is JSON5 as well.
        let on_disk: Value = serde_json::from_slice(&bytes).unwrap();
        assert_eq!(on_disk["debug"]["disableContainerNames"], json!(true));
        assert!(!dir.path().join("config").join("litematica.json").exists());
    }

    #[test]
    fn unknown_projects_have_no_files() {
        assert!(configs_for(["AANobbMI", ""]).is_empty());
    }

    #[tokio::test]
    async fn writes_a_missing_file_once_and_never_over_it() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let written = seed_mod_configs(game, &[LITEMATICA.to_owned(), MALILIB.to_owned()])
            .await
            .unwrap();
        assert_eq!(written, ["config/litematica.json", "config/malilib.json"]);
        let path = game.join("config").join("litematica.json");
        let on_disk: Value = serde_json::from_slice(&std::fs::read(&path).unwrap()).unwrap();
        assert_eq!(on_disk, file_of(LITEMATICA).json);

        // The player (or the mod) changes it: a later install leaves it as it is.
        std::fs::write(&path, b"{\"Hotkeys\":{\"openGuiMainMenu\":\"M\"}}").unwrap();
        let again = seed_mod_configs(game, &[LITEMATICA.to_owned()])
            .await
            .unwrap();
        assert!(again.is_empty());
        assert_eq!(
            std::fs::read(&path).unwrap(),
            b"{\"Hotkeys\":{\"openGuiMainMenu\":\"M\"}}"
        );
    }

    #[tokio::test]
    async fn anything_in_the_way_is_left_alone() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        std::fs::create_dir_all(game.join("config").join("litematica.json")).unwrap();
        let written = seed_mod_configs(game, &[LITEMATICA.to_owned()])
            .await
            .unwrap();
        assert!(written.is_empty());
        assert!(game.join("config").join("litematica.json").is_dir());
    }

    #[tokio::test]
    async fn mods_without_settings_write_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let written = seed_mod_configs(dir.path(), &["AANobbMI".to_owned()])
            .await
            .unwrap();
        assert!(written.is_empty());
        assert!(!dir.path().join("config").exists());
    }
}
