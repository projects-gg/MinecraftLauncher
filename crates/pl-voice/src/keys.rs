//! Shortcut keys (design §6.3): the page names a keyboard key by its place (`KeyboardEvent.code`),
//! Raw Input reports a key by its scan code; [`scan_code`] joins them, so a key picked on the
//! settings page is the same physical key in the game whatever the keyboard's layout. [`check`]
//! tells the key picker's single line: Simple Voice Chat's keys, the overlay's and the pack's mod
//! keys, the game's own keys, Left Alt (Alt+Shift switches the keyboard's language), and a mute or
//! deafen key that also fires while the player types (VOICE-KEYS). Mute and deafen may hold Ctrl,
//! Shift or Alt with their key ("Ctrl + Shift + M"); push-to-talk is one key or mouse button.

use crate::settings::{VoiceKey, VoiceModifier, VoiceMouseButton, VoiceSettings};
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

/// The page's name of a key by its scan code (the reverse of [`scan_code`]).
pub fn code_of(scan: ScanCode) -> Option<String> {
    let ScanCode { make, extended } = scan;
    if !extended {
        match make {
            0x02..=0x0A => return Some(format!("Digit{}", make - 1)),
            0x0B => return Some("Digit0".to_owned()),
            0x3B..=0x44 => return Some(format!("F{}", make - 0x3A)),
            0x57 => return Some("F11".to_owned()),
            0x58 => return Some("F12".to_owned()),
            0x64..=0x6E => return Some(format!("F{}", make - 0x64 + 13)),
            0x76 => return Some("F24".to_owned()),
            _ => {}
        }
    }
    NAMED
        .iter()
        .find(|name| scan_code(name) == Some(scan))
        .map(|name| (*name).to_owned())
}

/// Every key [`scan_code`] knows by name (digits and F keys are counted, not listed).
const NAMED: &[&str] = &[
    "Escape",
    "Minus",
    "Equal",
    "Backspace",
    "Tab",
    "KeyQ",
    "KeyW",
    "KeyE",
    "KeyR",
    "KeyT",
    "KeyY",
    "KeyU",
    "KeyI",
    "KeyO",
    "KeyP",
    "BracketLeft",
    "BracketRight",
    "Enter",
    "ControlLeft",
    "KeyA",
    "KeyS",
    "KeyD",
    "KeyF",
    "KeyG",
    "KeyH",
    "KeyJ",
    "KeyK",
    "KeyL",
    "Semicolon",
    "Quote",
    "Backquote",
    "ShiftLeft",
    "Backslash",
    "KeyZ",
    "KeyX",
    "KeyC",
    "KeyV",
    "KeyB",
    "KeyN",
    "KeyM",
    "Comma",
    "Period",
    "Slash",
    "ShiftRight",
    "NumpadMultiply",
    "AltLeft",
    "Space",
    "CapsLock",
    "NumLock",
    "ScrollLock",
    "Numpad7",
    "Numpad8",
    "Numpad9",
    "NumpadSubtract",
    "Numpad4",
    "Numpad5",
    "Numpad6",
    "NumpadAdd",
    "Numpad1",
    "Numpad2",
    "Numpad3",
    "Numpad0",
    "NumpadDecimal",
    "IntlBackslash",
    "IntlRo",
    "IntlYen",
    "NumpadEnter",
    "ControlRight",
    "NumpadDivide",
    "AltRight",
    "Home",
    "ArrowUp",
    "PageUp",
    "ArrowLeft",
    "ArrowRight",
    "End",
    "ArrowDown",
    "PageDown",
    "Insert",
    "Delete",
    "ContextMenu",
];

/// Ctrl, Shift and Alt themselves: a single key may be one, a combination's key may not.
pub fn is_modifier(code: &str) -> bool {
    matches!(
        code,
        "ControlLeft" | "ControlRight" | "ShiftLeft" | "ShiftRight" | "AltLeft" | "AltRight"
    )
}

/// Combinations Windows acts on whatever else hears them: Alt+Tab, Alt+F4 (closes the game),
/// Alt+Space, Alt+Esc, Ctrl+Esc, Ctrl+Shift+Esc and Ctrl+Alt+Delete.
fn windows_takes(code: &str, mods: &[VoiceModifier]) -> bool {
    let has = |m| mods.contains(&m);
    match code {
        "Tab" | "F4" | "Space" => has(VoiceModifier::Alt),
        "Escape" => has(VoiceModifier::Alt) || has(VoiceModifier::Ctrl),
        "Delete" => has(VoiceModifier::Ctrl) && has(VoiceModifier::Alt),
        _ => false,
    }
}

/// Keys that write something while the player types: letters, digits, punctuation, Space, Enter
/// and Backspace (the number pad is left out: players bind it because it is not typed with).
fn types(code: &str) -> bool {
    code.strip_prefix("Key")
        .is_some_and(|letter| letter.len() == 1)
        || code
            .strip_prefix("Digit")
            .is_some_and(|digit| digit.len() == 1)
        || matches!(
            code,
            "Space"
                | "Enter"
                | "Backspace"
                | "Minus"
                | "Equal"
                | "BracketLeft"
                | "BracketRight"
                | "Semicolon"
                | "Quote"
                | "Backquote"
                | "Backslash"
                | "Comma"
                | "Period"
                | "Slash"
                | "IntlBackslash"
                | "IntlRo"
                | "IntlYen"
        )
}

/// Whether `shortcut` can use `key`: a keyboard key [`scan_code`] knows, or a mouse button. Held
/// modifiers only for mute and deafen (push-to-talk is one key), never on Ctrl, Shift or Alt
/// itself, and never a combination Windows keeps for itself.
pub fn usable(key: &VoiceKey, shortcut: VoiceShortcut) -> bool {
    let mods = key.mods();
    match key {
        VoiceKey::Keyboard { code, .. } => {
            scan_code(code).is_some()
                && (mods.is_empty()
                    || (shortcut != VoiceShortcut::PushToTalk
                        && !is_modifier(code)
                        && !windows_takes(code, mods)))
        }
        VoiceKey::Mouse { .. } => mods.is_empty() || shortcut != VoiceShortcut::PushToTalk,
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

/// What `key` would clash with as `shortcut`, next to the other shortcuts in `settings`. The game
/// hears a combination's key whatever is held with it (Ctrl+E still opens the inventory), so the
/// game's keys clash with modifiers too. One line: another shortcut's key first, then the game's
/// voice chat, the overlay, the game, Left Alt, and last a mute or deafen key that fires while the
/// player types (alone or with Shift alone: Shift+M is a capital M).
pub fn check(key: &VoiceKey, shortcut: VoiceShortcut, settings: &VoiceSettings) -> VoiceKeyCheck {
    if !usable(key, shortcut) {
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
        .any(|(which, other)| *which != shortcut && other.is_some_and(|other| other.overlaps(key)));
    let mods = key.mods();
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
                } else if shortcut != VoiceShortcut::PushToTalk
                    && types(code)
                    && !mods.contains(&VoiceModifier::Ctrl)
                    && !mods.contains(&VoiceModifier::Alt)
                {
                    Some(VoiceKeyClash::Typing)
                } else {
                    None
                }
            }
            // The middle button picks a block in the game.
            VoiceKey::Mouse {
                button: VoiceMouseButton::Middle,
                ..
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
        VoiceKey::keyboard(code, "")
    }

    #[test]
    fn scan_codes_name_their_keys_back() {
        for code in NAMED
            .iter()
            .copied()
            .chain(["Digit0", "Digit5", "F1", "F12", "F13", "F24"])
        {
            let scan = scan_code(code).unwrap();
            assert_eq!(code_of(scan).as_deref(), Some(code));
        }
        assert_eq!(
            code_of(ScanCode {
                make: 0x7F,
                extended: true
            }),
            None
        );
    }

    #[test]
    fn combinations_are_for_mute_and_deafen() {
        use VoiceModifier::{Alt, Ctrl, Shift};
        let ctrl_shift_m = keyboard("KeyM").with_mods(&[Ctrl, Shift]);
        assert!(usable(&ctrl_shift_m, VoiceShortcut::Mute));
        assert!(usable(&ctrl_shift_m, VoiceShortcut::Deafen));
        assert!(!usable(&ctrl_shift_m, VoiceShortcut::PushToTalk));
        assert!(usable(
            &VoiceKey::mouse(VoiceMouseButton::Back).with_mods(&[Ctrl]),
            VoiceShortcut::Mute
        ));
        // Not on a modifier itself, nor what Windows keeps.
        assert!(!usable(
            &keyboard("ShiftLeft").with_mods(&[Ctrl]),
            VoiceShortcut::Mute
        ));
        for (code, mods) in [
            ("F4", &[Alt][..]),
            ("Tab", &[Alt, Shift][..]),
            ("Escape", &[Ctrl, Shift][..]),
            ("Delete", &[Ctrl, Alt][..]),
            ("Space", &[Alt][..]),
        ] {
            assert!(
                !usable(&keyboard(code).with_mods(mods), VoiceShortcut::Deafen),
                "{code}"
            );
        }
        assert!(usable(
            &keyboard("F4").with_mods(&[Ctrl]),
            VoiceShortcut::Deafen
        ));
    }

    #[test]
    fn a_typing_key_warns_unless_ctrl_or_alt_is_held() {
        use VoiceModifier::{Alt, Ctrl, Shift};
        let settings = VoiceSettings::default();
        let clash = |key: VoiceKey, shortcut| check(&key, shortcut, &settings).clash;
        for code in [
            "KeyM",
            "Digit0",
            "Comma",
            "Space",
            "Enter",
            "Backspace",
            "Quote",
        ] {
            let warned = if GAME.contains(&code) {
                VoiceKeyClash::Game
            } else {
                VoiceKeyClash::Typing
            };
            assert_eq!(
                clash(keyboard(code), VoiceShortcut::Mute),
                Some(warned),
                "{code}"
            );
        }
        // Shift alone still types (a capital letter); Ctrl or Alt do not.
        assert_eq!(
            clash(keyboard("KeyM").with_mods(&[Shift]), VoiceShortcut::Deafen),
            Some(VoiceKeyClash::Typing)
        );
        assert_eq!(
            clash(
                keyboard("KeyM").with_mods(&[Ctrl, Shift]),
                VoiceShortcut::Mute
            ),
            None
        );
        assert_eq!(
            clash(keyboard("KeyM").with_mods(&[Alt]), VoiceShortcut::Mute),
            None
        );
        // Push-to-talk is held while speaking, never toggled: no warning there.
        assert_eq!(clash(keyboard("KeyM"), VoiceShortcut::PushToTalk), None);
        // Keys that do not type stay quiet.
        assert_eq!(clash(keyboard("F8"), VoiceShortcut::Mute), None);
        assert_eq!(clash(keyboard("Numpad5"), VoiceShortcut::Mute), None);
        // The game hears a combination's key all the same.
        assert_eq!(
            clash(keyboard("KeyE").with_mods(&[Ctrl]), VoiceShortcut::Mute),
            Some(VoiceKeyClash::Game)
        );
    }

    #[test]
    fn a_combination_is_taken_only_when_it_would_fire_with_another() {
        use VoiceModifier::{Ctrl, Shift};
        let settings = VoiceSettings {
            mute_key: Some(keyboard("KeyM").with_mods(&[Ctrl])),
            ..VoiceSettings::default()
        };
        let clash = |key: VoiceKey| check(&key, VoiceShortcut::Deafen, &settings).clash;
        assert_eq!(
            clash(keyboard("KeyM").with_mods(&[Ctrl])),
            Some(VoiceKeyClash::Taken)
        );
        assert_eq!(clash(keyboard("KeyM")), Some(VoiceKeyClash::Taken));
        assert_eq!(clash(keyboard("KeyM").with_mods(&[Ctrl, Shift])), None);
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
        let middle = VoiceKey::mouse(VoiceMouseButton::Middle);
        assert_eq!(
            check(&middle, ptt, &settings).clash,
            Some(VoiceKeyClash::Game)
        );
        let back = VoiceKey::mouse(VoiceMouseButton::Back);
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
            mute_key: Some(keyboard("F8")),
            ..VoiceSettings::default()
        };
        assert_eq!(
            check(&keyboard("F8"), VoiceShortcut::PushToTalk, &settings).clash,
            Some(VoiceKeyClash::Taken)
        );
        // Its own key is not taken.
        assert_eq!(
            check(&keyboard("F8"), VoiceShortcut::Mute, &settings).clash,
            None
        );
    }
}
