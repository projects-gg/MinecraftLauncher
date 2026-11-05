//! Bitrate controller (pure): turns str0m's send-side bandwidth estimate (TWCC) into per-layer
//! encoder bitrates and, for "Netlik", a frame-rate ceiling. Lessons from Discord's Go Live posts:
//! never re-initialise the encoder (bitrate goes through `CODECAPI_AVEncCommonMeanBitRate`), cut
//! fast, raise slowly, and keep headroom for retransmissions and frame-size spikes.

use std::time::{Duration, Instant};

use crate::api::Tuning;
use crate::preset::LayerSpec;

/// Share of the estimate given to media; the rest covers RTX, RTCP, padding and keyframe spikes.
const HEADROOM: f64 = 0.85;
/// Raises wait this long after the last change (decreases apply at once).
const RAISE_EVERY: Duration = Duration::from_secs(2);
/// Changes smaller than this are not worth an encoder call.
const DEADBAND: f64 = 0.10;
/// The lowest frame rate "Netlik" goes down to.
const MIN_FPS: u8 = 5;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RatePlan {
    pub full_kbps: u32,
    pub small_kbps: u32,
    /// The full layer's frame-rate ceiling (the small layer keeps its own, lower one).
    pub full_fps: u8,
}

#[derive(Debug, Clone)]
pub struct RateController {
    full: LayerSpec,
    small: LayerSpec,
    audio_kbps: u32,
    tuning: Tuning,
    current: Option<RatePlan>,
    changed_at: Option<Instant>,
}

impl RateController {
    pub fn new(full: LayerSpec, small: LayerSpec, audio_kbps: u32, tuning: Tuning) -> Self {
        Self {
            full,
            small,
            audio_kbps,
            tuning,
            current: None,
            changed_at: None,
        }
    }

    /// What the estimator should probe up to (bits per second).
    pub fn desired_bps(&self) -> u64 {
        let kbps = self.full.max_kbps + self.small.max_kbps + self.audio_kbps;
        u64::from(kbps) * 1000 * 11 / 10
    }

    /// The minimum the estimator should keep sending (with padding), bits per second.
    pub fn minimum_bps(&self) -> u64 {
        u64::from(self.full.min_kbps + self.small.min_kbps + self.audio_kbps) * 1000
    }

    /// The plan before any estimate: the caps (the estimator starts at the configured initial
    /// rate and only goes down when the network says so).
    pub fn initial(&mut self, now: Instant) -> RatePlan {
        let plan = RatePlan {
            full_kbps: self.full.max_kbps,
            small_kbps: self.small.max_kbps,
            full_fps: self.full.fps,
        };
        self.current = Some(plan);
        self.changed_at = Some(now);
        plan
    }

    pub fn current(&self) -> Option<RatePlan> {
        self.current
    }

    /// Settings changed (preset, fps, tuning, audio): apply the new caps at once.
    pub fn reconfigure(
        &mut self,
        full: LayerSpec,
        small: LayerSpec,
        audio_kbps: u32,
        tuning: Tuning,
        bwe_kbps: Option<u32>,
        now: Instant,
    ) -> RatePlan {
        self.full = full;
        self.small = small;
        self.audio_kbps = audio_kbps;
        self.tuning = tuning;
        let plan = match bwe_kbps {
            Some(bwe) => self.plan(bwe),
            None => RatePlan {
                full_kbps: full.max_kbps,
                small_kbps: small.max_kbps,
                full_fps: full.fps,
            },
        };
        self.current = Some(plan);
        self.changed_at = Some(now);
        plan
    }

    /// A new estimate; `Some` when the encoders should change.
    pub fn on_estimate(&mut self, bwe_kbps: u32, now: Instant) -> Option<RatePlan> {
        let next = self.plan(bwe_kbps);
        let Some(cur) = self.current else {
            self.current = Some(next);
            self.changed_at = Some(now);
            return Some(next);
        };
        if next == cur {
            return None;
        }
        let lower = |a: u32, b: u32| f64::from(a) < f64::from(b) * (1.0 - DEADBAND / 2.0);
        let higher = |a: u32, b: u32| f64::from(a) > f64::from(b) * (1.0 + DEADBAND);
        let down = lower(next.full_kbps, cur.full_kbps)
            || lower(next.small_kbps, cur.small_kbps)
            || next.full_fps < cur.full_fps;
        let up = higher(next.full_kbps, cur.full_kbps)
            || higher(next.small_kbps, cur.small_kbps)
            || next.full_fps > cur.full_fps;
        let may_raise = self
            .changed_at
            .is_none_or(|at| now.duration_since(at) >= RAISE_EVERY);
        if down || (up && may_raise) {
            // A partial raise never pulls another value down below what the cut already set.
            self.current = Some(next);
            self.changed_at = Some(now);
            return Some(next);
        }
        None
    }

    fn plan(&self, bwe_kbps: u32) -> RatePlan {
        let media = (f64::from(bwe_kbps) * HEADROOM) as u32;
        let avail = media.saturating_sub(self.audio_kbps);
        let small = (avail / 6).clamp(self.small.min_kbps, self.small.max_kbps);
        let rest = avail.saturating_sub(small);
        let full = rest.clamp(self.full.min_kbps, self.full.max_kbps);
        let ratio = f64::from(rest) / f64::from(self.full.max_kbps.max(1));
        let fps = self.full.fps;
        let full_fps = match self.tuning {
            Tuning::Motion => fps,
            Tuning::Clarity if ratio >= 0.6 => fps,
            Tuning::Clarity if ratio >= 0.35 => (u16::from(fps) * 2 / 3) as u8,
            Tuning::Clarity if ratio >= 0.2 => fps / 2,
            Tuning::Clarity => fps / 4,
        }
        .max(MIN_FPS)
        .min(fps);
        RatePlan {
            full_kbps: full,
            small_kbps: small,
            full_fps,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{AudioScope, Preset, ShareOpts};
    use crate::preset::layers;

    fn controller(tuning: Tuning) -> RateController {
        let opts = ShareOpts {
            preset: Preset::Normal,
            fps: 30,
            tuning,
            cursor: true,
            audio: true,
            audio_scope: AudioScope::App,
        };
        let [f, q] = layers((1920, 1080), &opts, false);
        RateController::new(f, q, 96, tuning)
    }

    #[test]
    fn plenty_of_bandwidth_gives_the_caps() {
        let mut rc = controller(Tuning::Clarity);
        let now = Instant::now();
        let plan = rc.on_estimate(10_000, now).unwrap();
        assert_eq!(
            plan,
            RatePlan {
                full_kbps: 2500,
                small_kbps: 400,
                full_fps: 30
            }
        );
        assert_eq!(rc.on_estimate(12_000, now + Duration::from_secs(5)), None);
    }

    #[test]
    fn cuts_apply_at_once_raises_wait() {
        let mut rc = controller(Tuning::Motion);
        let t0 = Instant::now();
        rc.initial(t0);
        let cut = rc
            .on_estimate(1500, t0 + Duration::from_millis(100))
            .unwrap();
        assert!(cut.full_kbps < 1200, "{cut:?}");
        assert_eq!(cut.full_fps, 30, "motion keeps the frame rate");
        // Recovery within 2 s is held back, then applied.
        assert_eq!(
            rc.on_estimate(10_000, t0 + Duration::from_millis(1500)),
            None
        );
        let up = rc
            .on_estimate(10_000, t0 + Duration::from_millis(2200))
            .unwrap();
        assert_eq!(up.full_kbps, 2500);
    }

    #[test]
    fn clarity_trades_frame_rate() {
        let mut rc = controller(Tuning::Clarity);
        let now = Instant::now();
        let plan = rc.on_estimate(1200, now).unwrap();
        assert!(plan.full_fps < 30, "{plan:?}");
        let plan = rc.on_estimate(300, now).unwrap();
        assert!(plan.full_fps >= MIN_FPS);
        assert_eq!(
            plan.full_kbps, rc.full.min_kbps,
            "never below the encoder floor"
        );
        assert_eq!(plan.small_kbps, 100);
    }

    #[test]
    fn small_changes_are_ignored() {
        let mut rc = controller(Tuning::Clarity);
        let t0 = Instant::now();
        rc.on_estimate(2500, t0).unwrap();
        assert_eq!(rc.on_estimate(2560, t0 + Duration::from_secs(10)), None);
        assert_eq!(rc.on_estimate(2450, t0 + Duration::from_secs(11)), None);
    }

    #[test]
    fn desired_covers_all_layers_and_audio() {
        let rc = controller(Tuning::Clarity);
        assert_eq!(rc.desired_bps(), (2500 + 400 + 96) * 1100);
        assert!(rc.minimum_bps() < rc.desired_bps());
    }
}
