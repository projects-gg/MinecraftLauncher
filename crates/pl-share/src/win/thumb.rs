//! The still picture's GPU part and its worker ([`crate::thumb`] holds the rules).
//!
//! One video processor blit turns the encoder's NV12 input (or, while encoding rests, a captured
//! BGRA frame) into the picture's BGRA pixels at its small size, one copy brings those few hundred
//! kilobytes to a staging texture, and WIC writes the JPEG. The worker sleeps on its channel
//! between pictures (one wake a minute): nothing polls, and a still source costs no GPU work at all
//! (its last picture goes again).
//!
//! SQOL: the same worker makes the sharer's own preview while it is on: at most twice a second the
//! next captured frame (resting or not, never while the share holds a blank picture) becomes a
//! larger JPEG through a converter of its own. A still source makes none (nothing new to show), and
//! the worker's timer stops until a frame comes.

use std::mem::ManuallyDrop;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::mpsc::{Receiver, RecvTimeoutError, Sender, channel};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use tracing::debug;
use windows::Win32::Foundation::RECT;
use windows::Win32::Graphics::Direct3D11::{
    D3D11_BIND_RENDER_TARGET, D3D11_CPU_ACCESS_READ, D3D11_MAP_READ, D3D11_MAPPED_SUBRESOURCE,
    D3D11_TEX2D_VPIV, D3D11_TEX2D_VPOV, D3D11_TEXTURE2D_DESC, D3D11_USAGE_DEFAULT,
    D3D11_USAGE_STAGING, D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE, D3D11_VIDEO_PROCESSOR_CONTENT_DESC,
    D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0,
    D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC, D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0,
    D3D11_VIDEO_PROCESSOR_STREAM, D3D11_VIDEO_USAGE_OPTIMAL_QUALITY,
    D3D11_VPIV_DIMENSION_TEXTURE2D, D3D11_VPOV_DIMENSION_TEXTURE2D, ID3D11Texture2D,
    ID3D11VideoContext, ID3D11VideoContext1, ID3D11VideoDevice, ID3D11VideoProcessor,
    ID3D11VideoProcessorEnumerator, ID3D11VideoProcessorInputView, ID3D11VideoProcessorOutputView,
};
use windows::Win32::Graphics::Dxgi::Common::{
    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709, DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709,
    DXGI_FORMAT_B8G8R8A8_UNORM, DXGI_RATIONAL, DXGI_SAMPLE_DESC,
};
use windows::core::{BOOL, Interface, Result};

use super::SendCom;
use super::d3d::Gpu;
use crate::thumb;

/// The encoder's newest NV12 input: the texture, where the picture sits inside it (letterboxed),
/// and when its frame was captured.
pub type Latest = (SendCom<ID3D11Texture2D>, RECT, Instant);

/// What the worker needs from its share.
pub struct Host {
    /// Encoding rests (nobody watches): the encoder's input is as old as the rest.
    pub paused: Box<dyn Fn() -> bool + Send>,
    /// The encoder's newest input, once there is one.
    pub latest: Box<dyn Fn() -> Option<Latest> + Send>,
    /// A finished picture for the launcher (`ShareEvent::Thumb`).
    pub emit: Box<dyn Fn(Vec<u8>) + Send>,
    /// SQOL: a finished preview for the sharer (`ShareEvent::PreviewFrame`).
    pub preview: Box<dyn Fn(Vec<u8>) + Send>,
}

enum Msg {
    /// The share is live: the first picture is due soon.
    Start,
    /// The capture callback left a fresh frame's picture in the staging texture.
    Ready {
        at: Instant,
        frames: u64,
    },
    /// It could not.
    Missed,
    /// SQOL: the sharer's preview on or off.
    Preview(bool),
    /// SQOL: the capture callback left a fresh frame's preview in its staging texture (or could
    /// not).
    PreviewReady,
    PreviewMissed,
    /// SQOL-R3: what the share shows changed (a black stand-in, another source, a hold's end): the
    /// last picture is no longer it, a new one is due soon.
    Renew,
    Stop,
}

/// Shared by the capture callback and the worker.
pub struct Thumbs {
    gpu: Arc<Gpu>,
    /// The worker waits for a captured frame (encoding rests).
    wanted: AtomicBool,
    /// Frames the capture delivered so far, resting or not: the same count, the same picture.
    captured: AtomicU64,
    converter: Mutex<Option<Converter>>,
    /// SQOL: the next captured frame becomes the sharer's preview.
    preview_wanted: AtomicBool,
    preview_converter: Mutex<Option<Converter>>,
    tx: Sender<Msg>,
}

/// The share's hold on its worker: dropping it ends the worker.
pub struct ThumbHandle(Arc<Thumbs>);

impl Drop for ThumbHandle {
    fn drop(&mut self) {
        let _ = self.0.tx.send(Msg::Stop);
    }
}

impl Thumbs {
    /// The worker of one share (`n` names its thread).
    pub fn start(gpu: Arc<Gpu>, n: u8, host: Host) -> std::io::Result<(Arc<Self>, ThumbHandle)> {
        let (tx, rx) = channel();
        let thumbs = Arc::new(Self {
            gpu,
            wanted: AtomicBool::new(false),
            captured: AtomicU64::new(0),
            converter: Mutex::new(None),
            preview_wanted: AtomicBool::new(false),
            preview_converter: Mutex::new(None),
            tx,
        });
        let mut worker = Worker {
            thumbs: thumbs.clone(),
            host,
            next: None,
            waiting: None,
            extended: false,
            started: false,
            last: None,
            preview_on: false,
            preview_next: None,
        };
        std::thread::Builder::new()
            .name(format!("pl-share-thumb-{n}"))
            .spawn(move || {
                super::com_mta();
                worker.run(&rx);
            })?;
        Ok((thumbs.clone(), ThumbHandle(thumbs)))
    }

    /// The share went live (the SFU connected).
    pub fn started(&self) {
        let _ = self.tx.send(Msg::Start);
    }

    /// SQOL: the sharer's own preview on or off.
    pub fn preview(&self, on: bool) {
        let _ = self.tx.send(Msg::Preview(on));
    }

    /// SQOL-R3: what the share shows changed (black while it holds, another source): the room's
    /// picture follows within seconds instead of keeping the old one up to a minute (and a held
    /// share's picture is never the one from before the hold).
    pub fn renew(&self) {
        let _ = self.tx.send(Msg::Renew);
    }

    /// Every captured frame, resting or not, on the capture thread: counted, and turned into the
    /// picture when the worker waits for one (SQOL: and into the preview when that waits). `held`:
    /// the share shows a blank picture now, so this frame shows nothing to anyone.
    pub fn frame(&self, texture: &ID3D11Texture2D, content: (u32, u32), at: Instant, held: bool) {
        let frames = self.captured.fetch_add(1, Ordering::Relaxed) + 1;
        if held {
            return;
        }
        let rect = RECT {
            left: 0,
            top: 0,
            right: content.0.min(i32::MAX as u32) as i32,
            bottom: content.1.min(i32::MAX as u32) as i32,
        };
        if self.preview_wanted.swap(false, Ordering::SeqCst) {
            let msg = match blit_with(&self.gpu, &self.preview_converter, PREVIEW, texture, rect) {
                Ok(()) => Msg::PreviewReady,
                Err(e) => {
                    debug!("share preview capture: {e}");
                    Msg::PreviewMissed
                }
            };
            let _ = self.tx.send(msg);
        }
        if !self.wanted.swap(false, Ordering::SeqCst) {
            return;
        }
        let msg = match self.blit(texture, rect, false) {
            Ok(()) => Msg::Ready { at, frames },
            Err(e) => {
                debug!("share thumb capture: {e}");
                Msg::Missed
            }
        };
        let _ = self.tx.send(msg);
    }

    fn blit(&self, src: &ID3D11Texture2D, rect: RECT, nv12: bool) -> Result<()> {
        let mut slot = self
            .converter
            .lock()
            .map_err(|_| windows::core::Error::empty())?;
        if slot.is_none() {
            *slot = Some(Converter::new(&self.gpu, thumb::SIDE)?);
        }
        match slot.as_mut() {
            Some(converter) => converter.blit(&self.gpu, src, rect, nv12),
            None => Err(windows::core::Error::empty()),
        }
    }

    /// The staged picture as 24-bit BGR rows: (pixels, width, height, stride).
    fn read(&self) -> Result<(Vec<u8>, u32, u32, u32)> {
        read_with(&self.gpu, &self.converter)
    }

    /// SQOL: the staged preview, as [`Self::read`].
    fn read_preview(&self) -> Result<(Vec<u8>, u32, u32, u32)> {
        read_with(&self.gpu, &self.preview_converter)
    }
}

/// SQOL: the preview's long side (its converter's).
const PREVIEW: u32 = thumb::PREVIEW_SIDE;

/// A captured frame into `slot`'s converter (made the first time, `side` px on the long side).
fn blit_with(
    gpu: &Gpu,
    slot: &Mutex<Option<Converter>>,
    side: u32,
    src: &ID3D11Texture2D,
    rect: RECT,
) -> Result<()> {
    let mut slot = slot.lock().map_err(|_| windows::core::Error::empty())?;
    if slot.is_none() {
        *slot = Some(Converter::new(gpu, side)?);
    }
    match slot.as_mut() {
        Some(converter) => converter.blit(gpu, src, rect, false),
        None => Err(windows::core::Error::empty()),
    }
}

fn read_with(gpu: &Gpu, slot: &Mutex<Option<Converter>>) -> Result<(Vec<u8>, u32, u32, u32)> {
    let slot = slot.lock().map_err(|_| windows::core::Error::empty())?;
    match slot.as_ref() {
        Some(converter) => converter.read(gpu),
        None => Err(windows::core::Error::empty()),
    }
}

/// The newest picture made: its bytes, when its frame was captured, and the capture's count then.
struct Last {
    jpg: Vec<u8>,
    at: Instant,
    frames: u64,
}

struct Worker {
    thumbs: Arc<Thumbs>,
    host: Host,
    /// When the next picture is due.
    next: Option<Instant>,
    /// A captured frame is awaited until then.
    waiting: Option<Instant>,
    /// The wait was stretched once for a frame the capture had taken.
    extended: bool,
    started: bool,
    last: Option<Last>,
    /// SQOL: the sharer's preview is on, and when to ask the capture for the next one (none while
    /// one is asked for and no frame came: a still source wakes nobody).
    preview_on: bool,
    preview_next: Option<Instant>,
}

impl Worker {
    fn run(&mut self, rx: &Receiver<Msg>) {
        loop {
            let at = [self.waiting.or(self.next), self.preview_next]
                .into_iter()
                .flatten()
                .min();
            let msg = match at {
                Some(at) => rx.recv_timeout(at.saturating_duration_since(Instant::now())),
                None => rx.recv().map_err(|_| RecvTimeoutError::Disconnected),
            };
            match msg {
                Ok(Msg::Preview(on)) => {
                    self.preview_on = on;
                    if on {
                        self.preview_next.get_or_insert_with(Instant::now);
                    } else {
                        self.preview_next = None;
                        self.thumbs.preview_wanted.store(false, Ordering::SeqCst);
                    }
                }
                Ok(Msg::PreviewReady) => {
                    if self.preview_on {
                        if let Some(jpg) = self.encode_preview() {
                            (self.host.preview)(jpg);
                        }
                        self.preview_next = Some(Instant::now() + thumb::PREVIEW_EVERY);
                    }
                }
                Ok(Msg::PreviewMissed) => {
                    if self.preview_on {
                        self.preview_next = Some(Instant::now() + thumb::PREVIEW_EVERY);
                    }
                }
                Ok(Msg::Start) => {
                    if !self.started {
                        self.started = true;
                        self.next = Some(Instant::now() + thumb::FIRST);
                    }
                }
                Ok(Msg::Renew) => {
                    // The last picture is not what shows now; a frame asked for meanwhile is not
                    // either.
                    self.last = None;
                    self.waiting = None;
                    self.extended = false;
                    self.thumbs.wanted.store(false, Ordering::SeqCst);
                    if self.started {
                        self.next = Some(Instant::now() + thumb::FIRST);
                    }
                }
                Ok(Msg::Ready { at, frames }) => {
                    if self.waiting.take().is_some() {
                        let jpg = self.encode_staged();
                        self.made(jpg, at, frames);
                    }
                }
                Ok(Msg::Missed) => {
                    if self.waiting.take().is_some() {
                        self.fallback();
                    }
                }
                Ok(Msg::Stop) | Err(RecvTimeoutError::Disconnected) => return,
                Err(RecvTimeoutError::Timeout) => self.tick(),
            }
        }
    }

    fn tick(&mut self) {
        let now = Instant::now();
        // SQOL: the next captured frame becomes the preview; the timer waits for it.
        if self.preview_on && self.preview_next.is_some_and(|at| now >= at) {
            self.preview_next = None;
            self.thumbs.preview_wanted.store(true, Ordering::SeqCst);
        }
        if let Some(until) = self.waiting {
            if now < until {
                return;
            }
            if self.thumbs.wanted.swap(false, Ordering::SeqCst) || self.extended {
                // No frame came (a still picture): the encoder's last one.
                self.waiting = None;
                self.fallback();
            } else {
                // The capture took a frame just now: its picture is on its way.
                self.extended = true;
                self.waiting = Some(now + Duration::from_millis(250));
            }
        } else if self.next.is_some_and(|next| now >= next) {
            self.next = None;
            self.due();
        }
    }

    fn due(&mut self) {
        let frames = self.thumbs.captured.load(Ordering::Relaxed);
        // Nothing new since the last picture: the same one again (the room hands out none older
        // than a few minutes).
        if let Some(last) = &self.last
            && last.frames == frames
        {
            (self.host.emit)(last.jpg.clone());
            self.next = Some(Instant::now() + thumb::EVERY);
            return;
        }
        if !(self.host.paused)()
            && let Some((texture, rect, at)) = (self.host.latest)()
        {
            let jpg = self.encode_from(&texture.0, rect);
            self.made(jpg, at, frames);
            return;
        }
        // Encoding rests (its picture is old) or has none yet: the next captured frame.
        self.thumbs.wanted.store(true, Ordering::SeqCst);
        self.waiting = Some(Instant::now() + thumb::FRAME_WAIT);
        self.extended = false;
    }

    /// No fresh frame: the encoder's newest input when it is newer than the last picture, else the
    /// last picture again.
    fn fallback(&mut self) {
        let frames = self.thumbs.captured.load(Ordering::Relaxed);
        let newer = (self.host.latest)()
            .filter(|(_, _, at)| self.last.as_ref().is_none_or(|last| *at > last.at));
        if let Some((texture, rect, at)) = newer {
            let jpg = self.encode_from(&texture.0, rect);
            self.made(jpg, at, frames);
        } else if let Some(last) = &mut self.last {
            last.frames = frames;
            (self.host.emit)(last.jpg.clone());
            self.next = Some(Instant::now() + thumb::EVERY);
        } else {
            self.next = Some(Instant::now() + thumb::RETRY);
        }
    }

    fn made(&mut self, jpg: Option<Vec<u8>>, at: Instant, frames: u64) {
        match jpg {
            Some(jpg) => {
                (self.host.emit)(jpg.clone());
                self.last = Some(Last { jpg, at, frames });
                self.next = Some(Instant::now() + thumb::EVERY);
            }
            None => match &self.last {
                // A failed picture: the last one keeps its place.
                Some(last) => {
                    (self.host.emit)(last.jpg.clone());
                    self.next = Some(Instant::now() + thumb::EVERY);
                }
                None => self.next = Some(Instant::now() + thumb::RETRY),
            },
        }
    }

    fn encode_from(&self, texture: &ID3D11Texture2D, rect: RECT) -> Option<Vec<u8>> {
        if let Err(e) = self.thumbs.blit(texture, rect, true) {
            debug!("share thumb: {e}");
            return None;
        }
        self.encode_staged()
    }

    fn encode_staged(&self) -> Option<Vec<u8>> {
        let (bgr, w, h, stride) = self
            .thumbs
            .read()
            .map_err(|e| debug!("share thumb read: {e}"))
            .ok()?;
        let jpg =
            thumb::fit(|quality| super::sources::encode_jpeg(&bgr, w, h, stride, Some(quality)));
        if jpg.is_none() {
            debug!("share thumb: no quality fits {w}x{h}");
        }
        jpg
    }

    /// SQOL: the staged preview as its JPEG.
    fn encode_preview(&self) -> Option<Vec<u8>> {
        let (bgr, w, h, stride) = self
            .thumbs
            .read_preview()
            .map_err(|e| debug!("share preview read: {e}"))
            .ok()?;
        thumb::fit_preview(|quality| super::sources::encode_jpeg(&bgr, w, h, stride, Some(quality)))
    }
}

/// The picture's video processor, its BGRA target and the staging copy, rebuilt when a size
/// changes.
struct Converter {
    video_device: ID3D11VideoDevice,
    video_context: ID3D11VideoContext,
    /// The picture's long side at most (SQOL: the still picture's or the preview's).
    side: u32,
    processor: Option<Processor>,
    target: Option<Target>,
}

// SAFETY: D3D11 video objects are free-threaded; the immediate context is serialised by
// `Gpu::lock` around every use, and the converter is used behind a Mutex.
unsafe impl Send for Converter {}

struct Processor {
    enumerator: ID3D11VideoProcessorEnumerator,
    processor: ID3D11VideoProcessor,
    input: (u32, u32),
    output: (u32, u32),
    view: Option<ID3D11VideoProcessorOutputView>,
}

struct Target {
    texture: ID3D11Texture2D,
    staging: ID3D11Texture2D,
    size: (u32, u32),
}

impl Converter {
    fn new(gpu: &Gpu, side: u32) -> Result<Self> {
        Ok(Self {
            video_device: gpu.device.cast()?,
            video_context: gpu.context.cast()?,
            side,
            processor: None,
            target: None,
        })
    }

    /// The `rect` part of `src` (NV12 from the encoder's path, or a BGRA capture) scaled into the
    /// picture and copied to the staging texture.
    fn blit(&mut self, gpu: &Gpu, src: &ID3D11Texture2D, rect: RECT, nv12: bool) -> Result<()> {
        let mut desc = D3D11_TEXTURE2D_DESC::default();
        // SAFETY: a live texture.
        unsafe { src.GetDesc(&mut desc) };
        let input = (desc.Width, desc.Height);
        let (w, h) = (desc.Width as i32, desc.Height as i32);
        let left = rect.left.clamp(0, (w - 2).max(0));
        let top = rect.top.clamp(0, (h - 2).max(0));
        let rect = RECT {
            left,
            top,
            right: rect.right.clamp(left + 2, w.max(left + 2)),
            bottom: rect.bottom.clamp(top + 2, h.max(top + 2)),
        };
        let output = thumb::size_within(
            (rect.right - rect.left) as u32,
            (rect.bottom - rect.top) as u32,
            self.side,
        );
        if self.target.as_ref().map(|t| t.size) != Some(output) {
            self.target = Some(Target::new(gpu, output)?);
            self.processor = None;
        }
        if self.processor.as_ref().map(|p| (p.input, p.output)) != Some((input, output)) {
            self.processor = Some(self.make_processor(input, output)?);
        }
        let (Some(target), Some(processor)) = (self.target.as_ref(), self.processor.as_mut())
        else {
            return Err(windows::core::Error::empty());
        };
        if processor.view.is_none() {
            let view_desc = D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC {
                ViewDimension: D3D11_VPOV_DIMENSION_TEXTURE2D,
                Anonymous: D3D11_VIDEO_PROCESSOR_OUTPUT_VIEW_DESC_0 {
                    Texture2D: D3D11_TEX2D_VPOV { MipSlice: 0 },
                },
            };
            let mut view = None;
            // SAFETY: the target and the enumerator are live.
            unsafe {
                self.video_device.CreateVideoProcessorOutputView(
                    &target.texture,
                    &processor.enumerator,
                    &view_desc,
                    Some(&mut view),
                )?;
            }
            processor.view = view;
        }
        let output_view = processor
            .view
            .clone()
            .ok_or_else(windows::core::Error::empty)?;
        let input_desc = D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC {
            FourCC: 0,
            ViewDimension: D3D11_VPIV_DIMENSION_TEXTURE2D,
            Anonymous: D3D11_VIDEO_PROCESSOR_INPUT_VIEW_DESC_0 {
                Texture2D: D3D11_TEX2D_VPIV {
                    MipSlice: 0,
                    ArraySlice: 0,
                },
            },
        };
        let mut input_view: Option<ID3D11VideoProcessorInputView> = None;
        // SAFETY: live objects.
        unsafe {
            self.video_device.CreateVideoProcessorInputView(
                src,
                &processor.enumerator,
                &input_desc,
                Some(&mut input_view),
            )?;
        }
        let dest = RECT {
            left: 0,
            top: 0,
            right: output.0 as i32,
            bottom: output.1 as i32,
        };
        let mut stream = D3D11_VIDEO_PROCESSOR_STREAM {
            Enable: BOOL(1),
            pInputSurface: ManuallyDrop::new(input_view),
            ..Default::default()
        };
        let _lock = gpu.lock();
        // SAFETY: the views belong to the processor's enumerator; `stream` is released below.
        let result = unsafe {
            let ctx = &self.video_context;
            if let Ok(ctx1) = ctx.cast::<ID3D11VideoContext1>() {
                // The encoder's input is studio-range BT.709; a capture is full-range sRGB.
                ctx1.VideoProcessorSetStreamColorSpace1(
                    &processor.processor,
                    0,
                    if nv12 {
                        DXGI_COLOR_SPACE_YCBCR_STUDIO_G22_LEFT_P709
                    } else {
                        DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709
                    },
                );
                ctx1.VideoProcessorSetOutputColorSpace1(
                    &processor.processor,
                    DXGI_COLOR_SPACE_RGB_FULL_G22_NONE_P709,
                );
            }
            ctx.VideoProcessorSetStreamSourceRect(&processor.processor, 0, true, Some(&rect));
            ctx.VideoProcessorSetStreamDestRect(&processor.processor, 0, true, Some(&dest));
            ctx.VideoProcessorSetOutputTargetRect(&processor.processor, false, None);
            ctx.VideoProcessorBlt(
                &processor.processor,
                &output_view,
                0,
                std::slice::from_ref(&stream),
            )
        };
        // SAFETY: taken once; the view's reference is released here.
        unsafe { ManuallyDrop::drop(&mut stream.pInputSurface) };
        result?;
        // SAFETY: both textures are ours, the same size and format.
        unsafe { gpu.context.CopyResource(&target.staging, &target.texture) };
        Ok(())
    }

    fn make_processor(&self, input: (u32, u32), output: (u32, u32)) -> Result<Processor> {
        let desc = D3D11_VIDEO_PROCESSOR_CONTENT_DESC {
            InputFrameFormat: D3D11_VIDEO_FRAME_FORMAT_PROGRESSIVE,
            InputFrameRate: DXGI_RATIONAL {
                Numerator: 1,
                Denominator: 1,
            },
            InputWidth: input.0,
            InputHeight: input.1,
            OutputFrameRate: DXGI_RATIONAL {
                Numerator: 1,
                Denominator: 1,
            },
            OutputWidth: output.0,
            OutputHeight: output.1,
            // One picture a minute: the better scaling is worth its cost.
            Usage: D3D11_VIDEO_USAGE_OPTIMAL_QUALITY,
        };
        // SAFETY: live video device; the descriptor outlives the calls.
        unsafe {
            let enumerator = self.video_device.CreateVideoProcessorEnumerator(&desc)?;
            let processor = self.video_device.CreateVideoProcessor(&enumerator, 0)?;
            // No denoise or edge enhancement on text.
            self.video_context
                .VideoProcessorSetStreamAutoProcessingMode(&processor, 0, false);
            Ok(Processor {
                enumerator,
                processor,
                input,
                output,
                view: None,
            })
        }
    }

    /// The staged picture as 24-bit BGR rows (WIC's JPEG input): (pixels, width, height, stride).
    fn read(&self, gpu: &Gpu) -> Result<(Vec<u8>, u32, u32, u32)> {
        let target = self
            .target
            .as_ref()
            .ok_or_else(windows::core::Error::empty)?;
        let (w, h) = (target.size.0 as usize, target.size.1 as usize);
        let stride = (w * 3).div_ceil(4) * 4;
        let mut bgr = vec![0u8; stride * h];
        let _lock = gpu.lock();
        let mut map = D3D11_MAPPED_SUBRESOURCE::default();
        // SAFETY: the staging texture is CPU-readable; rows are read within its pitch and
        // unmapped before the lock goes.
        unsafe {
            gpu.context
                .Map(&target.staging, 0, D3D11_MAP_READ, 0, Some(&mut map))?;
            let pitch = map.RowPitch as usize;
            let src = map.pData as *const u8;
            for y in 0..h {
                let row = std::slice::from_raw_parts(src.add(y * pitch), w * 4);
                let (pixels, _) = row.as_chunks::<4>();
                let (out, _) = bgr[y * stride..y * stride + w * 3].as_chunks_mut::<3>();
                for (pixel, dst) in pixels.iter().zip(out.iter_mut()) {
                    dst.copy_from_slice(&pixel[..3]);
                }
            }
            gpu.context.Unmap(&target.staging, 0);
        }
        Ok((bgr, target.size.0, target.size.1, stride as u32))
    }
}

impl Target {
    fn new(gpu: &Gpu, size: (u32, u32)) -> Result<Self> {
        let mut desc = D3D11_TEXTURE2D_DESC {
            Width: size.0,
            Height: size.1,
            MipLevels: 1,
            ArraySize: 1,
            Format: DXGI_FORMAT_B8G8R8A8_UNORM,
            SampleDesc: DXGI_SAMPLE_DESC {
                Count: 1,
                Quality: 0,
            },
            Usage: D3D11_USAGE_DEFAULT,
            BindFlags: D3D11_BIND_RENDER_TARGET.0 as u32,
            CPUAccessFlags: 0,
            MiscFlags: 0,
        };
        let mut texture = None;
        // SAFETY: valid descriptors; out-parameters valid.
        unsafe {
            gpu.device
                .CreateTexture2D(&desc, None, Some(&mut texture))?
        };
        desc.Usage = D3D11_USAGE_STAGING;
        desc.BindFlags = 0;
        desc.CPUAccessFlags = D3D11_CPU_ACCESS_READ.0 as u32;
        let mut staging = None;
        // SAFETY: as above.
        unsafe {
            gpu.device
                .CreateTexture2D(&desc, None, Some(&mut staging))?
        };
        Ok(Self {
            texture: texture.ok_or_else(windows::core::Error::empty)?,
            staging: staging.ok_or_else(windows::core::Error::empty)?,
            size,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// On this computer's GPU (`cargo test -p pl-share -- --ignored`): a captured frame and an
    /// encoder frame each become a small JPEG that fits, in the colours they had.
    #[test]
    #[ignore = "needs a GPU with a video processor"]
    fn a_frame_becomes_a_small_jpeg() {
        use windows::Win32::Graphics::Direct3D11::{
            D3D11_BIND_SHADER_RESOURCE, D3D11_SUBRESOURCE_DATA,
        };
        use windows::Win32::Graphics::Dxgi::Common::DXGI_FORMAT_NV12;
        super::super::com_mta();
        let gpu = Arc::new(Gpu::new().expect("a GPU"));
        // A texture filled with `bytes` (rows of `pitch` bytes; NV12: the chroma rows follow).
        let make = |format, w: u32, h: u32, pitch: u32, bytes: &[u8]| {
            let desc = D3D11_TEXTURE2D_DESC {
                Width: w,
                Height: h,
                MipLevels: 1,
                ArraySize: 1,
                Format: format,
                SampleDesc: DXGI_SAMPLE_DESC {
                    Count: 1,
                    Quality: 0,
                },
                Usage: D3D11_USAGE_DEFAULT,
                BindFlags: (D3D11_BIND_RENDER_TARGET.0 | D3D11_BIND_SHADER_RESOURCE.0) as u32,
                CPUAccessFlags: 0,
                MiscFlags: 0,
            };
            let data = D3D11_SUBRESOURCE_DATA {
                pSysMem: bytes.as_ptr().cast(),
                SysMemPitch: pitch,
                SysMemSlicePitch: 0,
            };
            let mut texture = None;
            // SAFETY: valid descriptor; `bytes` holds every row the texture reads.
            unsafe {
                gpu.device
                    .CreateTexture2D(&desc, Some(&data), Some(&mut texture))
            }
            .unwrap();
            texture.unwrap()
        };
        let near = |got: &[u8], want: [u8; 3]| {
            got.iter()
                .zip(want)
                .all(|(got, want)| got.abs_diff(want) <= 4)
        };
        let emitted = Arc::new(Mutex::new(Vec::<Vec<u8>>::new()));
        let sink = emitted.clone();
        let (thumbs, _handle) = Thumbs::start(
            gpu.clone(),
            1,
            Host {
                paused: Box::new(|| true),
                latest: Box::new(|| None),
                emit: Box::new(move |jpg| sink.lock().unwrap().push(jpg)),
                preview: Box::new(|_| {}),
            },
        )
        .unwrap();
        // A capture: one colour (BGRA), the picture inside a larger texture.
        let bgra: Vec<u8> = [40u8, 120, 200, 255].repeat(1920 * 1080);
        let capture = make(DXGI_FORMAT_B8G8R8A8_UNORM, 1920, 1080, 1920 * 4, &bgra);
        let rect = RECT {
            left: 0,
            top: 0,
            right: 1600,
            bottom: 1000,
        };
        thumbs.blit(&capture, rect, false).unwrap();
        let (bgr, w, h, stride) = thumbs.read().unwrap();
        assert_eq!((w, h), thumb::size(1600, 1000));
        let middle = (h as usize / 2) * stride as usize + (w as usize / 2) * 3;
        assert!(
            near(&bgr[middle..middle + 3], [40, 120, 200]),
            "{:?}",
            &bgr[middle..middle + 3]
        );
        let jpg = thumb::fit(|q| super::super::sources::encode_jpeg(&bgr, w, h, stride, Some(q)));
        let jpg = jpg.expect("a picture");
        assert!(thumb::is_jpeg(&jpg) && jpg.len() <= thumb::MAX_BYTES);
        // The encoder's input: studio-range white (Y 235, no chroma) letterboxed in black.
        let (nw, nh) = (1280usize, 720usize);
        let mut nv12 = vec![16u8; nw * nh];
        for row in nv12.chunks_exact_mut(nw) {
            row[160..1120].fill(235);
        }
        nv12.extend(std::iter::repeat_n(128u8, nw * nh / 2));
        let nv12 = make(DXGI_FORMAT_NV12, 1280, 720, 1280, &nv12);
        let letterbox = RECT {
            left: 160,
            top: 0,
            right: 1120,
            bottom: 720,
        };
        thumbs.blit(&nv12, letterbox, true).unwrap();
        let (bgr, w, h, stride) = thumbs.read().unwrap();
        assert_eq!((w, h), (320, 240));
        // Only the picture, full range: white to its edges (no black bar beside it).
        for x in [3usize, 160, 316] {
            let at = (h as usize / 2) * stride as usize + x * 3;
            assert!(
                near(&bgr[at..at + 3], [255, 255, 255]),
                "{x}: {:?}",
                &bgr[at..at + 3]
            );
        }
        // The capture path: a frame the worker waits for is taken once; a frame while the share
        // holds a blank picture is nobody's.
        thumbs.wanted.store(true, Ordering::SeqCst);
        thumbs.frame(&capture, (1920, 1080), Instant::now(), true);
        assert!(thumbs.wanted.load(Ordering::SeqCst));
        thumbs.frame(&capture, (1920, 1080), Instant::now(), false);
        assert!(!thumbs.wanted.load(Ordering::SeqCst));
        assert_eq!(thumbs.captured.load(Ordering::SeqCst), 2);
        // SQOL: the preview's frame, larger, in its own converter.
        thumbs.preview_wanted.store(true, Ordering::SeqCst);
        thumbs.frame(&capture, (1920, 1080), Instant::now(), false);
        assert!(!thumbs.preview_wanted.load(Ordering::SeqCst));
        let (bgr, w, h, stride) = thumbs.read_preview().unwrap();
        assert_eq!((w, h), thumb::size_within(1920, 1080, thumb::PREVIEW_SIDE));
        let middle = (h as usize / 2) * stride as usize + (w as usize / 2) * 3;
        assert!(near(&bgr[middle..middle + 3], [40, 120, 200]));
        // The worker of a live share: its first picture comes about 3 s after the start; the
        // preview, once asked for, with the next captured frame.
        let (tx, rx) = std::sync::mpsc::channel();
        let (preview_tx, preview_rx) = std::sync::mpsc::channel();
        let input = SendCom(nv12.clone());
        let (live, handle) = Thumbs::start(
            gpu.clone(),
            2,
            Host {
                paused: Box::new(|| false),
                latest: Box::new(move || {
                    Some((SendCom(input.0.clone()), letterbox, Instant::now()))
                }),
                emit: Box::new(move |jpg| {
                    let _ = tx.send(jpg);
                }),
                preview: Box::new(move |jpg| {
                    let _ = preview_tx.send(jpg);
                }),
            },
        )
        .unwrap();
        live.preview(true);
        std::thread::sleep(Duration::from_millis(100));
        live.frame(&capture, (1920, 1080), Instant::now(), false);
        let preview = preview_rx
            .recv_timeout(Duration::from_secs(3))
            .expect("a preview");
        assert!(thumb::is_jpeg(&preview) && preview.len() <= thumb::PREVIEW_MAX_BYTES);
        let started = Instant::now();
        live.started();
        let jpg = rx
            .recv_timeout(thumb::FIRST + Duration::from_secs(3))
            .expect("a picture after the start");
        assert!(started.elapsed() >= thumb::FIRST);
        assert!(thumb::is_jpeg(&jpg) && jpg.len() <= thumb::MAX_BYTES);
        drop(handle);
    }
}
