//! The game's window on screen (PLAY1). `pl_mc::launch` asks [`probe`] while a game opens, so the
//! launcher says "Açılıyor" until the window is there and "Oyunda" from then on.
//!
//! Windows: a visible top-level window of the kind Minecraft draws in (GLFW for 1.13 and later and
//! for Forge's and NeoForge's loading window, LWJGL 2 for 1.12 and older), owned by the game's
//! process or by one it started (the Java behind a wrapper command). Dialogs do not count (a driver
//! warning, the mod loader's error window): the game is not on screen until its own window is.
//! Nothing is sent to any window, so a window that hangs cannot hold the look. Elsewhere there is
//! no probe and the game's log tells instead.

/// The probe for this platform: whether a window of the game started as a process id is on
/// screen. `None` where windows cannot be looked at.
pub fn probe() -> Option<fn(u32) -> bool> {
    cfg!(windows).then_some(game_window_shown as fn(u32) -> bool)
}

/// Window classes Minecraft draws in. GLFW's own hidden helper window is not one.
pub fn is_game_window_class(class: &str) -> bool {
    let class = class.to_ascii_lowercase();
    (class.starts_with("glfw") && !class.contains("helper")) || class.starts_with("lwjgl")
}

/// Whether `pid` is `root` or was started by it, directly or through others: a wrapper command
/// starts the Java that draws the window. `parent` gives the process that started one and when the
/// one asked about started. Windows reuses process ids, so a parent younger than its child is a
/// stale id and is not followed.
pub fn descends_from(pid: u32, root: u32, parent: impl Fn(u32) -> Option<(u32, u64)>) -> bool {
    if pid == root {
        return true;
    }
    let mut current = pid;
    let mut child_born = u64::MAX;
    // Wrappers are shallow; a loop of stale ids ends here too.
    for _ in 0..16 {
        let Some((up, born)) = parent(current) else {
            return false;
        };
        // A process cannot have been started by one younger than itself.
        if born > child_born {
            return false;
        }
        if current == root {
            return true;
        }
        if up == current {
            return false;
        }
        child_born = born;
        current = up;
    }
    false
}

/// Whether a window of the game started as `root` is on screen now.
pub fn game_window_shown(root: u32) -> bool {
    #[cfg(windows)]
    {
        let owners = win::game_window_owners();
        owners.contains(&root) || owners.iter().any(|&pid| win::started_by(pid, root))
    }
    #[cfg(not(windows))]
    {
        let _ = root;
        false
    }
}

#[cfg(windows)]
mod win {
    use std::collections::HashMap;
    use std::sync::Mutex;

    use windows::Win32::Foundation::{HWND, LPARAM, RECT};
    use windows::Win32::UI::WindowsAndMessaging::{
        EnumWindows, GetClassNameW, GetWindowRect, GetWindowThreadProcessId, IsWindowVisible,
    };
    use windows::core::BOOL;

    use super::{descends_from, is_game_window_class};

    /// What a process tree lookup found: whether each foreign window owner descends from the
    /// game being opened. Another program's GLFW window (a second Minecraft) is looked up once.
    type Verdicts = HashMap<u32, bool>;

    /// The smallest window side that counts: a minimized window is 28 px tall.
    const MIN_SIDE: i32 = 100;

    /// Process ids owning a visible, game-kind top-level window.
    pub fn game_window_owners() -> Vec<u32> {
        let mut owners: Vec<u32> = Vec::new();
        // SAFETY: `visit` only reads the windows it is given and writes into `owners`, which
        // outlives the enumeration (EnumWindows returns once every window was visited).
        let _ = unsafe { EnumWindows(Some(visit), LPARAM(&raw mut owners as isize)) };
        owners
    }

    unsafe extern "system" fn visit(hwnd: HWND, owners: LPARAM) -> BOOL {
        // SAFETY: `owners` is the vector `game_window_owners` passed for this enumeration.
        let owners = unsafe { &mut *(owners.0 as *mut Vec<u32>) };
        // SAFETY: plain reads of a window handle EnumWindows handed out; none sends a message.
        unsafe {
            if !IsWindowVisible(hwnd).as_bool() {
                return BOOL(1);
            }
            let mut pid = 0u32;
            GetWindowThreadProcessId(hwnd, Some(&raw mut pid));
            if pid == 0 || owners.contains(&pid) {
                return BOOL(1);
            }
            let mut class = [0u16; 64];
            let len = GetClassNameW(hwnd, &mut class);
            let Ok(len) = usize::try_from(len) else {
                return BOOL(1);
            };
            if len == 0 || !is_game_window_class(&String::from_utf16_lossy(&class[..len])) {
                return BOOL(1);
            }
            let mut rect = RECT::default();
            if GetWindowRect(hwnd, &raw mut rect).is_ok()
                && rect.right - rect.left >= MIN_SIDE
                && rect.bottom - rect.top >= MIN_SIDE
            {
                owners.push(pid);
            }
        }
        BOOL(1)
    }

    /// The answers for the game being opened (`root`), kept until another game opens.
    static VERDICTS: Mutex<Option<(u32, Verdicts)>> = Mutex::new(None);

    /// Whether `pid` (owning a game-kind window) was started by `root`, through a wrapper.
    pub fn started_by(pid: u32, root: u32) -> bool {
        let mut verdicts = VERDICTS.lock().unwrap_or_else(|e| e.into_inner());
        if verdicts.as_ref().is_none_or(|(game, _)| *game != root) {
            *verdicts = Some((root, Verdicts::new()));
        }
        let Some((_, known)) = verdicts.as_mut() else {
            return false;
        };
        if let Some(&answer) = known.get(&pid) {
            return answer;
        }
        let mut system = sysinfo::System::new();
        system.refresh_processes_specifics(
            sysinfo::ProcessesToUpdate::All,
            true,
            sysinfo::ProcessRefreshKind::nothing(),
        );
        let answer = descends_from(pid, root, |id| {
            let process = system.process(sysinfo::Pid::from_u32(id))?;
            Some((process.parent()?.as_u32(), process.start_time()))
        });
        known.insert(pid, answer);
        answer
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    #[test]
    fn minecraft_draws_in_glfw_or_lwjgl_windows() {
        for class in ["GLFW30", "glfw30", "LWJGL", "LWJGLWindow"] {
            assert!(is_game_window_class(class), "{class}");
        }
        for class in [
            "GLFW3 Helper",
            "SunAwtFrame",
            "SunAwtDialog",
            "#32770",
            "ConsoleWindowClass",
            "",
        ] {
            assert!(!is_game_window_class(class), "{class}");
        }
    }

    /// A wrapper (10) starts Java (11): its window belongs to the game started as 10.
    #[test]
    fn windows_of_processes_the_game_started_count() {
        let tree: HashMap<u32, (u32, u64)> = [
            (10, (4, 100)),
            (11, (10, 101)),
            (12, (11, 102)),
            // An unrelated program.
            (20, (4, 50)),
            // Its parent id 10 is stale: process 21 is older than the game now using id 10.
            (21, (10, 90)),
        ]
        .into();
        let parent = |id: u32| tree.get(&id).copied();
        assert!(descends_from(10, 10, parent));
        assert!(descends_from(11, 10, parent));
        assert!(descends_from(12, 10, parent));
        assert!(!descends_from(20, 10, parent));
        assert!(!descends_from(21, 10, parent));
        // Unknown processes and loops end without an answer.
        assert!(!descends_from(99, 10, parent));
        let looped: HashMap<u32, (u32, u64)> = [(30, (31, 5)), (31, (30, 5))].into();
        assert!(!descends_from(30, 10, |id| looped.get(&id).copied()));
    }

    #[test]
    fn only_windows_has_a_probe() {
        assert_eq!(probe().is_some(), cfg!(windows));
    }

    /// Real windows of this test process: a visible GLFW-class window counts, a hidden one and a
    /// dialog-kind one do not. The windows sit far off screen, have no taskbar button and never
    /// take the focus, so nothing shows on the desktop.
    #[cfg(windows)]
    #[test]
    fn a_visible_glfw_window_is_the_game_window() {
        use windows::Win32::Foundation::{HWND, LPARAM, LRESULT, WPARAM};
        use windows::Win32::System::LibraryLoader::GetModuleHandleW;
        use windows::Win32::UI::WindowsAndMessaging::{
            CreateWindowExW, DefWindowProcW, DestroyWindow, RegisterClassW, SW_HIDE,
            SW_SHOWNOACTIVATE, ShowWindow, WNDCLASSW, WS_EX_NOACTIVATE, WS_EX_TOOLWINDOW, WS_POPUP,
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
                // Already registered by an earlier run in this process: fine.
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
        let game = window("GLFW30");
        assert!(!game_window_shown(me), "a hidden window is not on screen");
        let dialog = window("SunAwtDialog");
        // SAFETY: windows of this thread, created above.
        unsafe {
            let _ = ShowWindow(dialog, SW_SHOWNOACTIVATE);
        }
        assert!(!game_window_shown(me), "a dialog is not the game");
        unsafe {
            let _ = ShowWindow(game, SW_SHOWNOACTIVATE);
        }
        assert!(game_window_shown(me));
        // No process has this id, so this window is not its game's.
        assert!(!game_window_shown(u32::MAX - 15), "another game's window");
        unsafe {
            let _ = ShowWindow(game, SW_HIDE);
        }
        assert!(!game_window_shown(me));
        unsafe {
            let _ = DestroyWindow(game);
            let _ = DestroyWindow(dialog);
        }
    }
}
