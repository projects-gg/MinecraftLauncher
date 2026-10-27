//! The main window's WebView2 while nobody can see it (Windows; elsewhere nothing happens).
//!
//! WebView2 treats the page as visible for as long as its controller says so, and hiding or
//! minimizing the window does not change that: Chromium kept the page's timers, priority and
//! memory as if the player were looking (`document.visibilityState` stayed "visible" in the tray,
//! G6-R). So the window procedure is watched here. Every way the window goes out of sight or comes
//! back (the close button, the tray, the taskbar, Win+D, the game starting, a `projects://` link,
//! a second launch, from JS or from Rust) ends in the same window messages, so no path is left out:
//!
//! - after `REST_AFTER` out of sight (hidden or minimized), the page rests: WebView2 treats it as
//!   hidden (no frames, background timer throttling), keeps as little memory as it can (memory
//!   usage target "low") and, when the page has nothing to do meanwhile (`may_suspend`), suspends
//!   the renderer: no script runs and Windows may give its memory to others (the game);
//! - coming back, the page is resumed, gets its normal memory target and is visible again before
//!   the window paints.
//!
//! The game's exit wakes a suspended page at once ([`wake`]): the page brings the window back and
//! tells about a crash. Debug builds take `PROJECTS_WEBVIEW_REST` (`off`, or a comma list of
//! `hide`, `low`, `suspend`; default all three) to measure each step.

use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(windows)]
use std::time::Duration;

use tauri::{AppHandle, Listener, Manager};

/// How long the window is out of sight before the page rests: a quick minimize and restore, or
/// the tray hint after "Kapat", never pay for it.
#[cfg(windows)]
const REST_AFTER: Duration = Duration::from_secs(10);
/// While the page rests without being suspended (it had something to do), whether it may be
/// suspended now is asked again this often.
#[cfg(windows)]
const CHECK_AGAIN: Duration = Duration::from_secs(5 * 60);

/// Background work the page reports itself (`page-duties` event, `src/lib/page-duties.ts`): a
/// reminder waiting to ring. A suspended page would miss it.
#[derive(Default)]
pub struct PageDuties(AtomicBool);

#[derive(serde::Deserialize)]
struct PageDutiesPayload {
    busy: bool,
}

/// What resting does (all of it in release builds).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Steps {
    hide: bool,
    low: bool,
    suspend: bool,
}

impl Steps {
    const ALL: Self = Self {
        hide: true,
        low: true,
        suspend: true,
    };

    fn any(self) -> bool {
        self.hide || self.low || self.suspend
    }

    #[cfg(debug_assertions)]
    fn from_env() -> Self {
        let Ok(value) = std::env::var("PROJECTS_WEBVIEW_REST") else {
            return Self::ALL;
        };
        let words: Vec<String> = value
            .split(',')
            .map(|word| word.trim().to_ascii_lowercase())
            .collect();
        let has = |word: &str| words.iter().any(|w| w == word);
        Self {
            // Suspending needs the page hidden (WebView2's rule).
            hide: has("hide") || has("suspend"),
            low: has("low"),
            suspend: has("suspend"),
        }
    }

    #[cfg(not(debug_assertions))]
    fn from_env() -> Self {
        Self::ALL
    }
}

/// Watches the main window (call once, in setup).
pub fn install(app: &tauri::App) {
    let steps = Steps::from_env();
    tracing::debug!(?steps, "webview rest");
    app.manage(PageDuties::default());
    if !steps.any() {
        return;
    }
    let handle = app.handle().clone();
    app.listen_any("page-duties", move |event| {
        let Ok(payload) = serde_json::from_str::<PageDutiesPayload>(event.payload()) else {
            return;
        };
        handle
            .state::<PageDuties>()
            .0
            .store(payload.busy, Ordering::Relaxed);
        if !payload.busy {
            // A page that rests unsuspended because of its duty may be suspended now.
            check_soon(&handle);
        }
    });
    #[cfg(windows)]
    platform::install(app, steps);
}

/// Wakes a suspended page (the window may stay out of sight): it has work to do now.
pub fn wake(app: &AppHandle, why: &'static str) {
    #[cfg(windows)]
    platform::wake(app, why);
    #[cfg(not(windows))]
    let _ = (app, why);
}

/// Wakes the page right now if it is suspended, and leaves an awake one alone (its rest stays
/// on time). Main thread only, so what is sent to the page next reaches it awake: the close
/// button of a window that rested minimized (`shell::close_requested`).
pub fn wake_if_suspended(app: &AppHandle, why: &'static str) {
    #[cfg(windows)]
    platform::wake_now(app, why, true);
    #[cfg(not(windows))]
    let _ = (app, why);
}

fn check_soon(app: &AppHandle) {
    #[cfg(windows)]
    platform::check_soon(app);
    #[cfg(not(windows))]
    let _ = app;
}

/// Whether the page may be suspended now: nothing it does in the background would wait for the
/// window to come back. A running game is fine (its exit wakes the page) once its window showed;
/// while it opens, the page answers its window (and reports that as its duty until it did).
#[cfg(windows)]
fn may_suspend(app: &AppHandle) -> Result<(), &'static str> {
    let state = app.state::<crate::state::AppState>();
    if state.launch_cancel.lock().is_some() {
        // Progress, the Java question and the long-install notification come from the page.
        return Err("a launch is being prepared");
    }
    if state
        .games
        .lock()
        .iter()
        .any(|game| !game.process.window_shown())
    {
        // "Oyunda", the presence and the launcher's own window answer the game's window.
        return Err("the game is opening");
    }
    if state.afk.any_active() {
        // The page tells when an AFK account stops for good.
        return Err("AFK accounts are active");
    }
    if crate::commands::modrinth::jobs_running(app)
        || crate::commands::library::jobs_running(app)
        || crate::commands::java::job_running(app)
        || crate::commands::data::job_running(app)
    {
        return Err("a job is running");
    }
    if state.social.view().auth != pl_social::AuthPhase::SignedOut {
        // Invites and friends coming online are told by the page.
        return Err("friends are signed in");
    }
    if app.state::<PageDuties>().0.load(Ordering::Relaxed) {
        return Err("the page has a reminder to ring");
    }
    Ok(())
}

#[cfg(windows)]
mod platform {
    use std::cell::RefCell;

    use tauri::{AppHandle, Manager};
    use webview2_com::Microsoft::Web::WebView2::Win32::{
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW,
        COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL, ICoreWebView2_3, ICoreWebView2_19,
        ICoreWebView2Controller,
    };
    use webview2_com::TrySuspendCompletedHandler;
    use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
    use windows::Win32::UI::Shell::{DefSubclassProc, SetWindowSubclass};
    use windows::Win32::UI::WindowsAndMessaging::{
        IsIconic, IsWindowVisible, SIZE_MINIMIZED, SWP_HIDEWINDOW, SWP_SHOWWINDOW, WINDOWPOS,
        WM_SIZE, WM_WINDOWPOSCHANGED, WM_WINDOWPOSCHANGING,
    };
    use windows::core::Interface;

    use super::{CHECK_AGAIN, REST_AFTER, Steps, may_suspend};

    const SUBCLASS_ID: usize = 0x504C_5257;

    /// The page's state. Lives on the main thread, where the window procedure and WebView2 run.
    struct Rest {
        app: AppHandle,
        hwnd: HWND,
        controller: ICoreWebView2Controller,
        steps: Steps,
        /// The window is out of sight (hidden or minimized).
        away: bool,
        /// What resting changed, undone when the window comes back.
        hidden: bool,
        low: bool,
        suspended: bool,
        /// Grows on every change, so a timer or a suspension of an older absence does nothing.
        epoch: u64,
    }

    thread_local! {
        static REST: RefCell<Option<Rest>> = const { RefCell::new(None) };
    }

    pub fn install(app: &tauri::App, steps: Steps) {
        let Some(window) = app.get_webview_window("main") else {
            return;
        };
        let Ok(hwnd) = window.hwnd() else {
            return;
        };
        // A window handle is only a number; it travels to the main thread as one.
        let raw = hwnd.0 as isize;
        let handle = app.handle().clone();
        let result = window.with_webview(move |webview| {
            let hwnd = HWND(raw as *mut std::ffi::c_void);
            let controller = webview.controller();
            REST.with(|rest| {
                *rest.borrow_mut() = Some(Rest {
                    app: handle.clone(),
                    hwnd,
                    controller,
                    steps,
                    away: false,
                    hidden: false,
                    low: false,
                    suspended: false,
                    epoch: 0,
                });
            });
            // SAFETY: `hwnd` is the main window, alive for the whole run; the procedure runs on
            // this (the main) thread and only forwards after looking at the message.
            if let Err(error) =
                unsafe { SetWindowSubclass(hwnd, Some(procedure), SUBCLASS_ID, 0) }.ok()
            {
                tracing::warn!(%error, "webview rest: window not watched");
                return;
            }
            // Started in the tray: the window was never shown, and the start reveals it only after
            // the page's first frame (an invisible page would not draw one). The page rests once
            // its start is done, if the window is still out of sight then.
            schedule(&handle, 0, REST_AFTER);
        });
        if let Err(error) = result {
            tracing::warn!(%error, "webview rest: no WebView2 controller");
        }
    }

    unsafe extern "system" fn procedure(
        hwnd: HWND,
        msg: u32,
        wparam: WPARAM,
        lparam: LPARAM,
        _id: usize,
        _data: usize,
    ) -> LRESULT {
        match msg {
            // Before the window appears, whatever asked for it: the page is ready by the time it
            // paints.
            WM_WINDOWPOSCHANGING => {
                // SAFETY: lParam of WM_WINDOWPOSCHANGING points to a WINDOWPOS.
                let flags = unsafe { (*(lparam.0 as *const WINDOWPOS)).flags };
                if flags.contains(SWP_SHOWWINDOW) {
                    came_back("shown");
                }
            }
            WM_WINDOWPOSCHANGED => {
                // SAFETY: lParam of WM_WINDOWPOSCHANGED points to a WINDOWPOS.
                let flags = unsafe { (*(lparam.0 as *const WINDOWPOS)).flags };
                if flags.contains(SWP_HIDEWINDOW) {
                    went_away("hidden");
                }
            }
            WM_SIZE => {
                if wparam.0 == SIZE_MINIMIZED as usize {
                    went_away("minimized");
                } else if unsafe { IsWindowVisible(hwnd) }.as_bool() {
                    came_back("restored");
                }
            }
            _ => {}
        }
        // SAFETY: forwards the message unchanged to the next procedure.
        unsafe { DefSubclassProc(hwnd, msg, wparam, lparam) }
    }

    /// Runs `f` on the page's state unless it is already in use (a WebView2 call made from inside
    /// the window procedure can send it messages again).
    fn with_rest<T>(f: impl FnOnce(&mut Rest) -> T) -> Option<T> {
        REST.with(|rest| rest.try_borrow_mut().ok()?.as_mut().map(f))
    }

    fn went_away(why: &'static str) {
        let scheduled = with_rest(|rest| {
            if rest.away {
                return None;
            }
            rest.away = true;
            rest.epoch += 1;
            tracing::debug!(why, "window out of sight");
            Some((rest.app.clone(), rest.epoch))
        })
        .flatten();
        if let Some((app, epoch)) = scheduled {
            schedule(&app, epoch, REST_AFTER);
        }
    }

    fn came_back(why: &'static str) {
        let page = with_rest(|rest| {
            if !rest.away && !rest.hidden && !rest.low && !rest.suspended {
                return None;
            }
            rest.away = false;
            rest.epoch += 1;
            let undo = (rest.suspended, rest.low, rest.hidden);
            rest.suspended = false;
            rest.low = false;
            rest.hidden = false;
            Some((rest.controller.clone(), undo))
        })
        .flatten();
        let Some((controller, (suspended, low, hidden))) = page else {
            return;
        };
        let started = std::time::Instant::now();
        let core = unsafe { controller.CoreWebView2() }.ok();
        if suspended
            && let Some(core) = core.as_ref().and_then(|c| c.cast::<ICoreWebView2_3>().ok())
            && let Err(error) = unsafe { core.Resume() }
        {
            tracing::debug!(%error, "webview did not resume");
        }
        if low && let Some(core) = &core {
            set_memory_target(core, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_NORMAL);
        }
        if hidden && let Err(error) = unsafe { controller.SetIsVisible(true) } {
            tracing::warn!(%error, "webview not made visible");
        }
        tracing::debug!(
            why,
            suspended,
            micros = started.elapsed().as_micros() as u64,
            "webview awake"
        );
    }

    /// Rests the page `after` from now if the window is still out of sight from the same absence.
    fn schedule(app: &AppHandle, epoch: u64, after: std::time::Duration) {
        let app = app.clone();
        tauri::async_runtime::spawn(async move {
            tokio::time::sleep(after).await;
            let main = app.clone();
            let _ = app.run_on_main_thread(move || rest_now(&main, epoch));
        });
    }

    fn rest_now(app: &AppHandle, epoch: u64) {
        // The window's own answer counts (a start in the tray never sent a message).
        let page = with_rest(|rest| {
            let away =
                unsafe { !IsWindowVisible(rest.hwnd).as_bool() || IsIconic(rest.hwnd).as_bool() };
            if rest.epoch != epoch || !away {
                return None;
            }
            rest.away = true;
            Some((
                rest.controller.clone(),
                rest.steps,
                rest.hidden,
                rest.low,
                rest.suspended,
            ))
        })
        .flatten();
        let Some((controller, steps, hidden, low, suspended)) = page else {
            return;
        };
        let core = unsafe { controller.CoreWebView2() }.ok();
        if steps.hide && !hidden {
            match unsafe { controller.SetIsVisible(false) } {
                Ok(()) => {
                    with_rest(|rest| rest.hidden = true);
                }
                Err(error) => tracing::debug!(%error, "webview not hidden"),
            }
        }
        if steps.low
            && !low
            && let Some(core) = &core
            && set_memory_target(core, COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL_LOW)
        {
            with_rest(|rest| rest.low = true);
        }
        if !steps.suspend || suspended {
            tracing::debug!(?steps, "webview rests");
            return;
        }
        if let Err(reason) = may_suspend(app) {
            tracing::debug!(reason, "webview rests, not suspended");
            schedule(app, epoch, CHECK_AGAIN);
            return;
        }
        let Some(core) = core.and_then(|c| c.cast::<ICoreWebView2_3>().ok()) else {
            return;
        };
        let resume = core.clone();
        let handler = TrySuspendCompletedHandler::create(Box::new(move |result, done| {
            let suspended = result.is_ok() && done;
            let current = with_rest(|rest| {
                let current = rest.epoch == epoch && rest.away;
                if current {
                    rest.suspended = suspended;
                }
                current
            })
            .unwrap_or(false);
            if suspended && !current {
                // The window came back, or the page was woken, while the request went out.
                let _ = unsafe { resume.Resume() };
            }
            tracing::debug!(suspended, current, "webview rests, suspend answered");
            Ok(())
        }));
        if let Err(error) = unsafe { core.TrySuspend(&handler) } {
            tracing::debug!(%error, "webview not suspended");
        }
    }

    fn set_memory_target(
        core: &webview2_com::Microsoft::Web::WebView2::Win32::ICoreWebView2,
        level: COREWEBVIEW2_MEMORY_USAGE_TARGET_LEVEL,
    ) -> bool {
        match core.cast::<ICoreWebView2_19>() {
            Ok(core) => match unsafe { core.SetMemoryUsageTargetLevel(level) } {
                Ok(()) => true,
                Err(error) => {
                    tracing::debug!(%error, "webview memory target not set");
                    false
                }
            },
            Err(_) => false,
        }
    }

    pub fn wake(app: &AppHandle, why: &'static str) {
        let main = app.clone();
        let _ = app.run_on_main_thread(move || wake_now(&main, why, false));
    }

    /// Wakes the page (main thread). `only_if_suspended`: an awake page is left alone.
    pub fn wake_now(app: &AppHandle, why: &'static str, only_if_suspended: bool) {
        let page = with_rest(|rest| {
            if only_if_suspended && !rest.suspended {
                return None;
            }
            // A new absence for the timers (and for a suspension still on its way): the page
            // has a while to do its work before it may rest again.
            rest.epoch += 1;
            let suspended = std::mem::take(&mut rest.suspended);
            Some((rest.controller.clone(), rest.epoch, suspended, rest.away))
        })
        .flatten();
        let Some((controller, epoch, suspended, away)) = page else {
            return;
        };
        if suspended
            && let Some(core) = unsafe { controller.CoreWebView2() }
                .ok()
                .and_then(|c| c.cast::<ICoreWebView2_3>().ok())
            && let Err(error) = unsafe { core.Resume() }
        {
            tracing::debug!(%error, "webview did not resume");
        }
        tracing::debug!(why, suspended, "webview woken");
        if away {
            schedule(app, epoch, CHECK_AGAIN);
        }
    }

    pub fn check_soon(app: &AppHandle) {
        let main = app.clone();
        let _ = app.run_on_main_thread(move || {
            let epoch = with_rest(|rest| {
                (rest.away && rest.hidden && !rest.suspended).then_some(rest.epoch)
            })
            .flatten();
            if let Some(epoch) = epoch {
                rest_now(&main, epoch);
            }
        });
    }
}
