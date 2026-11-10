//! The overlay mod's client tweaks (DIALOG-FOCUS): small fixes to the game's own screens, each with
//! its own switch in Settings → Genel, on by default. They live in the overlay's jar, so a tweak
//! that is on brings the jar into a game too ([`crate::OverlayConfig::wants_mod`]), with the panel
//! and the chat's emoji picker off as well (the mod's third mode).
//!
//! The mod reads which are on from one launch property ([`TWEAKS_ARG`], [`ClientTweaks::jvm_arg`])
//! on its own: they run with or without the bridge (the launcher closed after the start with
//! "Kapansın", or never reachable). A mod from before the tweaks ignores the property, and a mod
//! older than a tweak skips its id: nothing a launcher relies on.
//!
//! Adding a tweak: a field below with its id ([`ClientTweaks::ids`], [`ClientTweaks::OFF`]), the
//! mod's class and its line in the mod's `ClientTweaks.tweaks()`, and the settings row with its
//! words (`features/overlay`, `locales/*-tweaks.ts`).

use serde::{Deserialize, Serialize};

/// The launch property naming the tweaks on, ids separated by commas
/// (`-Dprojects.overlay.tweaks=dialog-focus`; the mod's `ClientTweaks.PROPERTY`). Never passed
/// while every tweak is off: the mod's default is none.
pub const TWEAKS_ARG: &str = "-Dprojects.overlay.tweaks=";

/// [`ClientTweaks::dialog_focus`]'s id (the mod's `DialogFocus.ID`).
pub const DIALOG_FOCUS: &str = "dialog-focus";

/// The player's switches of the client tweaks ([`crate::OverlayConfig::tweaks`]). A field the page
/// does not send keeps its default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ClientTweaks {
    /// "Sunucu Pencerelerinde İlk Yazı Kutusunu Seç" (on by default): a server window (the game's
    /// dialogs) with a text box opens with its first box ready to type in.
    pub dialog_focus: bool,
}

impl Default for ClientTweaks {
    fn default() -> Self {
        Self::ON
    }
}

impl ClientTweaks {
    /// Every tweak on: the switches' defaults.
    pub const ON: Self = Self { dialog_focus: true };
    /// Every tweak off: the mod taken out of the games as a whole (the crash help's "Bu modu
    /// kapat"), or a player who had it out before the tweaks came.
    pub const OFF: Self = Self {
        dialog_focus: false,
    };

    /// The ids of the tweaks on, in the order the mod starts them.
    pub fn ids(&self) -> Vec<&'static str> {
        [(self.dialog_focus, DIALOG_FOCUS)]
            .into_iter()
            .filter_map(|(on, id)| on.then_some(id))
            .collect()
    }

    /// Any tweak is on: the mod's jar is wanted for it.
    pub fn any(&self) -> bool {
        !self.ids().is_empty()
    }

    /// The launch argument naming the tweaks on ([`TWEAKS_ARG`]); none while all are off.
    pub fn jvm_arg(&self) -> Option<String> {
        let ids = self.ids();
        (!ids.is_empty()).then(|| format!("{TWEAKS_ARG}{}", ids.join(",")))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_tweak_is_on_by_default_and_named_on_the_command_line() {
        let tweaks = ClientTweaks::default();
        assert_eq!(tweaks, ClientTweaks::ON);
        assert!(tweaks.any());
        assert_eq!(tweaks.ids(), [DIALOG_FOCUS]);
        // The mod's ClientTweaks.PROPERTY and DialogFocus.ID (ClientTweaksTest asserts the same).
        assert_eq!(
            tweaks.jvm_arg().as_deref(),
            Some("-Dprojects.overlay.tweaks=dialog-focus")
        );
        assert!(!crate::is_secret_jvm_arg(&tweaks.jvm_arg().unwrap()));
    }

    #[test]
    fn no_tweak_on_passes_nothing() {
        assert!(!ClientTweaks::OFF.any());
        assert!(ClientTweaks::OFF.ids().is_empty());
        assert_eq!(ClientTweaks::OFF.jvm_arg(), None);
    }

    #[test]
    fn the_pages_json_keeps_a_missing_switch_at_its_default() {
        let tweaks: ClientTweaks = serde_json::from_str("{}").unwrap();
        assert_eq!(tweaks, ClientTweaks::ON);
        let tweaks: ClientTweaks =
            serde_json::from_str(r#"{"dialogFocus":false,"later":true}"#).unwrap();
        assert_eq!(tweaks, ClientTweaks::OFF);
        assert_eq!(
            serde_json::to_string(&ClientTweaks::ON).unwrap(),
            r#"{"dialogFocus":true}"#
        );
    }
}
