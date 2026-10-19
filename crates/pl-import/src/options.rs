//! The player's `options.txt` from another launcher, as the Projects instance's first one.
//!
//! The launcher's first-launch rules (`pl_mc::instance::seed_options`) only write a file that
//! does not exist, so an imported file is the player's: their language, controls, video
//! settings and GUI scale stay as they were, and so does its `version:` line (the game upgrades
//! older files itself). The one thing added is the official pack's key table (KEYS1,
//! `pl_mc::instance::official_pack_bindings`), and only for mappings the file does not have:
//! the player's own keys always win. A pack key that one of the player's own bindings already
//! uses in play is written unbound instead, so a press never does two things.

use std::collections::{HashMap, HashSet};

use pl_mc::instance::KeyBinding;

/// The value that leaves a key mapping unbound.
pub const UNBOUND: &str = "key.keyboard.unknown";

/// Vanilla mappings that act only in creative, spectator or with F3 held: a pack key may share
/// their key (the KEYS1 table relies on that for C and X).
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
}

/// What [`merge_options`] made.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OptionsMerge {
    pub text: String,
    /// Pack key mappings added with the table's key.
    pub added: usize,
    /// Pack key mappings added unbound because the player's own binding uses their key.
    pub unbound: usize,
}

/// `source` (the player's `options.txt`) with the pack's bindings it lacks appended. Files from
/// before 1.13 store keys as numbers: they get nothing (the names would mean nothing there).
pub fn merge_options(source: &str, pack: &[KeyBinding]) -> OptionsMerge {
    let text = source.replace("\r\n", "\n");
    let mut bound: HashMap<&str, &str> = HashMap::new();
    let mut numeric = false;
    for line in text.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        if let Some(name) = key.strip_prefix("key_") {
            let value = value.trim();
            numeric |= value.parse::<i32>().is_ok();
            bound.insert(name, value);
        }
    }
    let mut out = text.clone();
    let mut added = 0;
    let mut unbound = 0;
    if numeric || pack.is_empty() {
        return OptionsMerge {
            text: out,
            added,
            unbound,
        };
    }
    // Keys the player's own bindings hold in play.
    let held: HashSet<&str> = bound
        .iter()
        .filter(|(name, key)| !outside_play(name) && **key != UNBOUND)
        .map(|(_, key)| *key)
        .collect();
    if !out.is_empty() && !out.ends_with('\n') {
        out.push('\n');
    }
    for binding in pack {
        if bound.contains_key(binding.name.as_str()) {
            continue;
        }
        let key = if binding.key != UNBOUND && held.contains(binding.key.as_str()) {
            unbound += 1;
            UNBOUND
        } else {
            added += 1;
            binding.key.as_str()
        };
        out.push_str(&format!("key_{}:{key}\n", binding.name));
    }
    OptionsMerge {
        text: out,
        added,
        unbound,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn table() -> Vec<KeyBinding> {
        [
            ("key.wi_zoom.zoom", "key.keyboard.c"),
            ("key.gammautils.gammaToggle", "key.keyboard.x"),
            ("key.push_to_talk", "key.keyboard.caps.lock"),
            ("key.hide_icons", UNBOUND),
        ]
        .into_iter()
        .map(|(name, key)| KeyBinding {
            name: name.into(),
            key: key.into(),
            default: "key.keyboard.v".into(),
            mod_name: "Mod".into(),
            action: "Action".into(),
        })
        .collect()
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
    }
}
