//! Graphics presets in `options.txt`: "Akıcı", "Dengeli" and "Güzel".
//!
//! A preset sets a few video options, with the keys the instance's Minecraft version reads
//! ([`SETTINGS`]); every other line of the file stays exactly as it was (unknown keys, their
//! order, line endings, a file linked from elsewhere). The launcher writes one only when the
//! player picks it ([`set_graphics_preset`]; [`switch_graphics_preset`] keeps a copy first, for
//! "Geri al"), or into a brand-new `options.txt` at the first
//! launch ([`super::seed_options`]): the preset the player picked before the game ever ran, or
//! the one that suits the computer ([`this_machine_preset`]). It never changes the player's own
//! settings by itself; values that match no preset are "Özel" ([`GraphicsState::Custom`]).
//!
//! Keys by version (the Minecraft Wiki's options.txt table): `renderDistance` and `mipmapLevels`
//! from 1.7.2; `renderClouds` (before: `clouds`) from 1.8; `entityShadows` from 1.8.1;
//! `biomeBlendRadius` from 1.13; `graphicsMode` (before: `fancyGraphics`) and
//! `entityDistanceScaling` from 1.16; `simulationDistance` from 1.18; `cloudRange` from 1.21.6.
//! 1.21.11 replaced `graphicsMode` with `graphicsPreset` and the options it used to cover
//! (`cutoutLeaves`, `improvedTransparency`, `weatherRadius`). From 1.19 on, enum values are JSON
//! strings (`renderClouds:"fast"`); older versions read them bare.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use serde::{Deserialize, Serialize};

use super::defaults::{OPTIONS_FILE, at_least};
use super::memory::system_memory_mb;
use crate::error::{Error, Result};
use crate::gamefiles::fsops::blocking;
use crate::paths::{write_atomic, write_durable_blocking};

/// File in the instance's launcher folder that keeps a preset picked before the first launch.
const PENDING_FILE: &str = "graphics-preset";

/// A graphics preset the launcher can write into `options.txt`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum GraphicsPreset {
    /// "Akıcı": the most frames on a weak laptop (about the game's Fast, clouds off).
    Smooth,
    /// "Dengeli": the game's usual view distance with lighter effects.
    Balanced,
    /// "Güzel": farther and richer, for a capable computer (about the game's Fancy).
    Quality,
}

impl GraphicsPreset {
    pub const ALL: [Self; 3] = [Self::Smooth, Self::Balanced, Self::Quality];

    /// The name used in files and commands.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Smooth => "smooth",
            Self::Balanced => "balanced",
            Self::Quality => "quality",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        Self::ALL.into_iter().find(|preset| preset.as_str() == text)
    }

    fn index(self) -> usize {
        match self {
            Self::Smooth => 0,
            Self::Balanced => 1,
            Self::Quality => 2,
        }
    }
}

/// What an instance's `options.txt` says about graphics.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum GraphicsState {
    /// The file has this preset's values.
    Preset { preset: GraphicsPreset },
    /// The file has values of its own ("Özel"): changed in the game, a modpack's or the game's
    /// defaults.
    Custom,
    /// No `options.txt` yet: the first launch writes this preset (the one the player picked, or
    /// the computer's).
    FirstLaunch { preset: GraphicsPreset },
    /// A Minecraft version before 1.7.2, whose options presets do not cover.
    Unsupported,
}

/// A value in `options.txt`.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Value {
    Int(i32),
    Bool(bool),
    /// A slider, written with a decimal point (`0.75`, `1.0`).
    Float(f64),
    /// An enum value: `"fast"` from 1.19 on, `fast` before.
    Text(&'static str),
}

use Value::{Bool, Float, Int, Text};

/// A Minecraft version: a release and the first weekly snapshot (`YYwWWa`) on the way to it.
/// Year-numbered versions (`26.1`) come after every gate.
#[derive(Debug, Clone, Copy)]
struct Gate {
    release: (u32, u32, u32),
    snapshot: (u32, u32),
}

impl Gate {
    const fn new(release: (u32, u32, u32), snapshot: (u32, u32)) -> Self {
        Self { release, snapshot }
    }

    fn reached_by(self, minecraft: &str) -> bool {
        at_least(minecraft, self.release, self.snapshot)
    }
}

/// `renderDistance` and `mipmapLevels`: the oldest version presets cover.
const V1_7_2: Gate = Gate::new((1, 7, 2), (13, 38));
const V1_8: Gate = Gate::new((1, 8, 0), (14, 25));
/// 1.8.1-pre4; the 1.9 snapshots have it.
const V1_8_1: Gate = Gate::new((1, 8, 1), (15, 31));
const V1_13: Gate = Gate::new((1, 13, 0), (18, 15));
/// 1.16-pre1 (`graphicsMode`); the next snapshots belong to 1.16.2.
const V1_16: Gate = Gate::new((1, 16, 0), (20, 27));
const V1_16_ENTITIES: Gate = Gate::new((1, 16, 0), (20, 17));
const V1_18: Gate = Gate::new((1, 18, 0), (21, 38));
/// Options became JSON values with 1.19's option rework.
const ENUMS_QUOTED: Gate = Gate::new((1, 19, 0), (22, 11));
const V1_21_6: Gate = Gate::new((1, 21, 6), (25, 17));
const V1_21_11: Gate = Gate::new((1, 21, 11), (25, 41));
const V1_21_11_LEAVES: Gate = Gate::new((1, 21, 11), (25, 44));

/// One option a preset sets: its key, its value in Akıcı, Dengeli and Güzel, and the versions
/// that read it.
#[derive(Debug, Clone, Copy)]
struct Setting {
    key: &'static str,
    values: [Value; 3],
    /// First version with the key (`None`: every version presets cover).
    since: Option<Gate>,
    /// First version that reads another key instead.
    until: Option<Gate>,
    /// Part of what makes a file match a preset.
    compared: bool,
}

impl Setting {
    const fn new(key: &'static str, values: [Value; 3]) -> Self {
        Self {
            key,
            values,
            since: None,
            until: None,
            compared: true,
        }
    }

    const fn since(mut self, gate: Gate) -> Self {
        self.since = Some(gate);
        self
    }

    const fn until(mut self, gate: Gate) -> Self {
        self.until = Some(gate);
        self
    }

    /// Written, but not looked at when matching a file to a preset.
    const fn label(mut self) -> Self {
        self.compared = false;
        self
    }
}

/// The presets, option by option.
const SETTINGS: [Setting; 16] = [
    Setting::new("renderDistance", [Int(8), Int(12), Int(16)]),
    Setting::new("simulationDistance", [Int(6), Int(8), Int(10)]).since(V1_18),
    // Fast or fancy graphics: one switch until 1.21.10, then separate options under a preset.
    Setting::new("fancyGraphics", [Bool(false), Bool(true), Bool(true)]).until(V1_16),
    Setting::new("graphicsMode", [Int(0), Int(1), Int(1)])
        .since(V1_16)
        .until(V1_21_11),
    // Only the name the game's video settings show; the values are none of its own presets.
    Setting::new("graphicsPreset", [Text("custom"); 3])
        .since(V1_21_11)
        .label(),
    Setting::new("cutoutLeaves", [Bool(false), Bool(true), Bool(true)]).since(V1_21_11_LEAVES),
    Setting::new("improvedTransparency", [Bool(false); 3]).since(V1_21_11),
    Setting::new("weatherRadius", [Int(5), Int(10), Int(10)]).since(V1_21_11),
    Setting::new("particles", [Int(1), Int(0), Int(0)]),
    Setting::new("clouds", [Bool(false), Bool(true), Bool(true)]).until(V1_8),
    Setting::new("renderClouds", [Text("false"), Text("fast"), Text("true")]).since(V1_8),
    Setting::new("cloudRange", [Int(32), Int(64), Int(64)]).since(V1_21_6),
    Setting::new("entityShadows", [Bool(false), Bool(true), Bool(true)]).since(V1_8_1),
    Setting::new(
        "entityDistanceScaling",
        [Float(0.75), Float(1.0), Float(1.0)],
    )
    .since(V1_16_ENTITIES),
    Setting::new("biomeBlendRadius", [Int(1), Int(2), Int(3)]).since(V1_13),
    Setting::new("mipmapLevels", [Int(2), Int(4), Int(4)]),
];

/// How a version relates to a setting's key.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Role {
    /// The version reads this key.
    Current,
    /// An older key of the option. Updated where a file still has it (the game may convert it
    /// when it upgrades the file), never added.
    Superseded,
    /// A key of later versions: left alone.
    Newer,
}

fn role(setting: &Setting, minecraft: &str) -> Role {
    if setting
        .since
        .is_some_and(|gate| !gate.reached_by(minecraft))
    {
        Role::Newer
    } else if setting.until.is_some_and(|gate| gate.reached_by(minecraft)) {
        Role::Superseded
    } else {
        Role::Current
    }
}

/// True when presets cover Minecraft `version` (1.7.2 and newer).
pub fn supports_presets(version: &str) -> bool {
    V1_7_2.reached_by(version)
}

/// The lines of `content` as (text, line ending); the ending is empty on a last line without one.
pub(super) fn split_lines(content: &[u8]) -> Vec<(&[u8], &[u8])> {
    let mut lines = Vec::new();
    let mut rest = content;
    while !rest.is_empty() {
        match rest.iter().position(|&byte| byte == b'\n') {
            Some(newline) => {
                let (line, tail) = rest.split_at(newline + 1);
                let text_end = if newline > 0 && line[newline - 1] == b'\r' {
                    newline - 1
                } else {
                    newline
                };
                lines.push((&line[..text_end], &line[text_end..]));
                rest = tail;
            }
            None => {
                lines.push((rest, &rest[rest.len()..]));
                rest = &[];
            }
        }
    }
    lines
}

/// `key:value`, split at the first colon like the game does (values can hold colons).
pub(super) fn key_value(text: &[u8]) -> Option<(&str, &[u8])> {
    let colon = text.iter().position(|&byte| byte == b':')?;
    let key = std::str::from_utf8(&text[..colon]).ok()?;
    (!key.is_empty()).then(|| (key, &text[colon + 1..]))
}

/// Every key's value; a key written twice counts with its last value, as in the game.
fn values_of(content: &[u8]) -> HashMap<&str, String> {
    let mut values = HashMap::new();
    for (text, _) in split_lines(content) {
        if let Some((key, value)) = key_value(text) {
            values.insert(key, String::from_utf8_lossy(value).trim().to_owned());
        }
    }
    values
}

fn unquote(text: &str) -> &str {
    text.strip_prefix('"')
        .and_then(|inner| inner.strip_suffix('"'))
        .unwrap_or(text)
}

fn matches(found: &str, expected: Value) -> bool {
    match expected {
        Int(number) => found
            .parse::<i64>()
            .is_ok_and(|value| value == i64::from(number)),
        Float(number) => found
            .parse::<f64>()
            .is_ok_and(|value| (value - number).abs() < 1e-6),
        Bool(flag) => found == if flag { "true" } else { "false" },
        Text(text) => unquote(found) == text,
    }
}

fn format(value: Value, quoted: bool) -> String {
    match value {
        Int(number) => number.to_string(),
        Bool(flag) => flag.to_string(),
        Float(number) if number.fract() == 0.0 => format!("{number:.1}"),
        Float(number) => number.to_string(),
        Text(text) if quoted => format!("\"{text}\""),
        Text(text) => text.to_owned(),
    }
}

/// The preset `content` (an `options.txt`) has for Minecraft `version`: every option the preset
/// sets must have the preset's value. `None` when the values are the player's own ("Özel"), when
/// some are missing (the game uses its defaults for those) or for versions presets do not cover.
pub fn detect_preset(content: &[u8], version: &str) -> Option<GraphicsPreset> {
    if !supports_presets(version) {
        return None;
    }
    let found = values_of(content);
    let compared: Vec<&Setting> = SETTINGS
        .iter()
        .filter(|setting| setting.compared && role(setting, version) == Role::Current)
        .collect();
    GraphicsPreset::ALL.into_iter().find(|preset| {
        compared.iter().all(|setting| {
            found
                .get(setting.key)
                .is_some_and(|value| matches(value, setting.values[preset.index()]))
        })
    })
}

/// `content` (an `options.txt`) with `preset` for Minecraft `version`. Lines of the preset's keys
/// get the new value where they are (every copy of a key), keys the file lacks are added at the
/// end, older keys of the same options are updated only where the file still has them. Every
/// other byte stays: unknown keys, the order, the line endings (added lines follow the file's).
/// Enum values are written the way the version reads them: 1.19 and newer take JSON strings
/// only (a bare `false` would fall back to the default), older versions take them bare.
pub fn apply_preset(content: &[u8], version: &str, preset: GraphicsPreset) -> Vec<u8> {
    if !supports_presets(version) {
        return content.to_vec();
    }
    let index = preset.index();
    let quoted = ENUMS_QUOTED.reached_by(version);
    let mut written: Vec<&str> = Vec::new();
    let mut crlf = false;
    let mut out = Vec::with_capacity(content.len() + 512);
    for (text, ending) in split_lines(content) {
        crlf |= ending == b"\r\n";
        let setting = key_value(text).and_then(|(key, _)| {
            SETTINGS
                .iter()
                .find(|setting| setting.key == key && role(setting, version) != Role::Newer)
        });
        match setting {
            Some(setting) => {
                out.extend_from_slice(setting.key.as_bytes());
                out.push(b':');
                out.extend_from_slice(format(setting.values[index], quoted).as_bytes());
                written.push(setting.key);
            }
            None => out.extend_from_slice(text),
        }
        out.extend_from_slice(ending);
    }
    let missing: Vec<&Setting> = SETTINGS
        .iter()
        .filter(|setting| {
            role(setting, version) == Role::Current && !written.contains(&setting.key)
        })
        .collect();
    if !missing.is_empty() {
        let line_end: &[u8] = if crlf { b"\r\n" } else { b"\n" };
        if !out.is_empty() && !out.ends_with(b"\n") {
            out.extend_from_slice(line_end);
        }
        for setting in missing {
            let value = format(setting.values[index], quoted);
            out.extend_from_slice(format!("{}:{value}", setting.key).as_bytes());
            out.extend_from_slice(line_end);
        }
    }
    out
}

/// The preset's lines for a new `options.txt` (`key:value\n` each; empty for versions presets do
/// not cover).
pub(crate) fn preset_lines(version: &str, preset: GraphicsPreset) -> String {
    String::from_utf8_lossy(&apply_preset(b"", version, preset)).into_owned()
}

/// The preset a new instance starts with on a computer with `total_memory_mb` of memory and
/// `threads` hardware threads: "Akıcı" up to 8 GB or 4 threads, "Dengeli" otherwise (never
/// "Güzel" by itself). Windows reports a little less than what is installed (the graphics
/// share), hence half a gigabyte over 8 GB. An unknown figure (0) does not count.
pub fn preset_for_machine(total_memory_mb: u64, threads: usize) -> GraphicsPreset {
    const WEAK_MEMORY_MB: u64 = 8 * 1024 + 512;
    const WEAK_THREADS: usize = 4;
    let weak_memory = total_memory_mb > 0 && total_memory_mb <= WEAK_MEMORY_MB;
    let weak_threads = threads > 0 && threads <= WEAK_THREADS;
    if weak_memory || weak_threads {
        GraphicsPreset::Smooth
    } else {
        GraphicsPreset::Balanced
    }
}

/// [`preset_for_machine`] for this computer.
pub fn this_machine_preset() -> GraphicsPreset {
    let threads = std::thread::available_parallelism().map_or(0, |n| n.get());
    preset_for_machine(system_memory_mb(), threads)
}

/// Where a preset picked before the first launch waits: the instance's launcher folder, next to
/// its game folder (`<instance>/.launcher/graphics-preset`).
fn pending_path(game_dir: &Path) -> Option<PathBuf> {
    Some(game_dir.parent()?.join(".launcher").join(PENDING_FILE))
}

/// The preset the player picked while the instance had no `options.txt`, if any.
pub(crate) async fn pending_preset(game_dir: &Path) -> Option<GraphicsPreset> {
    let text = tokio::fs::read_to_string(pending_path(game_dir)?)
        .await
        .ok()?;
    GraphicsPreset::parse(text.trim())
}

async fn write_pending(game_dir: &Path, preset: GraphicsPreset) -> Result<()> {
    let path = pending_path(game_dir)
        .ok_or_else(|| Error::Invalid(format!("{} has no parent", game_dir.display())))?;
    write_atomic(&path, preset.as_str().as_bytes()).await
}

/// Forgets a picked preset once it is used (or can no longer be).
pub(crate) async fn clear_pending(game_dir: &Path) {
    let Some(path) = pending_path(game_dir) else {
        return;
    };
    match tokio::fs::remove_file(&path).await {
        Ok(()) => {}
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {}
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "could not remove the picked graphics preset");
        }
    }
}

/// Writes `options.txt` safely (a new file is flushed, then renamed over the old one). A file
/// linked from elsewhere (shared between instances) stays linked: the file it points to changes.
pub(crate) async fn write_options(path: &Path, content: Vec<u8>) -> Result<()> {
    let path = path.to_path_buf();
    blocking(move || {
        let target = match std::fs::symlink_metadata(&path) {
            Ok(meta) if meta.file_type().is_symlink() => {
                std::fs::canonicalize(&path).map_err(|e| Error::io(&path, e))?
            }
            _ => path,
        };
        write_durable_blocking(&target, &content)
    })
    .await
}

/// The graphics state of `<game_dir>/options.txt` for Minecraft `version`. A file that cannot be
/// read counts as the player's own.
pub async fn graphics_state(game_dir: &Path, version: &str) -> GraphicsState {
    graphics_state_with(game_dir, version, this_machine_preset).await
}

async fn graphics_state_with(
    game_dir: &Path,
    version: &str,
    machine: impl FnOnce() -> GraphicsPreset,
) -> GraphicsState {
    if !supports_presets(version) {
        return GraphicsState::Unsupported;
    }
    let path = game_dir.join(OPTIONS_FILE);
    match tokio::fs::read(&path).await {
        Ok(content) => match detect_preset(&content, version) {
            Some(preset) => GraphicsState::Preset { preset },
            None => GraphicsState::Custom,
        },
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            let preset = match pending_preset(game_dir).await {
                Some(picked) => picked,
                None => machine(),
            };
            GraphicsState::FirstLaunch { preset }
        }
        Err(error) => {
            tracing::warn!(%error, path = %path.display(), "options.txt unreadable");
            GraphicsState::Custom
        }
    }
}

/// Applies the preset the player picked for Minecraft `version`: into `<game_dir>/options.txt`
/// (only the preset's keys change), or, while there is no such file, kept for the first launch
/// to write ([`super::seed_options`]). Returns the new state.
pub async fn set_graphics_preset(
    game_dir: &Path,
    version: &str,
    preset: GraphicsPreset,
) -> Result<GraphicsState> {
    if !supports_presets(version) {
        return Err(Error::Invalid(format!(
            "graphics presets need Minecraft 1.7.2 or newer (got {version})"
        )));
    }
    let path = game_dir.join(OPTIONS_FILE);
    match tokio::fs::read(&path).await {
        Ok(content) => {
            write_options(&path, apply_preset(&content, version, preset)).await?;
            clear_pending(game_dir).await;
            Ok(GraphicsState::Preset { preset })
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => {
            write_pending(game_dir, preset).await?;
            Ok(GraphicsState::FirstLaunch { preset })
        }
        Err(error) => Err(Error::io(&path, error)),
    }
}

/// What [`switch_graphics_preset`] did.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GraphicsSwitch {
    /// The instance's graphics now.
    pub state: GraphicsState,
    /// The copy of `options.txt` from before, for [`undo_graphics_switch`]; `None` when there was
    /// no file yet (the pick waits for the first launch) or the file already had the preset.
    pub backup: Option<String>,
}

/// [`set_graphics_preset`] for a pick that applies at once (Settings → Grafik ve FPS): the file
/// is copied first, so "Geri al" ([`undo_graphics_switch`]) brings back what was there, the
/// player's own values included. The copies share the key fixes' folder
/// (`<instance>/.launcher/options-backups/`) and its limit. The game must not be running.
pub async fn switch_graphics_preset(
    game_dir: &Path,
    version: &str,
    preset: GraphicsPreset,
) -> Result<GraphicsSwitch> {
    let path = game_dir.join(OPTIONS_FILE);
    let backup = if supports_presets(version) {
        match tokio::fs::read(&path).await {
            Ok(content) if apply_preset(&content, version, preset) != content => {
                Some(super::keys::save_backup(game_dir, &content).await?)
            }
            Ok(_) => None,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => None,
            Err(error) => return Err(Error::io(&path, error)),
        }
    } else {
        None
    };
    let state = set_graphics_preset(game_dir, version, preset).await?;
    Ok(GraphicsSwitch { state, backup })
}

/// `current` (an `options.txt` after a preset was applied to `backup`) with the preset's options
/// back at their values in `backup`. When nothing else changed since, that is `backup` itself;
/// when the game saved the file meanwhile, only the preset's lines change (a line the preset
/// added goes) and the game's other changes stay.
fn undo_preset(current: &[u8], backup: &[u8], version: &str) -> Vec<u8> {
    if GraphicsPreset::ALL
        .into_iter()
        .any(|preset| apply_preset(backup, version, preset) == current)
    {
        return backup.to_vec();
    }
    let mut before: HashMap<&str, &[u8]> = HashMap::new();
    for (text, _) in split_lines(backup) {
        if let Some((key, value)) = key_value(text) {
            before.insert(key, value);
        }
    }
    let mut out = Vec::with_capacity(current.len());
    for (text, ending) in split_lines(current) {
        let preset_key = key_value(text).map(|(key, _)| key).filter(|key| {
            SETTINGS
                .iter()
                .any(|setting| setting.key == *key && role(setting, version) != Role::Newer)
        });
        match preset_key {
            Some(key) => {
                if let Some(value) = before.get(key) {
                    out.extend_from_slice(key.as_bytes());
                    out.push(b':');
                    out.extend_from_slice(value);
                    out.extend_from_slice(ending);
                }
            }
            None => {
                out.extend_from_slice(text);
                out.extend_from_slice(ending);
            }
        }
    }
    out
}

/// Undoes a [`switch_graphics_preset`] from its copy ([`undo_preset`]; a file that is gone
/// meanwhile comes back whole) and deletes the copy. Returns the graphics state afterwards, or
/// `None` when that copy no longer exists (nothing changes then). The game must not be running.
pub async fn undo_graphics_switch(
    game_dir: &Path,
    version: &str,
    backup: &str,
) -> Result<Option<GraphicsState>> {
    let Some(saved) = super::keys::read_backup(game_dir, backup).await? else {
        return Ok(None);
    };
    let path = game_dir.join(OPTIONS_FILE);
    let (current, restored) = match tokio::fs::read(&path).await {
        Ok(current) => {
            let restored = undo_preset(&current, &saved, version);
            (Some(current), restored)
        }
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => (None, saved),
        Err(error) => return Err(Error::io(&path, error)),
    };
    if current.as_deref() != Some(restored.as_slice()) {
        write_options(&path, restored).await?;
    }
    super::keys::remove_backup(game_dir, backup).await;
    tracing::info!(backup = %backup, "graphics preset switch undone");
    Ok(Some(graphics_state(game_dir, version).await))
}

#[cfg(test)]
mod tests {
    use super::*;
    use GraphicsPreset::{Balanced, Quality, Smooth};

    fn text(bytes: Vec<u8>) -> String {
        String::from_utf8(bytes).unwrap()
    }

    /// The keys a preset writes for `version`, in table order.
    fn keys(version: &str) -> Vec<&'static str> {
        SETTINGS
            .iter()
            .filter(|setting| role(setting, version) == Role::Current)
            .map(|setting| setting.key)
            .collect()
    }

    #[test]
    fn keys_follow_the_version() {
        assert!(!supports_presets("1.6.4"));
        assert!(!supports_presets("b1.7.3"));
        assert!(supports_presets("1.7.2"));
        assert_eq!(
            keys("1.7.10"),
            [
                "renderDistance",
                "fancyGraphics",
                "particles",
                "clouds",
                "mipmapLevels"
            ]
        );
        let legacy = [
            "renderDistance",
            "fancyGraphics",
            "particles",
            "renderClouds",
            "entityShadows",
            "mipmapLevels",
        ];
        assert_eq!(keys("1.8.9"), legacy);
        assert_eq!(keys("1.12.2"), legacy);
        assert_eq!(
            keys("1.16.5"),
            [
                "renderDistance",
                "graphicsMode",
                "particles",
                "renderClouds",
                "entityShadows",
                "entityDistanceScaling",
                "biomeBlendRadius",
                "mipmapLevels"
            ]
        );
        assert_eq!(
            keys("1.21.1"),
            [
                "renderDistance",
                "simulationDistance",
                "graphicsMode",
                "particles",
                "renderClouds",
                "entityShadows",
                "entityDistanceScaling",
                "biomeBlendRadius",
                "mipmapLevels"
            ]
        );
        assert!(keys("1.21.6").contains(&"cloudRange"));
        assert!(!keys("1.21.5").contains(&"cloudRange"));
        let preset_era = [
            "renderDistance",
            "simulationDistance",
            "graphicsPreset",
            "cutoutLeaves",
            "improvedTransparency",
            "weatherRadius",
            "particles",
            "renderClouds",
            "cloudRange",
            "entityShadows",
            "entityDistanceScaling",
            "biomeBlendRadius",
            "mipmapLevels",
        ];
        assert_eq!(keys("1.21.11"), preset_era);
        assert_eq!(keys("26.1.2"), preset_era);
        assert_eq!(keys("26.1-snapshot-1"), preset_era);
        // Snapshots between the gates: 1.16's before its first pre-release, 25w41a before
        // cutoutLeaves (25w44a).
        assert!(keys("20w20a").contains(&"fancyGraphics"));
        assert!(keys("20w20a").contains(&"entityDistanceScaling"));
        assert!(keys("1.16-pre1").contains(&"graphicsMode"));
        assert!(keys("25w41a").contains(&"graphicsPreset"));
        assert!(!keys("25w41a").contains(&"cutoutLeaves"));
        assert!(keys("25w44a").contains(&"cutoutLeaves"));
    }

    #[test]
    fn new_files_get_the_keys_of_their_version() {
        assert_eq!(
            text(apply_preset(b"", "26.1.2", Smooth)),
            "renderDistance:8\nsimulationDistance:6\ngraphicsPreset:\"custom\"\n\
             cutoutLeaves:false\nimprovedTransparency:false\nweatherRadius:5\nparticles:1\n\
             renderClouds:\"false\"\ncloudRange:32\nentityShadows:false\n\
             entityDistanceScaling:0.75\nbiomeBlendRadius:1\nmipmapLevels:2\n"
        );
        assert_eq!(
            text(apply_preset(b"", "1.21.1", Balanced)),
            "renderDistance:12\nsimulationDistance:8\ngraphicsMode:1\nparticles:0\n\
             renderClouds:\"fast\"\nentityShadows:true\nentityDistanceScaling:1.0\n\
             biomeBlendRadius:2\nmipmapLevels:4\n"
        );
        // Before 1.19, enum values are bare.
        assert_eq!(
            text(apply_preset(b"", "1.16.5", Quality)),
            "renderDistance:16\ngraphicsMode:1\nparticles:0\nrenderClouds:true\n\
             entityShadows:true\nentityDistanceScaling:1.0\nbiomeBlendRadius:3\nmipmapLevels:4\n"
        );
        assert_eq!(
            text(apply_preset(b"", "1.8.9", Smooth)),
            "renderDistance:8\nfancyGraphics:false\nparticles:1\nrenderClouds:false\n\
             entityShadows:false\nmipmapLevels:2\n"
        );
        // Versions presets do not cover are left alone.
        assert_eq!(apply_preset(b"fov:0.5\n", "1.6.4", Smooth), b"fov:0.5\n");
        assert_eq!(preset_lines("1.6.4", Smooth), "");
    }

    /// A 26.1.2 options.txt as the game writes it on Windows (shortened).
    const GAME_FILE: &str = "version:4790\r\nao:true\r\nbiomeBlendRadius:2\r\n\
        chunkSectionFadeInTime:0.75\r\ncutoutLeaves:true\r\nenableVsync:false\r\n\
        entityDistanceScaling:1.0\r\nentityShadows:true\r\nforceUnicodeFont:false\r\n\
        fov:0.0\r\ngraphicsPreset:\"fancy\"\r\nprioritizeChunkUpdates:0\r\ngamma:0.5\r\n\
        guiScale:2\r\ntextureFiltering:0\r\nmaxFps:260\r\nimprovedTransparency:false\r\n\
        mipmapLevels:4\r\nparticles:0\r\nrenderClouds:\"true\"\r\ncloudRange:128\r\n\
        renderDistance:12\r\nsimulationDistance:12\r\nvignette:true\r\nweatherRadius:10\r\n\
        resourcePacks:[\"vanilla\",\"file/Faithful.zip\"]\r\nlastServer:play.projects.gg:25565\r\n\
        lang:tr_tr\r\nkey_key.attack:key.mouse.left\r\nsoundCategory_master:1.0\r\n";

    #[test]
    fn everything_else_in_the_file_stays() {
        let written = text(apply_preset(GAME_FILE.as_bytes(), "26.1.2", Smooth));
        let before: Vec<&str> = GAME_FILE.split("\r\n").collect();
        let after: Vec<&str> = written.split("\r\n").collect();
        // The same lines in the same places: only the preset's values changed.
        assert_eq!(before.len(), after.len());
        let changed: Vec<(&str, &str)> = before
            .iter()
            .zip(&after)
            .filter(|(old, new)| old != new)
            .map(|(old, new)| (*old, *new))
            .collect();
        assert_eq!(
            changed,
            [
                ("biomeBlendRadius:2", "biomeBlendRadius:1"),
                ("cutoutLeaves:true", "cutoutLeaves:false"),
                ("entityDistanceScaling:1.0", "entityDistanceScaling:0.75"),
                ("entityShadows:true", "entityShadows:false"),
                ("graphicsPreset:\"fancy\"", "graphicsPreset:\"custom\""),
                ("mipmapLevels:4", "mipmapLevels:2"),
                ("particles:0", "particles:1"),
                ("renderClouds:\"true\"", "renderClouds:\"false\""),
                ("cloudRange:128", "cloudRange:32"),
                ("renderDistance:12", "renderDistance:8"),
                ("simulationDistance:12", "simulationDistance:6"),
                ("weatherRadius:10", "weatherRadius:5"),
            ]
        );
        assert!(!written.replace("\r\n", "").contains('\n'));
        assert_eq!(detect_preset(written.as_bytes(), "26.1.2"), Some(Smooth));
        // The game's own defaults are none of the presets.
        assert_eq!(detect_preset(GAME_FILE.as_bytes(), "26.1.2"), None);
    }

    #[test]
    fn missing_keys_go_at_the_end_in_the_files_line_endings() {
        // The first-launch file of a version before presets existed in the launcher.
        let seeded = "version:4790\nlang:tr_tr\nguiScale:2\n";
        let written = text(apply_preset(seeded.as_bytes(), "26.1.2", Balanced));
        assert!(written.starts_with(seeded));
        assert_eq!(written.lines().count(), 3 + keys("26.1.2").len());
        // No newline at the end and Windows line endings: the added lines follow.
        let written = text(apply_preset(b"fov:0.5\r\nlang:tr_tr", "1.21.1", Quality));
        assert!(written.starts_with("fov:0.5\r\nlang:tr_tr\r\nrenderDistance:16\r\n"));
        assert!(written.ends_with("mipmapLevels:4\r\n"));
        // A key written twice: both copies change (the game reads the last one).
        let written = text(apply_preset(
            b"renderDistance:20\nrenderDistance:24\n",
            "1.21.1",
            Smooth,
        ));
        assert!(written.starts_with("renderDistance:8\nrenderDistance:8\n"));
    }

    #[test]
    fn a_written_preset_is_recognised_again() {
        for version in [
            "1.7.10", "1.8.9", "1.12.2", "1.16.5", "1.20.1", "1.21.6", "26.1.2",
        ] {
            for preset in GraphicsPreset::ALL {
                let written = apply_preset(b"fov:0.5\nlang:tr_tr\n", version, preset);
                assert_eq!(
                    detect_preset(&written, version),
                    Some(preset),
                    "{version} {preset:?}"
                );
                // Writing it again changes nothing.
                assert_eq!(apply_preset(&written, version, preset), written);
            }
        }
    }

    #[test]
    fn one_changed_value_makes_it_the_players_own() {
        let smooth = text(apply_preset(GAME_FILE.as_bytes(), "26.1.2", Smooth));
        // Render distance changed in the game.
        let custom = smooth.replace("renderDistance:8", "renderDistance:10");
        assert_eq!(detect_preset(custom.as_bytes(), "26.1.2"), None);
        // An option missing from the file (the game uses its default for it).
        let missing = smooth.replace("particles:1\r\n", "");
        assert_eq!(detect_preset(missing.as_bytes(), "26.1.2"), None);
        // The name the game shows does not matter, nor bare or quoted enum values, nor how a
        // number is written.
        let renamed = smooth
            .replace("graphicsPreset:\"custom\"", "graphicsPreset:\"fast\"")
            .replace("renderClouds:\"false\"", "renderClouds:false")
            .replace("entityDistanceScaling:0.75", "entityDistanceScaling:0.750");
        assert_eq!(detect_preset(renamed.as_bytes(), "26.1.2"), Some(Smooth));
        // Options of other versions do not count.
        let other = format!("{smooth}graphicsMode:1\r\nfancyGraphics:true\r\n");
        assert_eq!(detect_preset(other.as_bytes(), "26.1.2"), Some(Smooth));
        assert_eq!(detect_preset(b"", "26.1.2"), None);
        assert_eq!(detect_preset(smooth.as_bytes(), "1.6.4"), None);
    }

    #[test]
    fn enum_values_are_written_the_way_the_version_reads_them() {
        // 1.21.1 reads JSON strings: a bare value (from an older version's file) becomes one.
        let written = text(apply_preset(b"renderClouds:true\n", "1.21.1", Smooth));
        assert!(written.starts_with("renderClouds:\"false\"\n"));
        // 1.18.2 compares the bare text: a quoted value (from a newer version's file) loses them.
        let written = text(apply_preset(b"renderClouds:\"true\"\n", "1.18.2", Balanced));
        assert!(written.starts_with("renderClouds:fast\n"));
        // Recognising a preset reads either form.
        let bare = text(apply_preset(b"", "1.21.1", Smooth))
            .replace("renderClouds:\"false\"", "renderClouds:false");
        assert_eq!(detect_preset(bare.as_bytes(), "1.21.1"), Some(Smooth));
        let quoted = text(apply_preset(b"", "1.18.2", Balanced))
            .replace("renderClouds:fast", "renderClouds:\"fast\"");
        assert_eq!(detect_preset(quoted.as_bytes(), "1.18.2"), Some(Balanced));
    }

    #[test]
    fn older_keys_change_only_where_the_file_still_has_them() {
        // A file last saved by 1.21.1 in an instance moved to 26.1.2: the game converts
        // graphicsMode when it upgrades the file, so it gets the preset's value too.
        let old = "version:3955\ngraphicsMode:1\nrenderDistance:12\n";
        let written = text(apply_preset(old.as_bytes(), "26.1.2", Smooth));
        assert!(written.starts_with("version:3955\ngraphicsMode:0\nrenderDistance:8\n"));
        assert!(written.contains("graphicsPreset:\"custom\"\n"));
        // A file without it does not get it.
        let written = text(apply_preset(b"version:4790\n", "26.1.2", Smooth));
        assert!(!written.contains("graphicsMode"));
        assert!(!written.contains("fancyGraphics"));
        // Keys of later versions stay as they are.
        let newer = "graphicsPreset:\"fancy\"\ncutoutLeaves:true\n";
        let written = text(apply_preset(newer.as_bytes(), "1.21.1", Smooth));
        assert!(written.starts_with(newer));
        assert!(written.contains("graphicsMode:0\n"));
    }

    #[test]
    fn weak_computers_start_smooth() {
        // 4 GB and 8 GB laptops (Windows reports a little less), a quad-thread CPU.
        assert_eq!(preset_for_machine(3_481, 8), Smooth);
        assert_eq!(preset_for_machine(7_986, 8), Smooth);
        assert_eq!(preset_for_machine(8_704, 12), Smooth);
        assert_eq!(preset_for_machine(16_303, 4), Smooth);
        // Anything stronger is balanced; never the richest by itself.
        assert_eq!(preset_for_machine(8_705, 8), Balanced);
        assert_eq!(preset_for_machine(32_000, 16), Balanced);
        // Unknown figures do not count.
        assert_eq!(preset_for_machine(0, 0), Balanced);
        assert_eq!(preset_for_machine(0, 2), Smooth);
        assert_eq!(preset_for_machine(4_096, 0), Smooth);
    }

    /// An instance folder with an empty game folder.
    fn instance() -> (tempfile::TempDir, PathBuf) {
        let dir = tempfile::tempdir().unwrap();
        let game = dir.path().join("minecraft");
        std::fs::create_dir_all(&game).unwrap();
        (dir, game)
    }

    #[tokio::test]
    async fn a_pick_before_the_first_launch_waits_for_it() {
        let (_dir, game) = instance();
        let machine = || Smooth;
        assert_eq!(
            graphics_state_with(&game, "26.1.2", machine).await,
            GraphicsState::FirstLaunch { preset: Smooth }
        );
        assert_eq!(
            set_graphics_preset(&game, "26.1.2", Quality).await.unwrap(),
            GraphicsState::FirstLaunch { preset: Quality }
        );
        // Kept beside the game folder, not in it: options.txt is still the game's to create.
        assert!(!game.join(OPTIONS_FILE).exists());
        assert_eq!(pending_preset(&game).await, Some(Quality));
        assert_eq!(
            graphics_state_with(&game, "26.1.2", machine).await,
            GraphicsState::FirstLaunch { preset: Quality }
        );
        clear_pending(&game).await;
        assert_eq!(pending_preset(&game).await, None);
        clear_pending(&game).await;
        assert_eq!(
            graphics_state_with(&game, "1.6.4", machine).await,
            GraphicsState::Unsupported
        );
        assert!(set_graphics_preset(&game, "1.6.4", Smooth).await.is_err());
    }

    #[tokio::test]
    async fn a_pick_goes_into_the_file() {
        let (_dir, game) = instance();
        let path = game.join(OPTIONS_FILE);
        std::fs::write(&path, GAME_FILE).unwrap();
        assert_eq!(
            graphics_state_with(&game, "26.1.2", || Smooth).await,
            GraphicsState::Custom
        );
        assert_eq!(
            set_graphics_preset(&game, "26.1.2", Balanced)
                .await
                .unwrap(),
            GraphicsState::Preset { preset: Balanced }
        );
        let written = std::fs::read(&path).unwrap();
        assert_eq!(
            written,
            apply_preset(GAME_FILE.as_bytes(), "26.1.2", Balanced)
        );
        assert_eq!(
            graphics_state_with(&game, "26.1.2", || Smooth).await,
            GraphicsState::Preset { preset: Balanced }
        );
        // No temporary file is left behind.
        let names: Vec<String> = std::fs::read_dir(&game)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        assert_eq!(names, [OPTIONS_FILE]);
    }

    #[tokio::test]
    async fn a_linked_file_stays_linked() {
        let (dir, game) = instance();
        let shared = dir.path().join("shared-options.txt");
        std::fs::write(&shared, "fov:0.5\n").unwrap();
        let link = game.join(OPTIONS_FILE);
        #[cfg(windows)]
        let made = std::os::windows::fs::symlink_file(&shared, &link);
        #[cfg(unix)]
        let made = std::os::unix::fs::symlink(&shared, &link);
        if made.is_err() {
            // Windows without Developer Mode cannot make file links (so no player has one).
            return;
        }
        set_graphics_preset(&game, "1.21.1", Smooth).await.unwrap();
        assert!(
            std::fs::symlink_metadata(&link)
                .unwrap()
                .file_type()
                .is_symlink()
        );
        let shared_now = std::fs::read(&shared).unwrap();
        assert_eq!(detect_preset(&shared_now, "1.21.1"), Some(Smooth));
        assert!(shared_now.starts_with(b"fov:0.5\n"));
    }

    fn backups(game: &Path) -> PathBuf {
        game.parent()
            .unwrap()
            .join(".launcher")
            .join("options-backups")
    }

    #[tokio::test]
    async fn a_switch_undoes_back_to_the_players_own_values() {
        let (_dir, game) = instance();
        let path = game.join(OPTIONS_FILE);
        std::fs::write(&path, GAME_FILE).unwrap();
        let switch = switch_graphics_preset(&game, "26.1.2", Smooth)
            .await
            .unwrap();
        assert_eq!(switch.state, GraphicsState::Preset { preset: Smooth });
        let backup = switch.backup.expect("a copy of the player's file");
        assert_eq!(
            std::fs::read(&path).unwrap(),
            apply_preset(GAME_FILE.as_bytes(), "26.1.2", Smooth)
        );
        // In the key fixes' folder, beside the game folder.
        assert!(backups(&game).join(&backup).is_file());

        assert_eq!(
            undo_graphics_switch(&game, "26.1.2", &backup)
                .await
                .unwrap(),
            Some(GraphicsState::Custom)
        );
        assert_eq!(std::fs::read(&path).unwrap(), GAME_FILE.as_bytes());
        // The copy is used up: a second undo changes nothing.
        assert!(!backups(&game).join(&backup).exists());
        assert_eq!(
            undo_graphics_switch(&game, "26.1.2", &backup)
                .await
                .unwrap(),
            None
        );
        assert!(
            undo_graphics_switch(&game, "26.1.2", "../options.txt")
                .await
                .is_err()
        );
    }

    #[tokio::test]
    async fn a_switch_that_changes_no_file_keeps_no_copy() {
        let (_dir, game) = instance();
        // No file yet: the pick waits for the first launch (undone by picking again).
        assert_eq!(
            switch_graphics_preset(&game, "26.1.2", Quality)
                .await
                .unwrap(),
            GraphicsSwitch {
                state: GraphicsState::FirstLaunch { preset: Quality },
                backup: None,
            }
        );
        // The file already has the preset.
        let path = game.join(OPTIONS_FILE);
        std::fs::write(&path, apply_preset(b"lang:tr_tr\n", "26.1.2", Balanced)).unwrap();
        let switch = switch_graphics_preset(&game, "26.1.2", Balanced)
            .await
            .unwrap();
        assert_eq!(switch.state, GraphicsState::Preset { preset: Balanced });
        assert_eq!(switch.backup, None);
        assert!(!backups(&game).exists());
        assert!(
            switch_graphics_preset(&game, "1.6.4", Smooth)
                .await
                .is_err()
        );
    }

    #[test]
    fn an_undo_after_the_game_saved_puts_back_only_the_preset() {
        let before = "version:4790\nrenderDistance:12\nparticles:0\nlang:tr_tr\n";
        let switched = text(apply_preset(before.as_bytes(), "26.1.2", Smooth));
        assert_eq!(
            text(undo_preset(
                switched.as_bytes(),
                before.as_bytes(),
                "26.1.2"
            )),
            before,
            "nothing else changed: the file comes back byte for byte"
        );
        // The game saved the file since, in another language.
        let saved = switched.replace("lang:tr_tr", "lang:en_us");
        assert_eq!(
            text(undo_preset(saved.as_bytes(), before.as_bytes(), "26.1.2")),
            "version:4790\nrenderDistance:12\nparticles:0\nlang:en_us\n",
            "the preset's values go back, the lines it added go, the game's change stays"
        );
    }

    #[tokio::test]
    async fn an_undo_brings_back_a_file_deleted_meanwhile() {
        let (_dir, game) = instance();
        let path = game.join(OPTIONS_FILE);
        std::fs::write(&path, "fov:0.5\nrenderDistance:20\n").unwrap();
        let backup = switch_graphics_preset(&game, "1.21.1", Quality)
            .await
            .unwrap()
            .backup
            .unwrap();
        std::fs::remove_file(&path).unwrap();
        assert_eq!(
            undo_graphics_switch(&game, "1.21.1", &backup)
                .await
                .unwrap(),
            Some(GraphicsState::Custom)
        );
        assert_eq!(
            std::fs::read_to_string(&path).unwrap(),
            "fov:0.5\nrenderDistance:20\n"
        );
    }
}
