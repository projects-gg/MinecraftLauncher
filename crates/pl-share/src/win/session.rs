//! The Windows backend of the controller: one [`Session`] per share, from `Prepare` (capture item,
//! offer) through `Answer` (encoders, capture, audio, transport) to its end.
//!
//! SQOL: a live share changes without a new SFU session (watchers stay): another source
//! (`Switch`: the new source is captured and the old capture goes; new encoders only for a new
//! size or frame rate, otherwise a keyframe), a blank picture (`Hold`, and a closed source while
//! the launcher decides), the game's window found again after the game recreated it, a minimized
//! window told, the sharer's own preview, and everything rebuilt on a new graphics device when the
//! old one is lost. A switch or a hold never shows the old source's last frame again: a black frame
//! stands in until the new source's first.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::Sender;
use std::sync::{Arc, Mutex};
use std::time::Instant;

use tracing::{info, warn};
use windows::Graphics::Capture::GraphicsCaptureItem;

use super::audio::{AudioHandle, AudioTarget};
use super::capture::{self, Capture};
use super::convert::Scaler;
use super::d3d::Gpu;
use super::encoder::{self, EncEvent, EncIn, EncoderConfig, EncoderHandle};
use super::minimize::MinimizeWatch;
use super::thumb::{Host, ThumbHandle, Thumbs};
use super::{SendCom, sources};
use crate::api::{
    AudioScope, AudioWhy, LostWhy, ShareCmd, ShareEnv, ShareEvent, ShareNo, ShareOpts, ShareStats,
    SourceId, SwitchWhy,
};
use crate::controller::{Ctl, EventTx};
use crate::preset::{AUDIO_KBPS, FrameGate, Layer, LayerSpec, layers};
use crate::switch::{Wait, restart_needed};
use crate::transport::{
    self, H264Profile, MediaCtl, Negotiated, Prepared, ToTransport, TransportHandle,
};

/// A share rebuilt on a new graphics device this many times ends instead (a device that keeps
/// failing).
const REBUILDS_MAX: u32 = 3;

pub struct Backend {
    env: ShareEnv,
    events: EventTx,
    internal: Sender<Ctl>,
    gpu: Option<Arc<Gpu>>,
    /// The hardware encoder's name, probed once (`Some(None)`: none).
    hardware: Option<Option<String>>,
    sessions: HashMap<ShareNo, Session>,
    generation: u64,
}

impl Backend {
    pub fn new(env: ShareEnv, events: EventTx, internal: Sender<Ctl>) -> Self {
        super::com_mta();
        Self {
            env,
            events,
            internal,
            gpu: None,
            hardware: None,
            sessions: HashMap::new(),
            generation: 0,
        }
    }

    fn emit(&self, event: ShareEvent) {
        let _ = self.events.send(event);
    }

    fn gpu(&mut self) -> Option<Arc<Gpu>> {
        if self.gpu.is_none() {
            match Gpu::new() {
                Ok(gpu) => {
                    info!("share gpu: {} (vendor {:#06x})", gpu.adapter, gpu.vendor_id);
                    self.gpu = Some(Arc::new(gpu));
                }
                Err(e) => warn!("share gpu: {e}"),
            }
        }
        self.gpu.clone()
    }

    fn hardware(&mut self) -> Option<String> {
        if self.hardware.is_none() {
            let name = self.gpu().and_then(|gpu| encoder::hardware_name(gpu.luid));
            self.hardware = Some(name);
        }
        self.hardware.clone().flatten()
    }

    pub fn handle(&mut self, cmd: ShareCmd) {
        match cmd {
            ShareCmd::Sources { game_pid, thumbs } => {
                let caps = sources::caps(self.hardware());
                let own: HashSet<u32> = self
                    .env
                    .own_pids
                    .iter()
                    .copied()
                    .chain([std::process::id()])
                    .collect();
                let events = self.events.clone();
                // Thumbnails take tens of milliseconds each: off the controller thread.
                let spawned = std::thread::Builder::new()
                    .name("pl-share-sources".into())
                    .spawn(move || {
                        super::com_mta();
                        let list = sources::list(game_pid, &own, thumbs);
                        let _ = events.send(ShareEvent::Sources { list, caps });
                    });
                if let Err(e) = spawned {
                    warn!("share sources: {e}");
                }
            }
            ShareCmd::Prepare { n, source, opts } => {
                if let Some(old) = self.sessions.remove(&n) {
                    old.stop();
                }
                self.generation += 1;
                let generation = self.generation;
                let hardware = self.hardware();
                let Some(gpu) = self.gpu() else {
                    self.emit(ShareEvent::Lost {
                        n,
                        why: LostWhy::Capture,
                    });
                    return;
                };
                match Session::prepare(
                    n,
                    generation,
                    source,
                    opts,
                    &self.env,
                    gpu,
                    hardware.is_none(),
                    self.events.clone(),
                    self.internal.clone(),
                ) {
                    Ok((session, sdp)) => {
                        self.sessions.insert(n, session);
                        self.emit(ShareEvent::Offer { n, sdp });
                    }
                    Err(why) => self.emit(why.event(n)),
                }
            }
            ShareCmd::Answer { n, sdp } => {
                let Some(session) = self.sessions.get_mut(&n) else {
                    return;
                };
                if let Err(why) = session.answer(&sdp) {
                    if let Some(s) = self.sessions.remove(&n) {
                        s.stop();
                    }
                    self.emit(why.event(n));
                }
            }
            ShareCmd::Pause { n, paused } => {
                if let Some(s) = self.sessions.get_mut(&n) {
                    s.pause(paused);
                }
            }
            ShareCmd::KeyFrame { n } => {
                if let Some(Stage::Live(live)) = self.sessions.get(&n).map(|s| &s.stage) {
                    live.transport.send(ToTransport::KeyframeAll);
                }
            }
            ShareCmd::Update { n, opts } => {
                if let Some(s) = self.sessions.get_mut(&n) {
                    s.update(opts);
                }
            }
            ShareCmd::Stop { n } => {
                if let Some(s) = self.sessions.remove(&n) {
                    s.stop();
                }
                self.emit(ShareEvent::Stopped { n });
            }
            ShareCmd::StopAll => {
                for (n, s) in self.sessions.drain() {
                    s.stop();
                    let _ = self.events.send(ShareEvent::Stopped { n });
                }
            }
            ShareCmd::Env { env } => {
                self.env = env;
                let env = self.env.clone();
                for s in self.sessions.values_mut() {
                    s.retarget_audio(&env);
                }
            }
            ShareCmd::Switch { n, source } => match self.sessions.get_mut(&n) {
                Some(s) => s.switch(source, Instant::now()),
                None => self.emit(ShareEvent::SwitchFailed {
                    n,
                    why: SwitchWhy::Gone,
                }),
            },
            ShareCmd::Hold { n, on } => {
                if let Some(s) = self.sessions.get_mut(&n) {
                    s.hold(on);
                }
            }
            ShareCmd::Preview { n, on } => {
                if let Some(s) = self.sessions.get_mut(&n) {
                    s.preview(on);
                }
            }
        }
    }

    pub fn ended(&mut self, n: ShareNo, generation: u64) {
        if self
            .sessions
            .get(&n)
            .is_some_and(|s| s.generation == generation)
            && let Some(s) = self.sessions.remove(&n)
        {
            s.stop();
        }
    }

    /// SQOL: a capture's source closed (its window, its monitor): the share holds a blank picture
    /// and, for the game, looks for its new window a moment.
    pub fn source_closed(&mut self, n: ShareNo, generation: u64, capture: u64) {
        if let Some(s) = self.sessions.get_mut(&n)
            && s.generation == generation
        {
            s.source_closed(capture, Instant::now());
        }
    }

    /// SQOL: the graphics device under a share was lost. Every share uses the one device: a new
    /// device, and each share on the lost one rebuilt on it (capture, scalers, encoders and still
    /// pictures; the SFU session and the sound go on). A share that cannot be rebuilt ends.
    pub fn device_lost(&mut self, n: ShareNo, generation: u64) {
        if !self
            .sessions
            .get(&n)
            .is_some_and(|s| s.generation == generation)
        {
            return;
        }
        if self.gpu.as_ref().is_some_and(|gpu| gpu.removed()) {
            warn!("share gpu: the device was lost; making a new one");
            self.gpu = None;
        }
        let gpu = self.gpu();
        let ns: Vec<ShareNo> = self.sessions.keys().copied().collect();
        for n in ns {
            let Some(s) = self.sessions.get_mut(&n) else {
                continue;
            };
            if !s.gpu.removed() {
                continue;
            }
            let rebuilt = match &gpu {
                Some(gpu) if !gpu.removed() => s.rebuild(gpu.clone()),
                _ => Err(Fail::Lost(LostWhy::Device)),
            };
            if let Err(why) = rebuilt {
                if let Some(s) = self.sessions.remove(&n) {
                    s.stop();
                }
                self.emit(why.event(n));
            }
        }
    }

    /// SQOL: the next moment a share waits for (the game's window), none when nothing waits.
    pub fn deadline(&self) -> Option<Instant> {
        self.sessions.values().filter_map(Session::deadline).min()
    }

    /// SQOL: that moment came.
    pub fn tick(&mut self, now: Instant) {
        for s in self.sessions.values_mut() {
            s.tick(now);
        }
    }
}

/// Why a step failed, as the event the launcher gets.
enum Fail {
    Lost(LostWhy),
    Gone,
}

impl Fail {
    fn event(self, n: ShareNo) -> ShareEvent {
        match self {
            Fail::Lost(why) => ShareEvent::Lost { n, why },
            Fail::Gone => ShareEvent::SourceGone { n },
        }
    }

    /// SQOL: as a switch's failure.
    fn switch_why(&self) -> SwitchWhy {
        match self {
            Fail::Gone => SwitchWhy::Gone,
            Fail::Lost(_) => SwitchWhy::Capture,
        }
    }
}

/// What both the controller thread and the media threads touch.
struct Shared {
    n: ShareNo,
    generation: u64,
    stopped: AtomicBool,
    paused: AtomicBool,
    /// SQOL: no captured frame reaches the encoders (the player's hold, a closed source).
    held: AtomicBool,
    /// SQOL: only this capture's frames count (a switch, a hold's end and a rebuild start a new
    /// capture; the old one's last frames are dropped).
    capture: AtomicU64,
    /// SQOL: the running encoders' generation (a replaced encoder's failure is none of the
    /// share's; the transport drops its last frames).
    encoders_gen: AtomicU32,
    /// SQOL: the device's loss went to the controller.
    device_lost: AtomicBool,
    /// SQOL: the shared window is minimized.
    minimized: AtomicBool,
    /// SQOL: grows with every new source (a switch, a closed one): an old window's minimize
    /// events are no longer the share's.
    source_epoch: AtomicU64,
    events: EventTx,
    internal: Sender<Ctl>,
    encoders: Mutex<[Option<Sender<EncIn>>; 2]>,
    sink: Mutex<Option<FrameSink>>,
    info: Mutex<StatsInfo>,
}

struct StatsInfo {
    w: u32,
    h: u32,
    encoder: String,
    software: bool,
    audio: bool,
    small_fps: u8,
}

impl Shared {
    fn emit(&self, event: ShareEvent) {
        if !self.stopped.load(Ordering::SeqCst) {
            let _ = self.events.send(event);
        }
    }

    /// The share ended on its own: tell the launcher once, and the controller to clean up.
    fn end(&self, event: ShareEvent) {
        if !self.stopped.swap(true, Ordering::SeqCst) {
            let _ = self.events.send(event);
            let _ = self.internal.send(Ctl::Ended {
                n: self.n,
                generation: self.generation,
            });
        }
    }

    fn to_encoder(&self, layer: Layer, msg: EncIn) {
        if let Ok(encoders) = self.encoders.lock()
            && let Some(tx) = &encoders[layer.index()]
        {
            let _ = tx.send(msg);
        }
    }

    /// SQOL: a keyframe on both layers now (the policy's one-second gap is for watchers' requests;
    /// a new source is no request).
    fn key_both(&self) {
        for layer in Layer::ALL {
            self.to_encoder(layer, EncIn::KeyFrame);
        }
    }

    /// SQOL: the device under the share was lost: the controller rebuilds it (told once).
    fn lose_device(&self) {
        if !self.stopped.load(Ordering::SeqCst) && !self.device_lost.swap(true, Ordering::SeqCst) {
            let _ = self.internal.send(Ctl::DeviceLost {
                n: self.n,
                generation: self.generation,
            });
        }
    }
}

/// Frames from the capture thread → scalers → encoders.
struct FrameSink {
    scalers: [Scaler; 2],
    gates: [FrameGate; 2],
    /// The full layer's newest encoder input (SHARE-LIVE's still picture is cut from it).
    latest: Option<super::thumb::Latest>,
}

// One per share and never copied: the size difference does not matter.
#[allow(clippy::large_enum_variant)]
enum Stage {
    Prepared(Box<Prepared>),
    Live(Live),
    Empty,
}

struct Live {
    transport: TransportHandle,
    /// SQOL: none while the source is gone.
    capture: Option<Capture>,
    /// SQOL: the capture's item, to capture it anew (a fresh first frame after a hold or new
    /// encoders: a still source sends none of its own).
    item: Option<SendCom<GraphicsCaptureItem>>,
    /// SQOL: the shared window's minimize events (none for a monitor).
    minimize: Option<MinimizeWatch>,
    encoders: [EncoderHandle; 2],
    audio: Option<AudioHandle>,
    layers: [LayerSpec; 2],
    profile: H264Profile,
    epoch: Instant,
    /// SHARE-LIVE: the still picture's worker (SQOL: and the sharer's preview); none when its
    /// thread could not start.
    thumbs: Option<Arc<Thumbs>>,
    _thumb_handle: Option<ThumbHandle>,
}

pub struct Session {
    generation: u64,
    source: SourceId,
    opts: ShareOpts,
    env: ShareEnv,
    gpu: Arc<Gpu>,
    /// Until the answer: what will be captured.
    item: Option<SendCom<GraphicsCaptureItem>>,
    src_size: (u32, u32),
    /// The program whose sound goes with a game/window share.
    target_pid: Option<u32>,
    /// SQOL: the shared window (none for a monitor).
    hwnd: Option<u32>,
    audio_line: bool,
    software: bool,
    shared: Arc<Shared>,
    stage: Stage,
    /// SQOL: the player's "Duraklat".
    held: bool,
    /// SQOL: the source is gone (nothing is captured): a blank picture until a switch.
    gone: bool,
    /// SQOL: waiting for the game's window.
    waiting: Option<Wait>,
    /// SQOL: the sharer's preview is on.
    preview: bool,
    /// SQOL: new encoders came while encoding rested: the share's return captures anew.
    fresh_on_resume: bool,
    rebuilds: u32,
}

/// The audio target for these settings, or why there is none.
fn audio_target(
    opts: &ShareOpts,
    source: &SourceId,
    target_pid: Option<u32>,
    env: &ShareEnv,
) -> Result<Option<AudioTarget>, AudioWhy> {
    if !opts.audio {
        return Ok(None);
    }
    if !super::process_audio_supported() {
        return Err(AudioWhy::Unsupported);
    }
    let whole =
        matches!(source, SourceId::Monitor { .. }) || opts.audio_scope == AudioScope::System;
    if whole {
        return match env.voice_browser_pid {
            Some(pid) => Ok(Some(AudioTarget::Exclude(pid))),
            None => Err(AudioWhy::NoVoiceProcess),
        };
    }
    Ok(target_pid.map(AudioTarget::Include))
}

/// What `source` is now: its capture item, the program whose sound goes with it, and its window.
fn resolve(source: &SourceId) -> Result<(GraphicsCaptureItem, Option<u32>, Option<u32>), Fail> {
    match source {
        SourceId::Game { pid } => {
            let window = sources::find_game_window(*pid).ok_or(Fail::Gone)?;
            let item = capture::item_for_window(window.hwnd as isize)
                .map_err(|_| Fail::Lost(LostWhy::Capture))?;
            Ok((item, Some(*pid), Some(window.hwnd)))
        }
        SourceId::Window { hwnd, pid } => {
            if !sources::window_alive(*hwnd, *pid) {
                return Err(Fail::Gone);
            }
            let item = capture::item_for_window(*hwnd as isize)
                .map_err(|_| Fail::Lost(LostWhy::Capture))?;
            Ok((item, Some(*pid), Some(*hwnd)))
        }
        SourceId::Monitor { device } => {
            let monitor = sources::monitor_by_device(device).ok_or(Fail::Gone)?;
            let item = capture::item_for_monitor(monitor.handle)
                .map_err(|_| Fail::Lost(LostWhy::Capture))?;
            Ok((item, None, None))
        }
    }
}

impl Session {
    #[allow(clippy::too_many_arguments)]
    fn prepare(
        n: ShareNo,
        generation: u64,
        source: SourceId,
        opts: ShareOpts,
        env: &ShareEnv,
        gpu: Arc<Gpu>,
        software: bool,
        events: EventTx,
        internal: Sender<Ctl>,
    ) -> Result<(Self, String), Fail> {
        if !capture::supported() {
            return Err(Fail::Lost(LostWhy::Capture));
        }
        let (item, target_pid, hwnd) = resolve(&source)?;
        let src_size = capture::item_size(&item);
        // The audio m-line is negotiated whenever this computer can capture sound, so turning
        // sound on later needs no renegotiation (a muted line carries 1 silent packet a second).
        let audio_line = super::process_audio_supported();
        let [full, small] = layers(src_size, &opts, software);
        let initial = u64::from(full.max_kbps + small.max_kbps + AUDIO_KBPS) * 1000;
        let (prepared, sdp) = transport::prepare(audio_line, initial).map_err(|e| {
            warn!("share prepare: {e}");
            Fail::Lost(LostWhy::Internal)
        })?;
        let shared = Arc::new(Shared {
            n,
            generation,
            stopped: AtomicBool::new(false),
            paused: AtomicBool::new(false),
            held: AtomicBool::new(false),
            capture: AtomicU64::new(0),
            encoders_gen: AtomicU32::new(0),
            device_lost: AtomicBool::new(false),
            minimized: AtomicBool::new(false),
            source_epoch: AtomicU64::new(0),
            events,
            internal,
            encoders: Mutex::new([None, None]),
            sink: Mutex::new(None),
            info: Mutex::new(StatsInfo {
                w: full.w,
                h: full.h,
                encoder: String::new(),
                software,
                audio: false,
                small_fps: small.fps,
            }),
        });
        if let Err(why) = audio_target(&opts, &source, target_pid, env) {
            shared.emit(ShareEvent::AudioUnavailable { n, why });
        }
        Ok((
            Self {
                generation,
                source,
                opts,
                env: env.clone(),
                gpu,
                item: Some(SendCom(item)),
                src_size,
                target_pid,
                hwnd,
                audio_line,
                software,
                shared,
                stage: Stage::Prepared(Box::new(prepared)),
                held: false,
                gone: false,
                waiting: None,
                preview: false,
                fresh_on_resume: false,
                rebuilds: 0,
            },
            sdp,
        ))
    }

    fn answer(&mut self, sdp: &str) -> Result<(), Fail> {
        let Stage::Prepared(prepared) = std::mem::replace(&mut self.stage, Stage::Empty) else {
            return Ok(());
        };
        let negotiated = prepared.accept(sdp).map_err(|e| {
            warn!("share answer: {e}");
            Fail::Lost(LostWhy::Negotiation)
        })?;
        let profile = negotiated.profile;
        let epoch = Instant::now();
        let (handle, inbox) = transport::channel();

        let generation = self.shared.encoders_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let (encoders, layers) = self.start_encoders(profile, epoch, &handle, generation)?;
        let [full, small] = layers;
        {
            let mut info = self
                .shared
                .info
                .lock()
                .map_err(|_| Fail::Lost(LostWhy::Internal))?;
            info.w = full.w;
            info.h = full.h;
            info.small_fps = small.fps;
            info.encoder = encoders[0].name.clone();
            info.software = !encoders[0].hardware;
        }
        self.install(&layers, &encoders)?;
        let (thumbs, thumb_handle) = match self.start_thumbs() {
            Ok((thumbs, handle)) => (Some(thumbs), Some(handle)),
            Err(e) => {
                warn!("share thumb thread: {e}");
                (None, None)
            }
        };
        if self.preview
            && let Some(thumbs) = &thumbs
        {
            thumbs.preview(true);
        }
        let item = self.item.take();
        let capture = match &item {
            // A source that went before the answer (a switch to a game still starting): blank
            // until it comes.
            Some(item) if !self.gone => {
                let next = self.next_capture();
                Some(self.start_capture(next, &item.0, thumbs.clone())?)
            }
            _ => None,
        };
        let audio = self.start_audio(&negotiated, epoch, &handle);
        let ctl = self.transport_ctl(thumbs.clone());
        negotiated
            .start(inbox, full, small, self.opts.tuning, ctl)
            .map_err(|_| Fail::Lost(LostWhy::Internal))?;
        let minimize = self.watch_minimize();
        self.stage = Stage::Live(Live {
            transport: handle,
            capture,
            item,
            minimize,
            encoders,
            audio,
            layers,
            profile,
            epoch,
            thumbs,
            _thumb_handle: thumb_handle,
        });
        if self.held || self.gone {
            self.sync_held();
            self.blank();
        }
        Ok(())
    }

    /// SHARE-LIVE: the still picture's worker, fed from the full layer's encoder input (or, while
    /// encoding rests, from the next captured frame); SQOL: and the sharer's preview.
    fn start_thumbs(&self) -> std::io::Result<(Arc<Thumbs>, ThumbHandle)> {
        let paused = self.shared.clone();
        let latest = self.shared.clone();
        let emit = self.shared.clone();
        let preview = self.shared.clone();
        Thumbs::start(
            self.gpu.clone(),
            self.shared.n,
            Host {
                paused: Box::new(move || paused.paused.load(Ordering::Relaxed)),
                latest: Box::new(move || {
                    let sink = latest.sink.lock().ok()?;
                    let (texture, picture, at) = sink.as_ref()?.latest.as_ref()?;
                    Some((SendCom(texture.0.clone()), *picture, *at))
                }),
                emit: Box::new(move |jpg| emit.emit(ShareEvent::Thumb { n: emit.n, jpg })),
                preview: Box::new(move |jpg| {
                    preview.emit(ShareEvent::PreviewFrame { n: preview.n, jpg });
                }),
            },
        )
    }

    /// Both layers' encoders (of `generation`); a hardware failure falls back to software (720p30)
    /// for both. They take frames once [`Self::install`] connects them.
    fn start_encoders(
        &mut self,
        profile: H264Profile,
        epoch: Instant,
        transport: &TransportHandle,
        generation: u32,
    ) -> Result<([EncoderHandle; 2], [LayerSpec; 2]), Fail> {
        let mut software = self.software;
        loop {
            let specs = layers(self.src_size, &self.opts, software);
            let spawn = |spec: LayerSpec| {
                let transport = transport.clone();
                let shared = self.shared.clone();
                let gpu = self.gpu.clone();
                encoder::spawn(
                    self.gpu.clone(),
                    EncoderConfig {
                        layer: spec.layer,
                        w: spec.w,
                        h: spec.h,
                        fps: spec.fps,
                        kbps: spec.max_kbps,
                        max_kbps: spec.max_kbps,
                        profile,
                        epoch,
                        software,
                    },
                    Box::new(move |event| match event {
                        EncEvent::Frame(f) => {
                            transport.send(ToTransport::Video {
                                layer: f.layer,
                                data: f.data,
                                at: f.at,
                                rtp: f.rtp,
                                key: f.key,
                                generation,
                            });
                        }
                        EncEvent::Failed => {
                            // A replaced encoder's end is none of the share's.
                            if shared.encoders_gen.load(Ordering::SeqCst) != generation {
                                return;
                            }
                            if gpu.removed() {
                                shared.lose_device();
                            } else {
                                shared.end(ShareEvent::Lost {
                                    n: shared.n,
                                    why: LostWhy::Encoder,
                                });
                            }
                        }
                    }),
                )
            };
            let full = spawn(specs[0]);
            let small = full.as_ref().ok().map(|_| spawn(specs[1]));
            match (full, small) {
                (Ok(f), Some(Ok(q))) => {
                    // Software on one layer means software on both (one fallback, one size rule).
                    if software || (f.hardware && q.hardware) {
                        self.software = software;
                        return Ok(([f, q], specs));
                    }
                }
                (Err(e), _) | (_, Some(Err(e))) => warn!("share encoder: {e}"),
                _ => {}
            }
            if software {
                return Err(Fail::Lost(LostWhy::Encoder));
            }
            software = true;
        }
    }

    /// New scalers for `specs`, and the capture's frames to `encoders` from now: both change under
    /// the sink's lock, so no frame of one size reaches an encoder of another.
    fn install(&self, specs: &[LayerSpec; 2], encoders: &[EncoderHandle; 2]) -> Result<(), Fail> {
        let scaler = |spec: &LayerSpec| {
            Scaler::new(self.gpu.clone(), (spec.w, spec.h)).map_err(|e| {
                warn!("share scaler: {e}");
                Fail::Lost(LostWhy::Encoder)
            })
        };
        let sink = FrameSink {
            scalers: [scaler(&specs[0])?, scaler(&specs[1])?],
            gates: [FrameGate::new(specs[0].fps), FrameGate::new(specs[1].fps)],
            latest: None,
        };
        let mut slot = self
            .shared
            .sink
            .lock()
            .map_err(|_| Fail::Lost(LostWhy::Internal))?;
        if let Ok(mut senders) = self.shared.encoders.lock() {
            *senders = [Some(encoders[0].sender()), Some(encoders[1].sender())];
        }
        *slot = Some(sink);
        Ok(())
    }

    /// SQOL: the number of the capture about to start; the one running now stops counting.
    fn next_capture(&self) -> u64 {
        self.shared.capture.fetch_add(1, Ordering::SeqCst) + 1
    }

    /// A capture of `item` whose frames count while it is capture number `number`.
    fn start_capture(
        &self,
        number: u64,
        item: &GraphicsCaptureItem,
        thumbs: Option<Arc<Thumbs>>,
    ) -> Result<Capture, Fail> {
        let shared = self.shared.clone();
        let gpu = self.gpu.clone();
        let on_frame = Box::new(
            move |texture: &windows::Win32::Graphics::Direct3D11::ID3D11Texture2D,
                  content: (u32, u32),
                  now: Instant| {
                if shared.stopped.load(Ordering::Relaxed)
                    || shared.capture.load(Ordering::Relaxed) != number
                {
                    return;
                }
                let held = shared.held.load(Ordering::Relaxed);
                // SHARE-LIVE: every frame counts for the still picture, resting or not (SQOL: and
                // the sharer's preview); a held share's frames show nothing to anyone.
                if let Some(thumbs) = &thumbs {
                    thumbs.frame(texture, content, now, held);
                }
                if held || shared.paused.load(Ordering::Relaxed) {
                    return;
                }
                let Ok(mut sink) = shared.sink.lock() else {
                    return;
                };
                let Some(sink) = sink.as_mut() else {
                    return;
                };
                for layer in Layer::ALL {
                    let i = layer.index();
                    if !sink.gates[i].admit(now) {
                        continue;
                    }
                    match sink.scalers[i].convert(texture, content) {
                        Ok((out, picture)) => {
                            if layer == Layer::Full {
                                sink.latest = Some((SendCom(out.clone()), picture, now));
                            }
                            shared.to_encoder(
                                layer,
                                EncIn::Frame {
                                    texture: SendCom(out),
                                    at: now,
                                },
                            );
                        }
                        Err(e) => {
                            // SQOL: a lost device fails every conversion: rebuilt elsewhere.
                            if gpu.removed() {
                                shared.lose_device();
                                return;
                            }
                            warn!("share convert: {e}");
                        }
                    }
                }
            },
        );
        let closed = self.shared.clone();
        let on_closed = Box::new(move || {
            // SQOL: the share goes on (the controller decides what comes instead).
            let _ = closed.internal.send(Ctl::SourceClosed {
                n: closed.n,
                generation: closed.generation,
                capture: number,
            });
        });
        Capture::start(
            &self.gpu,
            item.clone(),
            self.opts.cursor,
            on_frame,
            on_closed,
        )
        .map_err(|e| {
            warn!("share capture: {e}");
            Fail::Lost(LostWhy::Capture)
        })
    }

    /// SQOL: the shared window's minimize events (a game or a window), told while it is the
    /// share's source.
    fn watch_minimize(&self) -> Option<MinimizeWatch> {
        let (hwnd, pid) = (self.hwnd?, self.target_pid?);
        let shared = self.shared.clone();
        let epoch = shared.source_epoch.load(Ordering::SeqCst);
        MinimizeWatch::start(
            hwnd,
            pid,
            Box::new(move |on| {
                if shared.source_epoch.load(Ordering::SeqCst) == epoch
                    && shared.minimized.swap(on, Ordering::SeqCst) != on
                {
                    shared.emit(ShareEvent::Minimized { n: shared.n, on });
                }
            }),
        )
    }

    fn start_audio(
        &self,
        negotiated: &Negotiated,
        epoch: Instant,
        transport: &TransportHandle,
    ) -> Option<AudioHandle> {
        if !self.audio_line || !negotiated.has_audio() {
            return None;
        }
        let target = self.current_audio_target();
        let transport = transport.clone();
        let failed = Arc::new(AtomicBool::new(false));
        let shared = self.shared.clone();
        let on_failed = Box::new(move || {
            shared.emit(ShareEvent::AudioUnavailable {
                n: shared.n,
                why: AudioWhy::Failed,
            });
        });
        if let Ok(mut info) = self.shared.info.lock() {
            info.audio = target.is_some();
        }
        super::audio::start(
            target,
            epoch,
            Box::new(move |data, at, rtp| {
                transport.send(ToTransport::Audio { data, at, rtp });
            }),
            failed,
            on_failed,
        )
        .map_err(|e| warn!("share audio thread: {e}"))
        .ok()
    }

    fn transport_ctl(&self, thumbs: Option<Arc<Thumbs>>) -> Box<dyn FnMut(MediaCtl) + Send> {
        let shared = self.shared.clone();
        Box::new(move |msg| match msg {
            MediaCtl::KeyFrame(layer) => shared.to_encoder(layer, EncIn::KeyFrame),
            MediaCtl::Rate(plan) => {
                let small_fps = shared.info.lock().map(|i| i.small_fps).unwrap_or(15);
                shared.to_encoder(
                    Layer::Full,
                    EncIn::Rate {
                        kbps: plan.full_kbps,
                        fps: plan.full_fps,
                    },
                );
                shared.to_encoder(
                    Layer::Small,
                    EncIn::Rate {
                        kbps: plan.small_kbps,
                        fps: small_fps.min(plan.full_fps),
                    },
                );
                if let Ok(mut sink) = shared.sink.lock()
                    && let Some(sink) = sink.as_mut()
                {
                    sink.gates[Layer::Full.index()].set_fps(plan.full_fps);
                    sink.gates[Layer::Small.index()].set_fps(small_fps.min(plan.full_fps));
                }
            }
            MediaCtl::Connected => {
                shared.emit(ShareEvent::Started { n: shared.n });
                if let Some(thumbs) = &thumbs {
                    thumbs.started();
                }
            }
            MediaCtl::Stats {
                kbps,
                full_fps,
                bwe_kbps,
            } => {
                let stats = match shared.info.lock() {
                    Ok(info) => ShareStats {
                        kbps,
                        fps: full_fps,
                        w: info.w,
                        h: info.h,
                        encoder: info.encoder.clone(),
                        software: info.software,
                        bwe_kbps,
                        audio: info.audio,
                        paused: shared.paused.load(Ordering::Relaxed),
                    },
                    Err(_) => return,
                };
                shared.emit(ShareEvent::Stats { n: shared.n, stats });
            }
            MediaCtl::Lost(why) => shared.end(ShareEvent::Lost { n: shared.n, why }),
            MediaCtl::Closed => {}
        })
    }

    fn pause(&mut self, paused: bool) {
        self.shared.paused.store(paused, Ordering::SeqCst);
        if let Stage::Live(live) = &self.stage {
            live.transport.send(ToTransport::Paused(paused));
            if let Some(audio) = &live.audio {
                audio.set(if paused {
                    None
                } else {
                    self.current_audio_target()
                });
            }
        }
        // SQOL: encoders made while resting have no picture yet: a fresh capture gives them one.
        if !paused && std::mem::take(&mut self.fresh_on_resume) {
            self.recapture();
        }
    }

    /// The sound now: none while the share holds a blank picture (SQOL), else as chosen.
    fn current_audio_target(&self) -> Option<AudioTarget> {
        if self.held || self.gone {
            return None;
        }
        audio_target(&self.opts, &self.source, self.target_pid, &self.env)
            .ok()
            .flatten()
    }

    fn retarget_audio(&mut self, env: &ShareEnv) {
        self.env = env.clone();
        self.retarget_sound();
    }

    /// SQOL: the sound follows what is shared now (another program, a hold, the choice).
    fn retarget_sound(&mut self) {
        let target = self.current_audio_target();
        if let Stage::Live(live) = &self.stage
            && let Some(audio) = &live.audio
            && !self.shared.paused.load(Ordering::SeqCst)
        {
            audio.set(target);
        }
        if let Ok(mut info) = self.shared.info.lock() {
            info.audio = target.is_some();
        }
    }

    fn update(&mut self, opts: ShareOpts) {
        let old = self.opts;
        self.opts = opts;
        let Stage::Live(live) = &self.stage else {
            return;
        };
        let old_layers = live.layers;
        if old.cursor != opts.cursor
            && let Some(capture) = &live.capture
        {
            capture.set_cursor(opts.cursor);
        }
        if old.audio != opts.audio || old.audio_scope != opts.audio_scope {
            if let Err(why) = audio_target(&opts, &self.source, self.target_pid, &self.env) {
                self.shared.emit(ShareEvent::AudioUnavailable {
                    n: self.shared.n,
                    why,
                });
            }
            self.retarget_sound();
        }
        let specs = layers(self.src_size, &opts, self.software);
        if restart_needed(&old_layers, &specs) {
            // A new size or frame rate: new encoders and scalers, and a first frame for them.
            if let Err(why) = self.restart_encoders() {
                self.shared.end(why.event(self.shared.n));
                return;
            }
            self.refresh();
            return;
        }
        if let Ok(mut sink) = self.shared.sink.lock()
            && let Some(sink) = sink.as_mut()
        {
            sink.gates[0].set_fps(specs[0].fps);
            sink.gates[1].set_fps(specs[1].fps);
        }
        if let Stage::Live(live) = &mut self.stage {
            live.layers = specs;
            live.transport.send(ToTransport::Reconfigure {
                full: specs[0],
                small: specs[1],
                audio: opts.audio,
                tuning: opts.tuning,
            });
        }
    }

    /// SQOL: new encoders and scalers for the source's size now and the options (the old ones
    /// stop; frames they still had in flight are dropped by the transport). They have no picture
    /// until the next frame: [`Self::refresh`] gives them one.
    fn restart_encoders(&mut self) -> Result<(), Fail> {
        let (profile, epoch, transport) = match &self.stage {
            Stage::Live(live) => (live.profile, live.epoch, live.transport.clone()),
            _ => return Ok(()),
        };
        let generation = self.shared.encoders_gen.fetch_add(1, Ordering::SeqCst) + 1;
        let (encoders, specs) = self.start_encoders(profile, epoch, &transport, generation)?;
        transport.send(ToTransport::Floor(generation));
        self.install(&specs, &encoders)?;
        if let Ok(mut info) = self.shared.info.lock() {
            info.w = specs[0].w;
            info.h = specs[0].h;
            info.small_fps = specs[1].fps;
            info.encoder = encoders[0].name.clone();
            info.software = !encoders[0].hardware;
        }
        if let Stage::Live(live) = &mut self.stage {
            // The old encoders stop as their handles go.
            live.encoders = encoders;
            live.layers = specs;
            live.transport.send(ToTransport::Reconfigure {
                full: specs[0],
                small: specs[1],
                audio: self.opts.audio,
                tuning: self.opts.tuning,
            });
        }
        Ok(())
    }

    /// SQOL: a picture for encoders that have none: the source captured anew (its first frame
    /// comes even from a still window), or black while the share holds or its source is gone.
    fn refresh(&mut self) {
        // Black while the share holds, and while encoding rests (nobody sees it; the keep-alive
        // repeats it so the SFU keeps the tracks).
        if self.held || self.gone || self.shared.paused.load(Ordering::SeqCst) {
            self.blank();
        }
        if !self.held && !self.gone {
            self.fresh();
        }
        self.shared.key_both();
    }

    /// SQOL: the source's next frame for the encoders now, or (while encoding rests and drops
    /// frames) when it goes on.
    fn fresh(&mut self) {
        if self.shared.paused.load(Ordering::SeqCst) {
            self.fresh_on_resume = true;
        } else {
            self.recapture();
        }
    }

    /// SQOL: the same source captured anew (a fresh first frame); the running capture goes.
    fn recapture(&mut self) {
        let Stage::Live(live) = &self.stage else {
            return;
        };
        let Some(item) = live.item.as_ref().map(|item| item.0.clone()) else {
            return;
        };
        let thumbs = live.thumbs.clone();
        let number = self.next_capture();
        match self.start_capture(number, &item, thumbs) {
            Ok(capture) => {
                if let Stage::Live(live) = &mut self.stage {
                    live.capture = Some(capture);
                }
            }
            Err(_) => warn!("share {}: the capture did not start again", self.shared.n),
        }
    }

    /// SQOL: black on both layers now: the encoders' last picture (which their keep-alive repeats)
    /// and the still picture's source are no longer the source's.
    fn blank(&self) {
        let now = Instant::now();
        let Ok(mut sink) = self.shared.sink.lock() else {
            return;
        };
        let Some(sink) = sink.as_mut() else {
            return;
        };
        for layer in Layer::ALL {
            match sink.scalers[layer.index()].blank() {
                Ok((out, picture)) => {
                    if layer == Layer::Full {
                        sink.latest = Some((SendCom(out.clone()), picture, now));
                    }
                    self.shared.to_encoder(
                        layer,
                        EncIn::Frame {
                            texture: SendCom(out),
                            at: now,
                        },
                    );
                }
                Err(e) => {
                    if self.gpu.removed() {
                        self.shared.lose_device();
                        return;
                    }
                    warn!("share blank: {e}");
                }
            }
        }
    }

    fn sync_held(&self) {
        self.shared
            .held
            .store(self.held || self.gone, Ordering::SeqCst);
    }

    /// SQOL: share `source` instead (the player's "Ekranı Değiştir", or the launcher's switch to a
    /// game it started). A game without its window yet is waited for.
    fn switch(&mut self, source: SourceId, now: Instant) {
        if let SourceId::Game { pid } = source
            && sources::find_game_window(pid).is_none()
        {
            self.waiting = Some(Wait::switch(pid, now));
            return;
        }
        self.waiting = None;
        match self.capture_source(source, false) {
            Ok(()) => self.switched(),
            Err(why) => self.shared.emit(ShareEvent::SwitchFailed {
                n: self.shared.n,
                why: why.switch_why(),
            }),
        }
    }

    fn switched(&self) {
        let full = match &self.stage {
            Stage::Live(live) => live.layers[0],
            _ => layers(self.src_size, &self.opts, self.software)[0],
        };
        let (w, h) = (full.w, full.h);
        self.shared.emit(ShareEvent::Switched {
            n: self.shared.n,
            w,
            h,
        });
    }

    /// SQOL: captures `source` in place of what the share shows now. The old capture's frames stop
    /// counting, a black frame stands in (never the old source's last picture), new encoders come
    /// when the size or the frame rate changes (`rebuild`: always), then the new capture starts and
    /// the old one goes. When the new source cannot be captured the old one is captured anew and
    /// goes on.
    fn capture_source(&mut self, source: SourceId, rebuild: bool) -> Result<(), Fail> {
        let (item, target_pid, hwnd) = resolve(&source)?;
        let src_size = capture::item_size(&item);
        let old_layers = match &self.stage {
            Stage::Live(live) => live.layers,
            _ => {
                // Before the answer: this is what will be captured.
                self.item = Some(SendCom(item));
                self.adopt(source, src_size, target_pid, hwnd);
                return Ok(());
            }
        };
        let before = (
            self.source.clone(),
            self.src_size,
            self.target_pid,
            self.hwnd,
        );
        self.adopt(source, src_size, target_pid, hwnd);
        let number = self.next_capture();
        let specs = layers(src_size, &self.opts, self.software);
        if (rebuild || restart_needed(&old_layers, &specs))
            && let Err(why) = self.restart_encoders()
        {
            // No encoders: the share cannot go on.
            self.shared.end(ShareEvent::Lost {
                n: self.shared.n,
                why: LostWhy::Encoder,
            });
            return Err(why);
        }
        self.blank();
        self.shared.key_both();
        let thumbs = match &self.stage {
            Stage::Live(live) => live.thumbs.clone(),
            _ => None,
        };
        let capture = match self.start_capture(number, &item, thumbs) {
            Ok(capture) => capture,
            Err(why) => {
                // The old source goes on: captured anew, so even a still one shows again.
                let (source, src_size, target_pid, hwnd) = before;
                self.adopt(source, src_size, target_pid, hwnd);
                if !self.gone {
                    self.fresh();
                }
                return Err(why);
            }
        };
        if let Stage::Live(live) = &mut self.stage {
            live.capture = Some(capture);
            live.item = Some(SendCom(item));
            live.minimize = None;
        }
        // Encoding rests (nobody watches): the new capture's first frame is dropped meanwhile, so
        // the share's return captures anew.
        if self.shared.paused.load(Ordering::SeqCst) {
            self.fresh_on_resume = true;
        }
        self.gone = false;
        self.shared.source_epoch.fetch_add(1, Ordering::SeqCst);
        self.shared.minimized.store(false, Ordering::SeqCst);
        self.sync_held();
        let minimize = self.watch_minimize();
        if let Stage::Live(live) = &mut self.stage {
            live.minimize = minimize;
        }
        self.retarget_sound();
        Ok(())
    }

    fn adopt(
        &mut self,
        source: SourceId,
        src_size: (u32, u32),
        target_pid: Option<u32>,
        hwnd: Option<u32>,
    ) {
        self.source = source;
        self.src_size = src_size;
        self.target_pid = target_pid;
        self.hwnd = hwnd;
    }

    /// SQOL "Duraklat" (`on`) / "Sürdür": black and silent, or the source again with a fresh frame.
    fn hold(&mut self, on: bool) {
        if self.held == on {
            return;
        }
        self.held = on;
        self.sync_held();
        if matches!(self.stage, Stage::Live(_)) {
            if on {
                self.blank();
                self.shared.key_both();
            } else if !self.gone {
                self.fresh();
                self.shared.key_both();
            }
        }
        self.retarget_sound();
    }

    /// SQOL: capture `number`'s source closed. The share holds a blank picture; the game's new
    /// window (a fullscreen switch recreates it) is looked for a moment, any other source is gone.
    fn source_closed(&mut self, number: u64, now: Instant) {
        if self.shared.capture.load(Ordering::SeqCst) != number {
            return;
        }
        if let Stage::Live(live) = &mut self.stage {
            live.capture = None;
            live.item = None;
            live.minimize = None;
        }
        self.shared.source_epoch.fetch_add(1, Ordering::SeqCst);
        self.shared.minimized.store(false, Ordering::SeqCst);
        self.gone = true;
        self.sync_held();
        self.blank();
        self.shared.key_both();
        self.retarget_sound();
        match self.source {
            SourceId::Game { pid } => self.waiting = Some(Wait::reacquire(pid, now)),
            _ => self
                .shared
                .emit(ShareEvent::SourceGone { n: self.shared.n }),
        }
    }

    fn deadline(&self) -> Option<Instant> {
        self.waiting.map(|wait| wait.deadline())
    }

    /// SQOL: looks for the game's window when it is time.
    fn tick(&mut self, now: Instant) {
        let Some(mut wait) = self.waiting else {
            return;
        };
        if !wait.due(now) {
            return;
        }
        if sources::find_game_window(wait.pid).is_some() {
            self.waiting = None;
            match self.capture_source(SourceId::Game { pid: wait.pid }, false) {
                // Asked: the switch; not asked: the game's new window, the share goes on.
                Ok(()) => self.switched(),
                Err(why) if wait.asked => self.shared.emit(ShareEvent::SwitchFailed {
                    n: self.shared.n,
                    why: why.switch_why(),
                }),
                Err(_) => {
                    if self.gone {
                        self.shared
                            .emit(ShareEvent::SourceGone { n: self.shared.n });
                    }
                }
            }
            return;
        }
        if wait.missed(now) {
            self.waiting = Some(wait);
            return;
        }
        self.waiting = None;
        if wait.asked {
            self.shared.emit(ShareEvent::SwitchFailed {
                n: self.shared.n,
                why: SwitchWhy::Gone,
            });
        } else {
            self.shared
                .emit(ShareEvent::SourceGone { n: self.shared.n });
        }
    }

    /// SQOL: the sharer's own preview on or off.
    fn preview(&mut self, on: bool) {
        self.preview = on;
        if let Stage::Live(live) = &self.stage
            && let Some(thumbs) = &live.thumbs
        {
            thumbs.preview(on);
        }
    }

    /// SQOL: everything that lived on the lost device made again on `gpu`: still pictures,
    /// scalers, encoders and the capture. The SFU session and the sound go on; watchers get a
    /// keyframe of the source again.
    fn rebuild(&mut self, gpu: Arc<Gpu>) -> Result<(), Fail> {
        self.rebuilds += 1;
        if self.rebuilds > REBUILDS_MAX {
            return Err(Fail::Lost(LostWhy::Device));
        }
        info!("share {}: rebuilt on a new graphics device", self.shared.n);
        self.gpu = gpu;
        self.shared.device_lost.store(false, Ordering::SeqCst);
        if !matches!(self.stage, Stage::Live(_)) {
            // Before the answer nothing lives on the device yet.
            return Ok(());
        }
        // The old capture's frames stop counting; its worker and the rest go below.
        self.next_capture();
        if let Stage::Live(live) = &mut self.stage {
            live.capture = None;
            live.minimize = None;
            live.thumbs = None;
            live._thumb_handle = None;
        }
        let (thumbs, handle) = match self.start_thumbs() {
            Ok((thumbs, handle)) => (Some(thumbs), Some(handle)),
            Err(e) => {
                warn!("share thumb thread: {e}");
                (None, None)
            }
        };
        if self.preview
            && let Some(thumbs) = &thumbs
        {
            thumbs.preview(true);
        }
        if let Stage::Live(live) = &mut self.stage {
            live.thumbs = thumbs;
            live._thumb_handle = handle;
        }
        self.restart_encoders()?;
        if self.gone {
            self.blank();
            self.shared.key_both();
            return Ok(());
        }
        let source = self.source.clone();
        match self.capture_source(source, false) {
            Ok(()) => Ok(()),
            Err(Fail::Gone) => {
                // The source went meanwhile: as when it closes.
                self.gone = true;
                self.sync_held();
                self.blank();
                self.retarget_sound();
                match self.source {
                    SourceId::Game { pid } => {
                        self.waiting = Some(Wait::reacquire(pid, Instant::now()));
                    }
                    _ => self
                        .shared
                        .emit(ShareEvent::SourceGone { n: self.shared.n }),
                }
                Ok(())
            }
            Err(why) => Err(why),
        }
    }

    /// Releases everything; threads wind down on their own (within a second).
    fn stop(self) {
        self.shared.stopped.store(true, Ordering::SeqCst);
        if let Ok(mut sink) = self.shared.sink.lock() {
            *sink = None;
        }
        if let Ok(mut encoders) = self.shared.encoders.lock() {
            *encoders = [None, None];
        }
        if let Stage::Live(live) = self.stage {
            live.transport.send(ToTransport::Stop);
            drop(live);
        }
    }
}
