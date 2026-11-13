//! Presets and simulcast layers: sizes, frame rates and bitrate caps (pure).
//!
//! Two layers are always sent: `f` (the preset) and `q` (640x360 at most, 15 fps at most), the
//! names the room protocol and the SFU subscriptions use. Caps follow the research table (H.264,
//! screen and game content): 480p30 1.0 Mbps, 720p30 2.5 Mbps, 1080p30 4.5 Mbps, 1080p60 6 Mbps;
//! the thumbnail layer 400 kbps (the budget ledger counts `q` at 400 kbps, share.md §5).

use std::time::{Duration, Instant};

use crate::api::{Preset, ShareOpts};

/// The full layer's RID.
pub const RID_FULL: &str = "f";
/// The small layer's RID.
pub const RID_SMALL: &str = "q";

/// The small layer's box and limits.
const SMALL_BOX: (u32, u32) = (640, 360);
const SMALL_MAX_FPS: u8 = 15;
const SMALL_MAX_KBPS: u32 = 400;
const SMALL_MIN_KBPS: u32 = 100;

/// Software encoding is limited to 720p30 (CPU while the game runs).
const SOFTWARE_BOX: (u32, u32) = (1280, 720);
const SOFTWARE_MAX_FPS: u8 = 30;

/// Encoders reject tiny pictures; smaller sources are scaled up to this.
const MIN_SIDE: u32 = 64;

/// The stream's audio (Opus 48 kHz stereo).
pub const AUDIO_KBPS: u32 = 96;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Layer {
    Full,
    Small,
}

impl Layer {
    pub const ALL: [Layer; 2] = [Layer::Full, Layer::Small];

    pub fn rid(self) -> &'static str {
        match self {
            Layer::Full => RID_FULL,
            Layer::Small => RID_SMALL,
        }
    }

    pub fn from_rid(rid: &str) -> Option<Layer> {
        match rid {
            RID_FULL => Some(Layer::Full),
            RID_SMALL => Some(Layer::Small),
            _ => None,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Layer::Full => 0,
            Layer::Small => 1,
        }
    }
}

/// One encoder's job.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct LayerSpec {
    pub layer: Layer,
    pub w: u32,
    pub h: u32,
    pub fps: u8,
    pub max_kbps: u32,
    pub min_kbps: u32,
}

/// The preset's box, frame-rate ceiling and bitrate cap at that frame rate.
fn preset_box(preset: Preset) -> (u32, u32) {
    match preset {
        Preset::Low => (854, 480),
        Preset::Normal => (1280, 720),
        Preset::High => (1920, 1080),
    }
}

/// The frame rate actually used: 15, 30 or 60, at most 30 below `High`.
pub fn clamp_fps(preset: Preset, fps: u8) -> u8 {
    let ceiling = if preset == Preset::High { 60 } else { 30 };
    let wanted = match fps {
        0..=22 => 15,
        23..=45 => 30,
        _ => 60,
    };
    wanted.min(ceiling)
}

fn full_cap_kbps(preset: Preset, fps: u8) -> u32 {
    let at30 = match preset {
        Preset::Low => 1000,
        Preset::Normal => 2500,
        Preset::High => 4500,
    };
    match fps {
        60 => at30 * 4 / 3,  // 1080p60 6000, 720p60 is not offered
        15 => at30 * 7 / 10, // fewer frames, each a bit better
        _ => at30,
    }
}

/// Fits `src` into `bounds` keeping the aspect ratio: never upscales (except below
/// [`MIN_SIDE`]), even sides (NV12 needs them).
pub fn fit(src: (u32, u32), bounds: (u32, u32)) -> (u32, u32) {
    let (sw, sh) = (src.0.max(1) as f64, src.1.max(1) as f64);
    let scale = (bounds.0 as f64 / sw).min(bounds.1 as f64 / sh).min(1.0);
    let mut w = (sw * scale).round() as u32;
    let mut h = (sh * scale).round() as u32;
    if w < MIN_SIDE || h < MIN_SIDE {
        let up = (MIN_SIDE as f64 / w.max(1) as f64).max(MIN_SIDE as f64 / h.max(1) as f64);
        w = (w as f64 * up).round() as u32;
        h = (h as f64 * up).round() as u32;
    }
    (even(w), even(h))
}

fn even(v: u32) -> u32 {
    (v & !1).max(2)
}

/// The two layers for a source of `src` pixels. `software` limits the full layer to 720p30.
pub fn layers(src: (u32, u32), opts: &ShareOpts, software: bool) -> [LayerSpec; 2] {
    let mut bounds = preset_box(opts.preset);
    let mut fps = clamp_fps(opts.preset, opts.fps);
    if software {
        bounds = (bounds.0.min(SOFTWARE_BOX.0), bounds.1.min(SOFTWARE_BOX.1));
        fps = fps.min(SOFTWARE_MAX_FPS);
    }
    let (w, h) = fit(src, bounds);
    // A smaller picture than the preset's box needs proportionally fewer bits.
    let box_px = (bounds.0 * bounds.1) as u64;
    let px = (w * h) as u64;
    let cap = full_cap_kbps(opts.preset, fps);
    let max_kbps = ((cap as u64 * px.max(box_px / 4)) / box_px).max(300) as u32;
    let full = LayerSpec {
        layer: Layer::Full,
        w,
        h,
        fps,
        max_kbps,
        min_kbps: (max_kbps / 6).max(150),
    };
    let (sw, sh) = fit((w, h), SMALL_BOX);
    let small = LayerSpec {
        layer: Layer::Small,
        w: sw,
        h: sh,
        fps: fps.min(SMALL_MAX_FPS),
        max_kbps: SMALL_MAX_KBPS.min(max_kbps),
        min_kbps: SMALL_MIN_KBPS,
    };
    [full, small]
}

/// Admits frames at most at `fps` (with a quarter-frame tolerance, so a 60 Hz source feeds a
/// 30 fps layer every other frame and a 59.94 Hz one does not stutter).
#[derive(Debug, Clone)]
pub struct FrameGate {
    interval: Duration,
    next: Option<Instant>,
}

impl FrameGate {
    pub fn new(fps: u8) -> Self {
        Self {
            interval: interval(fps),
            next: None,
        }
    }

    pub fn set_fps(&mut self, fps: u8) {
        self.interval = interval(fps);
    }

    pub fn interval(&self) -> Duration {
        self.interval
    }

    /// SQOL: the next frame comes through at once (a new source's first frame, the first after a
    /// black stand-in): a still source sends no second one.
    pub fn reset(&mut self) {
        self.next = None;
    }

    pub fn admit(&mut self, t: Instant) -> bool {
        let slack = self.interval / 4;
        match self.next {
            Some(next) if t + slack < next => false,
            Some(next) => {
                // Far behind (a pause, a stall): restart the cadence from now.
                self.next = Some(if t > next + self.interval {
                    t + self.interval
                } else {
                    next + self.interval
                });
                true
            }
            None => {
                self.next = Some(t + self.interval);
                true
            }
        }
    }
}

fn interval(fps: u8) -> Duration {
    Duration::from_micros(1_000_000 / u64::from(fps.max(1)))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::{AudioScope, Tuning};

    fn opts(preset: Preset, fps: u8) -> ShareOpts {
        ShareOpts {
            preset,
            fps,
            tuning: Tuning::Clarity,
            cursor: true,
            audio: true,
            audio_scope: AudioScope::App,
        }
    }

    #[test]
    fn fit_keeps_aspect_never_upscales_and_is_even() {
        assert_eq!(fit((1920, 1080), (1280, 720)), (1280, 720));
        assert_eq!(fit((2560, 1440), (1920, 1080)), (1920, 1080));
        // Ultrawide: width-bound.
        assert_eq!(fit((3440, 1440), (1280, 720)), (1280, 536));
        // Tall window: height-bound.
        assert_eq!(fit((800, 1200), (1280, 720)), (480, 720));
        // Smaller than the box: as is (rounded to even).
        assert_eq!(fit((1001, 701), (1280, 720)), (1000, 700));
        // Tiny: scaled up to the minimum side.
        let (w, h) = fit((40, 20), (1280, 720));
        assert!(w >= 64 && h >= 64 && w % 2 == 0 && h % 2 == 0, "{w}x{h}");
    }

    #[test]
    fn fps_is_clamped_to_the_preset() {
        assert_eq!(clamp_fps(Preset::Low, 60), 30);
        assert_eq!(clamp_fps(Preset::Normal, 60), 30);
        assert_eq!(clamp_fps(Preset::High, 60), 60);
        assert_eq!(clamp_fps(Preset::High, 30), 30);
        assert_eq!(clamp_fps(Preset::Normal, 15), 15);
        assert_eq!(clamp_fps(Preset::Normal, 24), 30);
    }

    #[test]
    fn layers_follow_the_presets() {
        let [f, q] = layers((1920, 1080), &opts(Preset::Normal, 30), false);
        assert_eq!((f.w, f.h, f.fps, f.max_kbps), (1280, 720, 30, 2500));
        assert_eq!((q.w, q.h, q.fps, q.max_kbps), (640, 360, 15, 400));
        let [f, _] = layers((1920, 1080), &opts(Preset::High, 60), false);
        assert_eq!((f.w, f.h, f.fps, f.max_kbps), (1920, 1080, 60, 6000));
        let [f, q] = layers((1920, 1080), &opts(Preset::Low, 30), false);
        assert_eq!((f.w, f.h), (852, 480));
        assert!((990..=1000).contains(&f.max_kbps), "{}", f.max_kbps);
        assert_eq!((q.w, q.h), (638, 360));
    }

    #[test]
    fn software_is_held_to_720p30() {
        let [f, q] = layers((2560, 1440), &opts(Preset::High, 60), true);
        assert_eq!((f.w, f.h, f.fps), (1280, 720, 30));
        assert!(q.fps <= 15);
    }

    #[test]
    fn small_sources_get_a_proportional_cap() {
        let [f, q] = layers((640, 480), &opts(Preset::Normal, 30), false);
        assert_eq!((f.w, f.h), (640, 480));
        assert!(f.max_kbps < 2500 && f.max_kbps >= 300, "{}", f.max_kbps);
        assert!(q.max_kbps <= f.max_kbps);
        assert!(f.min_kbps < f.max_kbps);
    }

    #[test]
    fn gate_halves_a_60_hz_source_for_30_fps() {
        let mut gate = FrameGate::new(30);
        let t0 = Instant::now();
        let admitted = (0..120)
            .filter(|i| gate.admit(t0 + Duration::from_micros(16_667 * i)))
            .count();
        assert!((59..=61).contains(&admitted), "{admitted}");
    }

    #[test]
    fn gate_restarts_after_a_stall() {
        let mut gate = FrameGate::new(15);
        let t0 = Instant::now();
        assert!(gate.admit(t0));
        assert!(!gate.admit(t0 + Duration::from_millis(10)));
        // A 5 s stall, then frames resume at 60 Hz: the first passes, the next only after 1/15 s.
        let t1 = t0 + Duration::from_secs(5);
        assert!(gate.admit(t1));
        assert!(!gate.admit(t1 + Duration::from_millis(17)));
        assert!(gate.admit(t1 + Duration::from_millis(67)));
    }

    /// SQOL (P2-4): a switch to a still window sends one frame; the old source's cadence must not
    /// drop it (the share showed black).
    #[test]
    fn a_reset_gate_lets_the_next_frame_through() {
        let mut gate = FrameGate::new(30);
        let t0 = Instant::now();
        assert!(gate.admit(t0));
        assert!(!gate.admit(t0 + Duration::from_millis(5)));
        gate.reset();
        assert!(gate.admit(t0 + Duration::from_millis(6)));
        // The cadence goes on from there.
        assert!(!gate.admit(t0 + Duration::from_millis(10)));
        assert!(gate.admit(t0 + Duration::from_millis(40)));
    }

    #[test]
    fn rids_round_trip() {
        for layer in Layer::ALL {
            assert_eq!(Layer::from_rid(layer.rid()), Some(layer));
        }
        assert_eq!(Layer::from_rid("h"), None);
    }
}
