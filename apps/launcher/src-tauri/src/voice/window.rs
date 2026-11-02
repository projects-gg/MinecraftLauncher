//! The hidden voice window (design §6.1; VOICE-0's measured rules): never shown, never in the
//! taskbar, outside the page's rest; its page counts as visible so Chromium slows nothing down
//! (it draws nothing, so frames cost nothing). Only it gets the microphone, and only for the
//! launcher's own origin (`pl_voice::permission`): camera, screen, location and the rest are
//! refused, and no answer is ever kept in the profile all the launcher's windows share. Only its
//! responses may compile WebAssembly (`pl_voice::csp`). It may navigate nowhere but its own page,
//! and its commands are its own two (`pl_voice::guard`). A crash of its WebView, or a close the
//! launcher did not make, comes back to the machine (`page_crashed`): the window opens again and
//! rejoins.
//!
//! The main window gets a guard too ([`guard_main`]): its page never gets the microphone or the
//! camera (no prompt, nothing saved), and an answer an older launcher let WebView2 save for the
//! launcher's origin is reset at start.

use std::borrow::Cow;
use std::sync::atomic::{AtomicBool, Ordering};

use pl_voice::{VOICE_PAGE, VOICE_WINDOW};
use tauri::http::{HeaderValue, Response, header::CONTENT_SECURITY_POLICY};
use tauri::{AppHandle, Manager, WebviewUrl, WebviewWindowBuilder};

/// The launcher itself is closing the voice window (a destroyed window is a crash otherwise).
static CLOSING: AtomicBool = AtomicBool::new(false);

/// Opens the window (`again`: after a crash, the old one is gone first). Blocking: off the main
/// thread and off the async runtime's workers.
pub fn open(app: &AppHandle, again: bool) -> Result<(), String> {
    if let Some(old) = app.get_webview_window(VOICE_WINDOW) {
        if !again {
            return Ok(());
        }
        // Its `Destroyed` comes later and takes the mark back (`destroyed`).
        CLOSING.store(true, Ordering::SeqCst);
        let _ = old.destroy();
        // The label is free only once the old window is gone.
        for _ in 0..40 {
            if app.get_webview_window(VOICE_WINDOW).is_none() {
                break;
            }
            std::thread::sleep(std::time::Duration::from_millis(50));
        }
    }
    let window = WebviewWindowBuilder::new(app, VOICE_WINDOW, WebviewUrl::App(VOICE_PAGE.into()))
        .title("Projects Launcher")
        .visible(false)
        .skip_taskbar(true)
        .focused(false)
        .focusable(false)
        .decorations(false)
        .inner_size(320.0, 240.0)
        // Its own page only (the launcher's origin, `/voice.html`).
        .on_navigation(|url| {
            pl_voice::permission::is_app_origin(url.as_str())
                && url.path().trim_start_matches('/') == VOICE_PAGE
        })
        .on_web_resource_request(|_request, response| allow_wasm(response))
        .build()
        .map_err(|error| error.to_string())?;
    #[cfg(windows)]
    platform::prepare(&window)?;
    #[cfg(not(windows))]
    let _ = window;
    Ok(())
}

/// Closes the window (a minute after the last room, or for good).
pub fn close(app: &AppHandle) {
    if let Some(window) = app.get_webview_window(VOICE_WINDOW) {
        CLOSING.store(true, Ordering::SeqCst);
        let _ = window.destroy();
    }
}

/// The voice window was destroyed: by the launcher (nothing to do), or by something else (the
/// main page's window permissions reach it): as a crash, so a call opens it again.
pub fn destroyed(app: &AppHandle) {
    if CLOSING.swap(false, Ordering::SeqCst) {
        return;
    }
    if let Some(state) = app.try_state::<crate::state::AppState>() {
        tracing::warn!("voice window: closed by something else");
        let voice = state.voice.clone();
        tauri::async_runtime::spawn(async move { voice.page_crashed() });
    }
}

/// The main window's page never gets the microphone or the camera (call once, in setup).
pub fn guard_main(app: &tauri::App) {
    #[cfg(windows)]
    if let Some(window) = app.get_webview_window("main")
        && let Err(error) = platform::guard_main(&window)
    {
        tracing::warn!(%error, "main window: media permissions not guarded");
    }
    #[cfg(not(windows))]
    let _ = app;
}

fn allow_wasm(response: &mut Response<Cow<'static, [u8]>>) {
    let Some(policy) = response.headers_mut().get_mut(CONTENT_SECURITY_POLICY) else {
        return;
    };
    let Ok(text) = policy.to_str() else {
        return;
    };
    if let Ok(value) = HeaderValue::from_str(&pl_voice::csp::allow_wasm(text)) {
        *policy = value;
    }
}

#[cfg(windows)]
mod platform {
    use pl_voice::permission::{Answer, Ask};
    use tauri::{Manager, WebviewWindow};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_PERMISSION_KIND, COREWEBVIEW2_PERMISSION_KIND_CAMERA,
        COREWEBVIEW2_PERMISSION_KIND_MICROPHONE, COREWEBVIEW2_PERMISSION_STATE_ALLOW,
        COREWEBVIEW2_PERMISSION_STATE_DEFAULT, COREWEBVIEW2_PERMISSION_STATE_DENY,
        COREWEBVIEW2_PROCESS_FAILED_KIND, COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED,
        COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE, ICoreWebView2,
        ICoreWebView2_13, ICoreWebView2PermissionRequestedEventArgs,
        ICoreWebView2PermissionRequestedEventArgs3, ICoreWebView2Profile4,
    };
    use webview2_com::{
        PermissionRequestedEventHandler, ProcessFailedEventHandler,
        SetPermissionStateCompletedHandler, take_pwstr,
    };
    use windows::core::{HSTRING, Interface, PWSTR};

    /// The launcher's origins whose saved media answers are reset at start.
    const RESET_ORIGINS: [&str; 2] = ["http://tauri.localhost", "https://tauri.localhost"];

    /// The page counts as visible; the microphone is answered here; crashes come back.
    pub(super) fn prepare(window: &WebviewWindow) -> Result<(), String> {
        let app = window.app_handle().clone();
        let label = window.label().to_owned();
        window
            .with_webview(move |webview| {
                let controller = webview.controller();
                // SAFETY: COM calls on the WebView2 objects Tauri hands over, on their thread.
                if let Err(error) = unsafe { controller.SetIsVisible(true) } {
                    tracing::warn!(%error, "voice window: page not visible");
                }
                let core = match unsafe { controller.CoreWebView2() } {
                    Ok(core) => core,
                    Err(error) => {
                        tracing::warn!(%error, "voice window: no WebView2");
                        return;
                    }
                };
                if let Err(error) = answer_permissions(&core, label) {
                    // Without the handler WebView2 would ask in a window nobody sees: the page
                    // finds no microphone and the player listens only.
                    tracing::warn!(%error, "voice window: microphone unanswered");
                }
                watch_failures(&core, app.clone());
            })
            .map_err(|error| error.to_string())
    }

    /// The main window: microphone and camera refused, never saved; saved answers reset.
    pub(super) fn guard_main(window: &WebviewWindow) -> Result<(), String> {
        let label = window.label().to_owned();
        window
            .with_webview(move |webview| {
                // SAFETY: COM calls on the WebView2 objects Tauri hands over, on their thread.
                let core = match unsafe { webview.controller().CoreWebView2() } {
                    Ok(core) => core,
                    Err(error) => {
                        tracing::warn!(%error, "main window: no WebView2");
                        return;
                    }
                };
                if let Err(error) = answer_permissions(&core, label) {
                    tracing::warn!(%error, "main window: media permissions unanswered");
                }
                reset_saved(&core);
            })
            .map_err(|error| error.to_string())
    }

    /// Answers media requests in this window by `pl_voice::permission` (the asking origin, the
    /// window's label); other kinds in the main window keep WebView2's own handling. Nothing is
    /// ever kept in the profile: when that cannot be made sure of, the answer is "no".
    fn answer_permissions(core: &ICoreWebView2, window: String) -> windows::core::Result<()> {
        let voice = window == pl_voice::VOICE_WINDOW;
        let handler = PermissionRequestedEventHandler::create(Box::new(move |_, args| {
            let Some(args) = args else {
                return Ok(());
            };
            let mut kind = COREWEBVIEW2_PERMISSION_KIND::default();
            // SAFETY: reading the event's arguments inside its handler.
            if unsafe { args.PermissionKind(&mut kind) }.is_err() {
                return deny(&args);
            }
            let media = kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE
                || kind == COREWEBVIEW2_PERMISSION_KIND_CAMERA;
            if !voice && !media {
                // The main page's other permissions (clipboard and the like): as before.
                return Ok(());
            }
            let mut uri = PWSTR::null();
            if unsafe { args.Uri(&mut uri) }.is_err() {
                return deny(&args);
            }
            let uri = take_pwstr(uri);
            let ask = if kind == COREWEBVIEW2_PERMISSION_KIND_MICROPHONE {
                Ask::Microphone
            } else {
                Ask::Other
            };
            let answer = pl_voice::permission::decide(&window, &uri, ask);
            // Not kept in the shared profile first; an "allow" only when that held.
            let unsaved = args
                .cast::<ICoreWebView2PermissionRequestedEventArgs3>()
                .and_then(|args| unsafe { args.SetSavesInProfile(false) })
                .is_ok();
            let state = if answer == Answer::Allow && unsaved {
                COREWEBVIEW2_PERMISSION_STATE_ALLOW
            } else {
                COREWEBVIEW2_PERMISSION_STATE_DENY
            };
            unsafe { args.SetState(state)? };
            if state == COREWEBVIEW2_PERMISSION_STATE_DENY {
                tracing::debug!(kind = kind.0, voice, "a permission was refused");
            }
            Ok(())
        }));
        let mut token = 0;
        // SAFETY: registers the handler on this window's own WebView.
        unsafe { core.add_PermissionRequested(&handler, &mut token) }
    }

    fn deny(args: &ICoreWebView2PermissionRequestedEventArgs) -> windows::core::Result<()> {
        if let Ok(args3) = args.cast::<ICoreWebView2PermissionRequestedEventArgs3>() {
            // SAFETY: inside the event's handler.
            let _ = unsafe { args3.SetSavesInProfile(false) };
        }
        unsafe { args.SetState(COREWEBVIEW2_PERMISSION_STATE_DENY) }
    }

    /// Microphone and camera answers saved for the launcher's origins (by an older launcher, or
    /// WebView2's own prompt) go back to "ask", which the handlers above answer every time.
    fn reset_saved(core: &ICoreWebView2) {
        let Ok(profile) = core
            .cast::<ICoreWebView2_13>()
            .and_then(|core| unsafe { core.Profile() })
            .and_then(|profile| profile.cast::<ICoreWebView2Profile4>())
        else {
            tracing::debug!("WebView2 too old to reset saved permissions");
            return;
        };
        for origin in RESET_ORIGINS {
            for kind in [
                COREWEBVIEW2_PERMISSION_KIND_MICROPHONE,
                COREWEBVIEW2_PERMISSION_KIND_CAMERA,
            ] {
                let done = SetPermissionStateCompletedHandler::create(Box::new(|_| Ok(())));
                // SAFETY: a profile call with a string that outlives it.
                if let Err(error) = unsafe {
                    profile.SetPermissionState(
                        kind,
                        &HSTRING::from(origin),
                        COREWEBVIEW2_PERMISSION_STATE_DEFAULT,
                        &done,
                    )
                } {
                    tracing::debug!(%error, "a saved permission was not reset");
                }
            }
        }
    }

    /// The page's renderer exited or hung, or WebView2 itself went: the machine decides
    /// (open again and rejoin, or give up after a few).
    fn watch_failures(core: &ICoreWebView2, app: tauri::AppHandle) {
        let handler = ProcessFailedEventHandler::create(Box::new(move |_, args| {
            let mut kind = COREWEBVIEW2_PROCESS_FAILED_KIND::default();
            if let Some(args) = args {
                // SAFETY: reading the event's arguments inside its handler.
                unsafe { args.ProcessFailedKind(&mut kind)? };
            }
            let fatal = kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_EXITED
                || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_RENDER_PROCESS_UNRESPONSIVE
                || kind == COREWEBVIEW2_PROCESS_FAILED_KIND_BROWSER_PROCESS_EXITED;
            if fatal {
                tracing::warn!(kind = kind.0, "voice window: the page failed");
                if let Some(state) = app.try_state::<crate::state::AppState>() {
                    let voice = state.voice.clone();
                    tauri::async_runtime::spawn(async move { voice.page_crashed() });
                }
            }
            Ok(())
        }));
        let mut token = 0;
        // SAFETY: registers the handler on this window's own WebView.
        if let Err(error) = unsafe { core.add_ProcessFailed(&handler, &mut token) } {
            tracing::debug!(%error, "voice window: failures not watched");
        }
    }
}
