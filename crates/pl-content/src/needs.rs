//! Mods that do not start without another mod although they declare that one optional (Modrinth
//! lists it as "optional", the jar's `fabric.mod.json` only `suggests` it). A check that trusts
//! the declared dependencies passes a set the game then crashes with, so Studio's set editor adds
//! the needed mod with the one that needs it, and its compatibility check and publish check ask
//! for it ([`hidden_needs`]).
//!
//! A curated table rather than a guess from the jar: mods refer to their optional dependencies'
//! classes all the time (integrations that only load when the other mod is there), so a jar's
//! references cannot tell a real need from a safe one; only a crash does. A row is added when a
//! set crashes this way, with what showed it, and dropped once the mod declares the need itself
//! (Modrinth's own dependency covers it then).

/// One mod that needs another in practice ([`HIDDEN_NEEDS`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct HiddenNeed {
    /// Modrinth project of the mod.
    pub project: &'static str,
    pub name: &'static str,
    /// Modrinth project it cannot start without.
    pub needs: &'static str,
    pub needs_name: &'static str,
}

/// The table, one row per (mod, needed mod).
pub const HIDDEN_NEEDS: &[HiddenNeed] = &[
    // E2E4 (20 October 2026): a new PvP set's instance crashed as it started with
    // `NoClassDefFoundError: dev/isxander/yacl3/...` from TipTapShow 2.0.0's config class
    // (Minecraft 26.1.2, 26.2 and 26.3); its jar names YACL only in `suggests`.
    HiddenNeed {
        project: "lIQY64qr",
        name: "TipTapShow",
        needs: "1eAoo2KR",
        needs_name: "YetAnotherConfigLib",
    },
];

/// What the mod of Modrinth `project` needs in practice beyond its declared dependencies.
pub fn hidden_needs(project: &str) -> impl Iterator<Item = &'static HiddenNeed> + '_ {
    HIDDEN_NEEDS
        .iter()
        .filter(move |need| need.project == project)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rows_name_modrinth_projects_once() {
        let is_project_id =
            |id: &str| id.len() == 8 && id.chars().all(|c| c.is_ascii_alphanumeric());
        for (at, need) in HIDDEN_NEEDS.iter().enumerate() {
            assert!(is_project_id(need.project), "{need:?}");
            assert!(is_project_id(need.needs), "{need:?}");
            assert_ne!(need.project, need.needs);
            assert!(!need.name.is_empty() && !need.needs_name.is_empty());
            assert!(
                !HIDDEN_NEEDS[..at]
                    .iter()
                    .any(|other| other.project == need.project && other.needs == need.needs),
                "{need:?} is listed twice"
            );
        }
        let tiptap: Vec<&str> = hidden_needs("lIQY64qr").map(|need| need.needs).collect();
        assert_eq!(tiptap, ["1eAoo2KR"]);
        assert_eq!(hidden_needs("AANobbMI").count(), 0);
    }
}
