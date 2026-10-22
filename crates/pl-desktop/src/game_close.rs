//! Closing the game the way its own close button does (FIX2 "Bağlan": the game restarts into a
//! friend's or a crew's server). Minecraft then leaves a server and saves a singleplayer world as
//! it always does when the player closes it, so nothing is lost. Ending the process instead
//! ([`crate::process::kill`]) is the last resort, and only after the player was warned.
//!
//! Windows: every window of the kind Minecraft draws in ([`crate::game_window::is_game_window_class`])
//! owned by the game's process or by one it started (the Java behind a wrapper command) is sent
//! `WM_CLOSE`, the message its X button sends. The message is posted, never waited for, so a game
//! that hangs cannot hold the launcher. Elsewhere the game's processes get `SIGTERM`: the client's
//! shutdown hook stops a singleplayer world with a save.

use crate::game_window::descends_from;

/// What asking the game to close reached.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CloseAsk {
    /// This many of its windows were sent `WM_CLOSE` (Windows).
    Windows(usize),
    /// This many of its processes were sent `SIGTERM` (elsewhere).
    Signalled(usize),
    /// Nothing of the game was found to ask: it has no window yet (still opening), or it ended.
    Nothing,
}

impl CloseAsk {
    /// Something was asked: the game closes by itself unless it hangs.
    pub fn asked(self) -> bool {
        !matches!(self, Self::Nothing)
    }
}

/// Asks the game started as `root` (its process id) to close, as its close button does.
pub fn ask_to_close(root: u32) -> CloseAsk {
    #[cfg(windows)]
    {
        let windows = win::windows_of(root);
        let asked = windows
            .into_iter()
            .filter(|&hwnd| win::post_close(hwnd))
            .count();
        if asked == 0 {
            CloseAsk::Nothing
        } else {
            CloseAsk::Windows(asked)
        }
    }
    #[cfg(not(windows))]
    {
        let asked = unix::terminate_tree(root);
        if asked == 0 {
            CloseAsk::Nothing
        } else {
            CloseAsk::Signalled(asked)
        }
    }
}

/// The processes of a table that are `root` or were started by it (wrappers are followed),
/// `root` first. `table` lists each process with its parent and start time.
pub fn tree_of(root: u32, table: &[(u32, Option<u32>, u64)]) -> Vec<u32> {
    // A process without a parent (its parent's id is gone) still counts as started at its time.
    let parent = |id: u32| {
        table
            .iter()
            .find(|(pid, _, _)| *pid == id)
            .map(|(_, parent, born)| (parent.unwrap_or(0), *born))
    };
    let mut tree: Vec<u32> = table
        .iter()
        .map(|(pid, _, _)| *pid)
        .filter(|&pid| pid != root && descends_from(pid, root, parent))
        .collect();
    if table.iter().any(|(pid, _, _)| *pid == root) {
        tree.insert(0, root);
    }
    tree
}

/// Every process with its parent and start time, as [`tree_of`] reads them.
fn process_table() -> Vec<(u32, Option<u32>, u64)> {
    let mut system = sysinfo::System::new();
    system.refresh_processes_specifics(
        sysinfo::ProcessesToUpdate::All,
        true,
        sysinfo::ProcessRefreshKind::nothing(),
    );
    system
        .processes()
        .iter()
        .map(|(pid, process)| {
            (
                pid.as_u32(),
                process.parent().map(sysinfo::Pid::as_u32),
                process.start_time(),
            )
        })
        .collect()
}

#[cfg(windows)]
mod win {
    use windows::Win32::Foundation::{HWND, LPARAM, WPARAM};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowThreadProcessId, PostMessageW, WM_CLOSE,
    };
    use windows::core::BOOL;

    use crate::game_window::is_game_window_class;

    /// Top-level windows of the game's kind with the process that owns each.
    fn game_kind_windows() -> Vec<(HWND, u32)> {
        let mut found: Vec<(HWND, u32)> = Vec::new();
        // SAFETY: `visit` only reads the windows it is given and writes into `found`, which
        // outlives the enumeration (EnumWindows returns once every window was visited).
        let _ = unsafe { EnumWindows(Some(visit), LPARAM(&raw mut found as isize)) };
        found
    }

    unsafe extern "system" fn visit(hwnd: HWND, found: LPARAM) -> BOOL {
        // SAFETY: `found` is the vector `game_kind_windows` passed for this enumeration.
        let found = unsafe { &mut *(found.0 as *mut Vec<(HWND, u32)>) };
        // SAFETY: plain reads of a window handle EnumWindows handed out; none sends a message.
        unsafe {
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&raw mut pid));
            if pid == 0 {
                return BOOL(1);
            }
            let mut class = [0u16; 64];
            let len = GetClassNameW(hwnd, &mut class);
            let Ok(len) = usize::try_from(len) else {
                return BOOL(1);
            };
            // Hidden and minimized windows count too: closing is not about being on screen.
            if len > 0 && is_game_window_class(&String::from_utf16_lossy(&class[..len])) {
                found.push((hwnd, pid));
            }
        }
        BOOL(1)
    }

    /// The game's windows: owned by `root` or by a process it started.
    pub(super) fn windows_of(root: u32) -> Vec<HWND> {
        let windows = game_kind_windows();
        // The process table is read only when a window belongs to another process.
        let mut tree: Option<Vec<u32>> = None;
        windows
            .into_iter()
            .filter(|&(_, pid)| {
                pid == root
                    || tree
                        .get_or_insert_with(|| super::tree_of(root, &super::process_table()))
                        .contains(&pid)
            })
            .map(|(hwnd, _)| hwnd)
            .collect()
    }

    /// Posts `WM_CLOSE` to `hwnd` (never waits for the window to handle it).
    pub(super) fn post_close(hwnd: HWND) -> bool {
        // SAFETY: posting a message to a window handle; a stale handle only fails the call.
        unsafe { PostMessageW(Some(hwnd), WM_CLOSE, WPARAM(0), LPARAM(0)) }.is_ok()
    }
}

#[cfg(not(windows))]
mod unix {
    /// Sends `SIGTERM` to `root` and every process it started; how many got it.
    pub(super) fn terminate_tree(root: u32) -> usize {
        let tree = super::tree_of(root, &super::process_table());
        let mut system = sysinfo::System::new();
        let pids: Vec<sysinfo::Pid> = tree.iter().copied().map(sysinfo::Pid::from_u32).collect();
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::Some(&pids),
            true,
            sysinfo::ProcessRefreshKind::nothing(),
        );
        pids.iter()
            .filter(|pid| {
                system
                    .process(**pid)
                    .and_then(|process| process.kill_with(sysinfo::Signal::Term))
                    .unwrap_or(false)
            })
            .count()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A wrapper (10) started Java (11), which started a helper (12); an unrelated program (20)
    /// and a stale child of an older process 10 (21) are not the game's.
    #[test]
    fn the_game_is_its_process_and_those_it_started() {
        let table = [
            (10, Some(4), 100),
            (11, Some(10), 101),
            (12, Some(11), 102),
            (20, Some(4), 50),
            (21, Some(10), 90),
            (4, None, 1),
        ];
        assert_eq!(tree_of(10, &table), vec![10, 11, 12]);
        assert_eq!(tree_of(11, &table), vec![11, 12]);
        // A game whose launcher is gone (no parent any more) keeps its own.
        let orphan = [(10, None, 100), (11, Some(10), 101)];
        assert_eq!(tree_of(10, &orphan), vec![10, 11]);
        // A game that ended: nothing to ask.
        assert!(tree_of(99, &table).is_empty());
    }

    #[test]
    fn nothing_asked_is_no_close() {
        assert!(!CloseAsk::Nothing.asked());
        assert!(CloseAsk::Windows(1).asked());
        assert!(CloseAsk::Signalled(2).asked());
        // No process has this id: nothing of it can be asked.
        assert_eq!(ask_to_close(u32::MAX - 15), CloseAsk::Nothing);
    }

    /// A real window of this test process: a GLFW-class window is the game's (hidden or not) and
    /// gets the close button's message; a dialog-kind one is not asked. Nothing is posted to other
    /// windows of this process (the window test in `game_window` makes GLFW windows too), so the
    /// test posts to its own window only. The window sits far off screen and is never shown.
    #[cfg(windows)]
    #[test]
    fn the_game_window_gets_the_close_button_message() {
        use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, MSG, PM_REMOVE, PeekMessageW,
            RegisterClassW, WM_CLOSE, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
        };
        use windows::core::PCWSTR;

        unsafe extern "system" fn procedure(
            hwnd: HWND,
            message: u32,
            wparam: WPARAM,
            lparam: LPARAM,
        ) -> LRESULT {
            // SAFETY: the default handling of a message for a window of this thread.
            unsafe { DefWindowProcW(hwnd, message, wparam, lparam) }
        }
        fn wide(text: &str) -> Vec<u16> {
            text.encode_utf16().chain([0]).collect()
        }
        // SAFETY: plain window creation on this thread with a class registered here.
        let window = |class: &str| -> HWND {
            let name = wide(class);
            unsafe {
                let instance = GetModuleHandleW(PCWSTR::null()).unwrap();
                let class = WNDCLASSW {
                    lpfnWndProc: Some(procedure),
                    hInstance: instance.into(),
                    lpszClassName: PCWSTR(name.as_ptr()),
                    ..Default::default()
                };
                let _ = RegisterClassW(&class);
                CreateWindowExW(
                    WS_EX_TOOLWINDOW | WS_EX_NOACTIVATE,
                    PCWSTR(name.as_ptr()),
                    PCWSTR(name.as_ptr()),
                    WS_POPUP,
                    -30_000,
                    -30_000,
                    320,
                    200,
                    None,
                    None,
                    Some(instance.into()),
                    None,
                )
                .unwrap()
            }
        };
        let me = std::process::id();
        let game = window("GLFW30 close test");
        let dialog = window("SunAwtDialog close test");
        let found = win::windows_of(me);
        assert!(found.contains(&game), "the game's window, hidden or not");
        assert!(!found.contains(&dialog), "a dialog is not the game");
        assert!(
            win::windows_of(u32::MAX - 15).is_empty(),
            "another game's windows"
        );
        assert!(win::post_close(game));
        let mut message = MSG::default();
        // SAFETY: reads this thread's queue for the message posted to its own window.
        let got =
            unsafe { PeekMessageW(&raw mut message, Some(game), WM_CLOSE, WM_CLOSE, PM_REMOVE) };
        assert!(got.as_bool(), "WM_CLOSE waits in the window's queue");
        assert_eq!(message.message, WM_CLOSE);
        // SAFETY: windows of this thread, created above.
        unsafe {
            let _ = DestroyWindow(game);
            let _ = DestroyWindow(dialog);
        }
    }
}
