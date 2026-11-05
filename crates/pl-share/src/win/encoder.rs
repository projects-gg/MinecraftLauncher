//! H.264 with Media Foundation: the GPU vendor's hardware MFT (NVENC, Quick Sync, AMF behind one
//! API), asynchronous and fed with NV12 textures straight from the converter (no copy to the CPU);
//! the Microsoft software MFT as the fallback (720p30 at most, frames copied to the CPU).
//!
//! Settings follow Chromium's MF encoder and Discord's lessons: low-latency mode, no B-frames, a
//! long GOP with keyframes only when the policy forces them, peak-constrained VBR with a 1 s
//! buffer, and bitrate changes through `ICodecAPI` without re-initialising the encoder.
//!
//! Each encoder runs on its own thread and wakes only for a frame, an MFT event (delivered by an
//! `IMFAsyncCallback`, no polling), a control message, or the 1 s keep-alive.

use std::mem::ManuallyDrop;
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel, sync_channel};
use std::sync::{Arc, Mutex, Once};
use std::time::{Duration, Instant};

use tracing::{debug, info, warn};
use windows::Win32::Foundation::{E_NOTIMPL, LUID};
use windows::Win32::Graphics::Direct3D11::{
    D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE, D3D11_TEXTURE2D_DESC,
    D3D11_USAGE_STAGING, ID3D11Texture2D,
};
use windows::Win32::Graphics::Dxgi::Common::{DXGI_FORMAT_NV12, DXGI_SAMPLE_DESC};
use windows::Win32::Media::MediaFoundation::*;
use windows::Win32::System::Com::{CLSCTX_INPROC_SERVER, CoCreateInstance, CoTaskMemFree};
use windows::Win32::System::Variant::VARIANT;
use windows::core::{GUID, HRESULT, Interface, PWSTR, Ref, Result, implement};

use super::SendCom;
use super::d3d::Gpu;
use crate::h264;
use crate::preset::Layer;
use crate::transport::H264Profile;

/// With no new frame for this long, the last picture is encoded again (a few hundred bytes): the
/// SFU drops a track after 30 s without packets, and a watcher joining a still screen gets a
/// picture at once.
pub const KEEPALIVE: Duration = Duration::from_secs(1);

pub struct EncoderConfig {
    pub layer: Layer,
    pub w: u32,
    pub h: u32,
    pub fps: u8,
    pub kbps: u32,
    pub max_kbps: u32,
    pub profile: H264Profile,
    /// Time zero of the share: sample times and RTP times count from here.
    pub epoch: Instant,
    /// Skip the hardware encoders.
    pub software: bool,
}

pub struct Encoded {
    pub layer: Layer,
    pub data: Vec<u8>,
    pub at: Instant,
    /// 90 kHz since the epoch.
    pub rtp: u64,
    pub key: bool,
}

pub enum EncEvent {
    Frame(Encoded),
    /// The encoder stopped working (the session ends the share).
    Failed,
}

pub enum EncIn {
    Frame {
        texture: SendCom<ID3D11Texture2D>,
        at: Instant,
    },
    /// Mean bitrate and the frame-rate ceiling the capture gate uses (the encoder scales its
    /// per-frame budget so the total stays at `kbps`).
    Rate {
        kbps: u32,
        fps: u8,
    },
    KeyFrame,
    /// An event of the asynchronous MFT.
    Event {
        kind: i32,
        status: HRESULT,
    },
    Stop,
}

pub struct EncoderHandle {
    tx: Sender<EncIn>,
    pub name: String,
    pub hardware: bool,
    /// Which codec settings the encoder accepted (diagnostics).
    pub settings: String,
}

impl EncoderHandle {
    pub fn send(&self, msg: EncIn) -> bool {
        self.tx.send(msg).is_ok()
    }

    pub fn sender(&self) -> Sender<EncIn> {
        self.tx.clone()
    }
}

impl Drop for EncoderHandle {
    fn drop(&mut self) {
        let _ = self.tx.send(EncIn::Stop);
    }
}

pub fn mf_startup() {
    static START: Once = Once::new();
    START.call_once(|| {
        // SAFETY: process-wide initialisation, never shut down (the launcher lives on).
        if let Err(e) = unsafe { MFStartup(MF_VERSION, MFSTARTUP_LITE) } {
            warn!("MFStartup: {e}");
        }
    });
}

/// One H.264 encoder as Media Foundation lists it.
#[derive(Debug, Clone)]
pub struct EncoderListing {
    pub name: String,
    pub hardware: bool,
    pub vendor_id: String,
    pub is_async: bool,
}

fn type_info(subtype: GUID) -> MFT_REGISTER_TYPE_INFO {
    MFT_REGISTER_TYPE_INFO {
        guidMajorType: MFMediaType_Video,
        guidSubtype: subtype,
    }
}

fn take_activates(ptr: *mut Option<IMFActivate>, count: u32) -> Vec<IMFActivate> {
    if ptr.is_null() {
        return Vec::new();
    }
    // SAFETY: MFTEnum* returned `count` entries at `ptr`, ours to release.
    let out = unsafe {
        std::slice::from_raw_parts_mut(ptr, count as usize)
            .iter_mut()
            .filter_map(Option::take)
            .collect()
    };
    // SAFETY: allocated by MFTEnum* with CoTaskMemAlloc.
    unsafe { CoTaskMemFree(Some(ptr as *const _)) };
    out
}

/// Hardware H.264 encoders for one adapter (all adapters when `luid` is `None`).
fn hardware_activates(luid: Option<LUID>) -> Vec<IMFActivate> {
    let input = type_info(MFVideoFormat_NV12);
    let output = type_info(MFVideoFormat_H264);
    let flags = MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER;
    let mut ptr = std::ptr::null_mut();
    let mut count = 0u32;
    if let Some(luid) = luid {
        let mut attrs = None;
        // SAFETY: out-parameters valid; LUID bytes copied by SetBlob.
        let ok = unsafe {
            MFCreateAttributes(&mut attrs, 1).is_ok()
                && attrs.as_ref().is_some_and(|a| {
                    let bytes = std::slice::from_raw_parts(
                        (&luid as *const LUID).cast::<u8>(),
                        std::mem::size_of::<LUID>(),
                    );
                    a.SetBlob(&MFT_ENUM_ADAPTER_LUID, bytes).is_ok()
                })
                && MFTEnum2(
                    MFT_CATEGORY_VIDEO_ENCODER,
                    flags,
                    Some(&input),
                    Some(&output),
                    attrs.as_ref(),
                    &mut ptr,
                    &mut count,
                )
                .is_ok()
        };
        if ok {
            return take_activates(ptr, count);
        }
    }
    // SAFETY: as above.
    if unsafe {
        MFTEnumEx(
            MFT_CATEGORY_VIDEO_ENCODER,
            flags,
            Some(&input),
            Some(&output),
            &mut ptr,
            &mut count,
        )
    }
    .is_ok()
    {
        return take_activates(ptr, count);
    }
    Vec::new()
}

fn activate_string(activate: &IMFActivate, key: &GUID) -> String {
    let mut value = PWSTR::null();
    let mut len = 0u32;
    // SAFETY: the string is allocated by MF and freed here.
    unsafe {
        if activate
            .GetAllocatedString(key, &mut value, &mut len)
            .is_err()
            || value.is_null()
        {
            return String::new();
        }
        let s = value.to_string().unwrap_or_default();
        CoTaskMemFree(Some(value.0 as *const _));
        s
    }
}

/// Every H.264 encoder on this computer (hardware on any adapter, and software).
pub fn list() -> Vec<EncoderListing> {
    mf_startup();
    let mut out = Vec::new();
    let output = type_info(MFVideoFormat_H264);
    for (flags, hardware) in [
        (MFT_ENUM_FLAG_HARDWARE | MFT_ENUM_FLAG_SORTANDFILTER, true),
        (
            MFT_ENUM_FLAG_SYNCMFT | MFT_ENUM_FLAG_ASYNCMFT | MFT_ENUM_FLAG_SORTANDFILTER,
            false,
        ),
    ] {
        let mut ptr = std::ptr::null_mut();
        let mut count = 0u32;
        // SAFETY: out-parameters valid.
        if unsafe {
            MFTEnumEx(
                MFT_CATEGORY_VIDEO_ENCODER,
                flags,
                None,
                Some(&output),
                &mut ptr,
                &mut count,
            )
        }
        .is_err()
        {
            continue;
        }
        for activate in take_activates(ptr, count) {
            // SAFETY: a live activation object.
            let is_async = unsafe { activate.GetUINT32(&MF_TRANSFORM_ASYNC) }.unwrap_or(0) == 1;
            out.push(EncoderListing {
                name: activate_string(&activate, &MFT_FRIENDLY_NAME_Attribute),
                hardware,
                vendor_id: activate_string(&activate, &MFT_ENUM_HARDWARE_VENDOR_ID_Attribute),
                is_async,
            });
        }
    }
    out
}

/// The hardware encoder that would be used on this adapter.
pub fn hardware_name(luid: LUID) -> Option<String> {
    mf_startup();
    hardware_activates(Some(luid))
        .first()
        .map(|a| activate_string(a, &MFT_FRIENDLY_NAME_Attribute))
}

#[implement(IMFAsyncCallback)]
struct EventSink {
    generator: Mutex<Option<SendCom<IMFMediaEventGenerator>>>,
    me: Mutex<Option<SendCom<IMFAsyncCallback>>>,
    tx: Mutex<Sender<EncIn>>,
}

impl EventSink {
    fn arm(&self) -> Result<()> {
        let generator = self
            .generator
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|g| g.0.clone()));
        let me = self
            .me
            .lock()
            .ok()
            .and_then(|m| m.as_ref().map(|m| m.0.clone()));
        if let (Some(generator), Some(me)) = (generator, me) {
            // SAFETY: live generator; the callback object outlives the request (cleared on stop).
            unsafe { generator.BeginGetEvent(&me, None)? };
        }
        Ok(())
    }

    fn clear(&self) {
        if let Ok(mut g) = self.generator.lock() {
            *g = None;
        }
        if let Ok(mut m) = self.me.lock() {
            *m = None;
        }
    }
}

impl IMFAsyncCallback_Impl for EventSink_Impl {
    fn GetParameters(&self, _flags: *mut u32, _queue: *mut u32) -> Result<()> {
        Err(E_NOTIMPL.into())
    }

    fn Invoke(&self, result: Ref<IMFAsyncResult>) -> Result<()> {
        let generator = self
            .generator
            .lock()
            .ok()
            .and_then(|g| g.as_ref().map(|g| g.0.clone()));
        let Some(generator) = generator else {
            return Ok(());
        };
        // SAFETY: the result belongs to our BeginGetEvent.
        let event = unsafe { generator.EndGetEvent(result.ok()?) };
        let (kind, status) = match event {
            // SAFETY: a live event.
            Ok(event) => unsafe { (event.GetType()? as i32, event.GetStatus()?) },
            Err(e) if e.code() == MF_E_SHUTDOWN => return Ok(()),
            Err(e) => (MEError.0, e.code()),
        };
        if let Ok(tx) = self.tx.lock()
            && tx.send(EncIn::Event { kind, status }).is_err()
        {
            return Ok(());
        }
        if kind != MEError.0 {
            self.arm()?;
        }
        Ok(())
    }
}

struct Mft {
    transform: IMFTransform,
    codec: Option<ICodecAPI>,
    sink: Option<IMFAsyncCallback>,
    is_async: bool,
    d3d: bool,
    _manager: Option<IMFDXGIDeviceManager>,
    name: String,
    hardware: bool,
    vbr: bool,
    /// Which codec settings the MFT accepted (diagnostics).
    report: String,
}

fn pack(hi: u32, lo: u32) -> u64 {
    (u64::from(hi) << 32) | u64::from(lo)
}

/// Rate control and latency settings; returns whether VBR is used and which settings took.
fn apply_codec(c: &ICodecAPI, cfg: &EncoderConfig) -> (bool, String) {
    let bps = cfg.kbps * 1000;
    let mut report = Vec::new();
    let mut set = |label: &str, key: &GUID, value: VARIANT| {
        let ok = set_codec(c, key, value);
        report.push(format!("{label}={}", if ok { "ok" } else { "no" }));
        ok
    };
    set(
        "lowlatency",
        &CODECAPI_AVLowLatencyMode,
        VARIANT::from(true),
    );
    // CBR first, as Chromium does for WebRTC: it follows the bandwidth estimate most closely
    // (no filler data on these encoders, so a still picture stays small).
    let cbr = set(
        "cbr",
        &CODECAPI_AVEncCommonRateControlMode,
        VARIANT::from(eAVEncCommonRateControlMode_CBR.0 as u32),
    );
    let vbr = !cbr
        && set(
            "vbr",
            &CODECAPI_AVEncCommonRateControlMode,
            VARIANT::from(eAVEncCommonRateControlMode_PeakConstrainedVBR.0 as u32),
        );
    set("mean", &CODECAPI_AVEncCommonMeanBitRate, VARIANT::from(bps));
    if vbr {
        set(
            "max",
            &CODECAPI_AVEncCommonMaxBitRate,
            VARIANT::from(bps / 2 * 3),
        );
    }
    // A one-second buffer: frames may vary, the second averages out.
    set(
        "buffer",
        &CODECAPI_AVEncCommonBufferSize,
        VARIANT::from(bps),
    );
    set(
        "bframes",
        &CODECAPI_AVEncMPVDefaultBPictureCount,
        VARIANT::from(0u32),
    );
    // Two minutes: the policy forces one every 60 s, so the encoder never decides.
    set(
        "gop",
        &CODECAPI_AVEncMPVGOPSize,
        VARIANT::from(u32::from(cfg.fps) * 120),
    );
    (vbr, report.join(" "))
}

fn set_codec(codec: &ICodecAPI, key: &GUID, value: VARIANT) -> bool {
    // SAFETY: the VARIANT is valid for the call.
    unsafe { codec.SetValue(key, &value) }.is_ok()
}

/// Configures `transform` for `cfg`; `Err` lets the caller try the next encoder.
fn configure(
    gpu: &Gpu,
    transform: IMFTransform,
    cfg: &EncoderConfig,
    name: String,
    hardware: bool,
    tx: &Sender<EncIn>,
) -> Result<Mft> {
    // SAFETY: plain MF calls on a live transform; every argument outlives its call.
    unsafe {
        let attrs = transform.GetAttributes().ok();
        let is_async = attrs
            .as_ref()
            .and_then(|a| a.GetUINT32(&MF_TRANSFORM_ASYNC).ok())
            .unwrap_or(0)
            == 1;
        if is_async && let Some(a) = &attrs {
            a.SetUINT32(&MF_TRANSFORM_ASYNC_UNLOCK, 1)?;
        }
        if let Some(a) = &attrs {
            let _ = a.SetUINT32(&MF_LOW_LATENCY, 1);
        }
        let d3d_aware = attrs
            .as_ref()
            .and_then(|a| a.GetUINT32(&MF_SA_D3D11_AWARE).ok())
            .unwrap_or(0)
            == 1;
        let mut manager = None;
        if d3d_aware {
            let mut token = 0u32;
            MFCreateDXGIDeviceManager(&mut token, &mut manager)?;
            let m = manager.as_ref().ok_or_else(windows::core::Error::empty)?;
            m.ResetDevice(&gpu.device, token)?;
            transform.ProcessMessage(MFT_MESSAGE_SET_D3D_MANAGER, m.as_raw() as usize)?;
        }

        let codec: Option<ICodecAPI> = transform.cast().ok();
        // Before the media types (the Microsoft encoder reads the rate control mode there) and
        // again after them (some vendor MFTs reset their settings on SetOutputType).
        let (mut vbr, mut report) = (false, String::from("no ICodecAPI"));
        if let Some(c) = &codec {
            (vbr, report) = apply_codec(c, cfg);
        }

        let bps = cfg.kbps * 1000;
        let profile = match cfg.profile {
            H264Profile::ConstrainedHigh => eAVEncH264VProfile_High.0 as u32,
            H264Profile::ConstrainedBaseline => eAVEncH264VProfile_Base.0 as u32,
        };
        let output = MFCreateMediaType()?;
        output.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        output.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_H264)?;
        output.SetUINT32(&MF_MT_AVG_BITRATE, bps)?;
        output.SetUINT64(&MF_MT_FRAME_SIZE, pack(cfg.w, cfg.h))?;
        output.SetUINT64(&MF_MT_FRAME_RATE, pack(u32::from(cfg.fps), 1))?;
        output.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
        output.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        output.SetUINT32(&MF_MT_MPEG2_PROFILE, profile)?;
        // BT.709 studio range, written into the SPS's VUI.
        let _ = output.SetUINT32(&MF_MT_VIDEO_NOMINAL_RANGE, MFNominalRange_16_235.0 as u32);
        let _ = output.SetUINT32(&MF_MT_YUV_MATRIX, MFVideoTransferMatrix_BT709.0 as u32);
        let _ = output.SetUINT32(&MF_MT_VIDEO_PRIMARIES, MFVideoPrimaries_BT709.0 as u32);
        let _ = output.SetUINT32(&MF_MT_TRANSFER_FUNCTION, MFVideoTransFunc_709.0 as u32);
        transform.SetOutputType(0, &output, 0)?;

        let input = MFCreateMediaType()?;
        input.SetGUID(&MF_MT_MAJOR_TYPE, &MFMediaType_Video)?;
        input.SetGUID(&MF_MT_SUBTYPE, &MFVideoFormat_NV12)?;
        input.SetUINT64(&MF_MT_FRAME_SIZE, pack(cfg.w, cfg.h))?;
        input.SetUINT64(&MF_MT_FRAME_RATE, pack(u32::from(cfg.fps), 1))?;
        input.SetUINT64(&MF_MT_PIXEL_ASPECT_RATIO, pack(1, 1))?;
        input.SetUINT32(&MF_MT_INTERLACE_MODE, MFVideoInterlace_Progressive.0 as u32)?;
        transform.SetInputType(0, &input, 0)?;
        if let Some(c) = &codec {
            (vbr, report) = apply_codec(c, cfg);
        }

        let mut sink = None;
        if is_async {
            let generator: IMFMediaEventGenerator = transform.cast()?;
            let object = EventSink {
                generator: Mutex::new(Some(SendCom(generator))),
                me: Mutex::new(None),
                tx: Mutex::new(tx.clone()),
            };
            let callback: IMFAsyncCallback = object.into();
            let inner = callback.cast_object_ref::<EventSink>()?;
            if let Ok(mut me) = inner.me.lock() {
                *me = Some(SendCom(callback.clone()));
            }
            inner.arm()?;
            sink = Some(callback);
        }
        // Not every MFT accepts these before its first input (the Microsoft encoder fails the
        // flush); none of them is required to start.
        for message in [
            MFT_MESSAGE_COMMAND_FLUSH,
            MFT_MESSAGE_NOTIFY_BEGIN_STREAMING,
            MFT_MESSAGE_NOTIFY_START_OF_STREAM,
        ] {
            if let Err(e) = transform.ProcessMessage(message, 0) {
                debug!("{name}: message {}: {e}", message.0);
            }
        }
        Ok(Mft {
            transform,
            codec,
            sink,
            is_async,
            d3d: d3d_aware,
            _manager: manager,
            name,
            hardware,
            vbr,
            report,
        })
    }
}

fn open(gpu: &Gpu, cfg: &EncoderConfig, tx: &Sender<EncIn>) -> Result<Mft> {
    mf_startup();
    if !cfg.software {
        for activate in hardware_activates(Some(gpu.luid)) {
            let name = activate_string(&activate, &MFT_FRIENDLY_NAME_Attribute);
            // SAFETY: a live activation object.
            let transform = match unsafe { activate.ActivateObject::<IMFTransform>() } {
                Ok(t) => t,
                Err(e) => {
                    debug!("{name}: activate {e}");
                    continue;
                }
            };
            match configure(gpu, transform, cfg, name.clone(), true, tx) {
                Ok(mft) => return Ok(mft),
                Err(e) => {
                    warn!("{name}: {e}");
                    // SAFETY: releases the encoder this activation made.
                    let _ = unsafe { activate.ShutdownObject() };
                }
            }
        }
    }
    // SAFETY: the inbox software encoder.
    let transform: IMFTransform =
        unsafe { CoCreateInstance(&CLSID_MSH264EncoderMFT, None, CLSCTX_INPROC_SERVER)? };
    configure(
        gpu,
        transform,
        cfg,
        "Microsoft H.264 Encoder".into(),
        false,
        tx,
    )
}

/// Starts an encoder thread; returns once the encoder is configured (or failed).
pub fn spawn(
    gpu: Arc<Gpu>,
    cfg: EncoderConfig,
    mut out: Box<dyn FnMut(EncEvent) + Send>,
) -> std::result::Result<EncoderHandle, String> {
    let (tx, rx) = channel();
    let (ready_tx, ready_rx) = sync_channel(1);
    let thread_tx = tx.clone();
    std::thread::Builder::new()
        .name(format!("pl-share-enc-{}", cfg.layer.rid()))
        .spawn(move || {
            super::com_mta();
            let mft = match open(&gpu, &cfg, &thread_tx) {
                Ok(mft) => {
                    let _ = ready_tx.send(Ok((mft.name.clone(), mft.hardware, mft.report.clone())));
                    mft
                }
                Err(e) => {
                    let _ = ready_tx.send(Err(e.to_string()));
                    return;
                }
            };
            drop(thread_tx);
            info!(
                "share encoder {} {}x{}@{} ({}, async {}, d3d {}, vbr {})",
                cfg.layer.rid(),
                cfg.w,
                cfg.h,
                cfg.fps,
                mft.name,
                mft.is_async,
                mft.d3d,
                mft.vbr
            );
            let mut worker = Worker::new(gpu, cfg, mft);
            if worker.run(rx, &mut out).is_err() {
                out(EncEvent::Failed);
            }
            worker.shutdown();
        })
        .map_err(|e| e.to_string())?;
    match ready_rx.recv() {
        Ok(Ok((name, hardware, settings))) => Ok(EncoderHandle {
            tx,
            name,
            hardware,
            settings,
        }),
        Ok(Err(e)) => Err(e),
        Err(_) => Err("encoder thread ended".into()),
    }
}

struct Worker {
    gpu: Arc<Gpu>,
    cfg: EncoderConfig,
    mft: Mft,
    need: u32,
    pending: Option<(ID3D11Texture2D, Instant)>,
    last: Option<ID3D11Texture2D>,
    last_submit: Instant,
    force_key: bool,
    sets: Option<Vec<u8>>,
    staging: Option<ID3D11Texture2D>,
    failures: u32,
}

impl Worker {
    fn new(gpu: Arc<Gpu>, cfg: EncoderConfig, mft: Mft) -> Self {
        Self {
            gpu,
            cfg,
            mft,
            need: 0,
            pending: None,
            last: None,
            last_submit: Instant::now(),
            force_key: true,
            sets: None,
            staging: None,
            failures: 0,
        }
    }

    fn run(
        &mut self,
        rx: Receiver<EncIn>,
        out: &mut Box<dyn FnMut(EncEvent) + Send>,
    ) -> Result<()> {
        loop {
            let wait = (self.last_submit + KEEPALIVE).saturating_duration_since(Instant::now());
            let msg = match rx.recv_timeout(wait) {
                Ok(msg) => msg,
                Err(RecvTimeoutError::Timeout) => {
                    // Keep-alive: the last picture again (an almost empty P-frame).
                    if self.pending.is_none()
                        && !crate::test_no_keepalive()
                        && let Some(last) = self.last.clone()
                    {
                        self.pending = Some((last, Instant::now()));
                        self.feed(out)?;
                    }
                    self.last_submit = Instant::now();
                    continue;
                }
                Err(RecvTimeoutError::Disconnected) => return Ok(()),
            };
            match msg {
                EncIn::Frame { texture, at } => {
                    self.last = Some(texture.0.clone());
                    self.pending = Some((texture.0, at));
                    self.feed(out)?;
                }
                EncIn::Event { kind, status } => {
                    if status.is_err() || kind == MEError.0 {
                        warn!("share encoder {} event {kind}: {status:?}", self.mft.name);
                        return Err(status.into());
                    }
                    if kind == METransformNeedInput.0 {
                        self.need += 1;
                        self.feed(out)?;
                    } else if kind == METransformHaveOutput.0
                        && let Some(frame) = self.output()?
                    {
                        out(EncEvent::Frame(frame));
                    }
                }
                EncIn::Rate { kbps, fps } => self.set_rate(kbps, fps),
                EncIn::KeyFrame => {
                    self.force_key = true;
                    // A still picture (or a pause) sends a frame a second at most: encode the
                    // keyframe now instead of at the next keep-alive, so a new watcher sees the
                    // picture at once.
                    if self.pending.is_none()
                        && self.last_submit.elapsed() > Duration::from_millis(100)
                        && let Some(last) = self.last.clone()
                    {
                        self.pending = Some((last, Instant::now()));
                        self.feed(out)?;
                    }
                }
                EncIn::Stop => return Ok(()),
            }
        }
    }

    fn set_rate(&mut self, kbps: u32, fps: u8) {
        let Some(codec) = &self.mft.codec else {
            return;
        };
        // Fewer frames than configured: each one may use proportionally more, the total stays.
        let scaled = u64::from(kbps) * u64::from(self.cfg.fps) / u64::from(fps.max(1));
        let bps = (scaled.min(u64::from(self.cfg.max_kbps) * 2) * 1000) as u32;
        set_codec(codec, &CODECAPI_AVEncCommonMeanBitRate, VARIANT::from(bps));
        if self.mft.vbr {
            set_codec(
                codec,
                &CODECAPI_AVEncCommonMaxBitRate,
                VARIANT::from(bps / 2 * 3),
            );
        }
        debug!(
            "share encoder {} rate {kbps} kbps ({fps} fps)",
            self.cfg.layer.rid()
        );
    }

    fn feed(&mut self, out: &mut Box<dyn FnMut(EncEvent) + Send>) -> Result<()> {
        if self.mft.is_async && self.need == 0 {
            return Ok(());
        }
        let Some((texture, at)) = self.pending.take() else {
            return Ok(());
        };
        match self.submit(&texture, at) {
            Ok(()) => self.failures = 0,
            Err(e) => {
                self.failures += 1;
                warn!("share encoder {} input: {e}", self.mft.name);
                if self.failures > 30 {
                    return Err(e);
                }
                return Ok(());
            }
        }
        self.last_submit = Instant::now();
        if self.mft.is_async {
            self.need -= 1;
        } else {
            while let Some(frame) = self.output()? {
                out(EncEvent::Frame(frame));
            }
        }
        Ok(())
    }

    fn submit(&mut self, texture: &ID3D11Texture2D, at: Instant) -> Result<()> {
        let since = at.saturating_duration_since(self.cfg.epoch);
        let time = (since.as_nanos() / 100) as i64;
        // SAFETY: MF calls on live objects; buffers are sized for the copies.
        unsafe {
            let buffer = if self.mft.d3d {
                let buffer = MFCreateDXGISurfaceBuffer(&ID3D11Texture2D::IID, texture, 0, false)?;
                if let Ok(b2) = buffer.cast::<IMF2DBuffer>() {
                    let len = b2.GetContiguousLength()?;
                    buffer.SetCurrentLength(len)?;
                }
                buffer
            } else {
                self.cpu_copy(texture)?
            };
            let sample = MFCreateSample()?;
            sample.AddBuffer(&buffer)?;
            sample.SetSampleTime(time)?;
            sample.SetSampleDuration(10_000_000 / i64::from(self.cfg.fps.max(1)))?;
            if self.force_key {
                if let Some(codec) = &self.mft.codec {
                    set_codec(
                        codec,
                        &CODECAPI_AVEncVideoForceKeyFrame,
                        VARIANT::from(1u32),
                    );
                }
                self.force_key = false;
            }
            self.mft.transform.ProcessInput(0, &sample, 0)
        }
    }

    /// The software encoder takes frames in memory: copy through a staging texture.
    unsafe fn cpu_copy(&mut self, texture: &ID3D11Texture2D) -> Result<IMFMediaBuffer> {
        let (w, h) = (self.cfg.w as usize, self.cfg.h as usize);
        if self.staging.is_none() {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: self.cfg.w,
                Height: self.cfg.h,
                MipLevels: 1,
                ArraySize: 1,
                Format: DXGI_FORMAT_NV12,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_STAGING,
                BindFlags: 0,
                CPUAccessFlags: D3D11_CPU_ACCESS_READ.0 as u32,
                MiscFlags: 0,
            };
            let mut staging = None;
            // SAFETY: valid descriptor.
            unsafe {
                self.gpu
                    .device
                    .CreateTexture2D(&desc, None, Some(&mut staging))?
            };
            self.staging = staging;
        }
        let staging = self
            .staging
            .clone()
            .ok_or_else(windows::core::Error::empty)?;
        let size = w * h * 3 / 2;
        // SAFETY: the mapped rows are read within the subresource's pitch; the MF buffer holds
        // `size` bytes.
        unsafe {
            let buffer = MFCreateMemoryBuffer(size as u32)?;
            let mut dst = std::ptr::null_mut();
            buffer.Lock(&mut dst, None, None)?;
            {
                let _lock = self.gpu.lock();
                self.gpu.context.CopyResource(&staging, texture);
                let mut map = D3D11_MAPPED_SUBRESOURCE::default();
                self.gpu
                    .context
                    .Map(&staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
                let pitch = map.RowPitch as usize;
                let src = map.pData as *const u8;
                for row in 0..h {
                    std::ptr::copy_nonoverlapping(src.add(row * pitch), dst.add(row * w), w);
                }
                let uv_src = src.add(pitch * h);
                let uv_dst = dst.add(w * h);
                for row in 0..h / 2 {
                    std::ptr::copy_nonoverlapping(uv_src.add(row * pitch), uv_dst.add(row * w), w);
                }
                self.gpu.context.Unmap(&staging, 0);
            }
            buffer.Unlock()?;
            buffer.SetCurrentLength(size as u32)?;
            Ok(buffer)
        }
    }

    /// One `ProcessOutput`; `None` when the encoder has nothing yet.
    fn output(&mut self) -> Result<Option<Encoded>> {
        // SAFETY: MF calls on live objects; the output structure's references are released once.
        unsafe {
            let info = self.mft.transform.GetOutputStreamInfo(0)?;
            let provides = info.dwFlags
                & (MFT_OUTPUT_STREAM_PROVIDES_SAMPLES.0 | MFT_OUTPUT_STREAM_CAN_PROVIDE_SAMPLES.0)
                    as u32
                != 0;
            let sample = if provides {
                None
            } else {
                let size = info.cbSize.max(self.cfg.w * self.cfg.h * 2);
                let sample = MFCreateSample()?;
                sample.AddBuffer(&MFCreateMemoryBuffer(size)?)?;
                Some(sample)
            };
            let mut buffers = [MFT_OUTPUT_DATA_BUFFER {
                dwStreamID: 0,
                pSample: ManuallyDrop::new(sample),
                dwStatus: 0,
                pEvents: ManuallyDrop::new(None),
            }];
            let mut status = 0u32;
            let result = self
                .mft
                .transform
                .ProcessOutput(0, &mut buffers, &mut status);
            let sample = ManuallyDrop::take(&mut buffers[0].pSample);
            ManuallyDrop::drop(&mut buffers[0].pEvents);
            match result {
                Ok(()) => {}
                Err(e) if e.code() == MF_E_TRANSFORM_NEED_MORE_INPUT => return Ok(None),
                Err(e) if e.code() == MF_E_TRANSFORM_STREAM_CHANGE => {
                    let t = self.mft.transform.GetOutputAvailableType(0, 0)?;
                    self.mft.transform.SetOutputType(0, &t, 0)?;
                    self.sequence_header(&t);
                    return Ok(None);
                }
                Err(e) => return Err(e),
            }
            let Some(sample) = sample else {
                return Ok(None);
            };
            let time = sample.GetSampleTime().unwrap_or(0).max(0) as u64;
            let clean = sample.GetUINT32(&MFSampleExtension_CleanPoint).unwrap_or(0) == 1;
            let buffer = sample.ConvertToContiguousBuffer()?;
            let mut ptr = std::ptr::null_mut();
            let mut len = 0u32;
            buffer.Lock(&mut ptr, None, Some(&mut len))?;
            let data = std::slice::from_raw_parts(ptr, len as usize).to_vec();
            buffer.Unlock()?;
            if data.is_empty() {
                return Ok(None);
            }
            let key = clean || h264::is_keyframe(&data);
            if key && h264::has_parameter_sets(&data) {
                self.sets = h264::parameter_sets(&data);
            }
            if key
                && self.sets.is_none()
                && let Ok(t) = self.mft.transform.GetOutputCurrentType(0)
            {
                self.sequence_header(&t);
            }
            let data = h264::with_parameter_sets(data, self.sets.as_deref());
            Ok(Some(Encoded {
                layer: self.cfg.layer,
                data,
                at: self.cfg.epoch + Duration::from_nanos(time * 100),
                rtp: time * 9 / 1000,
                key,
            }))
        }
    }

    fn sequence_header(&mut self, media_type: &IMFMediaType) {
        // SAFETY: the blob is copied into a buffer of its reported size.
        unsafe {
            let Ok(size) = media_type.GetBlobSize(&MF_MT_MPEG_SEQUENCE_HEADER) else {
                return;
            };
            let mut blob = vec![0u8; size as usize];
            if media_type
                .GetBlob(&MF_MT_MPEG_SEQUENCE_HEADER, &mut blob, None)
                .is_ok()
                && let Some(sets) = h264::sequence_header_to_annex_b(&blob)
            {
                self.sets = Some(sets);
            }
        }
    }

    fn shutdown(&mut self) {
        // SAFETY: shutting down a live transform once.
        unsafe {
            let _ = self
                .mft
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_OF_STREAM, 0);
            let _ = self
                .mft
                .transform
                .ProcessMessage(MFT_MESSAGE_NOTIFY_END_STREAMING, 0);
            if let Some(sink) = self.mft.sink.take()
                && let Ok(inner) = sink.cast_object_ref::<EventSink>()
            {
                inner.clear();
            }
            if let Ok(shutdown) = self.mft.transform.cast::<IMFShutdown>() {
                let _ = shutdown.Shutdown();
            }
        }
        self.pending = None;
        self.last = None;
    }
}
