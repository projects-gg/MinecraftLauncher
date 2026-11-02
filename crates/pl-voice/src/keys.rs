//! Shortcut keys (design §6.3): the page names a keyboard key by its place (`KeyboardEvent.code`),
//! Raw Input reports a key by its scan code; [`scan_code`] joins them, so a key picked on the
//! settings page is the same physical key in the game whatever the keyboard's layout. [`check`]
//! tells the key picker's single line: Simple Voice Chat's keys, the overlay's and the pack's mod
//! keys, the game's own keys, and Left Alt (Alt+Shift switches the keyboard's language).

use crate::settings::{VoiceKey, VoiceMouseButton, VoiceSettings};
use crate::view::{VoiceKeyCheck, VoiceKeyClash, VoiceShortcut};

/// Raw Input's flag for an extended (`E0`) scan code (`RI_KEY_E0`).
pub const E0: u16 = 0x0002;

/// A key as Raw Input reports it: the make code and whether it is extended.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct ScanCode {
    pub make: u16,
    pub extended: bool,
}

const fn plain(make: u16) -> Option<ScanCode> {
    Some(ScanCode {
        make,
        extended: false,
    })
}

const fn ext(make: u16) -> Option<ScanCode> {
    Some(ScanCode {
        make,
        extended: true,
    })
}

/// The scan code of a page's `KeyboardEvent.code`; None for keys a shortcut may not use (the
/// Windows keys, Print Screen, Pause, and names the launcher does not know).
pub fn scan_code(code: &str) -> Option<ScanCode> {
    if let Some(digit) = code.strip_prefix("Digit")
        && let [d @ b'0'..=b'9'] = digit.as_bytes()
    {
        // 1–9 are 0x02–0x0A, 0 is 0x0B.
        return plain(if *d == b'0' {
            0x0B
        } else {
            u16::from(d - b'0') + 1
        });
    }
    if let Some(number) = code.strip_prefix('F')
        && let Ok(n) = number.parse::<u16>()
    {
        return match n {
            1..=10 => plain(0x3A + n),
            11 => plain(0x57),
            12 => plain(0x58),
            13..=23 => plain(0x64 + (n - 13)),
            24 => plain(0x76),
            _ => None,
        };
    }
    match code {
        "Escape" => plain(0x01),
        "Minus" => plain(0x0C),
        "Equal" => plain(0x0D),
        "Backspace" => plain(0x0E),
        "Tab" => plain(0x0F),
        "KeyQ" => plain(0x10),
        "KeyW" => plain(0x11),
        "KeyE" => plain(0x12),
        "KeyR" => plain(0x13),
        "KeyT" => plain(0x14),
        "KeyY" => plain(0x15),
        "KeyU" => plain(0x16),
        "KeyI" => plain(0x17),
        "KeyO" => plain(0x18),
        "KeyP" => plain(0x19),
        "BracketLeft" => plain(0x1A),
        "BracketRight" => plain(0x1B),
        "Enter" => plain(0x1C),
        "ControlLeft" => plain(0x1D),
        "KeyA" => plain(0x1E),
        "KeyS" => plain(0x1F),
        "KeyD" => plain(0x20),
        "KeyF" => plain(0x21),
        "KeyG" => plain(0x22),
        "KeyH" => plain(0x23),
        "KeyJ" => plain(0x24),
        "KeyK" => plain(0x25),
        "KeyL" => plain(0x26),
        "Semicolon" => plain(0x27),
        "Quote" => plain(0x28),
        "Backquote" => plain(0x29),
        "ShiftLeft" => plain(0x2A),
        "Backslash" => plain(0x2B),
        "KeyZ" => plain(0x2C),
        "KeyX" => plain(0x2D),
        "KeyC" => plain(0x2E),
        "KeyV" => plain(0x2F),
        "KeyB" => plain(0x30),
        "KeyN" => plain(0x31),
        "KeyM" => plain(0x32),
        "Comma" => plain(0x33),
        "Period" => plain(0x34),
        "Slash" => plain(0x35),
        "ShiftRight" => plain(0x36),
        "NumpadMultiply" => plain(0x37),
        "AltLeft" => plain(0x38),
        "Space" => plain(0x39),
        "CapsLock" => plain(0x3A),
        "NumLock" => plain(0x45),
        "ScrollLock" => plain(0x46),
        "Numpad7" => plain(0x47),
        "Numpad8" => plain(0x48),
        "Numpad9" => plain(0x49),
        "NumpadSubtract" => plain(0x4A),
        "Numpad4" => plain(0x4B),
        "Numpad5" => plain(0x4C),
        "Numpad6" => plain(0x4D),
        "NumpadAdd" => plain(0x4E),
        "Numpad1" => plain(0x4F),
        "Numpad2" => plain(0x50),
        "Numpad3" => plain(0x51),
        "Numpad0" => plain(0x52),
        "NumpadDecimal" => plain(0x53),
        "IntlBackslash" => plain(0x56),
        "IntlRo" => plain(0x73),
        "IntlYen" => plain(0x7D),
        "NumpadEnter" => ext(0x1C),
        "ControlRight" => ext(0x1D),
        "NumpadDivide" => ext(0x35),
        "AltRight" => ext(0x38),
        "Home" => ext(0x47),
        "ArrowUp" => ext(0x48),
        "PageUp" => ext(0x49),
        "ArrowLeft" => ext(0x4B),
        "ArrowRight" => ext(0x4D),
        "End" => ext(0x4F),
        "ArrowDown" => ext(0x50),
        "PageDown" => ext(0x51),
        "Insert" => ext(0x52),
        "Delete" => ext(0x53),
        "ContextMenu" => ext(0x5D),
        _ => None,
    }
}

/// Whether a shortcut can use `key` (a keyboard key [`scan_code`] knows, or a mouse button).
pub fn usable(key: &VoiceKey) -> bool {
    match key {
        VoiceKey::Keyboard { code, .. } => scan_code(code).is_some(),
        VoiceKey::Mouse { .. } => true,
    }
}

/// Simple Voice Chat in the official pack: Caps Lock (its push-to-talk, `key-bindings.json`) and
/// V (its menu).
const GAME_VOICE_CHAT: &[&str] = &["CapsLock", "KeyV"];
/// The overlay (F7 panel, F9 emojis) and the pack's mod keys (`key-bindings.json`: WI Zoom C,
/// Gamma Utils X; Decent Freelook F6).
const OVERLAY: &[&str] = &["F6", "F7", "F9", "KeyC", "KeyX"];
/// The game's keys in normal play (vanilla defaults).
const GAME: &[&str] = &[
    "KeyW",
    "KeyA",
    "KeyS",
    "KeyD",
    "Space",
    "ShiftLeft",
    "ControlLeft",
    "KeyE",
    "KeyQ",
    "KeyF",
    "KeyT",
    "KeyL",
    "KeyP",
    "Slash",
    "Tab",
    "Escape",
    "Digit1",
    "Digit2",
    "Digit3",
    "Digit4",
    "Digit5",
    "Digit6",
    "Digit7",
    "Digit8",
    "Digit9",
    "F1",
    "F2",
    "F3",
    "F5",
    "F11",
];

/// What `key` would clash with as `shortcut`, next to the other shortcuts in `settings`.
pub fn check(key: &VoiceKey, shortcut: VoiceShortcut, settings: &VoiceSettings) -> VoiceKeyCheck {
    if !usable(key) {
        return VoiceKeyCheck {
            usable: false,
            clash: None,
        };
    }
    let others = [
        (
            VoiceShortcut::PushToTalk,
            settings.push_to_talk_key.as_ref(),
        ),
        (VoiceShortcut::Mute, settings.mute_key.as_ref()),
        (VoiceShortcut::Deafen, settings.deafen_key.as_ref()),
    ];
    let taken = others
        .iter()
        .any(|(which, other)| *which != shortcut && other.is_some_and(|other| other.same_key(key)));
    let clash = if taken {
        Some(VoiceKeyClash::Taken)
    } else {
        match key {
            VoiceKey::Keyboard { code, .. } => {
                let code = code.as_str();
                if GAME_VOICE_CHAT.contains(&code) {
                    Some(VoiceKeyClash::GameVoiceChat)
                } else if OVERLAY.contains(&code) {
                    Some(VoiceKeyClash::Overlay)
                } else if GAME.contains(&code) {
                    Some(VoiceKeyClash::Game)
                } else if code == "AltLeft" {
                    Some(VoiceKeyClash::LanguageSwitch)
                } else {
                    None
                }
            }
            // The middle button picks a block in the game.
            VoiceKey::Mouse {
                button: VoiceMouseButton::Middle,
            } => Some(VoiceKeyClash::Game),
            VoiceKey::Mouse { .. } => None,
        }
    };
    VoiceKeyCheck {
        usable: true,
        clash,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn keyboard(code: &str) -> VoiceKey {
        VoiceKey::Keyboard {
            code: code.into(),
            label: String::new(),
        }
    }

    #[test]
    fn page_codes_meet_raw_input_scan_codes() {
        assert_eq!(scan_code("KeyV"), plain(0x2F));
        assert_eq!(scan_code("CapsLock"), plain(0x3A));
        assert_eq!(scan_code("Digit1"), plain(0x02));
        assert_eq!(scan_code("Digit9"), plain(0x0A));
        assert_eq!(scan_code("Digit0"), plain(0x0B));
        assert_eq!(scan_code("F1"), plain(0x3B));
        assert_eq!(scan_code("F7"), plain(0x41));
        assert_eq!(scan_code("F10"), plain(0x44));
        assert_eq!(scan_code("F11"), plain(0x57));
        assert_eq!(scan_code("F13"), plain(0x64));
        assert_eq!(scan_code("F24"), plain(0x76));
        // Left and right differ by the extended flag (Ctrl, Alt) or the code (Shift).
        assert_eq!(scan_code("ControlLeft"), plain(0x1D));
        assert_eq!(scan_code("ControlRight"), ext(0x1D));
        assert_eq!(scan_code("AltRight"), ext(0x38));
        assert_eq!(scan_code("ShiftRight"), plain(0x36));
        // The number pad and the arrows share make codes; the flag tells them apart.
        assert_eq!(scan_code("Numpad8"), plain(0x48));
        assert_eq!(scan_code("ArrowUp"), ext(0x48));
        for refused in [
            "MetaLeft",
            "MetaRight",
            "PrintScreen",
            "Pause",
            "F25",
            "Fn",
            "",
        ] {
            assert_eq!(scan_code(refused), None, "{refused}");
        }
    }

    #[test]
    fn every_listed_key_is_known() {
        for code in GAME_VOICE_CHAT.iter().chain(OVERLAY).chain(GAME) {
            assert!(scan_code(code).is_some(), "{code}");
        }
    }

    #[test]
    fn the_picker_names_one_clash() {
        let settings = VoiceSettings::default();
        let ptt = VoiceShortcut::PushToTalk;
        let clash = |code: &str| check(&keyboard(code), ptt, &settings).clash;
        assert_eq!(clash("CapsLock"), Some(VoiceKeyClash::GameVoiceChat));
        assert_eq!(clash("KeyV"), Some(VoiceKeyClash::GameVoiceChat));
        assert_eq!(clash("F7"), Some(VoiceKeyClash::Overlay));
        assert_eq!(clash("F6"), Some(VoiceKeyClash::Overlay));
        assert_eq!(clash("KeyC"), Some(VoiceKeyClash::Overlay));
        assert_eq!(clash("KeyW"), Some(VoiceKeyClash::Game));
        assert_eq!(clash("Digit3"), Some(VoiceKeyClash::Game));
        assert_eq!(clash("AltLeft"), Some(VoiceKeyClash::LanguageSwitch));
        assert_eq!(clash("KeyB"), None);
        assert_eq!(clash("F8"), None);
        let middle = VoiceKey::Mouse {
            button: VoiceMouseButton::Middle,
        };
        assert_eq!(
            check(&middle, ptt, &settings).clash,
            Some(VoiceKeyClash::Game)
        );
        let back = VoiceKey::Mouse {
            button: VoiceMouseButton::Back,
        };
        assert_eq!(
            check(&back, ptt, &settings),
            VoiceKeyCheck {
                usable: true,
                clash: None
            }
        );
        assert!(!check(&keyboard("MetaLeft"), ptt, &settings).usable);
    }

    #[test]
    fn another_shortcuts_key_is_taken() {
        let settings = VoiceSettings {
            mute_key: Some(keyboard("KeyB")),
            ..VoiceSettings::default()
        };
        assert_eq!(
            check(&keyboard("KeyB"), VoiceShortcut::PushToTalk, &settings).clash,
            Some(VoiceKeyClash::Taken)
        );
        // Its own key is not taken.
        assert_eq!(
            check(&keyboard("KeyB"), VoiceShortcut::Mute, &settings).clash,
            None
        );
    }
}
