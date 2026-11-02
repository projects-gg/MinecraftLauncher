//! Push-to-talk's key and the voice shortcuts (design §6.3).
//!
//! While the player is in a room, a hidden window gets every keyboard and mouse report through
//! Raw Input (`RIDEV_INPUTSINK`: also while the game is in front, and nothing is taken from the
//! game). Only the chosen keys' states are kept: no other key is looked at, counted or stored.
//!
//! - [`Shortcuts`]: the product's watch (push-to-talk, mute, deafen), keys by scan code
//!   ([`crate::keys`]);
//! - [`KeyWatch`]: VOICE-0's probe (one key by virtual-key code, debug builds).
//!
//! [`needs_input`] is the rule for whether anything is registered at all.

use serde::{Deserialize, Serialize};

use crate::keys::{E0, ScanCode, scan_code};
use crate::settings::{VoiceKey, VoiceMouseButton};
use crate::view::VoiceShortcut;

/// Raw Input is registered only while the player is in a room (or joining one) and at least one
/// shortcut needs watching: without a call no input is ever looked at.
pub fn needs_input(in_room: bool, shortcuts: usize) -> bool {
    in_room && shortcuts > 0
}

/// A watched key as Raw Input reports it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Watched {
    Scan(ScanCode),
    Mouse { down: u16, up: u16 },
}

impl Watched {
    fn of(key: &VoiceKey) -> Option<Self> {
        match key {
            VoiceKey::Keyboard { code, .. } => scan_code(code).map(Self::Scan),
            VoiceKey::Mouse { button } => Some(match button {
                VoiceMouseButton::Middle => Self::Mouse {
                    down: MIDDLE_DOWN,
                    up: MIDDLE_UP,
                },
                VoiceMouseButton::Back => Self::Mouse {
                    down: X1_DOWN,
                    up: X1_UP,
                },
                VoiceMouseButton::Forward => Self::Mouse {
                    down: X2_DOWN,
                    up: X2_UP,
                },
            }),
        }
    }
}

/// A shortcut's key went down or up.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KeyChange {
    pub shortcut: VoiceShortcut,
    pub down: bool,
}

/// The shortcuts' keys and their states.
#[derive(Debug, Clone, Default)]
pub struct Shortcuts {
    keys: Vec<(VoiceShortcut, Watched, bool)>,
}

impl Shortcuts {
    /// Watches `keys` (keys the launcher cannot watch are left out).
    pub fn new(keys: &[(VoiceShortcut, VoiceKey)]) -> Self {
        Self {
            keys: keys
                .iter()
                .filter_map(|(shortcut, key)| Watched::of(key).map(|w| (*shortcut, w, false)))
                .collect(),
        }
    }

    pub fn len(&self) -> usize {
        self.keys.len()
    }

    pub fn is_empty(&self) -> bool {
        self.keys.is_empty()
    }

    /// Whether any watched key is a mouse button (the mouse is registered only then).
    pub fn wants_mouse(&self) -> bool {
        self.keys
            .iter()
            .any(|(_, watched, _)| matches!(watched, Watched::Mouse { .. }))
    }

    /// A keyboard report: its make code and flags (`RI_KEY_BREAK` 1, `RI_KEY_E0` 2). Held keys'
    /// repeats change nothing.
    pub fn keyboard(&mut self, make: u16, flags: u16) -> Vec<KeyChange> {
        let code = ScanCode {
            make,
            extended: flags & E0 != 0,
        };
        let down = flags & 1 == 0;
        let mut changes = Vec::new();
        for (shortcut, watched, state) in &mut self.keys {
            if *watched == Watched::Scan(code) && *state != down {
                *state = down;
                changes.push(KeyChange {
                    shortcut: *shortcut,
                    down,
                });
            }
        }
        changes
    }

    /// A mouse report's button flags; a press and its release may come in one report.
    pub fn mouse(&mut self, flags: u16) -> Vec<KeyChange> {
        let mut changes = Vec::new();
        for (shortcut, watched, state) in &mut self.keys {
            let Watched::Mouse { down, up } = *watched else {
                continue;
            };
            for (bit, pressed) in [(down, true), (up, false)] {
                if flags & bit != 0 && *state != pressed {
                    *state = pressed;
                    changes.push(KeyChange {
                        shortcut: *shortcut,
                        down: pressed,
                    });
                }
            }
        }
        changes
    }

    /// Every held key let go (the watch stops, or the window lost its input): ups for the held.
    pub fn release_all(&mut self) -> Vec<KeyChange> {
        let mut changes = Vec::new();
        for (shortcut, _, state) in &mut self.keys {
            if *state {
                *state = false;
                changes.push(KeyChange {
                    shortcut: *shortcut,
                    down: false,
                });
            }
        }
        changes
    }
}

/// The push-to-talk key: a keyboard key (Windows virtual-key code) or a mouse button.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum Key {
    Keyboard(u16),
    MouseMiddle,
    /// The back side button (Windows' XBUTTON1).
    MouseX1,
    /// The forward side button (XBUTTON2).
    MouseX2,
}

/// Raw Input's mouse button flags (`usButtonFlags`, `RI_MOUSE_*`).
const MIDDLE_DOWN: u16 = 0x0010;
const MIDDLE_UP: u16 = 0x0020;
const X1_DOWN: u16 = 0x0040;
const X1_UP: u16 = 0x0080;
const X2_DOWN: u16 = 0x0100;
const X2_UP: u16 = 0x0200;

/// The chosen key's state, fed with every report.
#[derive(Debug, Clone)]
pub struct KeyWatch {
    key: Key,
    down: bool,
    presses: u64,
}

impl KeyWatch {
    pub fn new(key: Key) -> Self {
        Self {
            key,
            down: false,
            presses: 0,
        }
    }

    pub fn is_down(&self) -> bool {
        self.down
    }

    /// Presses since the watch began.
    pub fn presses(&self) -> u64 {
        self.presses
    }

    /// A keyboard report: its virtual-key code and whether the key went up (`RI_KEY_BREAK`).
    /// True when the chosen key changed (a held key's repeats change nothing).
    pub fn keyboard(&mut self, vkey: u16, released: bool) -> bool {
        match self.key {
            Key::Keyboard(key) if key == vkey => self.set(!released),
            _ => false,
        }
    }

    /// A mouse report's button flags. A report may carry a press and a release at once (a very
    /// short click): both count, in that order.
    pub fn mouse(&mut self, flags: u16) -> bool {
        let (down, up) = match self.key {
            Key::MouseMiddle => (MIDDLE_DOWN, MIDDLE_UP),
            Key::MouseX1 => (X1_DOWN, X1_UP),
            Key::MouseX2 => (X2_DOWN, X2_UP),
            Key::Keyboard(_) => return false,
        };
        let mut changed = false;
        if flags & down != 0 {
            changed |= self.set(true);
        }
        if flags & up != 0 {
            changed |= self.set(false);
        }
        changed
    }

    fn set(&mut self, down: bool) -> bool {
        if down == self.down {
            return false;
        }
        self.down = down;
        if down {
            self.presses += 1;
        }
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const V: u16 = 0x56;

    #[test]
    fn only_the_chosen_key_counts() {
        let mut watch = KeyWatch::new(Key::Keyboard(V));
        assert!(!watch.keyboard(0x41, false));
        assert!(watch.keyboard(V, false));
        // Repeats while held.
        assert!(!watch.keyboard(V, false));
        assert!(watch.is_down());
        assert!(!watch.mouse(X1_DOWN));
        assert!(watch.keyboard(V, true));
        assert!(!watch.is_down());
        assert_eq!(watch.presses(), 1);
    }

    #[test]
    fn mouse_buttons() {
        let mut watch = KeyWatch::new(Key::MouseX1);
        assert!(!watch.keyboard(V, false));
        assert!(!watch.mouse(X2_DOWN | MIDDLE_DOWN));
        assert!(watch.mouse(X1_DOWN));
        assert!(watch.is_down());
        assert!(watch.mouse(X1_UP));
        // A press and its release in one report.
        assert!(watch.mouse(X1_DOWN | X1_UP));
        assert!(!watch.is_down());
        assert_eq!(watch.presses(), 2);
        // Movement only.
        assert!(!watch.mouse(0));
    }

    fn keyboard(code: &str) -> VoiceKey {
        VoiceKey::Keyboard {
            code: code.into(),
            label: String::new(),
        }
    }

    #[test]
    fn input_only_in_a_room_with_a_shortcut() {
        assert!(!needs_input(false, 3));
        assert!(!needs_input(true, 0));
        assert!(needs_input(true, 1));
        // "Konuşunca" without shortcuts registers nothing, even in a room.
        let settings = crate::settings::VoiceSettings::default();
        let shortcuts = Shortcuts::new(&settings.watched_keys());
        assert!(!needs_input(true, shortcuts.len()));
    }

    #[test]
    fn shortcuts_follow_their_scan_codes() {
        let mut keys = Shortcuts::new(&[
            (VoiceShortcut::PushToTalk, keyboard("ControlRight")),
            (VoiceShortcut::Mute, keyboard("KeyM")),
            (VoiceShortcut::Deafen, keyboard("Hyper")),
        ]);
        assert_eq!(keys.len(), 2);
        assert!(!keys.wants_mouse());
        // Left Ctrl (no E0) is not Right Ctrl.
        assert!(keys.keyboard(0x1D, 0).is_empty());
        assert_eq!(
            keys.keyboard(0x1D, E0),
            vec![KeyChange {
                shortcut: VoiceShortcut::PushToTalk,
                down: true
            }]
        );
        // Repeats while held.
        assert!(keys.keyboard(0x1D, E0).is_empty());
        assert_eq!(
            keys.keyboard(0x32, 0),
            vec![KeyChange {
                shortcut: VoiceShortcut::Mute,
                down: true
            }]
        );
        assert_eq!(
            keys.keyboard(0x1D, E0 | 1),
            vec![KeyChange {
                shortcut: VoiceShortcut::PushToTalk,
                down: false
            }]
        );
        // Stopping lets the held mute key go.
        assert_eq!(
            keys.release_all(),
            vec![KeyChange {
                shortcut: VoiceShortcut::Mute,
                down: false
            }]
        );
    }

    #[test]
    fn mouse_shortcuts() {
        let mut keys = Shortcuts::new(&[(
            VoiceShortcut::PushToTalk,
            VoiceKey::Mouse {
                button: VoiceMouseButton::Back,
            },
        )]);
        assert!(keys.wants_mouse());
        assert!(keys.mouse(X2_DOWN).is_empty());
        assert!(keys.keyboard(0x2F, 0).is_empty());
        // A very short click: down and up in one report.
        let both = keys.mouse(X1_DOWN | X1_UP);
        assert_eq!(both.len(), 2);
        assert!(both[0].down && !both[1].down);
    }

    #[test]
    fn written_as_settings_write_it() {
        assert_eq!(
            serde_json::to_string(&Key::Keyboard(V)).unwrap(),
            r#"{"keyboard":86}"#
        );
        assert_eq!(
            serde_json::from_str::<Key>(r#""mouseX2""#).unwrap(),
            Key::MouseX2
        );
    }
}
