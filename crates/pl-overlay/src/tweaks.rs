//! The overlay mod's client tweaks (DIALOG-FOCUS, TIP-EYE): small fixes to the game's own
//! screens, each with its own switch in Settings → Genel, on by default. They live in the
//! overlay's jar, so a tweak that is on brings the jar into a game too
//! ([`crate::OverlayConfig::wants_mod`]), with the panel and the chat's emoji picker off as well
//! (the mod's third mode).
//!
//! The mod reads which are on from one launch property ([`TWEAKS_ARG`], [`ClientTweaks::jvm_arg`])
//! on its own: they run with or without the bridge (the launcher closed after the start with
//! "Kapansın", or never reachable). A mod from before the tweaks ignores the property, and a mod
//! older than a tweak skips its id: nothing a launcher relies on.
//!
//! Adding a tweak: a field below with its id ([`ClientTweaks::ids`], [`ClientTweaks::ON`],
//! [`ClientTweaks::OFF`]), the mod's class and its line in the mod's `ClientTweaks.tweaks()`, and
//! the settings row with its words (`features/overlay`, `locales/*-tweaks.ts`). A saved copy from
//! before it moves on its own ([`ClientTweaks::saved_or_off`]); the page's preferences need a
//! version (`migratePrefs`).

use serde::{Deserialize, Serialize};

/// The launch property naming the tweaks on, ids separated by commas
/// (`-Dprojects.overlay.tweaks=dialog-focus,password-eye`; the mod's `ClientTweaks.PROPERTY`).
/// Never passed while every tweak is off: the mod's default is none.
pub const TWEAKS_ARG: &str = "-Dprojects.overlay.tweaks=";

/// [`ClientTweaks::dialog_focus`]'s id (the mod's `DialogFocus.ID`).
pub const DIALOG_FOCUS: &str = "dialog-focus";

/// [`ClientTweaks::password_eye`]'s id (TIP-EYE; the mod's `PasswordEye.ID`).
pub const PASSWORD_EYE: &str = "password-eye";

/// [`ClientTweaks::tooltip_fit`]'s id (TIP-EYE; the mod's `TooltipFit.ID`).
pub const TOOLTIP_FIT: &str = "tooltip-fit";

/// The player's switches of the client tweaks ([`crate::OverlayConfig::tweaks`]). A field the page
/// does not send keeps its default.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct ClientTweaks {
    /// "Sunucu Pencerelerinde İlk Yazı Kutusunu Seç" (on by default): a server window (the game's
    /// dialogs) with a text box opens with its first box ready to type in.
    pub dialog_focus: bool,
    /// "Sunucu Pencerelerinde Şifreleri Gizle" (TIP-EYE, on by default): a server window's text box
    /// under a text that asks for a password shows dots, with an eye beside it that shows or hides
    /// what was typed.
    pub password_eye: bool,
    /// "İpuçlarını Ekrana Sığdır" (TIP-EYE, on by default): a tooltip that would run off the screen
    /// (Minecraft 26.2+ can put it above the top edge with room below) stays readable on it.
    pub tooltip_fit: bool,
}

impl Default for ClientTweaks {
    fn default() -> Self {
        Self::ON
    }
}

impl ClientTweaks {
    /// Every tweak on: the switches' defaults.
    pub const ON: Self = Self {
        dialog_focus: true,
        password_eye: true,
        tooltip_fit: true,
    };
    /// Every tweak off: the mod taken out of the games as a whole (the crash help's "Bu modu
    /// kapat"), or a player who had it out before the tweaks came.
    pub const OFF: Self = Self {
        dialog_focus: false,
        password_eye: false,
        tooltip_fit: false,
    };

    /// The ids of the tweaks on, in the order the mod starts them.
    pub fn ids(&self) -> Vec<&'static str> {
        [
            (self.dialog_focus, DIALOG_FOCUS),
            (self.password_eye, PASSWORD_EYE),
            (self.tooltip_fit, TOOLTIP_FIT),
        ]
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

    /// The switches a saved copy names (`overlay.json`'s `tweaks`; none in a file from before
    /// them), each one it does not name off: a tweak that came after the copy was written, for a
    /// player who had the mod's other switches off ([`crate::OverlayConfig::from_saved`]).
    pub fn saved_or_off(saved: Option<&serde_json::Value>) -> Self {
        let Ok(serde_json::Value::Object(mut switches)) = serde_json::to_value(Self::OFF) else {
            return Self::OFF;
        };
        if let Some(serde_json::Value::Object(named)) = saved {
            switches.extend(named.clone());
        }
        serde_json::from_value(serde_json::Value::Object(switches)).unwrap_or(Self::OFF)
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
        assert_eq!(tweaks.ids(), [DIALOG_FOCUS, PASSWORD_EYE, TOOLTIP_FIT]);
        // The mod's ClientTweaks.PROPERTY and its tweaks' ids (ClientTweaksTest asserts the same).
        assert_eq!(
            tweaks.jvm_arg().as_deref(),
            Some("-Dprojects.overlay.tweaks=dialog-focus,password-eye,tooltip-fit")
        );
        let some = ClientTweaks {
            password_eye: false,
            ..ClientTweaks::ON
        };
        assert_eq!(some.ids(), [DIALOG_FOCUS, TOOLTIP_FIT]);
        assert_eq!(
            some.jvm_arg().as_deref(),
            Some("-Dprojects.overlay.tweaks=dialog-focus,tooltip-fit")
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
        let tweaks: ClientTweaks = serde_json::from_str(
            r#"{"dialogFocus":false,"passwordEye":false,"tooltipFit":false,"later":true}"#,
        )
        .unwrap();
        assert_eq!(tweaks, ClientTweaks::OFF);
        // A page from before TIP-EYE names only its tweak: the new ones keep their default.
        let tweaks: ClientTweaks = serde_json::from_str(r#"{"dialogFocus":false}"#).unwrap();
        assert_eq!(
            tweaks,
            ClientTweaks {
                dialog_focus: false,
                ..ClientTweaks::ON
            }
        );
        assert_eq!(
            serde_json::to_string(&ClientTweaks::ON).unwrap(),
            r#"{"dialogFocus":true,"passwordEye":true,"tooltipFit":true}"#
        );
    }

    #[test]
    fn a_saved_copy_keeps_what_it_names_and_the_rest_off() {
        assert_eq!(ClientTweaks::saved_or_off(None), ClientTweaks::OFF);
        let saved = serde_json::json!({ "dialogFocus": true, "later": true });
        assert_eq!(
            ClientTweaks::saved_or_off(Some(&saved)),
            ClientTweaks {
                dialog_focus: true,
                ..ClientTweaks::OFF
            }
        );
        let saved = serde_json::to_value(ClientTweaks::ON).unwrap();
        assert_eq!(ClientTweaks::saved_or_off(Some(&saved)), ClientTweaks::ON);
    }
}
