//! Key bindings a new `options.txt` of the official pack starts with, and the one-tap fix for an
//! older file that still has the clashes.
//!
//! The pack's mods ship default keys that clash with each other and with the game (WI Zoom and
//! Simple Voice Chat's menu both on V, …), and since 26.1 one key press fires every mapping bound
//! to that key. `data/key-bindings.json` moves the mods' keys so that nothing shares a key in
//! normal play; the Android app writes the same lines. Vanilla keys are never changed. Only an
//! `options.txt` the launcher creates gets them ([`super::seed_options`]): keys the player changed
//! and a pack's own `options.txt` always win.
//!
//! A file from before the table keeps its keys until the player agrees to a fix (KEYS2): the mod
//! check reports the clashes it still has ([`find_key_clashes`]: both mappings on their shared
//! default key), and "Düzelt" moves just those lines ([`fix_key_clashes_in`], after a backup) with
//! "Geri al" ([`undo_key_fix_in`]).
//!
//! `testdata/key-bindings` holds the pack's default keys per Minecraft version (read from the
//! jars with `scripts/keys/KeyScan.java`); the tests check the table against them.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::LazyLock;

use serde::Deserialize;

use super::defaults::{OPTIONS_FILE, at_least};
use super::options::{key_value, split_lines, write_options};
use crate::error::{Error, Result};
use crate::paths::write_durable;

/// The key a binding gets to leave it unbound.
pub const UNBOUND: &str = "key.keyboard.unknown";
const TABLE_JSON: &str = include_str!("../../data/key-bindings.json");
const TABLE_SCHEMA: u32 = 1;

/// Folder of the copies of `options.txt` made before key fixes, in the instance's launcher folder
/// beside its game folder (`<instance>/.launcher/options-backups/`): out of the game's and the
/// packs' reach.
const BACKUP_DIR: &str = "options-backups";
const BACKUP_PREFIX: &str = "options-";
const BACKUP_SUFFIX: &str = ".txt";
/// Backups kept per instance; each fix makes one and the oldest go.
const KEPT_BACKUPS: usize = 5;

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
    /// The mapping that has the same default key in the pack, when moving this binding is what
    /// ends that clash (`key.voice_chat` for WI Zoom's zoom). `None` for a binding that only gets a
    /// key (push to talk).
    #[serde(default, rename = "clashesWith")]
    pub clashes_with: Option<String>,
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

// --- clashes in an existing options.txt (KEYS2) -------------------------------------------------

/// A clash the table ends that an `options.txt` still has: the table's binding and the mapping it
/// shares its default key with are both still on that key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyClash {
    /// The binding the fix moves.
    pub binding: &'static KeyBinding,
    /// The mapping it shares the key with (`key.voice_chat`).
    pub partner: &'static str,
}

impl KeyClash {
    /// The key both are on: the mod's default (`key.keyboard.v`).
    pub fn key(&self) -> &'static str {
        let binding: &'static KeyBinding = self.binding;
        binding.default.as_str()
    }

    /// The key the fix gives the binding (`key.keyboard.c`; [`UNBOUND`] leaves it without one).
    pub fn to(&self) -> &'static str {
        let binding: &'static KeyBinding = self.binding;
        binding.key.as_str()
    }
}

/// False for mappings that act only outside normal play: F3 combinations, creative mode's
/// hotbar modifiers, spectator keys and Replay Mod (its viewer and recording). Sharing a key with
/// them is no clash. The inventory's "when it acts" column says the same (tested).
fn acts_in_play(name: &str) -> bool {
    match name {
        // F3 itself and Replay Mod's settings screen work in normal play.
        "key.debug.overlay" | "key.replaymod.settings" => true,
        "key.saveToolbarActivator"
        | "key.loadToolbarActivator"
        | "key.spectatorHotbar"
        | "key.spectatorOutlines"
        | "key.toggleSpectatorShaderEffects" => false,
        _ => !(name.starts_with("key.debug.") || name.starts_with("key.replaymod.")),
    }
}

/// Each mapping of `options` (`key_<name>` lines) with its key, exactly as written; a mapping
/// written twice counts with its last line, as in the game.
fn key_lines(options: &[u8]) -> HashMap<&str, &[u8]> {
    let mut keys = HashMap::new();
    for (text, _) in split_lines(options) {
        if let Some((key, value)) = key_value(text)
            && let Some(name) = key.strip_prefix("key_")
        {
            keys.insert(name, value);
        }
    }
    keys
}

/// The clashes the official pack's table ends that `options` (an `options.txt`) still has: the
/// table's binding and the mapping it shares its default key with
/// ([`KeyBinding::clashes_with`]) are both written there, both on that key. A mapping without a
/// line does not count (the game drops the lines of mods it did not load and writes all of them
/// once it ran with them), and a key the player changed never counts. A clash is left out when
/// the table's new key already belongs to another mapping that acts in normal play: moving there
/// would only make a new clash.
pub fn find_key_clashes(options: &[u8]) -> Vec<KeyClash> {
    let keys = key_lines(options);
    official_pack_bindings()
        .iter()
        .filter_map(|binding| {
            let partner = binding.clashes_with.as_deref()?;
            let on_default = |name: &str| {
                keys.get(name)
                    .is_some_and(|key| *key == binding.default.as_bytes())
            };
            if !(on_default(&binding.name) && on_default(partner)) {
                return None;
            }
            let taken = binding.key != UNBOUND
                && keys.iter().any(|(name, key)| {
                    *name != binding.name && *key == binding.key.as_bytes() && acts_in_play(name)
                });
            (!taken).then_some(KeyClash { binding, partner })
        })
        .collect()
}

/// `options` with `key_<name>:<from>` lines turned into `key_<name>:<to>` for each move (every copy
/// of such a line). Every other byte stays: the order, unknown lines, the line endings.
fn move_keys(options: &[u8], moves: &[(&str, &str, &str)]) -> Vec<u8> {
    let mut out = Vec::with_capacity(options.len() + 64);
    for (text, ending) in split_lines(options) {
        let moved = key_value(text).and_then(|(key, value)| {
            let name = key.strip_prefix("key_")?;
            moves
                .iter()
                .find(|(mapping, from, _)| *mapping == name && value == from.as_bytes())
                .map(|(_, _, to)| (key, *to))
        });
        match moved {
            Some((key, to)) => {
                out.extend_from_slice(key.as_bytes());
                out.push(b':');
                out.extend_from_slice(to.as_bytes());
            }
            None => out.extend_from_slice(text),
        }
        out.extend_from_slice(ending);
    }
    out
}

/// `options` with the binding of each clash moved to the table's key. Only those lines change.
pub fn fix_key_clashes(options: &[u8], clashes: &[KeyClash]) -> Vec<u8> {
    let moves: Vec<(&str, &str, &str)> = clashes
        .iter()
        .map(|clash| (clash.binding.name.as_str(), clash.key(), clash.to()))
        .collect();
    move_keys(options, &moves)
}

/// Undoes [`fix_key_clashes`] on `current`, given the file as it was before (`backup`). When
/// nothing else changed since, the old file comes back byte for byte; when the game saved the
/// file meanwhile, only the lines still on the table's keys go back to their old key.
pub fn undo_key_fix(current: &[u8], backup: &[u8]) -> Vec<u8> {
    let clashes = find_key_clashes(backup);
    if fix_key_clashes(backup, &clashes) == current {
        return backup.to_vec();
    }
    let moves: Vec<(&str, &str, &str)> = clashes
        .iter()
        .map(|clash| (clash.binding.name.as_str(), clash.to(), clash.key()))
        .collect();
    move_keys(current, &moves)
}

/// [`find_key_clashes`] of `<game_dir>/options.txt`; none when there is no such file or it
/// cannot be read.
pub async fn key_clashes_in(game_dir: &Path) -> Vec<KeyClash> {
    match tokio::fs::read(game_dir.join(OPTIONS_FILE)).await {
        Ok(options) => find_key_clashes(&options),
        Err(_) => Vec::new(),
    }
}

/// What [`fix_key_clashes_in`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFix {
    /// The backup's file name, for [`undo_key_fix_in`].
    pub backup: String,
    /// The clashes it ended.
    pub clashes: Vec<KeyClash>,
}

fn backups_dir(game_dir: &Path) -> Result<PathBuf> {
    game_dir
        .parent()
        .map(|instance| instance.join(".launcher").join(BACKUP_DIR))
        .ok_or_else(|| Error::Invalid(format!("{} has no parent", game_dir.display())))
}

/// The time stamp and counter of a backup's name (`options-20261019-231500.txt` →
/// `("20261019-231500", 1)`, `…-231500-2.txt` → 2): `None` for any other name.
fn backup_order(name: &str) -> Option<(&str, u32)> {
    let stamp = name
        .strip_prefix(BACKUP_PREFIX)?
        .strip_suffix(BACKUP_SUFFIX)?;
    let digits =
        |part: &str, len: usize| part.len() == len && part.bytes().all(|b| b.is_ascii_digit());
    let mut parts = stamp.splitn(3, '-');
    let (day, time) = (parts.next()?, parts.next()?);
    if !(digits(day, 8) && digits(time, 6)) {
        return None;
    }
    let count = match parts.next() {
        None => 1,
        Some(n) if (1..=3).contains(&n.len()) && n.bytes().all(|b| b.is_ascii_digit()) => {
            n.parse().ok()?
        }
        Some(_) => return None,
    };
    Some((&stamp[..15], count))
}

/// The backup names in `dir`.
async fn backup_names(dir: &Path) -> Vec<String> {
    let mut names = Vec::new();
    let Ok(mut entries) = tokio::fs::read_dir(dir).await else {
        return names;
    };
    while let Ok(Some(entry)) = entries.next_entry().await {
        let name = entry.file_name().to_string_lossy().into_owned();
        if backup_order(&name).is_some() {
            names.push(name);
        }
    }
    names
}

/// A name for a new backup in `dir`: the time (UTC, so the names sort by time), with a counter
/// above every backup of the same second (pruning removes the lowest, so the newest is always
/// the last in order).
async fn new_backup_name(dir: &Path) -> String {
    let stamp = jiff::Timestamp::now().strftime("%Y%m%d-%H%M%S").to_string();
    let last = backup_names(dir)
        .await
        .iter()
        .filter_map(|name| backup_order(name))
        .filter(|(at, _)| *at == stamp)
        .map(|(_, count)| count)
        .max();
    match last {
        None => format!("{BACKUP_PREFIX}{stamp}{BACKUP_SUFFIX}"),
        Some(count) => format!("{BACKUP_PREFIX}{stamp}-{}{BACKUP_SUFFIX}", count + 1),
    }
}

/// Deletes all but the newest [`KEPT_BACKUPS`] backups in `dir`. Failures only leave files.
async fn prune_backups(dir: &Path) {
    let mut names = backup_names(dir).await;
    names.sort_by(|a, b| backup_order(a).cmp(&backup_order(b)));
    let extra = names.len().saturating_sub(KEPT_BACKUPS);
    for name in &names[..extra] {
        if let Err(error) = tokio::fs::remove_file(dir.join(name)).await {
            tracing::warn!(%error, name = %name, "could not remove an old options.txt backup");
        }
    }
}

/// Backs up `<game_dir>/options.txt` and moves the bindings of its clashes
/// ([`find_key_clashes`]) to the table's keys; nothing else in the file changes. `None` when
/// nothing clashes (or there is no file). The game must not be running: it writes its own copy of
/// every key back.
pub async fn fix_key_clashes_in(game_dir: &Path) -> Result<Option<KeyFix>> {
    let path = game_dir.join(OPTIONS_FILE);
    let options = match tokio::fs::read(&path).await {
        Ok(options) => options,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::io(&path, error)),
    };
    let clashes = find_key_clashes(&options);
    if clashes.is_empty() {
        return Ok(None);
    }
    let dir = backups_dir(game_dir)?;
    let backup = new_backup_name(&dir).await;
    write_durable(&dir.join(&backup), &options).await?;
    write_options(&path, fix_key_clashes(&options, &clashes)).await?;
    prune_backups(&dir).await;
    tracing::info!(clashes = clashes.len(), backup = %backup, "options.txt key clashes fixed");
    Ok(Some(KeyFix { backup, clashes }))
}

/// Undoes a fix of [`fix_key_clashes_in`] from its backup ([`undo_key_fix`]; a file that is gone
/// meanwhile comes back whole). The backup goes once it is used. `false` when that backup no
/// longer exists: nothing changes then.
pub async fn undo_key_fix_in(game_dir: &Path, backup: &str) -> Result<bool> {
    if backup_order(backup).is_none() {
        return Err(Error::Invalid(format!(
            "not an options.txt backup: {backup}"
        )));
    }
    let file = backups_dir(game_dir)?.join(backup);
    let saved = match tokio::fs::read(&file).await {
        Ok(saved) => saved,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(false),
        Err(error) => return Err(Error::io(&file, error)),
    };
    let path = game_dir.join(OPTIONS_FILE);
    let (current, restored) = match tokio::fs::read(&path).await {
        Ok(current) => {
            let restored = undo_key_fix(&current, &saved);
            (Some(current), restored)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (None, saved),
        Err(error) => return Err(Error::io(&path, error)),
    };
    if current.as_deref() != Some(restored.as_slice()) {
        write_options(&path, restored).await?;
    }
    if let Err(error) = tokio::fs::remove_file(&file).await {
        tracing::warn!(%error, backup = %backup, "could not remove a used options.txt backup");
    }
    tracing::info!(backup = %backup, "options.txt key fix undone");
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};

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
            if let Some(partner) = &binding.clashes_with {
                assert_ne!(partner, &binding.name);
                assert_ne!(binding.default, UNBOUND, "{binding:?}: no key, no clash");
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

    // --- KEYS2 ---------------------------------------------------------------------------------

    #[test]
    fn the_partners_are_the_clashes_of_the_pack() {
        // `clashesWith` names exactly the other side of each clash the mods' defaults make.
        let table = official_pack_bindings();
        for (version, text) in INVENTORIES {
            let rows = rows(text);
            let expected: BTreeSet<(&str, &str)> = shared_in_play(&rows, &[])
                .into_values()
                .map(|names| {
                    assert_eq!(names.len(), 2, "{version}: {names:?}");
                    let moved = table
                        .iter()
                        .find(|binding| names.contains(&binding.name.as_str()))
                        .unwrap_or_else(|| panic!("{version}: the table leaves {names:?}"));
                    let partner = names.iter().find(|name| **name != moved.name).unwrap();
                    (moved.name.as_str(), *partner)
                })
                .collect();
            let declared: BTreeSet<(&str, &str)> = table
                .iter()
                .filter_map(|binding| {
                    Some((binding.name.as_str(), binding.clashes_with.as_deref()?))
                })
                .collect();
            assert_eq!(declared, expected, "{version}");
        }
    }

    #[test]
    fn in_play_follows_the_inventory() {
        for (version, text) in INVENTORIES {
            for row in rows(text) {
                assert_eq!(
                    acts_in_play(row.name),
                    row.context == "game",
                    "{version}: {} ({})",
                    row.name,
                    row.context
                );
            }
        }
    }

    /// An `options.txt` as the game writes it on Windows (CRLF) with every key of the pack on its
    /// default: a file from before the table.
    fn game_file() -> String {
        let mut file = String::from("version:4671\r\nfov:0.0\r\nlang:tr_tr\r\n");
        for row in rows(INVENTORIES[0].1) {
            file.push_str(&format!("key_{}:{}\r\n", row.name, row.key));
        }
        file.push_str(
            "lastServer:play.projects.gg\r\nresourcePacks:[\"vanilla\",\"file/Gölge.zip\"]\r\n",
        );
        file
    }

    fn clash_names(clashes: &[KeyClash]) -> Vec<&str> {
        clashes
            .iter()
            .map(|clash| clash.binding.name.as_str())
            .collect()
    }

    #[test]
    fn a_file_from_before_the_table_gets_only_its_clashing_lines_moved() {
        let before = game_file();
        let clashes = find_key_clashes(before.as_bytes());
        assert_eq!(
            clash_names(&clashes),
            [
                "key.wi_zoom.zoom",
                "key.gammautils.gammaToggle",
                "key.hide_icons",
                "iris.keybind.reload"
            ]
        );
        assert_eq!(clashes[0].partner, "key.voice_chat");
        assert_eq!(
            (clashes[0].key(), clashes[0].to()),
            ("key.keyboard.v", "key.keyboard.c")
        );
        let after = fix_key_clashes(before.as_bytes(), &clashes);
        let after = String::from_utf8(after).unwrap();
        let changed: Vec<(&str, &str)> = before
            .split("\r\n")
            .zip(after.split("\r\n"))
            .filter(|(old, new)| old != new)
            .collect();
        assert_eq!(
            changed,
            [
                (
                    "key_key.gammautils.gammaToggle:key.keyboard.g",
                    "key_key.gammautils.gammaToggle:key.keyboard.x"
                ),
                (
                    "key_iris.keybind.reload:key.keyboard.r",
                    "key_iris.keybind.reload:key.keyboard.unknown"
                ),
                (
                    "key_key.hide_icons:key.keyboard.h",
                    "key_key.hide_icons:key.keyboard.unknown"
                ),
                (
                    "key_key.wi_zoom.zoom:key.keyboard.v",
                    "key_key.wi_zoom.zoom:key.keyboard.c"
                ),
            ]
        );
        // Same lines, same endings, nothing else: push to talk stays unbound (it is no clash).
        assert_eq!(
            before.matches("\r\n").count(),
            after.matches("\r\n").count()
        );
        assert!(after.contains("key_key.push_to_talk:key.keyboard.unknown\r\n"));
        assert!(find_key_clashes(after.as_bytes()).is_empty());
        // Undo right after: the old file byte for byte.
        assert_eq!(
            undo_key_fix(after.as_bytes(), before.as_bytes()),
            before.as_bytes()
        );
    }

    #[test]
    fn keys_the_player_changed_never_count() {
        let base = "key_key.wi_zoom.zoom:key.keyboard.v\nkey_key.voice_chat:key.keyboard.v\n";
        assert_eq!(
            clash_names(&find_key_clashes(base.as_bytes())),
            ["key.wi_zoom.zoom"]
        );
        for file in [
            // Zoom moved by the player.
            "key_key.wi_zoom.zoom:key.keyboard.b\nkey_key.voice_chat:key.keyboard.v\n",
            // The voice chat menu moved by the player.
            "key_key.wi_zoom.zoom:key.keyboard.v\nkey_key.voice_chat:key.keyboard.k\n",
            // Both moved to one key of the player's choice: not the clash the table ends.
            "key_key.wi_zoom.zoom:key.keyboard.k\nkey_key.voice_chat:key.keyboard.k\n",
            // A side without a line (the mod did not load the last time the game saved).
            "key_key.wi_zoom.zoom:key.keyboard.v\n",
            "key_key.voice_chat:key.keyboard.v\n",
            // Not quite the key (the game would not read it either).
            "key_key.wi_zoom.zoom:key.keyboard.v \nkey_key.voice_chat:key.keyboard.v\n",
            // Keys as numbers (before 1.13).
            "key_key.wi_zoom.zoom:47\nkey_key.voice_chat:47\n",
            "",
        ] {
            assert!(find_key_clashes(file.as_bytes()).is_empty(), "{file:?}");
            assert_eq!(fix_key_clashes(file.as_bytes(), &[]), file.as_bytes());
        }
        // The last line of a mapping counts, as in the game.
        let twice = format!("{base}key_key.wi_zoom.zoom:key.keyboard.b\n");
        assert!(find_key_clashes(twice.as_bytes()).is_empty());
        let back = format!("key_key.wi_zoom.zoom:key.keyboard.b\n{base}");
        let clashes = find_key_clashes(back.as_bytes());
        assert_eq!(clash_names(&clashes), ["key.wi_zoom.zoom"]);
        // Only the copy still on V moves.
        assert_eq!(
            fix_key_clashes(back.as_bytes(), &clashes),
            b"key_key.wi_zoom.zoom:key.keyboard.b\nkey_key.wi_zoom.zoom:key.keyboard.c\nkey_key.voice_chat:key.keyboard.v\n"
        );
    }

    #[test]
    fn a_taken_new_key_is_not_offered() {
        let base = "key_key.wi_zoom.zoom:key.keyboard.v\nkey_key.voice_chat:key.keyboard.v\n\
                    key_key.gammautils.gammaToggle:key.keyboard.g\nkey_key.quickActions:key.keyboard.g\n";
        // The game's own defaults on C and X (creative hotbar, F3, Replay Mod) are no clash.
        let defaults = format!(
            "{base}key_key.saveToolbarActivator:key.keyboard.c\nkey_key.loadToolbarActivator:key.keyboard.x\n\
             key_key.debug.copyLocation:key.keyboard.c\nkey_key.replaymod.keyframerepository:key.keyboard.x\n"
        );
        assert_eq!(
            clash_names(&find_key_clashes(defaults.as_bytes())),
            ["key.wi_zoom.zoom", "key.gammautils.gammaToggle"]
        );
        // The player put the chat on C: zoom stays where it is, gamma still moves to X.
        let chat_on_c = format!("{defaults}key_key.chat:key.keyboard.c\n");
        assert_eq!(
            clash_names(&find_key_clashes(chat_on_c.as_bytes())),
            ["key.gammautils.gammaToggle"]
        );
        // Unbinding never takes a key: a free H is not needed to leave hide_icons without one.
        let hide = "key_key.hide_icons:key.keyboard.h\nkey_key.gammautils.nightVisionToggle:key.keyboard.h\n\
                    key_key.inventory:key.keyboard.unknown\n";
        assert_eq!(
            clash_names(&find_key_clashes(hide.as_bytes())),
            ["key.hide_icons"]
        );
    }

    #[test]
    fn undo_after_the_game_saved_moves_back_only_the_fixed_lines() {
        let before = game_file();
        let fixed = fix_key_clashes(before.as_bytes(), &find_key_clashes(before.as_bytes()));
        // The game saved since: another option changed and the player moved gamma again.
        let saved = String::from_utf8(fixed)
            .unwrap()
            .replace("fov:0.0", "fov:0.5")
            .replace(
                "key_key.gammautils.gammaToggle:key.keyboard.x",
                "key_key.gammautils.gammaToggle:key.keyboard.j",
            );
        let undone = String::from_utf8(undo_key_fix(saved.as_bytes(), before.as_bytes())).unwrap();
        assert!(undone.contains("fov:0.5\r\n"));
        assert!(undone.contains("key_key.wi_zoom.zoom:key.keyboard.v\r\n"));
        assert!(undone.contains("key_key.hide_icons:key.keyboard.h\r\n"));
        assert!(undone.contains("key_iris.keybind.reload:key.keyboard.r\r\n"));
        // The player's own later choice stays.
        assert!(undone.contains("key_key.gammautils.gammaToggle:key.keyboard.j\r\n"));
        let changed = saved
            .split("\r\n")
            .zip(undone.split("\r\n"))
            .filter(|(a, b)| a != b)
            .count();
        assert_eq!(changed, 3);
    }

    #[test]
    fn backup_names_are_checked() {
        assert_eq!(
            backup_order("options-20261019-231500.txt"),
            Some(("20261019-231500", 1))
        );
        assert_eq!(
            backup_order("options-20261019-231500-12.txt"),
            Some(("20261019-231500", 12))
        );
        for name in [
            "options.txt",
            "options-20261019.txt",
            "options-2026101-231500.txt",
            "options-20261019-231500-.txt",
            "options-20261019-231500-1234.txt",
            "options-20261019-231500-2-3.txt",
            "../options-20261019-231500.txt",
            "options-20261019-231500.txt/..",
            "options-2026a019-231500.txt",
        ] {
            assert_eq!(backup_order(name), None, "{name}");
        }
        // Same second: the counter orders them.
        assert!(
            backup_order("options-20261019-231500.txt")
                < backup_order("options-20261019-231500-2.txt")
        );
    }

    /// An instance folder with a game folder holding `options`.
    fn instance(options: &str) -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("minecraft");
        std::fs::create_dir_all(&game).unwrap();
        std::fs::write(game.join(OPTIONS_FILE), options).unwrap();
        (dir, game)
    }

    fn backups(game: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(backups_dir(game).unwrap())
            .map(|entries| {
                entries
                    .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
                    .collect()
            })
            .unwrap_or_default();
        names.sort();
        names
    }

    #[tokio::test]
    async fn fix_and_undo_on_disk() {
        let before = game_file();
        let (dir, game) = instance(&before);
        let path = game.join(OPTIONS_FILE);
        assert_eq!(key_clashes_in(&game).await.len(), 4);
        let fix = fix_key_clashes_in(&game).await.unwrap().unwrap();
        assert_eq!(fix.clashes.len(), 4);
        // The backup sits beside the game folder, not in it, and holds the old file.
        assert_eq!(backups(&game), std::slice::from_ref(&fix.backup));
        let backup = dir
            .path()
            .join(".launcher")
            .join(BACKUP_DIR)
            .join(&fix.backup);
        assert_eq!(std::fs::read(&backup).unwrap(), before.as_bytes());
        let fixed = std::fs::read(&path).unwrap();
        assert_eq!(fixed, fix_key_clashes(before.as_bytes(), &fix.clashes));
        assert!(key_clashes_in(&game).await.is_empty());
        // Nothing left to fix: no new backup.
        assert_eq!(fix_key_clashes_in(&game).await.unwrap(), None);
        assert_eq!(backups(&game).len(), 1);
        // No temporary files in the game folder.
        let names: Vec<String> = std::fs::read_dir(&game)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [OPTIONS_FILE]);

        assert!(undo_key_fix_in(&game, &fix.backup).await.unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), before.as_bytes());
        assert!(backups(&game).is_empty());
        // A second undo finds no backup and changes nothing.
        assert!(!undo_key_fix_in(&game, &fix.backup).await.unwrap());
        assert_eq!(std::fs::read(&path).unwrap(), before.as_bytes());
        // Only backup names are accepted.
        for name in ["../minecraft/options.txt", "options.txt", ""] {
            assert!(undo_key_fix_in(&game, name).await.is_err(), "{name}");
        }
    }

    #[tokio::test]
    async fn no_file_no_fix_and_old_backups_go() {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("minecraft");
        std::fs::create_dir_all(&game).unwrap();
        assert!(key_clashes_in(&game).await.is_empty());
        assert_eq!(fix_key_clashes_in(&game).await.unwrap(), None);
        assert!(!dir.path().join(".launcher").exists());

        let before = game_file();
        let mut made = Vec::new();
        for _ in 0..KEPT_BACKUPS + 2 {
            std::fs::write(game.join(OPTIONS_FILE), &before).unwrap();
            made.push(fix_key_clashes_in(&game).await.unwrap().unwrap().backup);
        }
        let kept = backups(&game);
        assert_eq!(kept.len(), KEPT_BACKUPS);
        // The newest stay (made within a second or two: the counter orders them).
        let mut newest = made.clone();
        newest.sort_by(|a, b| backup_order(a).cmp(&backup_order(b)));
        let mut expected = newest[newest.len() - KEPT_BACKUPS..].to_vec();
        expected.sort();
        assert_eq!(kept, expected);
        assert!(kept.contains(made.last().unwrap()));
    }

    #[tokio::test]
    async fn undo_brings_back_a_file_that_went() {
        let before = game_file();
        let (_dir, game) = instance(&before);
        let fix = fix_key_clashes_in(&game).await.unwrap().unwrap();
        std::fs::remove_file(game.join(OPTIONS_FILE)).unwrap();
        assert!(undo_key_fix_in(&game, &fix.backup).await.unwrap());
        assert_eq!(
            std::fs::read(game.join(OPTIONS_FILE)).unwrap(),
            before.as_bytes()
        );
    }
}
