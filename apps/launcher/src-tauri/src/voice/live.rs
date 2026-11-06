//! SHARE-LIVE: while the player is live (a share the room took), the main window's taskbar button
//! wears the live mark ("Yayındasın" for screen readers; Windows only) and the tray's tooltip says
//! it. Both go with the last share, the stay's end, or the window itself when the launcher quits.
//! Nothing runs while nothing changes: the voice state's own changes drive it, and the main window
//! coming back to the front puts the mark on its new button again.
//!
//! The mark is LIVE-U's art (`icons/live-overlay-16.png`, set on whole pixels, and `-32.png` for
//! larger taskbars, which Windows scales): a jade disc with a thin dark rim and the broadcast mark
//! in obsidian, legible on light and dark taskbars.

use std::sync::atomic::{AtomicBool, Ordering};

use tauri::AppHandle;

/// The mark shows (or is on its way).
static LIVE: AtomicBool = AtomicBool::new(false);

/// The mark at 16 px (100 % scale) and at 32 px (anything larger).
#[cfg(windows)]
const MARK_16: &[u8] = include_bytes!("../../icons/live-overlay-16.png");
#[cfg(windows)]
const MARK_32: &[u8] = include_bytes!("../../icons/live-overlay-32.png");

/// The voice state's word (`VoiceShareView::live_now`) after every change; acts on a change only.
pub(crate) fn set(app: &AppHandle, live: bool) {
    if LIVE.swap(live, Ordering::SeqCst) == live {
        return;
    }
    tracing::info!(live, "share: the taskbar's live mark follows");
    apply(app, live);
    #[cfg(desktop)]
    crate::shell::set_tray_live(app, live);
}

/// The main window came to the front: a window back from the tray, or a restarted Explorer, has a
/// new taskbar button without the mark; while live it gets it again.
pub(crate) fn refresh(app: &AppHandle) {
    if LIVE.load(Ordering::SeqCst) {
        apply(app, true);
    }
}

/// The mark's square of RGBA pixels for this taskbar's small-icon size.
#[cfg(windows)]
fn mark() -> Option<(Vec<u8>, u32)> {
    let png = if pl_desktop::taskbar::small_icon_size() <= 16 {
        MARK_16
    } else {
        MARK_32
    };
    let image = tauri::image::Image::from_bytes(png).ok()?;
    (image.width() == image.height()).then(|| (image.rgba().to_vec(), image.width()))
}

/// Sets or clears the mark on the main window's button, on the UI thread (the window's own, in
/// its apartment).
fn apply(app: &AppHandle, live: bool) {
    #[cfg(windows)]
    {
        use tauri::Manager;
        let handle = app.clone();
        let queued = app.run_on_main_thread(move || {
            let Some(window) = handle.get_webview_window("main") else {
                return;
            };
            let Ok(hwnd) = window.hwnd() else {
                return;
            };
            // Asked again on the UI thread: a later change may have overtaken this one.
            let live = live && LIVE.load(Ordering::SeqCst);
            let badge = if live { mark() } else { None };
            let description = pl_lang::Lang::current().pick("Yayındasın", "You're live");
            if let Err(error) = pl_desktop::taskbar::set_overlay(
                hwnd.0 as isize,
                badge.as_ref().map(|(rgba, size)| (rgba.as_slice(), *size)),
                description,
            ) {
                tracing::debug!(%error, "share: the taskbar's live mark was not set");
            }
        });
        if let Err(error) = queued {
            tracing::debug!(%error, "share: the taskbar's live mark could not be queued");
        }
    }
    #[cfg(not(windows))]
    let _ = (app, live);
}
