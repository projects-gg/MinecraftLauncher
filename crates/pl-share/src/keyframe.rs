//! Keyframe policy for one layer (pure). Keyframes are expensive (5-20x a normal frame) and every
//! viewer joining or losing packets asks for one, so: on request (PLI/FIR) at most one per second,
//! a request inside that window is kept and served when it ends, plus one every 60 s while live so
//! a decoder that lost its way without asking recovers (Discord: "about one keyframe per minute").

use std::time::{Duration, Instant};

pub const MIN_GAP: Duration = Duration::from_secs(1);
pub const PERIODIC: Duration = Duration::from_secs(60);

#[derive(Debug, Clone, Default)]
pub struct KeyframePolicy {
    /// When the encoder last produced a keyframe (asked for or on its own).
    last: Option<Instant>,
    /// A request arrived inside the gap.
    pending: bool,
}

impl KeyframePolicy {
    pub fn new() -> Self {
        Self::default()
    }

    /// A PLI/FIR (or a new watcher): true when a keyframe should be forced now; otherwise it is
    /// remembered and [`Self::poll`] returns true once the gap has passed.
    pub fn request(&mut self, now: Instant) -> bool {
        if self.gap_over(now) {
            self.pending = false;
            self.last = Some(now);
            true
        } else {
            self.pending = true;
            false
        }
    }

    /// Called on timers: true when a kept request or the periodic keyframe is due. `live` is false
    /// while paused (nobody watches: no periodic keyframes).
    pub fn poll(&mut self, now: Instant, live: bool) -> bool {
        let due = (self.pending && self.gap_over(now))
            || (live && self.last.is_some_and(|t| now.duration_since(t) >= PERIODIC));
        if due {
            self.pending = false;
            self.last = Some(now);
        }
        due
    }

    /// The next instant [`Self::poll`] may return true.
    pub fn deadline(&self, live: bool) -> Option<Instant> {
        let last = self.last?;
        if self.pending {
            Some(last + MIN_GAP)
        } else if live {
            Some(last + PERIODIC)
        } else {
            None
        }
    }

    /// The encoder produced a keyframe (forced or spontaneous, e.g. a scene cut).
    pub fn produced(&mut self, now: Instant) {
        self.last = Some(now);
    }

    fn gap_over(&self, now: Instant) -> bool {
        self.last.is_none_or(|t| now.duration_since(t) >= MIN_GAP)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn first_request_is_served_at_once() {
        let mut p = KeyframePolicy::new();
        assert!(p.request(Instant::now()));
    }

    #[test]
    fn requests_inside_the_gap_wait_for_it() {
        let t0 = Instant::now();
        let mut p = KeyframePolicy::new();
        assert!(p.request(t0));
        assert!(!p.request(t0 + Duration::from_millis(200)));
        assert!(!p.request(t0 + Duration::from_millis(400)));
        assert_eq!(p.deadline(true), Some(t0 + MIN_GAP));
        assert!(!p.poll(t0 + Duration::from_millis(900), true));
        assert!(p.poll(t0 + Duration::from_millis(1000), true));
        // Served once only.
        assert!(!p.poll(t0 + Duration::from_millis(1500), true));
    }

    #[test]
    fn periodic_only_while_live() {
        let t0 = Instant::now();
        let mut p = KeyframePolicy::new();
        p.produced(t0);
        assert_eq!(p.deadline(true), Some(t0 + PERIODIC));
        assert_eq!(p.deadline(false), None);
        assert!(!p.poll(t0 + PERIODIC, false));
        assert!(p.poll(t0 + PERIODIC, true));
        assert_eq!(p.deadline(true), Some(t0 + PERIODIC + PERIODIC));
    }

    #[test]
    fn a_spontaneous_keyframe_resets_the_clock() {
        let t0 = Instant::now();
        let mut p = KeyframePolicy::new();
        p.produced(t0);
        p.produced(t0 + Duration::from_secs(50));
        assert!(!p.poll(t0 + PERIODIC, true));
        assert!(!p.request(t0 + Duration::from_millis(50_500)));
        assert!(p.poll(t0 + Duration::from_secs(51), true));
    }

    #[test]
    fn no_clock_before_the_first_keyframe() {
        let p = KeyframePolicy::new();
        assert_eq!(p.deadline(true), None);
    }
}
