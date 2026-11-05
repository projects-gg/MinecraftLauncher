//! Spike helpers: a test window of our own with moving content (never the owner's screen).

#![allow(dead_code)]

use std::sync::atomic::{AtomicU32, Ordering};
use std::sync::mpsc::sync_channel;

use windows::Win32::Foundation::{COLORREF, HWND, LPARAM, LRESULT, RECT, WPARAM};
use windows::Win32::Graphics::Gdi::{
    BeginPaint, CreateSolidBrush, DeleteObject, EndPaint, FillRect, InvalidateRect, PAINTSTRUCT,
    SetBkMode, SetTextColor, TRANSPARENT, TextOutW,
};
use windows::Win32::System::LibraryLoader::GetModuleHandleW;
use windows::Win32::UI::WindowsAndMessaging::{
    AdjustWindowRectEx, CreateWindowExW, DefWindowProcW, DispatchMessageW, GetClientRect,
    GetMessageW, HWND_BOTTOM, MSG, PostQuitMessage, RegisterClassW, SW_SHOWNOACTIVATE,
    SWP_NOACTIVATE, SWP_NOMOVE, SWP_NOSIZE, SetTimer, SetWindowPos, ShowWindow, TranslateMessage,
    WM_DESTROY, WM_PAINT, WM_TIMER, WNDCLASSW, WS_EX_NOACTIVATE, WS_OVERLAPPEDWINDOW,
};
use windows::core::{PCWSTR, w};

static FRAME: AtomicU32 = AtomicU32::new(0);

extern "system" fn wndproc(hwnd: HWND, msg: u32, wparam: WPARAM, lparam: LPARAM) -> LRESULT {
    unsafe {
        match msg {
            WM_TIMER => {
                FRAME.fetch_add(1, Ordering::Relaxed);
                let _ = InvalidateRect(Some(hwnd), None, false);
                LRESULT(0)
            }
            WM_PAINT => {
                let mut ps = PAINTSTRUCT::default();
                let dc = BeginPaint(hwnd, &mut ps);
                let mut rc = RECT::default();
                let _ = GetClientRect(hwnd, &mut rc);
                let f = FRAME.load(Ordering::Relaxed);
                let bg = CreateSolidBrush(COLORREF(0x00302020 + ((f % 32) << 1)));
                FillRect(dc, &rc, bg);
                let _ = DeleteObject(bg.into());
                // Moving bars: real motion for the encoder.
                for i in 0..12u32 {
                    let x = ((f * (3 + i) + i * 97) % (rc.right.max(1) as u32)) as i32;
                    let color = COLORREF(0x0000_00ff_u32.rotate_left((i % 3) * 8) | 0x40_40_40);
                    let brush = CreateSolidBrush(color);
                    let bar = RECT {
                        left: x,
                        top: (i as i32) * rc.bottom / 12,
                        right: x + 80,
                        bottom: (i as i32 + 1) * rc.bottom / 12,
                    };
                    FillRect(dc, &bar, brush);
                    let _ = DeleteObject(brush.into());
                }
                SetBkMode(dc, TRANSPARENT);
                SetTextColor(dc, COLORREF(0x00ffffff));
                let text: Vec<u16> = format!("pl-share test frame {f}").encode_utf16().collect();
                let _ = TextOutW(dc, 40, 40, &text);
                let _ = EndPaint(hwnd, &ps);
                LRESULT(0)
            }
            WM_DESTROY => {
                PostQuitMessage(0);
                LRESULT(0)
            }
            _ => DefWindowProcW(hwnd, msg, wparam, lparam),
        }
    }
}

/// Opens the test window (client `w` x `h`, repainting every ~16 ms) on its own thread, behind
/// other windows and without taking focus. Returns its handle.
pub fn test_window(w: i32, h: i32) -> isize {
    let (tx, rx) = sync_channel(1);
    std::thread::spawn(move || unsafe {
        let instance = GetModuleHandleW(None).unwrap();
        let class = w!("PlShareSpikeWindow");
        let wc = WNDCLASSW {
            lpfnWndProc: Some(wndproc),
            hInstance: instance.into(),
            lpszClassName: class,
            ..Default::default()
        };
        RegisterClassW(&wc);
        let mut rect = RECT {
            left: 0,
            top: 0,
            right: w,
            bottom: h,
        };
        let _ = AdjustWindowRectEx(&mut rect, WS_OVERLAPPEDWINDOW, false, WS_EX_NOACTIVATE);
        let hwnd = CreateWindowExW(
            WS_EX_NOACTIVATE,
            class,
            w!("pl-share spike"),
            WS_OVERLAPPEDWINDOW,
            40,
            40,
            rect.right - rect.left,
            rect.bottom - rect.top,
            None,
            None,
            Some(instance.into()),
            None,
        )
        .unwrap();
        let _ = ShowWindow(hwnd, SW_SHOWNOACTIVATE);
        let _ = SetWindowPos(
            hwnd,
            Some(HWND_BOTTOM),
            0,
            0,
            0,
            0,
            SWP_NOMOVE | SWP_NOSIZE | SWP_NOACTIVATE,
        );
        SetTimer(Some(hwnd), 1, 16, None);
        tx.send(hwnd.0 as isize).unwrap();
        let mut msg = MSG::default();
        while GetMessageW(&mut msg, None, 0, 0).as_bool() {
            let _ = TranslateMessage(&msg);
            DispatchMessageW(&msg);
        }
        let _ = PCWSTR::null();
    });
    rx.recv().unwrap()
}

/// CPU time (user + kernel) of this process, in seconds.
pub fn cpu_seconds() -> f64 {
    use windows::Win32::Foundation::FILETIME;
    use windows::Win32::System::Threading::{GetCurrentProcess, GetProcessTimes};
    let (mut c, mut e, mut k, mut u) = (
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
        FILETIME::default(),
    );
    unsafe {
        let _ = GetProcessTimes(GetCurrentProcess(), &mut c, &mut e, &mut k, &mut u);
    }
    let t = |f: FILETIME| {
        ((u64::from(f.dwHighDateTime) << 32) | u64::from(f.dwLowDateTime)) as f64 / 1e7
    };
    t(k) + t(u)
}

/// Logical processors (CPU % is reported against all of them, like Task Manager).
pub fn cpus() -> f64 {
    std::thread::available_parallelism()
        .map(|n| n.get() as f64)
        .unwrap_or(1.0)
}
