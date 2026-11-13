//! Push-to-talk's key and the voice shortcuts (design §6.3).
//!
//! While the player is in a room, a hidden window gets every keyboard and mouse report through
//! Raw Input (`RIDEV_INPUTSINK`: also while the game is in front, and nothing is taken from the
//! game). Only the chosen keys' states are kept: no other key is looked at, counted or stored.
//!
//! - [`Shortcuts`]: the product's watch (push-to-talk, mute, deafen), keys by scan code
//!   ([`crate::keys`]); a combination ("Ctrl + Shift + M") asks which modifiers are held only when
//!   its key goes down ([`Held`], the system's own key state: nothing is tracked or polled);
//! - [`KeyWatch`]: VOICE-0's probe (one key by virtual-key code, debug builds).
//!
//! [`needs_input`] is the rule for whether anything is registered at all.

use serde::{Deserialize, Serialize};

use crate::keys::{E0, ScanCode, scan_code};
use crate::settings::{VoiceKey, VoiceModifier, VoiceMouseButton};
use crate::view::VoiceShortcut;

/// The modifier keys held at the moment a combination's key went down (left and right alike).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Held {
    pub ctrl: bool,
    pub shift: bool,
    /// Left Alt.
    pub alt: bool,
    /// Right Alt: AltGr on most layouts (Windows adds a Left Ctrl to it), so the player is typing
    /// a character ("@" on a Turkish Q): no combination fires.
    pub alt_gr: bool,
}

impl Held {
    /// Exactly `mods` are held (Ctrl+M does not fire on Ctrl+Shift+M).
    fn exactly(self, mods: &[VoiceModifier]) -> bool {
        !self.alt_gr
            && self.ctrl == mods.contains(&VoiceModifier::Ctrl)
            && self.shift == mods.contains(&VoiceModifier::Shift)
            && self.alt == mods.contains(&VoiceModifier::Alt)
    }
}

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
            VoiceKey::Mouse { button, .. } => Some(match button {
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

/// One watched shortcut: its key, the modifiers it wants (none: fires whatever is held) and
/// whether it is down now.
#[derive(Debug, Clone)]
struct Entry {
    shortcut: VoiceShortcut,
    watched: Watched,
    mods: Vec<VoiceModifier>,
    down: bool,
}

/// The shortcuts' keys and their states.
#[derive(Debug, Clone, Default)]
pub struct Shortcuts {
    keys: Vec<Entry>,
}

impl Shortcuts {
    /// Watches `keys` (keys the launcher cannot watch are left out).
    pub fn new(keys: &[(VoiceShortcut, VoiceKey)]) -> Self {
        Self {
            keys: keys
                .iter()
                .filter_map(|(shortcut, key)| {
                    Watched::of(key).map(|watched| Entry {
                        shortcut: *shortcut,
                        watched,
                        mods: key.mods().to_vec(),
                        down: false,
                    })
                })
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
            .any(|entry| matches!(entry.watched, Watched::Mouse { .. }))
    }

    /// A keyboard report: its make code and flags (`RI_KEY_BREAK` 1, `RI_KEY_E0` 2). Held keys'
    /// repeats change nothing. `held` is asked at most once, only when a combination's key goes
    /// down.
    pub fn keyboard(
        &mut self,
        make: u16,
        flags: u16,
        held: impl FnOnce() -> Held,
    ) -> Vec<KeyChange> {
        self.keyboard_typing(make, flags, held, false)
    }

    /// As [`Self::keyboard`]; `typing` (SQOL): a text field of the launcher has the keyboard now, so
    /// a shortcut on a key without modifiers does not go down (typing "v" never opens push-to-talk,
    /// "m" never mutes); a key that was down still comes up.
    pub fn keyboard_typing(
        &mut self,
        make: u16,
        flags: u16,
        held: impl FnOnce() -> Held,
        typing: bool,
    ) -> Vec<KeyChange> {
        let code = Watched::Scan(ScanCode {
            make,
            extended: flags & E0 != 0,
        });
        let down = flags & 1 == 0;
        let mut held = Lazy::new(held);
        let mut changes = Vec::new();
        for entry in &mut self.keys {
            if entry.watched != code || (typing && down && entry.mods.is_empty()) {
                continue;
            }
            entry.press(down, &mut held, &mut changes);
        }
        changes
    }

    /// A mouse report's button flags; a press and its release may come in one report.
    pub fn mouse(&mut self, flags: u16, held: impl FnOnce() -> Held) -> Vec<KeyChange> {
        let mut held = Lazy::new(held);
        let mut changes = Vec::new();
        for entry in &mut self.keys {
            let Watched::Mouse { down, up } = entry.watched else {
                continue;
            };
            for (bit, pressed) in [(down, true), (up, false)] {
                if flags & bit != 0 {
                    entry.press(pressed, &mut held, &mut changes);
                }
            }
        }
        changes
    }

    /// Every held key let go (the watch stops, or the window lost its input): ups for the held.
    pub fn release_all(&mut self) -> Vec<KeyChange> {
        let mut changes = Vec::new();
        for entry in &mut self.keys {
            if entry.down {
                entry.down = false;
                changes.push(KeyChange {
                    shortcut: entry.shortcut,
                    down: false,
                });
            }
        }
        changes
    }
}

impl Entry {
    /// Its key went down or up. A combination goes down only with exactly its modifiers held; it
    /// comes up with its key (a modifier let go first changes nothing).
    fn press(
        &mut self,
        down: bool,
        held: &mut Lazy<impl FnOnce() -> Held>,
        changes: &mut Vec<KeyChange>,
    ) {
        if self.down == down {
            return;
        }
        if down && !self.mods.is_empty() && !held.get().exactly(&self.mods) {
            return;
        }
        self.down = down;
        changes.push(KeyChange {
            shortcut: self.shortcut,
            down,
        });
    }
}

/// [`Held`] asked once, the first time it is needed.
struct Lazy<F> {
    ask: Option<F>,
    known: Held,
}

impl<F: FnOnce() -> Held> Lazy<F> {
    fn new(ask: F) -> Self {
        Self {
            ask: Some(ask),
            known: Held::default(),
        }
    }

    fn get(&mut self) -> Held {
        if let Some(ask) = self.ask.take() {
            self.known = ask();
        }
        self.known
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
        VoiceKey::keyboard(code, "")
    }

    /// Nothing else held.
    fn free() -> Held {
        Held::default()
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
        assert!(keys.keyboard(0x1D, 0, free).is_empty());
        assert_eq!(
            keys.keyboard(0x1D, E0, free),
            vec![KeyChange {
                shortcut: VoiceShortcut::PushToTalk,
                down: true
            }]
        );
        // Repeats while held.
        assert!(keys.keyboard(0x1D, E0, free).is_empty());
        // A single key fires whatever is held (Ctrl here), and nobody asks what is.
        assert_eq!(
            keys.keyboard(0x32, 0, || unreachable!("a single key never asks")),
            vec![KeyChange {
                shortcut: VoiceShortcut::Mute,
                down: true
            }]
        );
        assert_eq!(
            keys.keyboard(0x1D, E0 | 1, free),
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
            VoiceKey::mouse(VoiceMouseButton::Back),
        )]);
        assert!(keys.wants_mouse());
        assert!(keys.mouse(X2_DOWN, free).is_empty());
        assert!(keys.keyboard(0x2F, 0, free).is_empty());
        // A very short click: down and up in one report.
        let both = keys.mouse(X1_DOWN | X1_UP, free);
        assert_eq!(both.len(), 2);
        assert!(both[0].down && !both[1].down);
    }

    #[test]
    fn a_combination_fires_with_exactly_its_modifiers() {
        use VoiceModifier::{Ctrl, Shift};
        const M: u16 = 0x32;
        let mut keys = Shortcuts::new(&[
            (VoiceShortcut::Mute, keyboard("KeyM").with_mods(&[Ctrl])),
            (
                VoiceShortcut::Deafen,
                keyboard("KeyM").with_mods(&[Ctrl, Shift]),
            ),
        ]);
        let ctrl = || Held {
            ctrl: true,
            ..Held::default()
        };
        let ctrl_shift = || Held {
            ctrl: true,
            shift: true,
            ..Held::default()
        };
        // M alone, or typing a capital M: nothing.
        assert!(keys.keyboard(M, 0, free).is_empty());
        assert!(keys.keyboard(M, 1, free).is_empty());
        assert!(
            keys.keyboard(M, 0, || Held {
                shift: true,
                ..Held::default()
            })
            .is_empty()
        );
        keys.keyboard(M, 1, free);
        // Ctrl + M: mute only; its repeats and its release as one would expect.
        let down = keys.keyboard(M, 0, ctrl);
        assert_eq!(
            down,
            vec![KeyChange {
                shortcut: VoiceShortcut::Mute,
                down: true
            }]
        );
        assert!(keys.keyboard(M, 0, ctrl).is_empty());
        // Ctrl let go before M: M's release still lets the shortcut go.
        assert_eq!(
            keys.keyboard(M, 1, free),
            vec![KeyChange {
                shortcut: VoiceShortcut::Mute,
                down: false
            }]
        );
        // Ctrl + Shift + M: deafen only.
        assert_eq!(
            keys.keyboard(M, 0, ctrl_shift),
            vec![KeyChange {
                shortcut: VoiceShortcut::Deafen,
                down: true
            }]
        );
        keys.keyboard(M, 1, free);
        // AltGr (a Left Ctrl Windows adds to Right Alt) types a character: nothing fires.
        assert!(
            keys.keyboard(M, 0, || Held {
                ctrl: true,
                alt_gr: true,
                ..Held::default()
            })
            .is_empty()
        );
        // A mouse button with Ctrl.
        let mut mouse = Shortcuts::new(&[(
            VoiceShortcut::Mute,
            VoiceKey::mouse(VoiceMouseButton::Forward).with_mods(&[Ctrl]),
        )]);
        assert!(mouse.mouse(X2_DOWN | X2_UP, free).is_empty());
        assert_eq!(mouse.mouse(X2_DOWN | X2_UP, ctrl).len(), 2);
    }

    /// SQOL: typing in a launcher text field never fires a single-key shortcut; a combination
    /// does, and a key held down before still comes up.
    #[test]
    fn typing_leaves_single_keys_alone() {
        use VoiceModifier::Ctrl;
        const M: u16 = 0x32;
        const V_SCAN: u16 = 0x2F;
        let mut keys = Shortcuts::new(&[
            (VoiceShortcut::PushToTalk, keyboard("KeyV")),
            (VoiceShortcut::Mute, keyboard("KeyM").with_mods(&[Ctrl])),
        ]);
        let ctrl = || Held {
            ctrl: true,
            ..Held::default()
        };
        assert!(keys.keyboard_typing(V_SCAN, 0, free, true).is_empty());
        assert!(keys.keyboard_typing(V_SCAN, 1, free, true).is_empty());
        assert_eq!(keys.keyboard_typing(M, 0, ctrl, true).len(), 1);
        keys.keyboard_typing(M, 1, free, true);
        // Held before the field took the keyboard: its release lets push-to-talk go.
        assert_eq!(keys.keyboard_typing(V_SCAN, 0, free, false).len(), 1);
        assert_eq!(
            keys.keyboard_typing(V_SCAN, 1, free, true),
            vec![KeyChange {
                shortcut: VoiceShortcut::PushToTalk,
                down: false
            }]
        );
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
