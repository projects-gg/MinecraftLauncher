//! The Windows backend of the controller: one [`Session`] per share, from `Prepare` (capture item,
//! offer) through `Answer` (encoders, capture, audio, transport) to its end.

use std::collections::{HashMap, HashSet};
use std::sync::atomic::{AtomicBool, Ordering};
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
use super::thumb::{Host, Latest, ThumbHandle, Thumbs};
use super::{SendCom, sources};
use crate::api::{
    AudioScope, AudioWhy, LostWhy, ShareCmd, ShareEnv, ShareEvent, ShareNo, ShareOpts, ShareStats,
    SourceId,
};
use crate::controller::{Ctl, EventTx};
use crate::preset::{AUDIO_KBPS, FrameGate, Layer, LayerSpec, layers};
use crate::transport::{
    self, H264Profile, MediaCtl, Negotiated, Prepared, ToTransport, TransportHandle,
};

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
}

/// What both the controller thread and the media threads touch.
struct Shared {
    n: ShareNo,
    generation: u64,
    stopped: AtomicBool,
    paused: AtomicBool,
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
}

/// Frames from the capture thread → scalers → encoders.
struct FrameSink {
    scalers: [Scaler; 2],
    gates: [FrameGate; 2],
    /// The full layer's newest encoder input (SHARE-LIVE's still picture is cut from it).
    latest: Option<Latest>,
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
    _capture: Capture,
    encoders: [EncoderHandle; 2],
    audio: Option<AudioHandle>,
    layers: [LayerSpec; 2],
    profile: H264Profile,
    epoch: Instant,
    /// SHARE-LIVE: the still picture's worker (none when its thread could not start).
    _thumbs: Option<ThumbHandle>,
}

pub struct Session {
    generation: u64,
    source: SourceId,
    opts: ShareOpts,
    env: ShareEnv,
    gpu: Arc<Gpu>,
    item: Option<SendCom<GraphicsCaptureItem>>,
    src_size: (u32, u32),
    /// The program whose sound goes with a game/window share.
    target_pid: Option<u32>,
    audio_line: bool,
    software: bool,
    shared: Arc<Shared>,
    stage: Stage,
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
        let (item, target_pid) = match &source {
            SourceId::Game { pid } => {
                let window = sources::find_game_window(*pid).ok_or(Fail::Gone)?;
                let item = capture::item_for_window(window.hwnd as isize)
                    .map_err(|_| Fail::Lost(LostWhy::Capture))?;
                (item, Some(*pid))
            }
            SourceId::Window { hwnd, pid } => {
                if !sources::window_alive(*hwnd, *pid) {
                    return Err(Fail::Gone);
                }
                let item = capture::item_for_window(*hwnd as isize)
                    .map_err(|_| Fail::Lost(LostWhy::Capture))?;
                (item, Some(*pid))
            }
            SourceId::Monitor { device } => {
                let monitor = sources::monitor_by_device(device).ok_or(Fail::Gone)?;
                let item = capture::item_for_monitor(monitor.handle)
                    .map_err(|_| Fail::Lost(LostWhy::Capture))?;
                (item, None)
            }
        };
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
                audio_line,
                software,
                shared,
                stage: Stage::Prepared(Box::new(prepared)),
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

        let (encoders, layers) = self.start_encoders(profile, epoch, &handle)?;
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
        self.install_sink(&layers)?;
        let (thumbs, thumb_handle) = match self.start_thumbs() {
            Ok((thumbs, handle)) => (Some(thumbs), Some(handle)),
            Err(e) => {
                warn!("share thumb thread: {e}");
                (None, None)
            }
        };
        let capture = self.start_capture(thumbs.clone())?;
        let audio = self.start_audio(&negotiated, epoch, &handle);
        let ctl = self.transport_ctl(thumbs);
        negotiated
            .start(inbox, full, small, self.opts.tuning, ctl)
            .map_err(|_| Fail::Lost(LostWhy::Internal))?;
        self.stage = Stage::Live(Live {
            transport: handle,
            _capture: capture,
            encoders,
            audio,
            layers,
            profile,
            epoch,
            _thumbs: thumb_handle,
        });
        Ok(())
    }

    /// SHARE-LIVE: the still picture's worker, fed from the full layer's encoder input (or, while
    /// encoding rests, from the next captured frame).
    fn start_thumbs(&self) -> std::io::Result<(Arc<Thumbs>, ThumbHandle)> {
        let paused = self.shared.clone();
        let latest = self.shared.clone();
        let emit = self.shared.clone();
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
            },
        )
    }

    /// Both layers' encoders; a hardware failure falls back to software (720p30) for both.
    fn start_encoders(
        &mut self,
        profile: H264Profile,
        epoch: Instant,
        transport: &TransportHandle,
    ) -> Result<([EncoderHandle; 2], [LayerSpec; 2]), Fail> {
        let mut software = self.software;
        loop {
            let specs = layers(self.src_size, &self.opts, software);
            let spawn = |spec: LayerSpec| {
                let transport = transport.clone();
                let shared = self.shared.clone();
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
                            });
                        }
                        EncEvent::Failed => shared.end(ShareEvent::Lost {
                            n: shared.n,
                            why: LostWhy::Encoder,
                        }),
                    }),
                )
            };
            let full = spawn(specs[0]);
            let small = full.as_ref().ok().map(|_| spawn(specs[1]));
            match (full, small) {
                (Ok(f), Some(Ok(q))) => {
                    // Software on one layer means software on both (one fallback, one size rule).
                    if software || (f.hardware && q.hardware) {
                        if let Ok(mut encoders) = self.shared.encoders.lock() {
                            *encoders = [Some(f.sender()), Some(q.sender())];
                        }
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

    fn install_sink(&self, specs: &[LayerSpec; 2]) -> Result<(), Fail> {
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
        if let Ok(mut slot) = self.shared.sink.lock() {
            *slot = Some(sink);
        }
        Ok(())
    }

    fn start_capture(&mut self, thumbs: Option<Arc<Thumbs>>) -> Result<Capture, Fail> {
        let item = self.item.take().ok_or(Fail::Lost(LostWhy::Internal))?.0;
        let shared = self.shared.clone();
        let on_frame = Box::new(
            move |texture: &windows::Win32::Graphics::Direct3D11::ID3D11Texture2D,
                  content: (u32, u32),
                  now: Instant| {
                if shared.stopped.load(Ordering::Relaxed) {
                    return;
                }
                // SHARE-LIVE: every frame counts for the still picture, resting or not.
                if let Some(thumbs) = &thumbs {
                    thumbs.frame(texture, content, now);
                }
                if shared.paused.load(Ordering::Relaxed) {
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
                        Err(e) => warn!("share convert: {e}"),
                    }
                }
            },
        );
        let gone = self.shared.clone();
        let on_closed = Box::new(move || gone.end(ShareEvent::SourceGone { n: gone.n }));
        Capture::start(&self.gpu, item, self.opts.cursor, on_frame, on_closed).map_err(|e| {
            warn!("share capture: {e}");
            Fail::Lost(LostWhy::Capture)
        })
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
    }

    fn current_audio_target(&self) -> Option<AudioTarget> {
        audio_target(&self.opts, &self.source, self.target_pid, &self.env)
            .ok()
            .flatten()
    }

    fn retarget_audio(&mut self, env: &ShareEnv) {
        self.env = env.clone();
        if let Stage::Live(live) = &self.stage
            && let Some(audio) = &live.audio
            && !self.shared.paused.load(Ordering::SeqCst)
        {
            let target = self.current_audio_target();
            audio.set(target);
            if let Ok(mut info) = self.shared.info.lock() {
                info.audio = target.is_some();
            }
        }
    }

    fn update(&mut self, opts: ShareOpts) {
        let old = self.opts;
        self.opts = opts;
        let Stage::Live(live) = &mut self.stage else {
            return;
        };
        if old.cursor != opts.cursor {
            live._capture.set_cursor(opts.cursor);
        }
        if old.audio != opts.audio || old.audio_scope != opts.audio_scope {
            match audio_target(&opts, &self.source, self.target_pid, &self.env) {
                Ok(target) => {
                    if let Some(audio) = &live.audio
                        && !self.shared.paused.load(Ordering::SeqCst)
                    {
                        audio.set(target);
                    }
                    if let Ok(mut info) = self.shared.info.lock() {
                        info.audio = target.is_some();
                    }
                }
                Err(why) => self.shared.emit(ShareEvent::AudioUnavailable {
                    n: self.shared.n,
                    why,
                }),
            }
        }
        let specs = layers(self.src_size, &opts, self.software);
        let size_changed = specs[0].w != live.layers[0].w || specs[0].h != live.layers[0].h;
        if size_changed {
            // A new preset means a new picture size: new encoders and scalers, then a keyframe.
            let profile = live.profile;
            let epoch = live.epoch;
            let transport = live.transport.clone();
            match self.start_encoders(profile, epoch, &transport) {
                Ok((encoders, specs)) => {
                    if self.install_sink(&specs).is_err() {
                        self.shared.end(ShareEvent::Lost {
                            n: self.shared.n,
                            why: LostWhy::Encoder,
                        });
                        return;
                    }
                    if let Ok(mut info) = self.shared.info.lock() {
                        info.w = specs[0].w;
                        info.h = specs[0].h;
                        info.small_fps = specs[1].fps;
                        info.encoder = encoders[0].name.clone();
                        info.software = !encoders[0].hardware;
                    }
                    if let Stage::Live(live) = &mut self.stage {
                        live.encoders = encoders;
                        live.layers = specs;
                    }
                }
                Err(why) => {
                    self.shared.end(why.event(self.shared.n));
                    return;
                }
            }
        } else if let Ok(mut sink) = self.shared.sink.lock()
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
            if size_changed {
                live.transport.send(ToTransport::KeyframeAll);
            }
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
