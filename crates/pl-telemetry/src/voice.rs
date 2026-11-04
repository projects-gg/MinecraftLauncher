//! Voice chat's connection quality, one summary per stay in a room, read from the states the voice
//! machine already sends the page (its phase and the player's own quality mark). Nothing of the
//! room goes anywhere: its id is only compared in memory to tell one stay from the next.

/// Where the player's stay is (`pl_voice::view::VoicePhase`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Phase {
    Joining,
    Live,
    Reconnecting,
}

/// One look at the player's room.
#[derive(Debug, Clone, Copy)]
pub struct VoiceSeen<'a> {
    /// Compared only, never kept beyond the stay.
    pub room: &'a str,
    pub crew: bool,
    pub phase: Phase,
    /// The player's connection: 0 good, 1 weak, 2 bad.
    pub quality: u8,
}

/// A finished stay.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub struct VoiceStay {
    pub crew: bool,
    /// The audio connection worked at some point.
    pub connected: bool,
    /// It never did, though the player waited for it.
    pub failed: bool,
    /// From joining to the first live audio.
    pub setup_ms: Option<u32>,
    pub live_secs: u32,
    /// Live time with a weak, and with a bad, connection.
    pub weak_secs: u32,
    pub bad_secs: u32,
    /// Times the connection dropped and came back.
    pub reconnects: u32,
}

/// A stay that never got live counts as failed only after this long: a call cancelled at once is
/// no failure.
const FAIL_AFTER_MS: i64 = 5_000;
/// A gap between two looks longer than this (the computer slept) counts as this long.
const GAP_MAX_MS: i64 = 6 * 60 * 60 * 1000;

#[derive(Debug)]
struct Current {
    room: String,
    crew: bool,
    started_ms: i64,
    last_ms: i64,
    phase: Phase,
    quality: u8,
    connected: bool,
    setup_ms: Option<u32>,
    live_ms: i64,
    weak_ms: i64,
    bad_ms: i64,
    reconnects: u32,
}

impl Current {
    fn start(now_ms: i64, seen: VoiceSeen<'_>) -> Self {
        Self {
            room: seen.room.to_owned(),
            crew: seen.crew,
            started_ms: now_ms,
            last_ms: now_ms,
            phase: seen.phase,
            quality: seen.quality.min(2),
            // First seen live (the launcher started its watch mid-stay): no setup time to tell.
            connected: seen.phase != Phase::Joining,
            setup_ms: None,
            live_ms: 0,
            weak_ms: 0,
            bad_ms: 0,
            reconnects: 0,
        }
    }

    /// The time since the last look, in the state it was in.
    fn advance(&mut self, now_ms: i64) {
        let gap = (now_ms - self.last_ms).clamp(0, GAP_MAX_MS);
        self.last_ms = now_ms.max(self.last_ms);
        if self.phase != Phase::Live {
            return;
        }
        self.live_ms += gap;
        match self.quality {
            0 => {}
            1 => self.weak_ms += gap,
            _ => self.bad_ms += gap,
        }
    }

    fn update(&mut self, now_ms: i64, seen: VoiceSeen<'_>) {
        if seen.phase == Phase::Live && self.phase != Phase::Live {
            if !self.connected {
                let setup = (now_ms - self.started_ms).clamp(0, i64::from(u32::MAX));
                self.setup_ms = u32::try_from(setup).ok();
            }
            self.connected = true;
        }
        if self.phase == Phase::Live && seen.phase == Phase::Reconnecting {
            self.reconnects += 1;
        }
        self.phase = seen.phase;
        self.quality = seen.quality.min(2);
    }

    fn finish(self) -> VoiceStay {
        let secs = |ms: i64| u32::try_from(ms / 1000).unwrap_or(u32::MAX);
        VoiceStay {
            crew: self.crew,
            connected: self.connected,
            failed: !self.connected && self.last_ms - self.started_ms >= FAIL_AFTER_MS,
            setup_ms: self.setup_ms,
            live_secs: secs(self.live_ms),
            weak_secs: secs(self.weak_ms),
            bad_secs: secs(self.bad_ms),
            reconnects: self.reconnects,
        }
    }
}

/// Follows the player's stays; each look may end one.
#[derive(Debug, Default)]
pub struct VoiceTracker {
    stay: Option<Current>,
}

impl VoiceTracker {
    /// One look at the voice state (`None`: in no room). Answers the stay that ended with it.
    pub fn observe(&mut self, now_ms: i64, seen: Option<VoiceSeen<'_>>) -> Option<VoiceStay> {
        if let Some(stay) = &mut self.stay {
            stay.advance(now_ms);
        }
        let same =
            matches!((&self.stay, &seen), (Some(stay), Some(seen)) if stay.room == seen.room);
        if same {
            if let (Some(stay), Some(seen)) = (&mut self.stay, seen) {
                stay.update(now_ms, seen);
            }
            return None;
        }
        let ended = self.stay.take().map(Current::finish);
        self.stay = seen.map(|seen| Current::start(now_ms, seen));
        ended
    }

    /// The launcher closes: the stay in progress ends here.
    pub fn close(&mut self, now_ms: i64) -> Option<VoiceStay> {
        self.observe(now_ms, None)
    }

    /// A stay is being followed.
    pub fn in_stay(&self) -> bool {
        self.stay.is_some()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn seen(room: &str, phase: Phase, quality: u8) -> Option<VoiceSeen<'_>> {
        Some(VoiceSeen {
            room,
            crew: room.starts_with("crew"),
            phase,
            quality,
        })
    }

    #[test]
    fn a_call_with_a_weak_stretch() {
        let mut t = VoiceTracker::default();
        assert_eq!(t.observe(0, seen("r1", Phase::Joining, 0)), None);
        assert_eq!(t.observe(800, seen("r1", Phase::Live, 0)), None);
        assert_eq!(t.observe(60_800, seen("r1", Phase::Live, 1)), None);
        assert_eq!(t.observe(90_800, seen("r1", Phase::Live, 0)), None);
        let stay = t.observe(120_800, None).unwrap();
        assert_eq!(
            stay,
            VoiceStay {
                crew: false,
                connected: true,
                failed: false,
                setup_ms: Some(800),
                live_secs: 120,
                weak_secs: 30,
                bad_secs: 0,
                reconnects: 0,
            }
        );
        assert_eq!(t.observe(130_000, None), None);
    }

    #[test]
    fn reconnects_and_bad_time() {
        let mut t = VoiceTracker::default();
        t.observe(0, seen("crew-a", Phase::Joining, 0));
        t.observe(1_000, seen("crew-a", Phase::Live, 2));
        t.observe(11_000, seen("crew-a", Phase::Reconnecting, 2));
        t.observe(14_000, seen("crew-a", Phase::Live, 0));
        let stay = t.close(24_000).unwrap();
        assert!(stay.crew && stay.connected && !stay.failed);
        assert_eq!(stay.reconnects, 1);
        assert_eq!(stay.bad_secs, 10);
        // Reconnecting is no live time.
        assert_eq!(stay.live_secs, 20);
        // Coming back is no second setup.
        assert_eq!(stay.setup_ms, Some(1_000));
    }

    #[test]
    fn a_stay_that_never_connected() {
        let mut t = VoiceTracker::default();
        t.observe(0, seen("r1", Phase::Joining, 0));
        let stay = t.observe(9_000, None).unwrap();
        assert!(stay.failed && !stay.connected);
        assert_eq!(stay.setup_ms, None);
        // Cancelled at once: no failure.
        t.observe(10_000, seen("r2", Phase::Joining, 0));
        let quick = t.observe(11_000, None).unwrap();
        assert!(!quick.failed && !quick.connected);
    }

    #[test]
    fn moving_to_another_room_ends_the_stay() {
        let mut t = VoiceTracker::default();
        t.observe(0, seen("crew-a", Phase::Live, 0));
        let first = t.observe(5_000, seen("crew-b", Phase::Joining, 0)).unwrap();
        // Seen live first: connected, setup unknown.
        assert!(first.connected);
        assert_eq!(first.setup_ms, None);
        assert_eq!(first.live_secs, 5);
        assert!(t.close(6_000).is_some());
    }

    #[test]
    fn a_sleeping_computer_counts_little() {
        let mut t = VoiceTracker::default();
        t.observe(0, seen("r1", Phase::Live, 0));
        let stay = t.observe(30 * 60 * 60 * 1000, None).unwrap();
        assert_eq!(stay.live_secs, 6 * 60 * 60);
        // A clock that went back adds nothing.
        t.observe(100_000, seen("r1", Phase::Live, 0));
        let back = t.observe(50_000, None).unwrap();
        assert_eq!(back.live_secs, 0);
    }
}
