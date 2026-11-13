//! Screen share publisher (share.md §2, §6).
//!
//! The launcher shares a game, a window or a monitor into its voice room through the Cloudflare
//! Realtime SFU. Everything heavy stays on the GPU: Windows.Graphics.Capture delivers BGRA
//! textures, the D3D11 video processor converts and scales them to NV12 per simulcast layer, and
//! the GPU vendor's Media Foundation H.264 encoder takes those textures directly. Sound comes from
//! WASAPI process loopback and is encoded to Opus. str0m (sans-IO WebRTC) sends both to the SFU
//! with simulcast (`f`, `q`), TWCC bandwidth estimation, NACK/RTX and PLI/FIR handling.
//!
//! The launcher talks to [`ShareController`] with [`ShareCmd`] and gets [`ShareEvent`]s back; the
//! room socket and the SFU's HTTP API stay with `pl-voice` and the service.

pub mod api;
pub mod controller;
pub mod h264;
pub mod keyframe;
pub mod opus;
pub mod preset;
pub mod rate;
pub mod sources;
pub mod switch;
pub mod thumb;
pub mod transport;
#[cfg(windows)]
pub mod win;

pub use api::*;
pub use controller::ShareController;

/// Spikes only (debug builds): no 1 fps video / 1 s silence keep-alive, to see the SFU's 30 s
/// rule fire.
#[doc(hidden)]
pub fn test_no_keepalive() -> bool {
    cfg!(debug_assertions) && std::env::var_os("PL_SHARE_TEST_NO_KEEPALIVE").is_some()
}

/// Hides one of our windows from every capture (the launcher while sharing, the share bar).
/// Returns false where Windows cannot (before 10 2004) or on other systems.
pub fn exclude_from_capture(hwnd: isize, exclude: bool) -> bool {
    #[cfg(windows)]
    {
        win::exclude_from_capture(hwnd, exclude)
    }
    #[cfg(not(windows))]
    {
        let _ = (hwnd, exclude);
        false
    }
}
