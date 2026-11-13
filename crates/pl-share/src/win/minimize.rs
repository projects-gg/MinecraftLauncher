//! SQOL: whether the shared window is minimized, from Windows' own minimize events: a WinEvent
//! hook on the window's process (out of context), on a small thread that sleeps in its message loop
//! (no polling). Watchers keep the window's last picture meanwhile; the room hears `st: "min"`.

use std::cell::RefCell;
use std::sync::mpsc::sync_channel;

use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
use windows::Win32::System::Threading::GetCurrentThreadId;
use windows::Win32::UI::Accessibility::{HWINEVENTHOOK, SetWinEventHook, UnhookWinEvent};
use windows::Win32::UI::WindowsAndMessaging::{
    DispatchMessageW, EVENT_SYSTEM_MINIMIZEEND, EVENT_SYSTEM_MINIMIZESTART, GetMessageW, IsIconic,
    MSG, OBJID_WINDOW, PM_NOREMOVE, PeekMessageW, PostThreadMessageW, TranslateMessage,
    WINEVENT_OUTOFCONTEXT, WM_QUIT, WM_USER,
};

/// Told `true` when the window is minimized, `false` when it comes back.
pub type OnChange = Box<dyn Fn(bool) + Send>;

/// The running watch: dropping it ends its thread (and the hook).
pub struct MinimizeWatch {
    thread: u32,
}

thread_local! {
    /// The watch thread's window and its callback (the hook's procedure has no context of its own).
    static WATCHED: RefCell<Option<(isize, OnChange)>> = const { RefCell::new(None) };
}

unsafe extern "system" fn on_event(
    _hook: HWINEVENTHOOK,
    event: u32,
    hwnd: HWND,
    object: i32,
    _child: i32,
    _thread: u32,
    _time: u32,
) {
    if object != OBJID_WINDOW.0 {
        return;
    }
    WATCHED.with(|watched| {
        if let Some((target, on_change)) = watched.borrow().as_ref()
            && hwnd.0 as isize == *target
        {
            on_change(event == EVENT_SYSTEM_MINIMIZESTART);
        }
    });
}

impl MinimizeWatch {
    /// Watches window `hwnd` of process `pid`; `on_change` hears its state once at the start when
    /// it is minimized already, then every change. None when Windows refused the hook.
    pub fn start(hwnd: u32, pid: u32, on_change: OnChange) -> Option<Self> {
        let (tx, rx) = sync_channel(1);
        let spawned = std::thread::Builder::new()
            .name("pl-share-min".into())
            .spawn(move || {
                let target = hwnd as usize as isize;
                // SAFETY: plain Win32 calls on this thread; the hook goes before the thread ends.
                unsafe {
                    let mut msg = MSG::default();
                    // The thread's message queue exists before anyone posts to it.
                    let _ = PeekMessageW(&mut msg, None, WM_USER, WM_USER, PM_NOREMOVE);
                    let hook = SetWinEventHook(
                        EVENT_SYSTEM_MINIMIZESTART,
                        EVENT_SYSTEM_MINIMIZEEND,
                        None,
                        Some(on_event),
                        pid,
                        0,
                        WINEVENT_OUTOFCONTEXT,
                    );
                    if hook.is_invalid() {
                        let _ = tx.send(None);
                        return;
                    }
                    let minimized = IsIconic(HWND(target as *mut _)).as_bool();
                    WATCHED.with(|watched| *watched.borrow_mut() = Some((target, on_change)));
                    let _ = tx.send(Some(GetCurrentThreadId()));
                    if minimized {
                        WATCHED.with(|watched| {
                            if let Some((_, on_change)) = watched.borrow().as_ref() {
                                on_change(true);
                            }
                        });
                    }
                    while GetMessageW(&mut msg, None, 0, 0).as_bool() {
                        let _ = TranslateMessage(&msg);
                        DispatchMessageW(&msg);
                    }
                    let _ = UnhookWinEvent(hook);
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
