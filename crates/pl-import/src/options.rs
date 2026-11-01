//! The player's `options.txt` from another launcher, into the Projects instance (IMPORT1,
//! IMPORT-SET).
//!
//! The player ticked "Oyun ayarları": their own settings win. Every line of their file comes over
//! as it is (language, every key binding with the hotbar and inventory keys, video, sound and
//! music volumes, accessibility, chat) with its `version:` line, so the game upgrades an older file
//! itself with its data fixers; a file from a newer game is read as far as the game knows it (the
//! fixers never run backwards) and saved in its own version at the next save. Lines only the
//! Projects instance has stay (the pack's mod keys, options of newer versions), and the official
//! pack's key table (KEYS1, `pl_mc::instance::official_pack_bindings`) fills in mappings neither
//! file has. A key of the instance's own or of the table that one of the player's bindings already
//! uses in play is written unbound instead, so a press never does two things.
//!
//! Key names (`key.keyboard.c`) are written only into a file whose `version:` line is from 18w21a
//! (data version 1446) on: older files store keys as numbers, and the game's fixers for them parse
//! every key as a number (`OptionsKeyLwjgl3Fix`: one name there and the game drops the whole file)
//! or prefix it again (`OptionsKeyTranslationFix`). Such a file keeps its own keys only. The other
//! fixers read the values they move leniently, so the instance's other lines may come along.
//!
//! OptiFine is never part of an instance here (Sodium and Iris do its work): its zoom key
//! (`key_of.key.zoom`) becomes the pack's zoom key, and its own mappings hold no key.

use std::collections::{HashMap, HashSet};

use pl_mc::instance::KeyBinding;

/// The value that leaves a key mapping unbound.
pub const UNBOUND: &str = "key.keyboard.unknown";

/// The first data version whose `options.txt` stores keys by name (18w21a, Minecraft 1.13): the
/// game's fixers for older files cannot read a name.
pub const NAMED_KEYS_VERSION: u32 = 1446;

/// A mapping the game always writes: a file without it was not written by the game (the
/// launcher's own first file), so it does not tell which mappings the game knows.
const ALWAYS_WRITTEN: &str = "key_key.attack";

/// OptiFine's zoom mapping (`key_of.key.zoom`).
const OPTIFINE_ZOOM: &str = "of.key.zoom";

/// The resource pack lists: the instance's packs stay below the player's.
const PACK_LISTS: [&str; 2] = ["resourcePacks", "incompatibleResourcePacks"];

/// Mappings that act only in creative, spectator or with F3 held (a pack key may share their key:
/// the KEYS1 table relies on that for C and X), and OptiFine's, which no instance here has.
fn outside_play(name: &str) -> bool {
    const NOT_IN_PLAY: [&str; 5] = [
        "key.saveToolbarActivator",
        "key.loadToolbarActivator",
        "key.spectatorHotbar",
        "key.spectatorOutlines",
        "key.toggleSpectatorShaderEffects",
    ];
    NOT_IN_PLAY.contains(&name)
        || (name.starts_with("key.debug.") && name != "key.debug.overlay")
        || name.starts_with("key.replaymod.")
        || name.starts_with("of.")
}

/// What [`merge_options`] and [`merge_into`] made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsMerge {
    pub text: String,
    /// Pack key mappings given a key: the table's, or OptiFine's zoom key.
    pub added: usize,
    /// Mappings written unbound because one of the player's bindings uses their key in play.
    pub unbound: usize,
}

/// One line of a file: its text, and its key and value when it has a colon (the game splits at
/// the first one).
struct Line<'a> {
    text: &'a str,
    key: Option<&'a str>,
    value: &'a str,
}

fn parse(text: &str) -> Vec<Line<'_>> {
    text.lines()
        .map(|text| match text.split_once(':') {
            Some((key, value)) if !key.is_empty() => Line {
                text,
                key: Some(key),
                value,
            },
            _ => Line {
                text,
                key: None,
                value: "",
            },
        })
        .collect()
}

/// The data version of a file's `version:` line (the last one, as the game reads it).
pub fn data_version(text: &str) -> Option<u32> {
    parse(text)
        .iter()
        .rev()
        .find(|line| line.key == Some("version"))
        .and_then(|line| line.value.trim().parse().ok())
}

/// The file stores keys by name: its version says so and no key is a number.
fn named_keys(lines: &[Line<'_>]) -> bool {
    let version = lines
        .iter()
        .rev()
        .find(|line| line.key == Some("version"))
        .and_then(|line| line.value.trim().parse::<u32>().ok());
    let numeric = lines.iter().any(|line| {
        line.key.is_some_and(|key| key.starts_with("key_"))
            && line.value.trim().parse::<i32>().is_ok()
    });
    version.is_some_and(|v| v >= NAMED_KEYS_VERSION) && !numeric
}

/// The keys in play while merging: key → the mappings on it that act in play.
#[derive(Default)]
struct Held<'a> {
    by_key: HashMap<String, Vec<&'a str>>,
}

impl<'a> Held<'a> {
    fn hold(&mut self, mapping: &'a str, key: &str) {
        if key != UNBOUND && !outside_play(mapping) {
            self.by_key.entry(key.to_owned()).or_default().push(mapping);
        }
    }

    /// `mapping` no longer uses its key (it gets another one).
    fn release(&mut self, mapping: &str) {
        for mappings in self.by_key.values_mut() {
            mappings.retain(|m| *m != mapping);
        }
    }

    /// Whether a mapping other than `but` uses `key` in play.
    fn taken(&self, key: &str, but: &[&str]) -> bool {
        key != UNBOUND
            && self
                .by_key
                .get(key)
                .is_some_and(|mappings| mappings.iter().any(|m| !but.contains(m)))
    }
}

/// `source` (the player's `options.txt`) as a new file: their lines with the pack's bindings they
/// lack (see [`merge_into`]).
pub fn merge_options(source: &str, pack: &[KeyBinding]) -> OptionsMerge {
    merge_into(None, source, pack)
}

/// The player's `options.txt` (`source`) merged into the instance's own (`existing`, when it has
/// one): see the module's rules. `pack` is the official pack's key table (empty for an instance
/// that does not follow the pack).
pub fn merge_into(existing: Option<&str>, source: &str, pack: &[KeyBinding]) -> OptionsMerge {
    let source = source.replace("\r\n", "\n");
    let existing = existing.map(|text| text.replace("\r\n", "\n"));
    let theirs = parse(&source);
    let ours = existing.as_deref().map(parse).unwrap_or_default();
    let named = named_keys(&theirs);
    let in_source: HashSet<&str> = theirs.iter().filter_map(|line| line.key).collect();
    let their_value = |key: &str| -> Option<&str> {
        theirs
            .iter()
            .rev()
            .find(|line| line.key == Some(key))
            .map(|line| line.value.trim())
    };

    // The mappings the instance's game knows, when the game wrote its file: a binding of the
    // player's for anything else (a mod the instance does not have, OptiFine) does nothing there.
    let known: Option<HashSet<&str>> = ours
        .iter()
        .any(|line| line.key == Some(ALWAYS_WRITTEN))
        .then(|| {
            ours.iter()
                .filter_map(|line| line.key?.strip_prefix("key_"))
                .collect()
        });
    let mut held = Held::default();
    for line in &theirs {
        if let Some(mapping) = line.key.and_then(|key| key.strip_prefix("key_"))
            && known.as_ref().is_none_or(|known| known.contains(mapping))
        {
            held.hold(mapping, line.value.trim());
        }
    }

    let mut out: Vec<String> = theirs
        .iter()
        .map(|line| match line.key {
            Some(key) if PACK_LISTS.contains(&key) => {
                let ours = ours
                    .iter()
                    .rev()
                    .find(|o| o.key == Some(key))
                    .map(|o| o.value);
                match ours.and_then(|ours| pack_union(ours, line.value)) {
                    Some(value) => format!("{key}:{value}"),
                    None => line.text.to_owned(),
                }
            }
            _ => line.text.to_owned(),
        })
        .collect();
    let mut placed: HashMap<String, usize> = HashMap::new();
    let mut unbound = 0;
    let mut added = 0;

    // The instance's own lines the player's file does not have (a key written twice counts with
    // its last line, as in the game).
    let last: HashMap<&str, usize> = ours
        .iter()
        .enumerate()
        .filter_map(|(at, line)| Some((line.key?, at)))
        .collect();
    for (at, line) in ours.iter().enumerate() {
        let Some(key) = line.key else {
            continue;
        };
        if key == "version" || in_source.contains(key) || last.get(key) != Some(&at) {
            continue;
        }
        if let Some(mapping) = key.strip_prefix("key_") {
            if !named {
                continue;
            }
            let value = line.value.trim();
            if held.taken(value, &[mapping]) {
                unbound += 1;
                out.push(format!("{key}:{UNBOUND}"));
            } else {
                held.hold(mapping, value);
                out.push(line.text.to_owned());
            }
        } else {
            out.push(line.text.to_owned());
        }
        placed.insert(key.to_owned(), out.len() - 1);
    }

    if named {
        // The player's OptiFine zoom key becomes the pack's zoom key, unless their file binds that
        // mapping itself or another binding uses the key in play.
        let zoom = their_value(&format!("key_{OPTIFINE_ZOOM}"))
            .filter(|key| *key != UNBOUND && key.starts_with("key."));
        if let Some(zoom) = zoom {
            for binding in pack
                .iter()
                .filter(|b| b.action.eq_ignore_ascii_case("zoom"))
            {
                let key = format!("key_{}", binding.name);
                if in_source.contains(key.as_str()) || held.taken(zoom, &[binding.name.as_str()]) {
                    continue;
                }
                let line = format!("{key}:{zoom}");
                match placed.get(&key) {
                    Some(&at) => out[at] = line,
                    None => {
                        out.push(line);
                        placed.insert(key, out.len() - 1);
                    }
                }
                held.release(&binding.name);
                held.hold(&binding.name, zoom);
                added += 1;
            }
        }
        // The table's bindings neither file has.
        for binding in pack {
            let key = format!("key_{}", binding.name);
            if in_source.contains(key.as_str()) || placed.contains_key(&key) {
                continue;
            }
            let value = if held.taken(&binding.key, &[binding.name.as_str()]) {
                unbound += 1;
                UNBOUND
            } else {
                added += 1;
                binding.key.as_str()
            };
            held.hold(&binding.name, value);
            out.push(format!("{key}:{value}"));
            placed.insert(key, out.len() - 1);
        }
    }

    let mut text = out.join("\n");
    if !text.is_empty() {
        text.push('\n');
    }
    OptionsMerge {
        text,
        added,
        unbound,
    }
}

/// The player's pack list (`theirs`, JSON) with the instance's packs it lacks (`ours`) below
/// them, lowest first as the game keeps it. `None` when one of them cannot be read or nothing is
/// missing: the player's line stays as it is.
fn pack_union(ours: &str, theirs: &str) -> Option<String> {
    let ours: Vec<String> = serde_json::from_str(ours.trim()).ok()?;
    let theirs: Vec<String> = serde_json::from_str(theirs.trim()).ok()?;
    let missing: Vec<String> = ours.into_iter().filter(|p| !theirs.contains(p)).collect();
    if missing.is_empty() {
        return None;
    }
    serde_json::to_string(&missing.into_iter().chain(theirs).collect::<Vec<_>>()).ok()
}

/// Every key's value (the last one where a key is written twice, as in the game), for telling
/// whether a merge changes anything: the line order and line endings do not count.
pub fn same_settings(a: &str, b: &str) -> bool {
    let values = |text: &str| -> HashMap<String, String> {
        text.replace("\r\n", "\n")
            .lines()
            .filter_map(|line| line.split_once(':'))
            .map(|(key, value)| (key.to_owned(), value.trim().to_owned()))
            .collect()
    };
    values(a) == values(b)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Vec<KeyBinding> {
        [
            ("key.wi_zoom.zoom", "key.keyboard.c", "Zoom"),
            (
                "key.gammautils.gammaToggle",
                "key.keyboard.x",
                "Toggle gamma",
            ),
            ("key.push_to_talk", "key.keyboard.caps.lock", "Push to talk"),
            ("key.hide_icons", UNBOUND, "Hide icons"),
        ]
        .into_iter()
        .map(|(name, key, action)| KeyBinding {
            name: name.into(),
            key: key.into(),
            default: "key.keyboard.v".into(),
            clashes_with: None,
            mod_name: "Mod".into(),
            action: action.into(),
        })
        .collect()
    }

    fn line_of<'a>(text: &'a str, key: &str) -> Option<&'a str> {
        text.lines()
            .rev()
            .find_map(|line| line.strip_prefix(&format!("{key}:")[..]))
    }

    #[test]
    fn the_players_own_keys_win_and_missing_pack_keys_fill_in() {
        let player = "version:4189\nlang:tr_tr\nguiScale:3\nkey_key.saveToolbarActivator:key.keyboard.c\nkey_key.wi_zoom.zoom:key.keyboard.z\nkey_key.sprint:key.keyboard.caps.lock";
        let merged = merge_options(player, &table());
        // Their lines stay exactly as they were (language, GUI scale, the version line).
        assert!(
            merged
                .text
                .starts_with("version:4189\nlang:tr_tr\nguiScale:3\n")
        );
        // Their own zoom key wins; gamma takes X; push to talk would share sprint's Caps Lock and
        // stays unbound; hide icons is unbound by the table anyway. C next to the creative
        // hotbar modifier is fine (the table relies on it).
        assert!(
            merged
                .text
                .contains("key_key.wi_zoom.zoom:key.keyboard.z\n")
        );
        assert!(!merged.text.contains("key_key.wi_zoom.zoom:key.keyboard.c"));
        assert!(
            merged
                .text
                .contains("key_key.gammautils.gammaToggle:key.keyboard.x\n")
        );
        assert!(
            merged
                .text
                .contains("key_key.push_to_talk:key.keyboard.unknown\n")
        );
        assert!(
            merged
                .text
                .contains("key_key.hide_icons:key.keyboard.unknown\n")
        );
        assert_eq!((merged.added, merged.unbound), (2, 1));
    }

    #[test]
    fn old_number_keys_and_other_instances_get_nothing() {
        let old = "key_key.attack:-100\nkey_key.forward:17\n";
        assert_eq!(merge_options(old, &table()).text, old);
        let player = "lang:en_us\r\nkey_key.jump:key.keyboard.space\r\n";
        let merged = merge_options(player, &[]);
        assert_eq!(merged.text, "lang:en_us\nkey_key.jump:key.keyboard.space\n");
        assert_eq!((merged.added, merged.unbound), (0, 0));
    }

    #[test]
    fn key_names_go_only_into_files_the_game_reads_names_from() {
        // No version line: the game runs every fixer from the first, and the one for number keys
        // would drop the whole file over a single name.
        let unversioned = "lang:en_us\n";
        assert_eq!(merge_options(unversioned, &table()).text, unversioned);
        let before_names = "version:1343\nlang:en_us\n";
        assert_eq!(merge_options(before_names, &table()).text, before_names);
        let from_names = "version:1446\nlang:en_us\n";
        assert!(
            merge_options(from_names, &table())
                .text
                .contains("key_key.wi_zoom.zoom:key.keyboard.c\n")
        );
        assert_eq!(data_version("lang:x\nversion:4189\n"), Some(4189));
        assert_eq!(data_version("version:x\n"), None);
    }

    #[test]
    fn the_real_table_applies_to_a_vanilla_file() {
        let player = "version:3953\nlang:en_us\nkey_key.drop:key.keyboard.c\n";
        let merged = merge_options(player, pl_mc::instance::official_pack_bindings());
        // C is the player's drop key: zoom stays unbound rather than dropping items.
        assert!(
            merged
                .text
                .contains("key_key.wi_zoom.zoom:key.keyboard.unknown\n")
        );
        assert!(merged.unbound >= 1);
        // The pack's zoom is the mapping OptiFine's zoom key moves to.
        assert!(
            pl_mc::instance::official_pack_bindings()
                .iter()
                .any(|b| b.action.eq_ignore_ascii_case("zoom"))
        );
    }

    /// The Projects instance's file after the game ran (CRLF like the game on Windows): vanilla
    /// keys, the pack's mods' keys, newer options and its own resource packs.
    const PROJECTS: &str = "version:4790\r\nlang:tr_tr\r\nguiScale:2\r\nrenderDistance:8\r\nsoundCategory_music:0.5\r\nmenuBackgroundBlurriness:5\r\nresourcePacks:[\"vanilla\",\"fabric\",\"file/SetPack.zip\"]\r\nkey_key.attack:key.mouse.left\r\nkey_key.drop:key.keyboard.q\r\nkey_key.hotbar.1:key.keyboard.1\r\nkey_key.voice_chat:key.keyboard.v\r\nkey_key.wi_zoom.zoom:key.keyboard.c\r\nkey_key.gammautils.gammaToggle:key.keyboard.x\r\nkey_key.push_to_talk:key.keyboard.caps.lock\r\n";

    #[test]
    fn the_players_settings_win_over_the_instances_own() {
        // TLauncher 1.21.4: other hotbar keys, music off, a longer view, their own pack, a mod
        // Projects does not have on G, and their drop key on V.
        let player = "version:4189\nlang:en_us\nguiScale:3\nrenderDistance:16\nsoundCategory_music:0.0\nresourcePacks:[\"vanilla\",\"file/Faithful.zip\"]\nkey_key.attack:key.mouse.left\nkey_key.drop:key.keyboard.v\nkey_key.hotbar.1:key.keyboard.r\nkey_key.minihud.toggle:key.keyboard.x\n";
        let merged = merge_into(Some(PROJECTS), player, &table()).text;
        // Their values, their version line (the game upgrades their file itself).
        for (key, value) in [
            ("version", "4189"),
            ("lang", "en_us"),
            ("guiScale", "3"),
            ("renderDistance", "16"),
            ("soundCategory_music", "0.0"),
            ("key_key.hotbar.1", "key.keyboard.r"),
            ("key_key.drop", "key.keyboard.v"),
        ] {
            assert_eq!(line_of(&merged, key), Some(value), "{key}");
        }
        assert_eq!(merged.matches("version:").count(), 1);
        // The instance's own lines stay: newer options and the pack's mod keys...
        assert_eq!(line_of(&merged, "menuBackgroundBlurriness"), Some("5"));
        assert_eq!(
            line_of(&merged, "key_key.wi_zoom.zoom"),
            Some("key.keyboard.c")
        );
        assert_eq!(
            line_of(&merged, "key_key.push_to_talk"),
            Some("key.keyboard.caps.lock")
        );
        // ...except a key one of the player's bindings uses in play: their drop key is V, so the
        // voice chat menu goes unbound. A mod Projects does not have holds no key (MiniHUD on X):
        // gamma keeps X.
        assert_eq!(
            line_of(&merged, "key_key.voice_chat"),
            Some("key.keyboard.unknown")
        );
        assert_eq!(
            line_of(&merged, "key_key.gammautils.gammaToggle"),
            Some("key.keyboard.x")
        );
        // Their packs on top, the instance's below them.
        assert_eq!(
            line_of(&merged, "resourcePacks"),
            Some("[\"fabric\",\"file/SetPack.zip\",\"vanilla\",\"file/Faithful.zip\"]")
        );
        assert!(!merged.contains('\r'));
    }

    #[test]
    fn a_file_from_before_key_names_keeps_its_own_keys_only() {
        // Minecraft 1.8.9 wrote no version line and number keys: the instance's key lines would
        // make the game drop the file; its other lines are safe and stay.
        let player = "lang:tr_TR\nrenderDistance:12\nkey_key.drop:16\nkey_key.hotbar.1:2\n";
        let merged = merge_into(Some(PROJECTS), player, &table()).text;
        assert!(merged.starts_with(player));
        assert!(!merged.contains("version:"));
        assert!(!merged.contains("key.keyboard"));
        assert!(!merged.contains("key.mouse"));
        assert_eq!(line_of(&merged, "menuBackgroundBlurriness"), Some("5"));
        assert_eq!(line_of(&merged, "guiScale"), Some("2"));
    }

    #[test]
    fn a_newer_file_comes_as_it_is() {
        // A newer game's file keeps its version: the game reads what it knows and writes its own
        // version at the next save.
        let player = "version:4999\nlang:tr_tr\nnewOption:true\nkey_key.attack:key.mouse.left\n";
        let merged = merge_into(Some(PROJECTS), player, &table()).text;
        assert_eq!(line_of(&merged, "version"), Some("4999"));
        assert_eq!(line_of(&merged, "newOption"), Some("true"));
        assert_eq!(line_of(&merged, "renderDistance"), Some("8"));
    }

    #[test]
    fn optifines_zoom_key_becomes_the_packs_zoom() {
        let player =
            "version:3953\nkey_key.attack:key.mouse.left\nkey_of.key.zoom:key.keyboard.z\n";
        let merged = merge_into(Some(PROJECTS), player, &table());
        assert_eq!(
            line_of(&merged.text, "key_key.wi_zoom.zoom"),
            Some("key.keyboard.z")
        );
        // A new file too; OptiFine's own line holds no key (OptiFine is never here).
        let fresh = merge_options(
            "version:3953\nkey_of.key.zoom:key.keyboard.c\nkey_key.drop:key.keyboard.q\n",
            &table(),
        );
        assert_eq!(
            line_of(&fresh.text, "key_key.wi_zoom.zoom"),
            Some("key.keyboard.c")
        );
        // Their own zoom line wins, and a key another of their bindings uses is not taken.
        let own =
            "version:3953\nkey_of.key.zoom:key.keyboard.z\nkey_key.wi_zoom.zoom:key.keyboard.b\n";
        assert_eq!(
            line_of(
                &merge_into(Some(PROJECTS), own, &table()).text,
                "key_key.wi_zoom.zoom"
            ),
            Some("key.keyboard.b")
        );
        let taken = "version:3953\nkey_key.attack:key.mouse.left\nkey_of.key.zoom:key.keyboard.q\nkey_key.drop:key.keyboard.q\n";
        assert_eq!(
            line_of(
                &merge_into(Some(PROJECTS), taken, &table()).text,
                "key_key.wi_zoom.zoom"
            ),
            Some("key.keyboard.c")
        );
    }

    #[test]
    fn the_launchers_own_first_file_does_not_hide_the_players_keys() {
        // The instance never ran: its file is the launcher's (no vanilla mappings), so every
        // binding of the player's counts.
        let seeded = "version:4790\nlang:tr_tr\nguiScale:2\nkey_key.wi_zoom.zoom:key.keyboard.c\n";
        let player = "version:4189\nkey_key.drop:key.keyboard.c\n";
        let merged = merge_into(Some(seeded), player, &table()).text;
        assert_eq!(
            line_of(&merged, "key_key.wi_zoom.zoom"),
            Some("key.keyboard.unknown")
        );
    }

    #[test]
    fn merging_the_same_file_again_changes_nothing() {
        let player = "version:4189\nlang:en_us\nkey_key.attack:key.mouse.left\nkey_key.drop:key.keyboard.c\n";
        let first = merge_options(player, &table()).text;
        let again = merge_into(Some(&first), player, &table()).text;
        assert_eq!(again, first);
        assert!(same_settings(&first.replace('\n', "\r\n"), &again));
        assert!(!same_settings(PROJECTS, &again));
    }
}
