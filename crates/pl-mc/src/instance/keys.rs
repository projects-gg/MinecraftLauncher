//! Key bindings a new `options.txt` of the official pack starts with, and the one-tap fix for an
//! older file that still has the clashes. The desktop launcher uses the same table for an instance
//! with ready-made sets, which bring the pack's WI Zoom and Gamma Utils.
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
//! "Geri al" ([`undo_key_fix_in`]). Along with them it gives a push to talk the file still leaves
//! unbound its key (KEYS3, [`find_unbound_keys`]); that alone never makes a fix.
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

/// A row of `data/key-bindings.json`: the binding, and whether the fix also binds it while a file
/// leaves it unbound (`bindIfUnbound`, KEYS3: [`find_unbound_keys`]). The flag stays out of
/// [`KeyBinding`], whose fields other crates spell out.
#[derive(Deserialize)]
struct Row {
    #[serde(flatten)]
    binding: KeyBinding,
    #[serde(default, rename = "bindIfUnbound")]
    bind_if_unbound: bool,
}

#[derive(Deserialize)]
struct Table {
    schema: u32,
    bindings: Vec<Row>,
}

/// The table as loaded: its bindings and, in the same order, their `bindIfUnbound`.
#[derive(Default)]
struct Loaded {
    bindings: Vec<KeyBinding>,
    bind_if_unbound: Vec<bool>,
}

static TABLE: LazyLock<Loaded> =
    LazyLock::new(|| match serde_json::from_str::<Table>(TABLE_JSON) {
        Ok(table) if table.schema == TABLE_SCHEMA => {
            let (bindings, bind_if_unbound): (Vec<KeyBinding>, Vec<bool>) = table
                .bindings
                .into_iter()
                .map(|row| (row.binding, row.bind_if_unbound))
                .unzip();
            Loaded {
                bindings,
                bind_if_unbound,
            }
        }
        Ok(table) => {
            tracing::error!(
                schema = table.schema,
                "key binding table has an unknown schema"
            );
            Loaded::default()
        }
        Err(error) => {
            tracing::error!(%error, "key binding table is unreadable");
            Loaded::default()
        }
    });

/// The official pack's bindings (`data/key-bindings.json`).
pub fn official_pack_bindings() -> &'static [KeyBinding] {
    &TABLE.bindings
}

/// The table's bindings with `bindIfUnbound` (push to talk).
fn bind_if_unbound() -> impl Iterator<Item = &'static KeyBinding> {
    let table: &'static Loaded = &TABLE;
    table
        .bindings
        .iter()
        .zip(&table.bind_if_unbound)
        .filter(|(_, bind)| **bind)
        .map(|(binding, _)| binding)
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

/// Whether another mapping that acts in normal play already has the table's key for `binding` in
/// `keys`: giving it that key would only make a new clash. Leaving a binding unbound takes none.
fn key_taken(keys: &HashMap<&str, &[u8]>, binding: &KeyBinding) -> bool {
    binding.key != UNBOUND
        && keys.iter().any(|(name, key)| {
            *name != binding.name && *key == binding.key.as_bytes() && acts_in_play(name)
        })
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
            (on_default(&binding.name) && on_default(partner) && !key_taken(&keys, binding))
                .then_some(KeyClash { binding, partner })
        })
        .collect()
}

/// The table's bindings with `bindIfUnbound` (push to talk) that `options` leaves unbound,
/// exactly [`UNBOUND`], while no other mapping that acts in normal play has their key: "Düzelt"
/// gives them that key along with the clashes it ends (KEYS3). Such a binding has no default key,
/// so an unbound line is the mod's default, not the player's choice. A mapping without a line
/// gets none (its mod did not load the last time the game saved), and a key the player gave it
/// never changes.
pub fn find_unbound_keys(options: &[u8]) -> Vec<&'static KeyBinding> {
    let keys = key_lines(options);
    bind_if_unbound()
        .filter(|binding| {
            keys.get(binding.name.as_str())
                .is_some_and(|key| *key == UNBOUND.as_bytes())
                && !key_taken(&keys, binding)
        })
        .collect()
}

/// What "Düzelt" changes in an `options.txt` ([`plan_key_fix`]).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct KeyFixPlan {
    /// The clashes it ends ([`find_key_clashes`]).
    pub clashes: Vec<KeyClash>,
    /// The bindings it gives their key along with them while the file leaves them unbound
    /// ([`find_unbound_keys`]: push to talk).
    pub binds: Vec<&'static KeyBinding>,
}

impl KeyFixPlan {
    /// Nothing to change.
    pub fn is_empty(&self) -> bool {
        self.clashes.is_empty() && self.binds.is_empty()
    }

    /// Its line changes as (mapping, from, to).
    fn moves(&self) -> Vec<(&'static str, &'static str, &'static str)> {
        let clashes = self.clashes.iter().map(|clash| {
            let binding: &'static KeyBinding = clash.binding;
            (binding.name.as_str(), clash.key(), clash.to())
        });
        let binds = self
            .binds
            .iter()
            .map(|&binding| (binding.name.as_str(), UNBOUND, binding.key.as_str()));
        clashes.chain(binds).collect()
    }
}

/// What "Düzelt" changes in `options`: its clashes and, along with them, the unbound bindings
/// that get their key. Nothing when nothing clashes: an unbound push to talk alone is no reason
/// to touch the file (KEYS3; the mod check and the question before a launch are about clashes).
pub fn plan_key_fix(options: &[u8]) -> KeyFixPlan {
    let clashes = find_key_clashes(options);
    if clashes.is_empty() {
        return KeyFixPlan::default();
    }
    KeyFixPlan {
        clashes,
        binds: find_unbound_keys(options),
    }
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

/// `options` with `plan` applied: the binding of each clash moved to the table's key, and each
/// unbound binding of it given its key. Only those lines change.
pub fn apply_key_fix(options: &[u8], plan: &KeyFixPlan) -> Vec<u8> {
    move_keys(options, &plan.moves())
}

/// Undoes [`apply_key_fix`] on `current`, given the file as it was before (`backup`). When
/// nothing else changed since, the old file comes back byte for byte; when the game saved the
/// file meanwhile, only the lines still on the table's keys go back to their old key (push to
/// talk to unbound).
pub fn undo_key_fix(current: &[u8], backup: &[u8]) -> Vec<u8> {
    let plan = plan_key_fix(backup);
    if apply_key_fix(backup, &plan) == current {
        return backup.to_vec();
    }
    let back: Vec<(&str, &str, &str)> = plan
        .moves()
        .into_iter()
        .map(|(name, from, to)| (name, to, from))
        .collect();
    move_keys(current, &back)
}

/// [`plan_key_fix`] of `<game_dir>/options.txt`; nothing when there is no such file or it cannot
/// be read.
pub async fn plan_key_fix_in(game_dir: &Path) -> KeyFixPlan {
    match tokio::fs::read(game_dir.join(OPTIONS_FILE)).await {
        Ok(options) => plan_key_fix(&options),
        Err(_) => KeyFixPlan::default(),
    }
}

/// What [`fix_key_clashes_in`] did.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct KeyFix {
    /// The backup's file name, for [`undo_key_fix_in`].
    pub backup: String,
    /// The clashes it ended.
    pub clashes: Vec<KeyClash>,
    /// The unbound bindings it gave their key along with them (push to talk).
    pub binds: Vec<&'static KeyBinding>,
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

/// Keeps `options` (the instance's `options.txt` before a change) as a new backup and drops the
/// oldest beyond [`KEPT_BACKUPS`]; returns the backup's name. Key fixes and graphics switches
/// share the folder and the limit.
pub(super) async fn save_backup(game_dir: &Path, options: &[u8]) -> Result<String> {
    let dir = backups_dir(game_dir)?;
    let backup = new_backup_name(&dir).await;
    write_durable(&dir.join(&backup), options).await?;
    prune_backups(&dir).await;
    Ok(backup)
}

/// The content of the backup named `backup`; `None` when it is gone. Any other name is refused.
pub(super) async fn read_backup(game_dir: &Path, backup: &str) -> Result<Option<Vec<u8>>> {
    if backup_order(backup).is_none() {
        return Err(Error::Invalid(format!(
            "not an options.txt backup: {backup}"
        )));
    }
    let file = backups_dir(game_dir)?.join(backup);
    match tokio::fs::read(&file).await {
        Ok(saved) => Ok(Some(saved)),
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(error) => Err(Error::io(&file, error)),
    }
}

/// Deletes a backup once it is used. Failures only leave the file (the oldest go anyway).
pub(super) async fn remove_backup(game_dir: &Path, backup: &str) {
    let Ok(dir) = backups_dir(game_dir) else {
        return;
    };
    if let Err(error) = tokio::fs::remove_file(dir.join(backup)).await {
        tracing::warn!(%error, backup = %backup, "could not remove a used options.txt backup");
    }
}

/// Backs up `<game_dir>/options.txt` and moves the bindings of its clashes
/// ([`find_key_clashes`]) to the table's keys, giving an unbound push to talk its key along
/// ([`plan_key_fix`]); nothing else in the file changes. `None` when nothing clashes (or there is
/// no file). The game must not be running: it writes its own copy of every key back.
pub async fn fix_key_clashes_in(game_dir: &Path) -> Result<Option<KeyFix>> {
    let path = game_dir.join(OPTIONS_FILE);
    let options = match tokio::fs::read(&path).await {
        Ok(options) => options,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(Error::io(&path, error)),
    };
    let plan = plan_key_fix(&options);
    if plan.is_empty() {
        return Ok(None);
    }
    let backup = save_backup(game_dir, &options).await?;
    write_options(&path, apply_key_fix(&options, &plan)).await?;
    tracing::info!(
        clashes = plan.clashes.len(),
        binds = plan.binds.len(),
        backup = %backup,
        "options.txt key clashes fixed"
    );
    let KeyFixPlan { clashes, binds } = plan;
    Ok(Some(KeyFix {
        backup,
        clashes,
        binds,
    }))
}

/// Undoes a fix of [`fix_key_clashes_in`] from its backup ([`undo_key_fix`]; a file that is gone
/// meanwhile comes back whole). The backup goes once it is used. `false` when that backup no
/// longer exists: nothing changes then.
pub async fn undo_key_fix_in(game_dir: &Path, backup: &str) -> Result<bool> {
    let Some(saved) = read_backup(game_dir, backup).await? else {
        return Ok(false);
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
    remove_backup(game_dir, backup).await;
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
        // KEYS3: a binding the fix binds while unbound has no default key (an unbound line is the
        // mod's default, not the player's choice), gets one, and ends no clash.
        assert_eq!(TABLE.bind_if_unbound.len(), table.len());
        let bound: Vec<&str> = bind_if_unbound()
            .map(|binding| binding.name.as_str())
            .collect();
        assert_eq!(bound, ["key.push_to_talk"]);
        for binding in bind_if_unbound() {
            assert_eq!(binding.default, UNBOUND, "{binding:?}");
            assert_ne!(binding.key, UNBOUND, "{binding:?}");
            assert_eq!(binding.clashes_with, None, "{binding:?}");
        }
    }

    #[test]
    fn the_table_file_stays_as_the_android_app_copies_it() {
        // The phone app keeps a byte copy (assets/key-bindings.json): LF only, two-space indent,
        // `bindIfUnbound` right after `default`.
        assert!(!TABLE_JSON.contains('\r'));
        assert!(TABLE_JSON.starts_with("{\n  \"schema\": 1,\n"));
        assert!(TABLE_JSON.ends_with("\n  ]\n}\n"));
        assert!(TABLE_JSON.contains(
            "      \"default\": \"key.keyboard.unknown\",\n      \"bindIfUnbound\": true,\n"
        ));
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

    fn bound_names(binds: &[&KeyBinding]) -> Vec<String> {
        binds.iter().map(|binding| binding.name.clone()).collect()
    }

    /// The lines that differ between two files of the same lines (CRLF).
    fn changed_lines<'a>(before: &'a str, after: &'a str) -> Vec<(&'a str, &'a str)> {
        assert_eq!(
            before.matches("\r\n").count(),
            after.matches("\r\n").count()
        );
        before
            .split("\r\n")
            .zip(after.split("\r\n"))
            .filter(|(old, new)| old != new)
            .collect()
    }

    #[test]
    fn a_file_from_before_the_table_gets_only_its_clashing_lines_moved() {
        let before = game_file();
        let plan = plan_key_fix(before.as_bytes());
        let clashes = &plan.clashes;
        assert_eq!(clashes, &find_key_clashes(before.as_bytes()));
        assert_eq!(
            clash_names(clashes),
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
        // Push to talk is no clash, but the pack leaves it unbound and Caps Lock free: it comes
        // along (KEYS3).
        assert_eq!(bound_names(&plan.binds), ["key.push_to_talk"]);
        let after = String::from_utf8(apply_key_fix(before.as_bytes(), &plan)).unwrap();
        // Same lines, same endings (CRLF), every other byte the same.
        assert_eq!(after.matches('\n').count(), after.matches("\r\n").count());
        assert_eq!(
            changed_lines(&before, &after),
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
                    "key_key.push_to_talk:key.keyboard.unknown",
                    "key_key.push_to_talk:key.keyboard.caps.lock"
                ),
                (
                    "key_key.wi_zoom.zoom:key.keyboard.v",
                    "key_key.wi_zoom.zoom:key.keyboard.c"
                ),
            ]
        );
        assert!(plan_key_fix(after.as_bytes()).is_empty());
        assert!(find_unbound_keys(after.as_bytes()).is_empty());
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
            let plan = plan_key_fix(file.as_bytes());
            assert!(plan.is_empty(), "{file:?}");
            assert_eq!(apply_key_fix(file.as_bytes(), &plan), file.as_bytes());
        }
        // The last line of a mapping counts, as in the game.
        let twice = format!("{base}key_key.wi_zoom.zoom:key.keyboard.b\n");
        assert!(find_key_clashes(twice.as_bytes()).is_empty());
        let back = format!("key_key.wi_zoom.zoom:key.keyboard.b\n{base}");
        let plan = plan_key_fix(back.as_bytes());
        assert_eq!(clash_names(&plan.clashes), ["key.wi_zoom.zoom"]);
        // Only the copy still on V moves.
        assert_eq!(
            apply_key_fix(back.as_bytes(), &plan),
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
        let fixed = apply_key_fix(before.as_bytes(), &plan_key_fix(before.as_bytes()));
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
        assert!(undone.contains("key_key.push_to_talk:key.keyboard.unknown\r\n"));
        // The player's own later choice stays.
        assert!(undone.contains("key_key.gammautils.gammaToggle:key.keyboard.j\r\n"));
        assert_eq!(changed_lines(&saved, &undone).len(), 4);
    }

    // --- KEYS3 ---------------------------------------------------------------------------------

    /// Zoom and the voice chat menu both on V: a clash, so "Düzelt" is offered.
    const ZOOM_ON_V: &str =
        "key_key.wi_zoom.zoom:key.keyboard.v\nkey_key.voice_chat:key.keyboard.v\n";

    #[test]
    fn an_unbound_push_to_talk_gets_caps_lock_along_with_the_fix() {
        let file = format!(
            "{ZOOM_ON_V}key_key.push_to_talk:key.keyboard.unknown\nkey_key.sprint:key.keyboard.left.control\n"
        );
        let plan = plan_key_fix(file.as_bytes());
        assert_eq!(clash_names(&plan.clashes), ["key.wi_zoom.zoom"]);
        assert_eq!(bound_names(&plan.binds), ["key.push_to_talk"]);
        assert_eq!(
            String::from_utf8(apply_key_fix(file.as_bytes(), &plan)).unwrap(),
            "key_key.wi_zoom.zoom:key.keyboard.c\nkey_key.voice_chat:key.keyboard.v\n\
             key_key.push_to_talk:key.keyboard.caps.lock\nkey_key.sprint:key.keyboard.left.control\n"
        );
        // Mappings that act only outside normal play may share Caps Lock (F3 combinations,
        // creative mode's hotbar, Replay Mod).
        let outside = format!(
            "{file}key_key.debug.reloadChunk:key.keyboard.caps.lock\nkey_key.saveToolbarActivator:key.keyboard.caps.lock\n\
             key_key.replaymod.marker:key.keyboard.caps.lock\n"
        );
        assert_eq!(
            bound_names(&plan_key_fix(outside.as_bytes()).binds),
            ["key.push_to_talk"]
        );
    }

    #[test]
    fn push_to_talk_keeps_a_key_it_has_and_gets_no_line_or_taken_key() {
        for (file, why) in [
            (
                ZOOM_ON_V.to_owned(),
                "no line: the mod did not load, none is added",
            ),
            (
                format!("{ZOOM_ON_V}key_key.push_to_talk:key.keyboard.k\n"),
                "the player's key",
            ),
            (
                format!("{ZOOM_ON_V}key_key.push_to_talk:key.keyboard.caps.lock\n"),
                "already Caps Lock",
            ),
            (
                format!("{ZOOM_ON_V}key_key.push_to_talk:key.keyboard.unknown \n"),
                "not quite unbound (the game would not read it either)",
            ),
            (
                format!(
                    "{ZOOM_ON_V}key_key.push_to_talk:key.keyboard.unknown\nkey_key.push_to_talk:key.keyboard.k\n"
                ),
                "the last line counts, as in the game",
            ),
            (
                format!(
                    "{ZOOM_ON_V}key_key.push_to_talk:key.keyboard.unknown\nkey_key.sprint:key.keyboard.caps.lock\n"
                ),
                "Caps Lock sprints",
            ),
            (
                format!(
                    "{ZOOM_ON_V}key_key.push_to_talk:key.keyboard.unknown\nkey_key.voice_chat_group:key.keyboard.caps.lock\n"
                ),
                "Caps Lock opens a mod's screen",
            ),
        ] {
            assert!(find_unbound_keys(file.as_bytes()).is_empty(), "{why}");
            let plan = plan_key_fix(file.as_bytes());
            assert_eq!(clash_names(&plan.clashes), ["key.wi_zoom.zoom"], "{why}");
            assert!(plan.binds.is_empty(), "{why}");
            // Only the clash's line changes.
            assert_eq!(
                String::from_utf8(apply_key_fix(file.as_bytes(), &plan)).unwrap(),
                file.replacen(
                    "key_key.wi_zoom.zoom:key.keyboard.v",
                    "key_key.wi_zoom.zoom:key.keyboard.c",
                    1
                ),
                "{why}"
            );
        }
    }

    #[test]
    fn an_unbound_push_to_talk_alone_makes_no_fix() {
        // The mod check and the question before a launch are about clashes: nothing clashes, so
        // nothing changes, though push to talk is unbound and Caps Lock free.
        let file = "key_key.wi_zoom.zoom:key.keyboard.c\nkey_key.voice_chat:key.keyboard.v\n\
                    key_key.push_to_talk:key.keyboard.unknown\n";
        assert_eq!(
            bound_names(&find_unbound_keys(file.as_bytes())),
            ["key.push_to_talk"]
        );
        let plan = plan_key_fix(file.as_bytes());
        assert_eq!(plan, KeyFixPlan::default());
        assert!(plan.is_empty());
        assert_eq!(apply_key_fix(file.as_bytes(), &plan), file.as_bytes());
    }

    #[test]
    fn undo_gives_push_to_talk_back_its_old_state() {
        let before = game_file();
        let fixed = apply_key_fix(before.as_bytes(), &plan_key_fix(before.as_bytes()));
        // Right after: the old file byte for byte.
        assert_eq!(undo_key_fix(&fixed, before.as_bytes()), before.as_bytes());
        // The game saved since: push to talk, still on Caps Lock, is unbound again ...
        let saved = String::from_utf8(fixed)
            .unwrap()
            .replace("fov:0.0", "fov:0.5");
        let undone = String::from_utf8(undo_key_fix(saved.as_bytes(), before.as_bytes())).unwrap();
        assert_eq!(undone, before.replace("fov:0.0", "fov:0.5"));
        // ... unless the player gave it a key of their own meanwhile.
        let rebound = saved.replace(
            "key_key.push_to_talk:key.keyboard.caps.lock",
            "key_key.push_to_talk:key.keyboard.b",
        );
        let undone =
            String::from_utf8(undo_key_fix(rebound.as_bytes(), before.as_bytes())).unwrap();
        assert_eq!(
            changed_lines(&before, &undone),
            [
                ("fov:0.0", "fov:0.5"),
                (
                    "key_key.push_to_talk:key.keyboard.unknown",
                    "key_key.push_to_talk:key.keyboard.b"
                ),
            ]
        );
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
        let plan = plan_key_fix_in(&game).await;
        assert_eq!((plan.clashes.len(), plan.binds.len()), (4, 1));
        let fix = fix_key_clashes_in(&game).await.unwrap().unwrap();
        assert_eq!(fix.clashes, plan.clashes);
        assert_eq!(bound_names(&fix.binds), ["key.push_to_talk"]);
        // The backup sits beside the game folder, not in it, and holds the old file.
        assert_eq!(backups(&game), std::slice::from_ref(&fix.backup));
        let backup = dir
            .path()
            .join(".launcher")
            .join(BACKUP_DIR)
            .join(&fix.backup);
        assert_eq!(std::fs::read(&backup).unwrap(), before.as_bytes());
        let fixed = std::fs::read(&path).unwrap();
        assert_eq!(fixed, apply_key_fix(before.as_bytes(), &plan));
        assert!(plan_key_fix_in(&game).await.is_empty());
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
        assert!(plan_key_fix_in(&game).await.is_empty());
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
    async fn an_unbound_push_to_talk_alone_leaves_the_file_on_disk() {
        let file =
            "key_key.voice_chat:key.keyboard.v\r\nkey_key.push_to_talk:key.keyboard.unknown\r\n";
        let (dir, game) = instance(file);
        assert!(plan_key_fix_in(&game).await.is_empty());
        assert_eq!(fix_key_clashes_in(&game).await.unwrap(), None);
        assert_eq!(
            std::fs::read(game.join(OPTIONS_FILE)).unwrap(),
            file.as_bytes()
        );
        assert!(!dir.path().join(".launcher").exists());
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
