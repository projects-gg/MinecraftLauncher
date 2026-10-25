//! How a launch enters the singleplayer world it targets ("Devam et").
//!
//! The game's quick play (`--quickPlaySingleplayer`) hands the world over while the game starts;
//! the game opens it once its loading overlay finishes, after its first frames. Some mods finish
//! the loading before the first frame (`data/quick-play.json`), and mods that set up on the first
//! frame then crash as the world's server starts (OVL-FIX, 25 Oct 2026: Remove Reloading Screen
//! with XaeroLib, "Registry is not frozen yet!"). From the title screen every mod has had its
//! frame, as when the player opens the world themselves. So:
//!
//! - with the launcher's own overlay mod in place, the overlay opens the world once the title
//!   screen drew a frame, whatever the mods (the launcher passes the world in a JVM property,
//!   `pl_overlay::world_jvm_arg`, instead of quick play);
//! - without it, an instance with one of the table's mods starts on the title screen and the
//!   launcher says why;
//! - otherwise quick play, as before.
//!
//! Servers keep quick play everywhere: they start no server in the game.

use std::sync::LazyLock;

use serde::Deserialize;

use crate::mods::scan::ModSnapshot;

const TABLE_JSON: &str = include_str!("../../data/quick-play.json");
const TABLE_SCHEMA: u32 = 1;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Table {
    schema: u32,
    before_first_frame: Vec<Breaker>,
}

/// A mod of `data/quick-play.json`: it opens the game before its first frame.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct Breaker {
    /// The mod id its descriptor declares.
    pub id: String,
    /// Its name, for the launcher's note when the jar names none.
    pub name: String,
    /// What goes wrong and how it was found.
    pub why: String,
}

static TABLE: LazyLock<Vec<Breaker>> =
    LazyLock::new(|| match serde_json::from_str::<Table>(TABLE_JSON) {
        Ok(table) if table.schema == TABLE_SCHEMA => table.before_first_frame,
        Ok(table) => {
            tracing::error!(
                schema = table.schema,
                "quick play table has an unknown schema"
            );
            Vec::new()
        }
        Err(error) => {
            tracing::error!(%error, "quick play table is unreadable");
            Vec::new()
        }
    });

/// The mods with which quick play never opens a world (`data/quick-play.json`).
pub fn quick_play_breakers() -> &'static [Breaker] {
    &TABLE
}

/// How a launch enters its singleplayer world.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldEntry {
    /// The game's own quick play.
    QuickPlay,
    /// The launcher's overlay mod opens it once the title screen drew a frame.
    Overlay,
    /// The game starts on its title screen and the player opens the world there. `mods` names the
    /// installed mods that rule quick play out, for the launcher's note.
    TitleScreen { mods: Vec<String> },
}

/// `overlay`: the launcher's own overlay mod (this launcher's version) is in place with a bridge
/// for this game, so it opens the world itself whatever the mods. Otherwise the mods decide.
pub fn world_entry(overlay: bool, mods: &ModSnapshot) -> WorldEntry {
    if overlay {
        return WorldEntry::Overlay;
    }
    let found = installed_breakers(mods);
    if found.is_empty() {
        WorldEntry::QuickPlay
    } else {
        WorldEntry::TitleScreen { mods: found }
    }
}

/// The table's mods among the enabled jars (top level or nested), by the name their jar gives
/// (else the table's), each once.
pub fn installed_breakers(mods: &ModSnapshot) -> Vec<String> {
    let mut names: Vec<String> = Vec::new();
    for breaker in quick_play_breakers() {
        let found = mods
            .enabled()
            .flat_map(|jar| jar.contents.all_mods())
            .find(|m| m.answers_to(&breaker.id));
        if let Some(declared) = found {
            let name = declared
                .name
                .as_deref()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .unwrap_or(&breaker.name)
                .to_owned();
            if !names.contains(&name) {
                names.push(name);
            }
        }
    }
    names
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::mods::scan::ScannedJar;
    use crate::mods::scan::test_jars::{fabric_jar, fabric_jar_with_nested, toml_jar};
    use crate::mods::test_support::zip_bytes;

    fn snapshot(jars: &[(&str, Vec<u8>)]) -> ModSnapshot {
        ModSnapshot::new(
            "mods",
            jars.iter()
                .map(|(name, bytes)| ScannedJar::from_bytes(name, bytes))
                .collect(),
        )
    }

    fn rrls() -> Vec<u8> {
        zip_bytes(&[(
            "fabric.mod.json",
            br#"{"schemaVersion": 1, "id": "rrls", "version": "5.2.6+mc.26.1", "name": "Remove Reloading Screen", "environment": "client"}"#,
        )])
    }

    #[test]
    fn the_table_loads_and_names_remove_reloading_screen() {
        let table = quick_play_breakers();
        assert!(!table.is_empty(), "data/quick-play.json did not load");
        assert!(table.iter().any(|b| b.id == "rrls"));
        for breaker in table {
            assert!(!breaker.name.trim().is_empty() && !breaker.why.trim().is_empty());
        }
    }

    #[test]
    fn the_overlay_opens_the_world_whatever_the_mods() {
        let mods = snapshot(&[("rrls-5.2.6+mc.26.1.jar", rrls())]);
        assert_eq!(world_entry(true, &mods), WorldEntry::Overlay);
        assert_eq!(world_entry(true, &snapshot(&[])), WorldEntry::Overlay);
    }

    #[test]
    fn without_the_overlay_remove_reloading_screen_keeps_the_title_screen() {
        // The testers' set: Better PvP (with XaeroLib nested) next to Remove Reloading Screen.
        let pvp = fabric_jar_with_nested(
            "xaerobetterpvp",
            "26.1.0",
            &[(
                "xaerolib-fabric-26.1.2-1.1.15.jar",
                fabric_jar("xaerolib", "1.1.15", ""),
            )],
        );
        let mods = snapshot(&[
            ("rrls-5.2.6+mc.26.1.jar", rrls()),
            ("xaerobetterpvp-fabric-26.1.2-26.1.0.jar", pvp.clone()),
        ]);
        assert_eq!(
            world_entry(false, &mods),
            WorldEntry::TitleScreen {
                mods: vec!["Remove Reloading Screen".to_owned()]
            }
        );
        // XaeroLib alone is fine with quick play: the game draws its frames before the world.
        let xaero = snapshot(&[("xaerobetterpvp-fabric-26.1.2-26.1.0.jar", pvp)]);
        assert_eq!(world_entry(false, &xaero), WorldEntry::QuickPlay);
    }

    #[test]
    fn a_disabled_or_nested_copy_counts_as_it_loads() {
        let off = snapshot(&[("rrls-5.2.6+mc.26.1.jar.disabled", rrls())]);
        assert_eq!(world_entry(false, &off), WorldEntry::QuickPlay);
        let nested = snapshot(&[(
            "bundle-1.0.jar",
            fabric_jar_with_nested("bundle", "1.0", &[("rrls-5.2.6.jar", rrls())]),
        )]);
        assert_eq!(
            world_entry(false, &nested),
            WorldEntry::TitleScreen {
                mods: vec!["Remove Reloading Screen".to_owned()]
            }
        );
    }

    #[test]
    fn neoforge_and_nameless_jars_are_found_by_id() {
        let neo = snapshot(&[("rrls-neoforge.jar", toml_jar(true, "rrls", "5.2.6", ""))]);
        // The TOML fixture names the mod "rrls name": the jar's own name wins.
        assert_eq!(
            world_entry(false, &neo),
            WorldEntry::TitleScreen {
                mods: vec!["rrls name".to_owned()]
            }
        );
        let bare = snapshot(&[(
            "rrls.jar",
            zip_bytes(&[(
                "fabric.mod.json",
                br#"{"schemaVersion": 1, "id": "rrls", "version": "5.2.6"}"#,
            )]),
        )]);
        assert_eq!(installed_breakers(&bare), vec!["Remove Reloading Screen"]);
    }
}
