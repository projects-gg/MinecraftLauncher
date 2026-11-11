//! Settings files a mod gets when the official pack or a ready-made set puts it in place
//! (MODHUB2, CHEST-TRACKER).
//!
//! Some mods ship defaults that do not suit the official pack's instances or the Projects
//! server: Litematica opens its menu with M, Simple Voice Chat's microphone mute in the pack,
//! MaLiLib opens its settings with A then C, the pack's zoom key, and Chest Tracker draws the title
//! of a menu opened at a block above that block, which on the Projects server is the menu's
//! picture (resource pack glyphs). Sodium Extra Information's library and XaeroLib call out
//! while the game starts and the render thread waits for them (LAUNCH-SPEED): their switches are
//! off. `data/mod-configs.json` lists a settings file per mod holding only the values that
//! matter (JSON, or a mod's own text format). [`seed_mod_configs`] writes it
//! while no file is at its path and never touches one that is: the player's later changes and
//! the mod's own file always win. The mod keeps its own defaults for the rest and writes its full
//! file itself. A value that existing instances need too (Better PvP's off hand armor status,
//! XAERO-OFFHAND) is changed there once, after a backup ([`fix_mod_settings_once`]).
//!
//! The caller holds the instance's lock and the game is not running (a pack or a set is never
//! synced while it runs), so nothing else writes these files meanwhile.

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
    /// What the file holds, as JSON (an object).
    #[serde(default)]
    pub json: serde_json::Value,
    /// Instead of `json`: the file in the mod's own text format, written as it is (XaeroLib's
    /// `key = value` lines).
    #[serde(default)]
    pub text: Option<String>,
}

impl ModConfig {
    /// The file's bytes: `json` pretty-printed, or `text`; `None` for an entry that holds
    /// neither or both.
    fn contents(&self) -> Option<Vec<u8>> {
        match (&self.text, self.json.is_object()) {
            (None, true) => {
                let mut bytes = serde_json::to_vec_pretty(&self.json).ok()?;
                bytes.push(b'\n');
                Some(bytes)
            }
            (Some(text), false) if self.json.is_null() && !text.trim().is_empty() => {
                Some(text.as_bytes().to_vec())
            }
            _ => None,
        }
    }
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
                let ok = settings_path(&file.path).is_some() && file.contents().is_some();
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
        let Some(bytes) = file.contents() else {
            continue;
        };
        write_durable(&path, &bytes).await?;
        tracing::info!(mod_name = %file.mod_name, path = %file.path, "wrote a mod's first settings");
        written.push(file.path.clone());
    }
    Ok(written)
}

/// Better PvP's profile and the value [`fix_mod_settings_once`] turns off (XAERO-OFFHAND).
const OFFHAND_PROFILE: &str = "config/xaero/betterpvp/profiles/default.cfg";
const OFFHAND_KEY: &str = "armor_status_off_hand";
/// Folder of the one-off fixes' marks and backups, in the instance's launcher folder beside its
/// game folder (`<instance>/.launcher/mod-settings-fixes/`).
const FIXES_DIR: &str = "mod-settings-fixes";
const OFFHAND_MARK: &str = "xaero-offhand.done";
const OFFHAND_BACKUP: &str = "xaero-offhand-default.cfg";

/// `profile` (Better PvP's `key = value` lines) with `armor_status_off_hand = true` turned
/// `false`, or `None` when no line has it on (or the file is not text).
fn offhand_off(profile: &[u8]) -> Option<Vec<u8>> {
    let text = std::str::from_utf8(profile).ok()?;
    let mut changed = false;
    let mut out = String::with_capacity(text.len());
    for line in text.split_inclusive('\n') {
        let body = line.trim_end_matches(['\r', '\n']);
        let on = body
            .split_once('=')
            .is_some_and(|(key, value)| key.trim() == OFFHAND_KEY && value.trim() == "true");
        if on {
            out.push_str(OFFHAND_KEY);
            out.push_str(" = false");
            out.push_str(&line[body.len()..]);
            changed = true;
        } else {
            out.push_str(line);
        }
    }
    changed.then(|| out.into_bytes())
}

/// The one-off fix of a value existing instances got before [`mod_configs`] had it
/// (XAERO-OFFHAND): Better PvP's armor status off hand item, on by default, draws the survival
/// inventory's off hand slot empty while an item is in it. Once per instance, at the first launch
/// that finds the profile: a `true` is turned `false` after a copy of the file is kept beside the
/// mark (`<instance>/.launcher/mod-settings-fixes/`), every other line stays as it is, and the
/// mark is left whatever the value was, so a player who turns it back on later keeps it. No
/// profile yet (the mod never ran, or is not installed): nothing, and no mark. Returns whether
/// the value was changed. The game must not be running.
pub async fn fix_mod_settings_once(game_dir: &Path) -> Result<bool> {
    let Some(instance) = game_dir.parent() else {
        return Ok(false);
    };
    let fixes = instance.join(".launcher").join(FIXES_DIR);
    let mark = fixes.join(OFFHAND_MARK);
    if tokio::fs::symlink_metadata(&mark).await.is_ok() {
        return Ok(false);
    }
    let Some(relative) = settings_path(OFFHAND_PROFILE) else {
        return Ok(false);
    };
    let profile = game_dir.join(relative);
    let bytes = match tokio::fs::read(&profile).await {
        Ok(bytes) => bytes,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(Error::io(&profile, error)),
    };
    let fixed = offhand_off(&bytes);
    if let Some(fixed) = &fixed {
        write_durable(&fixes.join(OFFHAND_BACKUP), &bytes).await?;
        write_durable(&profile, fixed).await?;
        tracing::info!(
            path = OFFHAND_PROFILE,
            "turned Better PvP's off hand armor status off once"
        );
    }
    write_durable(&mark, b"XAERO-OFFHAND\n").await?;
    Ok(fixed.is_some())
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
    /// Sodium Extra Information (Modrinth).
    const SODIUM_EXTRA_INFO: &str = "696WgnvT";
    /// Xaero's Better PvP: the official pack's own item (Projects hosts the file).
    const BETTER_PVP: &str = "better-pvp";

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

    /// The pack's mappings on `key` in normal play (`game` context).
    fn in_play_on(key: &str) -> Vec<&'static str> {
        PACK_KEYS
            .lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .filter_map(|line| {
                let cols: Vec<&str> = line.split('\t').collect();
                (cols.get(1) == Some(&key) && cols.get(2) == Some(&"game")).then(|| cols[0])
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
            assert!(file.contents().is_some(), "{}", file.path);
            let json_file = file.path.ends_with(".json") || file.path.ends_with(".json5");
            if file.text.is_none() {
                // Written as plain JSON, which is JSON5 too (YACL's files).
                assert!(json_file, "{}", file.path);
                assert!(file.json.is_object(), "{}", file.path);
            } else {
                // A mod's own text format, one value per line, ending with a line break.
                let text = file.text.as_deref().unwrap();
                assert!(!json_file && file.json.is_null(), "{}", file.path);
                assert!(
                    text.ends_with('\n') && !text.contains('\r'),
                    "{}",
                    file.path
                );
            }
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
        // YACL's Gson names the fields in snake_case: a camelCase key is ignored and the mod
        // writes its own default (false) over it (seen in the game, 27 Oct).
        assert_eq!(
            file.json,
            json!({ "debug": { "disable_container_names": true } })
        );
    }

    #[test]
    fn chest_tracker_menu_key_is_free_in_play() {
        // Chest Tracker opens its menu with the key below Esc (GLFW's grave accent) and keeps it:
        // nothing else acts on it in play. Its bundled Where Is It searches with Y in inventory
        // screens (in the world only with an option that is off by default).
        assert_eq!(
            in_play_on("key.keyboard.grave.accent"),
            ["key.chesttracker.open_gui"]
        );
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
        assert_eq!(on_disk["debug"]["disable_container_names"], json!(true));
        assert!(!dir.path().join("config").join("litematica.json").exists());
    }

    #[test]
    fn unknown_projects_have_no_files() {
        assert!(configs_for(["AANobbMI", ""]).is_empty());
    }

    /// The pack's mods that would hold the game's start up with a request (LAUNCH-SPEED): Sodium
    /// Extra Information's library sends usage data, XaeroLib (inside Better PvP) asks about
    /// Patreon perks and updates, each waited for on the render thread.
    #[tokio::test]
    async fn mods_that_call_out_at_the_start_get_it_off() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path();
        let written =
            seed_mod_configs(game, &[SODIUM_EXTRA_INFO.to_owned(), BETTER_PVP.to_owned()])
                .await
                .unwrap();
        assert_eq!(
            written,
            [
                "config/sodiumextrainformation/telemetry-config.json",
                "config/xaero/lib/common.cfg",
                OFFHAND_PROFILE
            ]
        );
        let telemetry: Value = serde_json::from_slice(
            &std::fs::read(game.join("config/sodiumextrainformation/telemetry-config.json"))
                .unwrap(),
        )
        .unwrap();
        assert_eq!(telemetry, json!({ "enable_telemetry": false }));
        assert_eq!(
            std::fs::read_to_string(game.join("config/xaero/lib/common.cfg")).unwrap(),
            "allow_internet_access = false\n"
        );
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

    /// An instance folder with a game folder inside, as the launcher lays them out.
    fn instance() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("minecraft");
        std::fs::create_dir_all(&game).unwrap();
        (dir, game)
    }

    #[tokio::test]
    async fn a_new_better_pvp_gets_the_off_hand_item_off() {
        let (_dir, game) = instance();
        seed_mod_configs(&game, &[BETTER_PVP.to_owned()])
            .await
            .unwrap();
        assert_eq!(
            std::fs::read_to_string(game.join(OFFHAND_PROFILE)).unwrap(),
            "armor_status_off_hand = false\n"
        );
        // The launch after it finds the value off: nothing to change, and it never looks again.
        assert!(!fix_mod_settings_once(&game).await.unwrap());
        assert!(
            game.parent()
                .unwrap()
                .join(".launcher")
                .join(FIXES_DIR)
                .join(OFFHAND_MARK)
                .exists()
        );
    }

    #[tokio::test]
    async fn an_old_profile_is_fixed_once_with_a_backup() {
        let (_dir, game) = instance();
        let profile = game.join(OFFHAND_PROFILE);
        std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
        let old = "armor_status = true\r\narmor_status_off_hand = true\r\nminimap_zoom = 2\r\n";
        std::fs::write(&profile, old).unwrap();

        assert!(fix_mod_settings_once(&game).await.unwrap());
        assert_eq!(
            std::fs::read_to_string(&profile).unwrap(),
            "armor_status = true\r\narmor_status_off_hand = false\r\nminimap_zoom = 2\r\n"
        );
        let fixes = game.parent().unwrap().join(".launcher").join(FIXES_DIR);
        assert_eq!(
            std::fs::read_to_string(fixes.join(OFFHAND_BACKUP)).unwrap(),
            old
        );

        // The player turns it back on: later launches leave it.
        std::fs::write(&profile, old).unwrap();
        assert!(!fix_mod_settings_once(&game).await.unwrap());
        assert_eq!(std::fs::read_to_string(&profile).unwrap(), old);
    }

    #[tokio::test]
    async fn no_profile_yet_leaves_no_mark() {
        let (_dir, game) = instance();
        assert!(!fix_mod_settings_once(&game).await.unwrap());
        assert!(!game.parent().unwrap().join(".launcher").exists());

        // The mod writes its profile with the value on; the next launch turns it off.
        let profile = game.join(OFFHAND_PROFILE);
        std::fs::create_dir_all(profile.parent().unwrap()).unwrap();
        std::fs::write(&profile, "armor_status_off_hand=true\n").unwrap();
        assert!(fix_mod_settings_once(&game).await.unwrap());
        assert_eq!(
            std::fs::read_to_string(&profile).unwrap(),
            "armor_status_off_hand = false\n"
        );
    }

    #[test]
    fn only_the_off_hand_line_on_is_changed() {
        assert_eq!(offhand_off(b"armor_status_off_hand = false\n"), None);
        assert_eq!(offhand_off(b"armor_status_off_hand_x = true\n"), None);
        assert_eq!(offhand_off(b"a = 1\n"), None);
        assert_eq!(offhand_off(&[0xff, 0xfe]), None);
        assert_eq!(
            offhand_off(b"a = 1\n  armor_status_off_hand =  true").as_deref(),
            Some(&b"a = 1\narmor_status_off_hand = false"[..])
        );
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
