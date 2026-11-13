//! SQOL: the pure rules of a live share that changes, so they are tested here (the Windows session
//! in `win::session` follows them):
//!
//! - which changes need new encoders ([`restart_needed`]): a new picture size, or a new frame rate
//!   (the encoder's sample duration, level and rate control follow the rate it was made for);
//! - the encoders' generations ([`GenFloor`]): once new encoders start, frames the old ones still
//!   had in flight are dropped by the transport, so a watcher never decodes the old stream after the
//!   new keyframe;
//! - waiting for the game's window ([`Wait`]): Minecraft recreates its window when it goes
//!   fullscreen; a new window within [`GAME_REACQUIRE`] is captured again without a word to the
//!   player, and a game that is still starting gets up to [`GAME_WINDOW_WAIT`] when the player (or
//!   the launcher, for a game it just started) asked to share it.

use std::time::{Duration, Instant};

use crate::preset::LayerSpec;

/// The game's window closed: a new window of the game this soon is the same share going on (a
/// fullscreen switch recreates it); later, the source is gone.
pub const GAME_REACQUIRE: Duration = Duration::from_secs(3);
/// Asked to share a game that has no window yet (it is loading): wait this long for it. The voice
/// machine ends a share whose source is gone after 60 s anyway.
pub const GAME_WINDOW_WAIT: Duration = Duration::from_secs(55);
/// How often the game's windows are looked at while it recreates its window.
pub const REACQUIRE_EVERY: Duration = Duration::from_millis(250);
/// How often a starting game's windows are looked at.
pub const WINDOW_WAIT_EVERY: Duration = Duration::from_millis(500);

/// New encoders are needed for `new` layers (a size or a frame rate changed); otherwise the
/// running ones go on and a keyframe marks the change.
pub fn restart_needed(old: &[LayerSpec; 2], new: &[LayerSpec; 2]) -> bool {
    old.iter()
        .zip(new)
        .any(|(a, b)| a.w != b.w || a.h != b.h || a.fps != b.fps)
}

/// The oldest encoder generation the transport still sends frames of.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct GenFloor(u32);

impl GenFloor {
    /// New encoders of `generation` run: anything older is stale from now on.
    pub fn raise(&mut self, generation: u32) {
        self.0 = self.0.max(generation);
    }

    pub fn admits(&self, generation: u32) -> bool {
        generation >= self.0
    }
}

/// Waiting for the game's window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Wait {
    pub pid: u32,
    /// The player (or the launcher) asked for this game: the answer is `Switched` or
    /// `SwitchFailed`. Otherwise its window was recreated: found, the share goes on unasked;
    /// not found, the source is gone.
    pub asked: bool,
    until: Instant,
    next: Instant,
    every: Duration,
}

impl Wait {
    /// The shared game's window closed just now.
    pub fn reacquire(pid: u32, now: Instant) -> Self {
        Self {
            pid,
            asked: false,
            until: now + GAME_REACQUIRE,
            next: now + REACQUIRE_EVERY,
            every: REACQUIRE_EVERY,
        }
    }

    /// Asked to share the game, which has no window yet.
    pub fn switch(pid: u32, now: Instant) -> Self {
        Self {
            pid,
            asked: true,
            until: now + GAME_WINDOW_WAIT,
            next: now + WINDOW_WAIT_EVERY,
            every: WINDOW_WAIT_EVERY,
        }
    }

    /// When to look next.
    pub fn deadline(&self) -> Instant {
        self.next
    }

    /// Time to look for the window.
    pub fn due(&self, now: Instant) -> bool {
        now >= self.next
    }

    /// The window was not there at `now`: true to look again later, false to give up.
    pub fn missed(&mut self, now: Instant) -> bool {
        if now >= self.until {
            return false;
        }
        self.next = (now + self.every).min(self.until);
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{Preset, ShareOpts};
    use crate::preset::layers;

    fn opts(preset: Preset, fps: u8) -> ShareOpts {
        ShareOpts {
            preset,
            fps,
            ..ShareOpts::default()
        }
    }

    #[test]
    fn new_encoders_only_for_a_new_size_or_frame_rate() {
        let normal = layers((1920, 1080), &opts(Preset::Normal, 30), false);
        // The same layers: the encoders go on (a keyframe marks a new source of the same size).
        assert!(!restart_needed(
            &normal,
            &layers((2560, 1440), &opts(Preset::Normal, 30), false)
        ));
        // Another size: a 4:3 window after a 16:9 one, or another quality.
        assert!(restart_needed(
            &normal,
            &layers((1024, 768), &opts(Preset::Normal, 30), false)
        ));
        assert!(restart_needed(
            &normal,
            &layers((1920, 1080), &opts(Preset::Low, 30), false)
        ));
        // Another frame rate (15 fps keeps the size).
        assert!(restart_needed(
            &normal,
            &layers((1920, 1080), &opts(Preset::Normal, 15), false)
        ));
        let high = layers((1920, 1080), &opts(Preset::High, 30), false);
        assert!(restart_needed(
            &high,
            &layers((1920, 1080), &opts(Preset::High, 60), false)
        ));
    }

    #[test]
    fn stale_frames_of_old_encoders_are_dropped() {
        let mut floor = GenFloor::default();
        assert!(floor.admits(0) && floor.admits(1));
        floor.raise(2);
        assert!(!floor.admits(1));
        assert!(floor.admits(2) && floor.admits(3));
        // A late, lower raise changes nothing.
        floor.raise(1);
        assert!(!floor.admits(1));
    }

    #[test]
    fn a_recreated_game_window_is_looked_for_three_seconds() {
        let t0 = Instant::now();
        let mut wait = Wait::reacquire(7, t0);
        assert!(!wait.asked);
        assert!(!wait.due(t0));
        assert_eq!(wait.deadline(), t0 + REACQUIRE_EVERY);
        assert!(wait.due(t0 + REACQUIRE_EVERY));
        assert!(wait.missed(t0 + REACQUIRE_EVERY));
        assert_eq!(wait.deadline(), t0 + REACQUIRE_EVERY * 2);
        // The last look is at the end itself, then it gives up.
        assert!(wait.missed(t0 + GAME_REACQUIRE - Duration::from_millis(100)));
        assert_eq!(wait.deadline(), t0 + GAME_REACQUIRE);
        assert!(!wait.missed(t0 + GAME_REACQUIRE));
    }

    #[test]
    fn a_starting_game_is_waited_for_longer() {
        let t0 = Instant::now();
        let mut wait = Wait::switch(9, t0);
        assert!(wait.asked);
        assert!(wait.missed(t0 + Duration::from_secs(30)));
        assert_eq!(
            wait.deadline(),
            t0 + Duration::from_secs(30) + WINDOW_WAIT_EVERY
        );
        assert!(!wait.missed(t0 + GAME_WINDOW_WAIT));
    }
}
