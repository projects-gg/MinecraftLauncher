//! SQOL-O: the launcher's single-key voice shortcuts rest while a text field in a game has the
//! keyboard (the game's side of the launcher's own `voice_typing` guard: typing "v" in the chat
//! never opens push-to-talk, "m" never mutes; a combination with Ctrl, Alt or Shift still works).
//!
//! A mod whose hello lists `text-focus` hears `keys-guard` once it is greeted, and then says when
//! a text field has the keyboard while the game's window is active (`text-focus`, debounced, said
//! again every 20 s while it lasts: the mod's `TextFocusGuard.REFRESH_MS`). Each word holds for
//! [`HOLD`] at most: a lost "off" (a frozen game, a mod that went away without a word) never
//! leaves the shortcuts dead for long; the connection's end ends it at once.

use std::collections::HashMap;
use std::time::{Duration, Instant};

/// How long one "a text field has the keyboard" holds without being said again (the mod says it
/// again every 20 s, well inside).
pub const HOLD: Duration = Duration::from_secs(45);

/// Which games say a text field of theirs has the keyboard, since when (by session).
#[derive(Debug, Default)]
pub struct GameTyping {
    since: HashMap<u64, Instant>,
}

impl GameTyping {
    /// Session `session`'s game said a text field has the keyboard (`on`) or lets it go.
    pub fn set(&mut self, session: u64, on: bool, now: Instant) {
        if on {
            self.since.insert(session, now);
        } else {
            self.since.remove(&session);
        }
    }

    /// Whether a game's text field has the keyboard at `now` (its word still holds).
    pub fn typing_at(&self, now: Instant) -> bool {
        self.since
            .values()
            .any(|since| now.saturating_duration_since(*since) < HOLD)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// How often the mod says it again while it lasts (`TextFocusGuard.REFRESH_MS`).
    const REFRESH: Duration = Duration::from_secs(20);

    #[test]
    fn a_games_word_holds_until_it_lets_go_or_its_hold_ends() {
        let start = Instant::now();
        let mut typing = GameTyping::default();
        assert!(!typing.typing_at(start));
        typing.set(1, true, start);
        assert!(typing.typing_at(start + Duration::from_secs(10)));
        // A lost "off": the shortcuts come back by themselves.
        assert!(!typing.typing_at(start + HOLD));
        // Said again in time, it goes on.
        typing.set(1, true, start + REFRESH);
        assert!(typing.typing_at(start + HOLD));
        typing.set(1, false, start + HOLD);
        assert!(!typing.typing_at(start + HOLD));
        // Two games: either one typing is enough; one letting go leaves the other's.
        typing.set(1, true, start);
        typing.set(2, true, start);
        typing.set(1, false, start);
        assert!(typing.typing_at(start + Duration::from_secs(1)));
        typing.set(2, false, start);
        assert!(!typing.typing_at(start));
        // Letting go of nothing changes nothing.
        typing.set(3, false, start);
        assert!(!typing.typing_at(start));
    }
}
