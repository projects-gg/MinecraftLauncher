//! The picker's sources: top-level windows, monitors and the game, with small JPEG previews.
//! Previews are made with GDI (`PrintWindow` with full-content rendering for windows, a scaled
//! `BitBlt` for monitors) and WIC; nothing is stored, they go to the picker only.

use std::collections::{HashMap, HashSet};

use base64::Engine;
use windows::Win32::Foundation::{CloseHandle, HWND, LPARAM, RECT};
use windows::Win32::Graphics::Dwm::{
    DWMWA_CLOAKED, DWMWA_EXTENDED_FRAME_BOUNDS, DwmGetWindowAttribute,
};
use windows::Win32::Graphics::Gdi::{
    BI_RGB, BITMAPINFO, BITMAPINFOHEADER, CreateCompatibleBitmap, CreateCompatibleDC,
    CreateDIBSection, DIB_RGB_COLORS, DeleteDC, DeleteObject, EnumDisplayMonitors, GetDC,
    GetMonitorInfoW, HALFTONE, HBITMAP, HDC, HMONITOR, MONITORINFO, MONITORINFOEXW, ReleaseDC,
    SRCCOPY, SelectObject, SetStretchBltMode, StretchBlt,
};
use windows::Win32::Graphics::Imaging::{
    CLSID_WICImagingFactory, GUID_ContainerFormatJpeg, GUID_WICPixelFormat24bppBGR,
    IWICImagingFactory, WICBitmapEncoderNoCache,
};
use windows::Win32::Storage::Xps::{PRINT_WINDOW_FLAGS, PrintWindow};
use windows::Win32::System::Com::StructuredStorage::{
    CreateStreamOnHGlobal, IPropertyBag2, PROPBAG2,
};
use windows::Win32::System::Com::{
    CLSCTX_INPROC_SERVER, CoCreateInstance, STATFLAG_NONAME, STREAM_SEEK_SET,
};
use windows::Win32::System::Threading::{
    OpenProcess, PROCESS_NAME_WIN32, PROCESS_QUERY_LIMITED_INFORMATION, QueryFullProcessImageNameW,
};
use windows::Win32::System::Variant::VARIANT;
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GW_OWNER, GWL_EXSTYLE, GetClassNameW, GetWindow, GetWindowLongPtrW,
    GetWindowTextW, GetWindowThreadProcessId, IsIconic, IsWindow, IsWindowVisible,
    MONITORINFOF_PRIMARY, WS_EX_APPWINDOW, WS_EX_TOOLWINDOW,
};
use windows::core::{BOOL, PWSTR, w};

use crate::api::{ShareCaps, SourceId, SourceInfo, SourceKind};
use crate::sources::{WindowFacts, game_window, keep_window, order};

/// Preview width; the height follows the aspect ratio (at most 240 x 180).
const THUMB_W: i32 = 240;
const THUMB_MAX_H: i32 = 180;
/// `PW_RENDERFULLCONTENT`: DirectX and OpenGL windows draw into the preview too.
const PW_RENDERFULLCONTENT: u32 = 2;

unsafe extern "system" fn collect(hwnd: HWND, lparam: LPARAM) -> BOOL {
    // SAFETY: `lparam` is the Vec passed by `top_level_windows` for the duration of EnumWindows.
    let list = unsafe { &mut *(lparam.0 as *mut Vec<HWND>) };
    list.push(hwnd);
    BOOL(1)
}

fn top_level_windows() -> Vec<HWND> {
    let mut list: Vec<HWND> = Vec::new();
    // SAFETY: the callback only pushes into `list`, which outlives the call.
    unsafe {
        let _ = EnumWindows(Some(collect), LPARAM(&mut list as *mut Vec<HWND> as isize));
    }
    list
}

fn wide_to_string(buf: &[u16]) -> String {
    let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
    String::from_utf16_lossy(&buf[..len])
}

/// The program's file name without `.exe`.
fn app_name(pid: u32, cache: &mut HashMap<u32, String>) -> String {
    cache
        .entry(pid)
        .or_insert_with(|| {
            // SAFETY: query-only handle, closed below.
            unsafe {
                let Ok(process) = OpenProcess(PROCESS_QUERY_LIMITED_INFORMATION, false, pid) else {
                    return String::new();
                };
                let mut buf = [0u16; 520];
                let mut len = buf.len() as u32;
                let ok = QueryFullProcessImageNameW(
                    process,
                    PROCESS_NAME_WIN32,
                    PWSTR(buf.as_mut_ptr()),
                    &mut len,
                )
                .is_ok();
                let _ = CloseHandle(process);
                if !ok {
                    return String::new();
                }
                let path = String::from_utf16_lossy(&buf[..len as usize]);
                let file = path.rsplit(['\\', '/']).next().unwrap_or("");
                file.strip_suffix(".exe")
                    .or_else(|| file.strip_suffix(".EXE"))
                    .unwrap_or(file)
                    .to_string()
            }
        })
        .clone()
}

pub fn window_facts(hwnd: HWND, cache: &mut HashMap<u32, String>) -> WindowFacts {
    // SAFETY: read-only queries on a window handle (a stale one yields empty values).
    unsafe {
        let mut pid = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut pid));
        let mut title = [0u16; 256];
        let n = GetWindowTextW(hwnd, &mut title);
        let mut class = [0u16; 128];
        let c = GetClassNameW(hwnd, &mut class);
        let mut cloaked = 0u32;
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_CLOAKED,
            (&mut cloaked as *mut u32).cast(),
            std::mem::size_of::<u32>() as u32,
        );
        let mut rect = RECT::default();
        let _ = DwmGetWindowAttribute(
            hwnd,
            DWMWA_EXTENDED_FRAME_BOUNDS,
            (&mut rect as *mut RECT).cast(),
            std::mem::size_of::<RECT>() as u32,
        );
        let ex = GetWindowLongPtrW(hwnd, GWL_EXSTYLE) as u32;
        let owned = GetWindow(hwnd, GW_OWNER).is_ok_and(|o| !o.is_invalid());
        WindowFacts {
            hwnd: hwnd.0 as usize as u32,
            pid,
            title: String::from_utf16_lossy(&title[..n.max(0) as usize]),
            class: String::from_utf16_lossy(&class[..c.max(0) as usize]),
            app: app_name(pid, cache),
            visible: IsWindowVisible(hwnd).as_bool(),
            cloaked: cloaked != 0,
            tool_window: ex & WS_EX_TOOLWINDOW.0 != 0,
            owned,
            app_window: ex & WS_EX_APPWINDOW.0 != 0,
            minimized: IsIconic(hwnd).as_bool(),
            w: rect.right - rect.left,
            h: rect.bottom - rect.top,
        }
    }
}

pub struct MonitorFacts {
    pub handle: isize,
    pub device: String,
    pub rect: RECT,
    pub primary: bool,
}

unsafe extern "system" fn collect_monitor(
    monitor: HMONITOR,
    _: HDC,
    _: *mut RECT,
    lparam: LPARAM,
) -> BOOL {
    // SAFETY: `lparam` is the Vec passed by `monitors`.
    let list = unsafe { &mut *(lparam.0 as *mut Vec<HMONITOR>) };
    list.push(monitor);
    BOOL(1)
}

/// Monitors in display order (left to right, then top to bottom).
pub fn monitors() -> Vec<MonitorFacts> {
    let mut handles: Vec<HMONITOR> = Vec::new();
    // SAFETY: the callback only pushes into `handles`.
    unsafe {
        let _ = EnumDisplayMonitors(
            None,
            None,
            Some(collect_monitor),
            LPARAM(&mut handles as *mut _ as isize),
        );
    }
    let mut out: Vec<MonitorFacts> = handles
        .into_iter()
        .filter_map(|m| {
            let mut info = MONITORINFOEXW::default();
            info.monitorInfo.cbSize = std::mem::size_of::<MONITORINFOEXW>() as u32;
            // SAFETY: cbSize set for the extended structure.
            unsafe { GetMonitorInfoW(m, &mut info as *mut MONITORINFOEXW as *mut MONITORINFO) }
                .as_bool()
                .then(|| MonitorFacts {
                    handle: m.0 as isize,
                    device: wide_to_string(&info.szDevice),
                    rect: info.monitorInfo.rcMonitor,
                    primary: info.monitorInfo.dwFlags & MONITORINFOF_PRIMARY != 0,
                })
        })
        .collect();
    out.sort_by_key(|m| (m.rect.left, m.rect.top));
    out
}

pub fn monitor_by_device(device: &str) -> Option<MonitorFacts> {
    monitors().into_iter().find(|m| m.device == device)
}

/// A window handle that still exists and still belongs to `pid`.
pub fn window_alive(hwnd: u32, pid: u32) -> bool {
    let hwnd = HWND(hwnd as usize as *mut _);
    // SAFETY: read-only queries.
    unsafe {
        if !IsWindow(Some(hwnd)).as_bool() {
            return false;
        }
        let mut owner = 0u32;
        GetWindowThreadProcessId(hwnd, Some(&mut owner));
        owner == pid
    }
}

/// The game's window now (Minecraft may recreate it).
pub fn find_game_window(pid: u32) -> Option<WindowFacts> {
    let mut cache = HashMap::new();
    let facts: Vec<WindowFacts> = top_level_windows()
        .into_iter()
        .map(|h| window_facts(h, &mut cache))
        .filter(|f| f.pid == pid)
        .collect();
    game_window(&facts, pid).cloned()
}

/// Everything the picker lists.
pub fn list(game_pid: Option<u32>, own: &HashSet<u32>, thumbs: bool) -> Vec<SourceInfo> {
    let mut cache = HashMap::new();
    let facts: Vec<WindowFacts> = top_level_windows()
        .into_iter()
        .map(|h| window_facts(h, &mut cache))
        .collect();
    let game = game_pid
        .and_then(|pid| game_window(&facts, pid))
        .map(|g| g.hwnd);
    let mut out = Vec::new();
    for f in &facts {
        let is_game = Some(f.hwnd) == game;
        if !is_game && !keep_window(f, own) {
            continue;
        }
        let thumb = (thumbs && !f.minimized)
            .then(|| window_thumb(HWND(f.hwnd as usize as *mut _), f.w, f.h))
            .flatten();
        out.push(SourceInfo {
            id: if is_game {
                SourceId::Game { pid: f.pid }
            } else {
                SourceId::Window {
                    hwnd: f.hwnd,
                    pid: f.pid,
                }
            },
            kind: if is_game {
                SourceKind::Game
            } else {
                SourceKind::Window
            },
            title: f.title.clone(),
            app: f.app.clone(),
            index: 0,
            primary: false,
            w: f.w.max(0) as u32,
            h: f.h.max(0) as u32,
            minimized: f.minimized,
            thumb,
        });
    }
    for (i, m) in monitors().into_iter().enumerate() {
        let (w, h) = (m.rect.right - m.rect.left, m.rect.bottom - m.rect.top);
        out.push(SourceInfo {
            id: SourceId::Monitor {
                device: m.device.clone(),
            },
            kind: SourceKind::Monitor,
            title: String::new(),
            app: String::new(),
            index: (i + 1).min(255) as u8,
            primary: m.primary,
            w: w.max(0) as u32,
            h: h.max(0) as u32,
            minimized: false,
            thumb: thumbs.then(|| monitor_thumb(&m.rect)).flatten(),
        });
    }
    order(&mut out);
    out
}

pub fn caps(hardware_encoder: Option<String>) -> ShareCaps {
    ShareCaps {
        audio: super::process_audio_supported(),
        border_off: super::capture::border_toggle_supported(),
        cursor_toggle: super::capture::cursor_toggle_supported(),
        hardware_encoder,
    }
}

fn thumb_size(w: i32, h: i32) -> (i32, i32) {
    let tw = THUMB_W;
    let th = ((h as f64 * tw as f64 / w.max(1) as f64).round() as i32).clamp(16, THUMB_MAX_H);
    (tw, th)
}

/// A 32-bit top-down DIB section of `w` x `h` selected into a new memory DC.
struct Dib {
    dc: HDC,
    bitmap: HBITMAP,
    bits: *mut u8,
    w: i32,
    h: i32,
}

impl Dib {
    fn new(reference: HDC, w: i32, h: i32) -> Option<Self> {
        let info = BITMAPINFO {
            bmiHeader: BITMAPINFOHEADER {
                biSize: std::mem::size_of::<BITMAPINFOHEADER>() as u32,
                biWidth: w,
                biHeight: -h,
                biPlanes: 1,
                biBitCount: 32,
                biCompression: BI_RGB.0,
                ..Default::default()
            },
            ..Default::default()
        };
        let mut bits = std::ptr::null_mut();
        // SAFETY: GDI objects are deleted in Drop.
        unsafe {
            let dc = CreateCompatibleDC(Some(reference));
            let bitmap =
                CreateDIBSection(Some(dc), &info, DIB_RGB_COLORS, &mut bits, None, 0).ok()?;
            SelectObject(dc, bitmap.into());
            Some(Self {
                dc,
                bitmap,
                bits: bits.cast(),
                w,
                h,
            })
        }
    }

    /// The pixels as JPEG (24-bit BGR, WIC's default quality).
    fn jpeg(&self) -> Option<Vec<u8>> {
        let (w, h) = (self.w as usize, self.h as usize);
        // SAFETY: the DIB holds w*h BGRA pixels.
        let src = unsafe { std::slice::from_raw_parts(self.bits, w * h * 4) };
        let stride = (w * 3).div_ceil(4) * 4;
        let mut bgr = vec![0u8; stride * h];
        for y in 0..h {
            for x in 0..w {
                let s = (y * w + x) * 4;
                let d = y * stride + x * 3;
                bgr[d..d + 3].copy_from_slice(&src[s..s + 3]);
            }
        }
        encode_jpeg(&bgr, w as u32, h as u32, stride as u32, None)
    }
}

impl Drop for Dib {
    fn drop(&mut self) {
        // SAFETY: created in `new`.
        unsafe {
            let _ = DeleteObject(self.bitmap.into());
            let _ = DeleteDC(self.dc);
        }
    }
}

/// 24-bit BGR rows as a JPEG in memory; `quality` 0–1 (none: WIC's default). Nothing touches the
/// disk.
pub(super) fn encode_jpeg(
    bgr: &[u8],
    w: u32,
    h: u32,
    stride: u32,
    quality: Option<f32>,
) -> Option<Vec<u8>> {
    // SAFETY: WIC calls on live objects; the property's name and value outlive the write; the
    // stream is read back with its reported size.
    unsafe {
        let factory: IWICImagingFactory =
            CoCreateInstance(&CLSID_WICImagingFactory, None, CLSCTX_INPROC_SERVER).ok()?;
        let stream = CreateStreamOnHGlobal(Default::default(), true).ok()?;
        let encoder = factory
            .CreateEncoder(&GUID_ContainerFormatJpeg, std::ptr::null())
            .ok()?;
        encoder.Initialize(&stream, WICBitmapEncoderNoCache).ok()?;
        let mut frame = None;
        let mut options: Option<IPropertyBag2> = None;
        encoder.CreateNewFrame(&mut frame, &mut options).ok()?;
        let frame = frame?;
        if let (Some(quality), Some(options)) = (quality, options.as_ref()) {
            let name = PROPBAG2 {
                pstrName: PWSTR(w!("ImageQuality").0.cast_mut()),
                ..Default::default()
            };
            options
                .Write(1, &name, &VARIANT::from(quality.clamp(0.0, 1.0)))
                .ok()?;
        }
        frame.Initialize(options.as_ref()).ok()?;
        frame.SetSize(w, h).ok()?;
        let mut format = GUID_WICPixelFormat24bppBGR;
        frame.SetPixelFormat(&mut format).ok()?;
        frame.WritePixels(h, stride, bgr).ok()?;
        frame.Commit().ok()?;
        encoder.Commit().ok()?;
        let mut stat = Default::default();
        stream.Stat(&mut stat, STATFLAG_NONAME).ok()?;
        let size = stat.cbSize as usize;
        stream.Seek(0, STREAM_SEEK_SET, None).ok()?;
        let mut out = vec![0u8; size];
        let mut read = 0u32;
        stream
            .Read(out.as_mut_ptr().cast(), size as u32, Some(&mut read))
            .ok()
            .ok()?;
        out.truncate(read as usize);
        Some(out)
    }
}

fn base64(jpeg: Vec<u8>) -> String {
    base64::engine::general_purpose::STANDARD.encode(jpeg)
}

/// A window's preview. `w`/`h` are its frame size.
pub fn window_thumb(hwnd: HWND, w: i32, h: i32) -> Option<String> {
    if w <= 0 || h <= 0 {
        return None;
    }
    let (tw, th) = thumb_size(w, h);
    // SAFETY: GDI objects are released on every path (Dib's Drop, explicit deletes below).
    unsafe {
        let screen = GetDC(None);
        let result = {
            let full_dc = CreateCompatibleDC(Some(screen));
            let full = CreateCompatibleBitmap(screen, w, h);
            let old = SelectObject(full_dc, full.into());
            let printed =
                PrintWindow(hwnd, full_dc, PRINT_WINDOW_FLAGS(PW_RENDERFULLCONTENT)).as_bool();
            let small = Dib::new(screen, tw, th);
            if let Some(small) = &small
                && printed
            {
                SetStretchBltMode(small.dc, HALFTONE);
                let _ = StretchBlt(small.dc, 0, 0, tw, th, Some(full_dc), 0, 0, w, h, SRCCOPY);
            }
            SelectObject(full_dc, old);
            let _ = DeleteObject(full.into());
            let _ = DeleteDC(full_dc);
            if printed {
                small.and_then(|s| s.jpeg())
            } else {
                None
            }
        };
        ReleaseDC(None, screen);
        result.map(base64)
    }
}

/// A monitor's preview.
pub fn monitor_thumb(rect: &RECT) -> Option<String> {
    let (w, h) = (rect.right - rect.left, rect.bottom - rect.top);
    if w <= 0 || h <= 0 {
        return None;
    }
    let (tw, th) = thumb_size(w, h);
    // SAFETY: as above.
    unsafe {
        let screen = GetDC(None);
        let result = Dib::new(screen, tw, th).and_then(|small| {
            SetStretchBltMode(small.dc, HALFTONE);
            StretchBlt(
                small.dc,
                0,
                0,
                tw,
                th,
                Some(screen),
                rect.left,
                rect.top,
                w,
                h,
                SRCCOPY,
            )
            .as_bool()
            .then(|| small.jpeg())
            .flatten()
        });
        ReleaseDC(None, screen);
        result.map(base64)
    }
}
