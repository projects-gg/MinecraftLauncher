//! Push-to-talk's key (design §6.3).
//!
//! While the player is in a room, a hidden window gets every keyboard and mouse report through
//! Raw Input (`RIDEV_INPUTSINK`: also while the game is in front, and nothing is taken from the
//! game). Only the chosen key's state is kept: no other key is looked at, counted or stored.

use serde::{Deserialize, Serialize};

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
