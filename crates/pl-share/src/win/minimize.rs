//! SQOL: whether the shared window is minimized, from Windows' own minimize events: a WinEvent
//! hook on the window's process (out of context), on a small thread that sleeps in its message loop
//! (no polling). Watchers keep the window's last picture meanwhile; the room hears `st: "min"`.
//!
//! The same thread hears the window's end (`EVENT_OBJECT_DESTROY`): the capture item's own
//! `Closed` did not come for a closed window in the real-SFU spike (13 Nov), so the share kept
//! sending the closed window's last picture and never said it was gone.

use std::cell::RefCell;
use std::sync::mpsc::sync_channel;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    CHILDID_SELF, DispatchMessageW, EVENT_OBJECT_DESTROY, EVENT_SYSTEM_MINIMIZEEND,
    EVENT_SYSTEM_MINIMIZESTART, GetMessageW, IsIconic, IsWindow, MSG, OBJID_WINDOW, PM_NOREMOVE,
    PeekMessageW, PostThreadMessageW, TranslateMessage, WINEVENT_OUTOFCONTEXT, WM_QUIT, WM_USER,
};

/// Told `true` when the window is minimized, `false` when it comes back.
pub type OnChange = Box<dyn Fn(bool) + Send>;
/// Told once when the window is gone (closed, or its process ended).
pub type OnClosed = Box<dyn Fn() + Send>;

/// The running watch: dropping it ends its thread (and the hooks).
pub struct MinimizeWatch {
    thread: u32,
}

/// What the watch thread watches (the hook's procedure has no context of its own).
struct Watched {
    target: isize,
    on_change: OnChange,
    on_closed: OnClosed,
    closed: bool,
}

thread_local! {
    static WATCHED: RefCell<Option<Watched>> = const { RefCell::new(None) };
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    child: i32,
    _thread: u32,
    _time: u32,
) {
    // The window itself (not a part of it).
    if object != OBJID_WINDOW.0 || child != CHILDID_SELF as i32 {
        return;
    }
    WATCHED.with(|watched| {
        let mut watched = watched.borrow_mut();
        let Some(watched) = watched.as_mut().filter(|w| hwnd.0 as isize == w.target) else {
            return;
        };
        if event == EVENT_OBJECT_DESTROY {
            if !watched.closed {
                watched.closed = true;
                (watched.on_closed)();
            }
        } else if !watched.closed {
            (watched.on_change)(event == EVENT_SYSTEM_MINIMIZESTART);
        }
    });
}

/// An out-of-context hook on `pid`'s events `from..=to`, to `on_event` (None: Windows refused). The
/// calling thread owns it and unhooks it before it ends.
fn hook(from: u32, to: u32, pid: u32) -> Option<HWINEVENTHOOK> {
    // SAFETY: a plain Win32 call; `on_event` lives as long as the program.
    let hook = unsafe {
        SetWinEventHook(
            from,
            to,
            None,
            Some(on_event),
            pid,
            0,
            WINEVENT_OUTOFCONTEXT,
        )
    };
    (!hook.is_invalid()).then_some(hook)
}

impl MinimizeWatch {
    /// Watches window `hwnd` of process `pid`; `on_change` hears its state once at the start when
    /// it is minimized already, then every change; `on_closed` hears its end (at once when it is
    /// gone already). None when Windows refused the hooks.
    pub fn start(hwnd: u32, pid: u32, on_change: OnChange, on_closed: OnClosed) -> Option<Self> {
        let (tx, rx) = sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("pl-share-min".into())
            .spawn(move || {
                let target = hwnd as usize as isize;
                // SAFETY: plain Win32 calls on this thread; the hooks go before the thread ends.
                unsafe {
                    let mut msg = MSG::default();
                    // The thread's message queue exists before anyone posts to it.
                    let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
                    let Some(minimize) =
                        hook(EVENT_SYSTEM_MINIMIZESTART, EVENT_SYSTEM_MINIMIZEEND, pid)
                    else {
                        let _ = tx.send(None);
                        return;
                    };
                    // Without it the capture's own `Closed` is all there is.
                    let destroy = hook(EVENT_OBJECT_DESTROY, EVENT_OBJECT_DESTROY, pid);
                    let window = HWND(target as *mut _);
                    let minimized = IsIconic(window).as_bool();
                    let gone = !IsWindow(Some(window)).as_bool();
                    WATCHED.with(|watched| {
                        *watched.borrow_mut() = Some(Watched {
                            target,
                            on_change,
                            on_closed,
                            closed: false,
                        });
                    });
                    let _ = tx.send(Some(GetCurrentThreadId()));
                    WATCHED.with(|watched| {
                        if let Some(watched) = watched.borrow_mut().as_mut() {
                            if gone {
                                watched.closed = true;
                                (watched.on_closed)();
                            } else if minimized {
                                (watched.on_change)(true);
                            }
                        }
                    });
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    let _ = UnhookWinEvent(minimize);
                    if let Some(destroy) = destroy {
                        let _ = UnhookWinEvent(destroy);
                    }
                    WATCHED.with(|watched| watched.borrow_mut().take());
                }
            });
        if spawned.is_err() {
            return None;
        }
        rx.recv().ok().flatten().map(|thread| Self { thread })
    }
}

impl Drop for MinimizeWatch {
    fn drop(&mut self) {
        // SAFETY: posting to our own thread's queue; a thread that ended already just fails.
        unsafe {
            let _ = PostThreadMessageW(self.thread, WM_QUIT, WPARAM(0), LPARAM(0));
        }
    }
}
