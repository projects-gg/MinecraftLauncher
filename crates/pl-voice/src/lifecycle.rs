//! When the voice window exists (design §6.1, §6.6).
//!
//! It opens when the player joins a room and closes a minute after the room was left, so a quick
//! way back costs nothing; without a call there is no window, no socket and no audio work. If its
//! WebView crashes during a call it is opened again and the room joined again, at most
//! [`MAX_REOPENS`] times in [`REOPEN_SPAN_MS`]: after that the call ends and the player sees
//! "Bağlanamadı." instead of a window that keeps dying. Times are milliseconds on any monotonic
//! clock the caller keeps.

/// How long the window stays after leaving a room.
pub const LINGER_MS: u64 = 60_000;
/// Reopens after crashes within [`REOPEN_SPAN_MS`] before the call ends.
pub const MAX_REOPENS: usize = 3;
pub const REOPEN_SPAN_MS: u64 = 5 * 60_000;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum Phase {
    /// No window.
    #[default]
    Closed,
    /// In a room.
    Open,
    /// The room was left; the window closes at `close_at` unless a room is joined first.
    Lingering { close_at: u64 },
}

/// What the caller does with the window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Action {
    None,
    Open,
    Close,
    /// Open it again and join the room again.
    Reopen,
    /// It crashed too often: the call ends ("Bağlanamadı.").
    GiveUp,
}

#[derive(Debug, Default)]
pub struct Lifecycle {
    phase: Phase,
    /// When it was reopened after a crash, the last [`REOPEN_SPAN_MS`] only.
    reopens: Vec<u64>,
}

impl Lifecycle {
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// The player joins a room (a call, an invitation, a crew room).
    pub fn join(&mut self, _now: u64) -> Action {
        match std::mem::replace(&mut self.phase, Phase::Open) {
            Phase::Closed => Action::Open,
            Phase::Open | Phase::Lingering { .. } => Action::None,
        }
    }

    /// The player left the room (or was taken out of it).
    pub fn leave(&mut self, now: u64) -> Action {
        if self.phase == Phase::Open {
            self.phase = Phase::Lingering {
                close_at: now + LINGER_MS,
            };
        }
        Action::None
    }

    /// Time passed; call at [`Lifecycle::deadline`].
    pub fn tick(&mut self, now: u64) -> Action {
        match self.phase {
            Phase::Lingering { close_at } if now >= close_at => {
                self.phase = Phase::Closed;
                Action::Close
            }
            _ => Action::None,
        }
    }

    /// The window's WebView crashed (or its page could not load).
    pub fn crashed(&mut self, now: u64) -> Action {
        match self.phase {
            Phase::Closed => Action::None,
            // Nothing to keep after leaving: the next room opens a new one.
            Phase::Lingering { .. } => {
                self.phase = Phase::Closed;
                Action::None
            }
            Phase::Open => {
                self.reopens
                    .retain(|&at| now.saturating_sub(at) < REOPEN_SPAN_MS);
                if self.reopens.len() < MAX_REOPENS {
                    self.reopens.push(now);
                    Action::Reopen
                } else {
                    self.phase = Phase::Closed;
                    self.reopens.clear();
                    Action::GiveUp
                }
            }
        }
    }

    /// When [`Lifecycle::tick`] has something to do.
    pub fn deadline(&self) -> Option<u64> {
        match self.phase {
            Phase::Lingering { close_at } => Some(close_at),
            _ => None,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn opens_on_join_and_closes_a_minute_after_leaving() {
        let mut life = Lifecycle::default();
        assert_eq!(life.tick(0), Action::None);
        assert_eq!(life.join(1_000), Action::Open);
        assert_eq!(life.join(2_000), Action::None);
        assert_eq!(life.leave(10_000), Action::None);
        assert_eq!(life.deadline(), Some(70_000));
        assert_eq!(life.tick(69_999), Action::None);
        assert_eq!(life.tick(70_000), Action::Close);
        assert_eq!(life.phase(), Phase::Closed);
        assert_eq!(life.deadline(), None);
    }

    #[test]
    fn a_quick_way_back_keeps_the_window() {
        let mut life = Lifecycle::default();
        life.join(0);
        life.leave(1_000);
        assert_eq!(life.join(30_000), Action::None);
        assert_eq!(life.phase(), Phase::Open);
        assert_eq!(life.tick(200_000), Action::None);
    }

    #[test]
    fn a_crash_during_a_call_reopens_a_few_times() {
        let mut life = Lifecycle::default();
        life.join(0);
        for at in [1_000, 2_000, 3_000] {
            assert_eq!(life.crashed(at), Action::Reopen);
        }
        assert_eq!(life.crashed(4_000), Action::GiveUp);
        assert_eq!(life.phase(), Phase::Closed);
        // A new call starts with a clean count.
        assert_eq!(life.join(5_000), Action::Open);
        assert_eq!(life.crashed(6_000), Action::Reopen);
    }

    #[test]
    fn old_crashes_are_forgotten() {
        let mut life = Lifecycle::default();
        life.join(0);
        for at in [0, 1_000, 2_000] {
            assert_eq!(life.crashed(at), Action::Reopen);
        }
        assert_eq!(life.crashed(REOPEN_SPAN_MS + 1_000), Action::Reopen);
    }

    #[test]
    fn a_crash_after_leaving_just_closes() {
        let mut life = Lifecycle::default();
        life.join(0);
        life.leave(1_000);
        assert_eq!(life.crashed(2_000), Action::None);
        assert_eq!(life.phase(), Phase::Closed);
        assert_eq!(life.crashed(3_000), Action::None);
    }
}
