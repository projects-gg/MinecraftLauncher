//! The Windows backend: Direct3D 11, Windows.Graphics.Capture, Media Foundation, WASAPI.

pub mod audio;
pub mod capture;
pub mod convert;
pub mod d3d;
pub mod encoder;
pub mod minimize;
pub mod session;
pub mod sources;
pub mod thumb;

use std::sync::OnceLock;

use windows::Win32::Foundation::HWND;
use windows::Win32::System::Com::{COINIT_MULTITHREADED, CoInitializeEx};
use windows::Win32::System::Registry::{HKEY_LOCAL_MACHINE, RRF_RT_REG_SZ, RegGetValueW};
use windows::Win32::UI::WindowsAndMessaging::{
    SetWindowDisplayAffinity, WDA_EXCLUDEFROMCAPTURE, WDA_NONE,
};
use windows::core::w;

/// Joins this thread to the multithreaded apartment (COM and WinRT). Idempotent per thread.
pub fn com_mta() {
    // SAFETY: plain initialisation; S_FALSE (already initialised) is fine, RPC_E_CHANGED_MODE
    // means the thread is STA, where our free-threaded objects still work.
    let _ = unsafe { CoInitializeEx(None, COINIT_MULTITHREADED) };
}

/// The Windows build number (e.g. 26100), 0 when unknown.
pub fn os_build() -> u32 {
    static BUILD: OnceLock<u32> = OnceLock::new();
    *BUILD.get_or_init(|| {
        let mut buf = [0u16; 32];
        let mut size = (buf.len() * 2) as u32;
        // SAFETY: the buffer and its size in bytes are passed together.
        let result = unsafe {
            RegGetValueW(
                HKEY_LOCAL_MACHINE,
                w!(r"SOFTWARE\Microsoft\Windows NT\CurrentVersion"),
                w!("CurrentBuildNumber"),
                RRF_RT_REG_SZ,
                None,
                Some(buf.as_mut_ptr().cast()),
                Some(&mut size),
            )
        };
        if result.is_err() {
            return 0;
        }
        let len = buf.iter().position(|&c| c == 0).unwrap_or(buf.len());
        String::from_utf16_lossy(&buf[..len])
            .trim()
            .parse()
            .unwrap_or(0)
    })
}

/// Process loopback capture (INCLUDE/EXCLUDE a process tree) exists from build 20348.
pub fn process_audio_supported() -> bool {
    os_build() >= 20348
}

/// Hides (or shows again) one of our top-level windows from every capture API
/// (`WDA_EXCLUDEFROMCAPTURE`, Windows 10 2004+; older systems show a black box instead, so it is
/// only applied from build 19041).
pub fn exclude_from_capture(hwnd: isize, exclude: bool) -> bool {
    if exclude && os_build() < 19041 {
        return false;
    }
    let affinity = if exclude {
        WDA_EXCLUDEFROMCAPTURE
    } else {
        WDA_NONE
    };
    // SAFETY: a window handle owned by the caller's process.
    unsafe { SetWindowDisplayAffinity(HWND(hwnd as *mut _), affinity) }.is_ok()
}

/// Wraps a COM pointer that is used from another thread. Direct3D 11 devices and resources are
/// free-threaded (the immediate context is serialised with `ID3D11Multithread`); Media Foundation
/// samples are too. The `windows` crate cannot know that, so it marks them `!Send`.
pub struct SendCom<T>(pub T);

// SAFETY: see the type's documentation; only used for free-threaded interfaces.
unsafe impl<T> Send for SendCom<T> {}
// SAFETY: as above.
unsafe impl<T> Sync for SendCom<T> {}
