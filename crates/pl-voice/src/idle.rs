//! The inactivity rule (design §3.5): the player leaves the room by themselves after the computer
//! went untouched and they did not speak for Studio's `idleMinutes` (30 by default): "Uzun süre
//! hareketsiz kaldığın için ayrıldın."
//!
//! Nothing polls: the launcher sets one timer at [`Idle::deadline`] and, when it fires, asks the
//! system when it was last touched; a touch or speech since moves the deadline and the timer is set
//! again. Times are milliseconds on one monotonic clock the caller keeps.

/// Studio's range for `idleMinutes`.
pub const MIN_MINUTES: u32 = 10;
pub const MAX_MINUTES: u32 = 240;
pub const DEFAULT_MINUTES: u32 = 30;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Idle {
    limit_ms: u64,
    /// When the stay began, or the player last spoke.
    last_speech: u64,
}

impl Idle {
    /// A stay that begins at `now` with Studio's `minutes` (held to its range).
    pub fn new(now: u64, minutes: u32) -> Self {
        let minutes = if minutes == 0 {
            DEFAULT_MINUTES
        } else {
            minutes.clamp(MIN_MINUTES, MAX_MINUTES)
        };
        Self {
            limit_ms: u64::from(minutes) * 60_000,
            last_speech: now,
        }
    }

    /// Studio changed `idleMinutes` during the stay.
    pub fn set_minutes(&mut self, minutes: u32) {
        *self = Self {
            last_speech: self.last_speech,
            ..Self::new(self.last_speech, minutes)
        };
    }

    /// The player's microphone sent speech.
    pub fn spoke(&mut self, now: u64) {
        self.last_speech = self.last_speech.max(now);
    }

    /// When the stay is over unless the computer is touched or the player speaks before;
    /// `last_input` is the system's last touch (keyboard, mouse, controller) on the same clock.
    pub fn deadline(&self, last_input: u64) -> u64 {
        self.last_speech
            .max(last_input)
            .saturating_add(self.limit_ms)
    }

    /// Whether the stay is over at `now`.
    pub fn due(&self, now: u64, last_input: u64) -> bool {
        now >= self.deadline(last_input)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MIN: u64 = 60_000;

    #[test]
    fn leaves_only_without_input_and_speech() {
        let mut idle = Idle::new(0, 30);
        assert_eq!(idle.deadline(0), 30 * MIN);
        assert!(!idle.due(29 * MIN, 0));
        assert!(idle.due(30 * MIN, 0));
        // A touch moves it.
        assert!(!idle.due(30 * MIN, 10 * MIN));
        assert_eq!(idle.deadline(10 * MIN), 40 * MIN);
        // So does speech.
        idle.spoke(25 * MIN);
        assert_eq!(idle.deadline(10 * MIN), 55 * MIN);
        assert!(!idle.due(54 * MIN, 0));
        // An older speech report changes nothing.
        idle.spoke(5 * MIN);
        assert_eq!(idle.deadline(0), 55 * MIN);
    }

    #[test]
    fn studios_minutes_are_held_to_their_range() {
        assert_eq!(Idle::new(0, 1).deadline(0), 10 * MIN);
        assert_eq!(Idle::new(0, 10_000).deadline(0), 240 * MIN);
        assert_eq!(Idle::new(0, 0).deadline(0), 30 * MIN);
        let mut idle = Idle::new(5 * MIN, 30);
        idle.set_minutes(60);
        assert_eq!(idle.deadline(0), 65 * MIN);
    }
}
