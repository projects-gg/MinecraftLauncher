//! Key bindings a new `options.txt` of the official pack starts with.
//!
//! The pack's mods ship default keys that clash with each other and with the game (WI Zoom and
//! Simple Voice Chat's menu both on V, …), and since 26.1 one key press fires every mapping bound
//! to that key. `data/key-bindings.json` moves the mods' keys so that nothing shares a key in
//! normal play; the Android app writes the same lines. Vanilla keys are never changed. Only an
//! `options.txt` the launcher creates gets them ([`super::seed_options`]): keys the player changed
//! and a pack's own `options.txt` always win.
//!
//! `testdata/key-bindings` holds the pack's default keys per Minecraft version (read from the
//! jars with `scripts/keys/KeyScan.java`); the tests check the table against them.

use std::sync::LazyLock;

use serde::Deserialize;

use super::defaults::at_least;

/// The key a binding gets to leave it unbound.
pub const UNBOUND: &str = "key.keyboard.unknown";
const TABLE_JSON: &str = include_str!("../../data/key-bindings.json");
const TABLE_SCHEMA: u32 = 1;

/// Which key bindings a new `options.txt` starts with.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FirstKeys {
    /// The game's and the mods' own defaults (instances the player put together).
    Defaults,
    /// The official pack's table ([`official_pack_bindings`]).
    OfficialPack,
}

/// One binding of the official pack's table.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct KeyBinding {
    /// The key mapping's name as `options.txt` stores it after `key_` (`key.wi_zoom.zoom`).
    pub name: String,
    /// The key it gets (`key.keyboard.c`); [`UNBOUND`] leaves it without one.
    pub key: String,
    /// The mod's own default key.
    pub default: String,
    /// The mod the key mapping belongs to.
    #[serde(rename = "mod")]
    pub mod_name: String,
    /// What the key does.
    pub action: String,
}

#[derive(Deserialize)]
struct Table {
    schema: u32,
    bindings: Vec<KeyBinding>,
}

static TABLE: LazyLock<Vec<KeyBinding>> =
    LazyLock::new(|| match serde_json::from_str::<Table>(TABLE_JSON) {
        Ok(table) if table.schema == TABLE_SCHEMA => table.bindings,
        Ok(table) => {
            tracing::error!(
                schema = table.schema,
                "key binding table has an unknown schema"
            );
            Vec::new()
        }
        Err(error) => {
            tracing::error!(%error, "key binding table is unreadable");
            Vec::new()
        }
    });

/// The official pack's bindings (`data/key-bindings.json`).
pub fn official_pack_bindings() -> &'static [KeyBinding] {
    &TABLE
}

/// The `options.txt` lines (`key_<name>:<key>`) a new file of Minecraft `version` gets: none
/// before 1.13 (snapshot 17w43a), which stored keys as numbers.
pub fn official_pack_key_lines(version: &str) -> String {
    if !at_least(version, (1, 13, 0), (17, 43)) {
        return String::new();
    }
    official_pack_bindings()
        .iter()
        .map(|binding| format!("key_{}:{}\n", binding.name, binding.key))
        .collect()
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, HashMap, HashSet};

    use super::*;

    /// The official pack's default key bindings per Minecraft version it is published for.
    const INVENTORIES: &[(&str, &str)] = &[(
        "26.1.2",
        include_str!("../../testdata/key-bindings/projects-26.1.2.tsv"),
    )];

    struct Row<'a> {
        name: &'a str,
        key: &'a str,
        /// When the key acts: only `game` keys can clash in normal play.
        context: &'a str,
        source: &'a str,
    }

    fn rows(text: &str) -> Vec<Row<'_>> {
        text.lines()
            .filter(|line| !line.starts_with('#') && !line.trim().is_empty())
            .map(|line| {
                let cols: Vec<&str> = line.split('\t').collect();
                assert_eq!(cols.len(), 4, "{line}");
                Row {
                    name: cols[0],
                    key: cols[1],
                    context: cols[2],
                    source: cols[3],
                }
            })
            .collect()
    }

    fn is_key_name(key: &str) -> bool {
        let keyboard = key.strip_prefix("key.keyboard.").is_some_and(|rest| {
            !rest.is_empty()
                && rest
                    .bytes()
                    .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'.')
        });
        let mouse = key.strip_prefix("key.mouse.").is_some_and(|rest| {
            matches!(rest, "left" | "right" | "middle") || rest.parse::<u8>().is_ok()
        });
        keyboard || mouse
    }

    /// Keys used by more than one `game` binding once `table` is applied.
    fn shared_in_play<'a>(
        rows: &'a [Row<'a>],
        table: &'a [KeyBinding],
    ) -> BTreeMap<&'a str, Vec<&'a str>> {
        let mut by_key: BTreeMap<&str, Vec<&str>> = BTreeMap::new();
        for row in rows.iter().filter(|row| row.context == "game") {
            let key = table
                .iter()
                .find(|binding| binding.name == row.name)
                .map_or(row.key, |binding| binding.key.as_str());
            if key != UNBOUND {
                by_key.entry(key).or_default().push(row.name);
            }
        }
        by_key.retain(|_, names| names.len() > 1);
        by_key
    }

    #[test]
    fn the_table_is_well_formed() {
        let table = official_pack_bindings();
        assert!(!table.is_empty(), "data/key-bindings.json did not load");
        let mut names = HashSet::new();
        let mut keys = HashSet::new();
        for binding in table {
            assert!(names.insert(&binding.name), "{} twice", binding.name);
            assert!(
                !binding.name.is_empty()
                    && binding
                        .name
                        .bytes()
                        .all(|c| c.is_ascii_graphic() && c != b':'),
                "{binding:?}"
            );
            assert!(
                is_key_name(&binding.key) && is_key_name(&binding.default),
                "{binding:?}"
            );
            assert_ne!(binding.key, binding.default, "{binding:?} changes nothing");
            assert!(!binding.mod_name.is_empty() && !binding.action.is_empty());
            if binding.key != UNBOUND {
                assert!(keys.insert(&binding.key), "{} twice", binding.key);
            }
        }
    }

    #[test]
    fn the_official_pack_shares_no_key_in_play() {
        let table = official_pack_bindings();
        for (version, text) in INVENTORIES {
            let rows = rows(text);
            let by_name: HashMap<&str, &Row> = rows.iter().map(|row| (row.name, row)).collect();
            assert_eq!(by_name.len(), rows.len(), "{version}: a name twice");
            for row in &rows {
                assert!(is_key_name(row.key), "{version}: {}", row.key);
                assert!(
                    matches!(
                        row.context,
                        "game" | "creative" | "spectator" | "debug" | "recording" | "replay"
                    ),
                    "{version}: {}",
                    row.context
                );
            }
            for binding in table {
                let row = by_name
                    .get(binding.name.as_str())
                    .unwrap_or_else(|| panic!("{version}: no {} in the pack", binding.name));
                assert_eq!(row.key, binding.default, "{version}: {}", binding.name);
                assert_ne!(row.source, "Minecraft", "{version}: {}", binding.name);
            }
            assert_eq!(
                shared_in_play(&rows, table),
                BTreeMap::new(),
                "{version}: keys shared in play"
            );
        }
    }

    #[test]
    fn the_mods_own_defaults_clash() {
        // What the table fixes (26.1.2): both mappings act on one key press.
        let rows = rows(INVENTORIES[0].1);
        let shared = shared_in_play(&rows, &[]);
        assert_eq!(
            shared,
            BTreeMap::from([
                (
                    "key.keyboard.g",
                    vec!["key.quickActions", "key.gammautils.gammaToggle"]
                ),
                (
                    "key.keyboard.h",
                    vec!["key.gammautils.nightVisionToggle", "key.hide_icons"]
                ),
                (
                    "key.keyboard.r",
                    vec!["iris.keybind.reload", "gui.xaero_quick_eat"]
                ),
                ("key.keyboard.v", vec!["key.voice_chat", "key.wi_zoom.zoom"]),
            ])
        );
    }

    #[test]
    fn versions_before_1_13_get_no_key_lines() {
        let lines = official_pack_key_lines("26.1.2");
        assert_eq!(lines.lines().count(), official_pack_bindings().len());
        assert!(lines.starts_with("key_key.wi_zoom.zoom:key.keyboard.c\n"));
        assert!(lines.contains("key_key.push_to_talk:key.keyboard.caps.lock\n"));
        assert!(lines.contains("key_iris.keybind.reload:key.keyboard.unknown\n"));
        assert_eq!(official_pack_key_lines("1.13"), lines);
        assert_eq!(official_pack_key_lines("17w43a"), lines);
        assert_eq!(official_pack_key_lines("1.12.2"), "");
        assert_eq!(official_pack_key_lines("17w31a"), "");
    }
}
