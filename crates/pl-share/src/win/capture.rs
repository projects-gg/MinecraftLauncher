//! Windows.Graphics.Capture: a free-threaded frame pool, so frames arrive on a system thread
//! without a message loop. The compositor delivers a frame only when the content changed: a still
//! screen costs nothing (the encoders' 1 fps keep-alive covers the SFU's 30 s rule).

use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicI32, Ordering};
use std::time::Instant;

use windows::Foundation::Metadata::ApiInformation;
use windows::Foundation::TypedEventHandler;
use windows::Graphics::Capture::{
    Direct3D11CaptureFramePool, GraphicsCaptureItem, GraphicsCaptureSession,
};
use windows::Graphics::DirectX::DirectXPixelFormat;
use windows::Graphics::SizeInt32;
use windows::Win32::Foundation::HWND;
use windows::Win32::Graphics::Direct3D11::ID3D11Texture2D;
use windows::Win32::Graphics::Gdi::HMONITOR;
use windows::Win32::System::WinRT::Direct3D11::IDirect3DDxgiInterfaceAccess;
use windows::Win32::System::WinRT::Graphics::Capture::IGraphicsCaptureItemInterop;
use windows::core::{HSTRING, IInspectable, Interface, Result};

use super::d3d::Gpu;

const SESSION: &str = "Windows.Graphics.Capture.GraphicsCaptureSession";

fn has_property(name: &str) -> bool {
    ApiInformation::IsPropertyPresent(&HSTRING::from(SESSION), &HSTRING::from(name))
        .unwrap_or(false)
}

pub fn supported() -> bool {
    GraphicsCaptureSession::IsSupported().unwrap_or(false)
}

/// The cursor can be left out (Windows 10 2004+).
pub fn cursor_toggle_supported() -> bool {
    has_property("IsCursorCaptureEnabled")
}

/// The yellow border can be turned off (Windows 11).
pub fn border_toggle_supported() -> bool {
    has_property("IsBorderRequired")
}

pub fn item_for_window(hwnd: isize) -> Result<GraphicsCaptureItem> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    // SAFETY: the handle is checked by the caller; a stale one fails here.
    unsafe { interop.CreateForWindow(HWND(hwnd as *mut _)) }
}

pub fn item_for_monitor(monitor: isize) -> Result<GraphicsCaptureItem> {
    let interop = windows::core::factory::<GraphicsCaptureItem, IGraphicsCaptureItemInterop>()?;
    // SAFETY: as above.
    unsafe { interop.CreateForMonitor(HMONITOR(monitor as *mut _)) }
}

pub fn item_size(item: &GraphicsCaptureItem) -> (u32, u32) {
    item.Size()
        .map(|s| (s.Width.max(1) as u32, s.Height.max(1) as u32))
        .unwrap_or((1280, 720))
}

/// One running capture. Dropping it stops the capture.
pub struct Capture {
    item: GraphicsCaptureItem,
    pool: Direct3D11CaptureFramePool,
    session: GraphicsCaptureSession,
    frame_token: i64,
    closed_token: i64,
    halt: Arc<AtomicBool>,
}

// SAFETY: the WinRT capture objects are agile (free-threaded).
unsafe impl Send for Capture {}

/// What the frame callback receives: the frame's texture (valid during the call), the content
/// size inside it, and the arrival instant.
pub type OnFrame = Box<dyn FnMut(&ID3D11Texture2D, (u32, u32), Instant) + Send>;

impl Capture {
    pub fn start(
        gpu: &Gpu,
        item: GraphicsCaptureItem,
        cursor: bool,
        on_frame: OnFrame,
        on_closed: Box<dyn FnMut() + Send>,
    ) -> Result<Self> {
        let format = DirectXPixelFormat::B8G8R8A8UIntNormalized;
        let size = item.Size()?;
        // Two buffers: one being read by our conversion while the compositor fills the other.
        let pool = Direct3D11CaptureFramePool::CreateFreeThreaded(&gpu.winrt, format, 2, size)?;
        let session = pool.CreateCaptureSession(&item)?;
        let halt = Arc::new(AtomicBool::new(false));

        let closed_token = item.Closed(
            &TypedEventHandler::<GraphicsCaptureItem, IInspectable>::new({
                let halt = halt.clone();
                let on_closed = std::sync::Mutex::new(on_closed);
                move |_, _| {
                    if !halt.swap(true, Ordering::SeqCst)
                        && let Ok(mut f) = on_closed.lock()
                    {
                        f();
                    }
                    Ok(())
                }
            }),
        )?;

        let frame_token = pool.FrameArrived(&TypedEventHandler::<
            Direct3D11CaptureFramePool,
            IInspectable,
        >::new({
            let halt = halt.clone();
            let device = super::SendCom(gpu.winrt.clone());
            let last = Arc::new((AtomicI32::new(size.Width), AtomicI32::new(size.Height)));
            let on_frame = std::sync::Mutex::new(on_frame);
            move |pool, _| {
                let Some(pool) = pool.as_ref() else {
                    return Ok(());
                };
                let frame = pool.TryGetNextFrame()?;
                if halt.load(Ordering::Relaxed) {
                    return Ok(());
                }
                let now = Instant::now();
                let content = frame.ContentSize()?;
                // The window grew: the pool's buffers must follow, or the picture is cropped.
                if content.Width > last.0.load(Ordering::Relaxed)
                    || content.Height > last.1.load(Ordering::Relaxed)
                    || content.Width * 2 < last.0.load(Ordering::Relaxed)
                    || content.Height * 2 < last.1.load(Ordering::Relaxed)
                {
                    drop(frame);
                    let next = SizeInt32 {
                        Width: content.Width.max(1),
                        Height: content.Height.max(1),
                    };
                    // Capture the whole wrapper (not its field) so the closure stays Send.
                    let device: &super::SendCom<_> = &device;
                    pool.Recreate(&device.0, format, 2, next)?;
                    last.0.store(next.Width, Ordering::Relaxed);
                    last.1.store(next.Height, Ordering::Relaxed);
                    return Ok(());
                }
                let access: IDirect3DDxgiInterfaceAccess = frame.Surface()?.cast()?;
                // SAFETY: the surface is a D3D11 texture on our device.
                let texture: ID3D11Texture2D = unsafe { access.GetInterface()? };
                if let Ok(mut on_frame) = on_frame.lock() {
                    on_frame(
                        &texture,
                        (content.Width.max(1) as u32, content.Height.max(1) as u32),
                        now,
                    );
                }
                Ok(())
            }
        }))?;

        if cursor_toggle_supported() {
            let _ = session.SetIsCursorCaptureEnabled(cursor);
        }
        if border_toggle_supported() {
            // Unpackaged apps get no prompt; when Windows refuses, the border just stays.
            let _ = session.SetIsBorderRequired(false);
        }
        session.StartCapture()?;
        Ok(Self {
            item,
            pool,
            session,
            frame_token,
            closed_token,
            halt,
        })
    }

    pub fn set_cursor(&self, on: bool) {
        if cursor_toggle_supported() {
            let _ = self.session.SetIsCursorCaptureEnabled(on);
        }
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        self.halt.store(true, Ordering::SeqCst);
        let _ = self.pool.RemoveFrameArrived(self.frame_token);
        let _ = self.item.RemoveClosed(self.closed_token);
        let _ = self.session.Close();
        let _ = self.pool.Close();
    }
}
