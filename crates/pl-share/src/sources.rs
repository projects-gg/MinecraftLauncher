//! Which windows the picker lists, and in what order (pure). Enumeration itself is in
//! `win::sources`.

use std::collections::HashSet;

use crate::api::{SourceInfo, SourceKind};

/// What enumeration learned about one top-level window.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WindowFacts {
    pub hwnd: u32,
    pub pid: u32,
    pub title: String,
    pub class: String,
    /// The program's file name without `.exe`, lower-case kept as is.
    pub app: String,
    pub visible: bool,
    /// DWM-cloaked (hidden UWP frames, windows on another virtual desktop).
    pub cloaked: bool,
    pub tool_window: bool,
    /// Has an owner window (dialogs, popups) and is not marked as an app window.
    pub owned: bool,
    pub app_window: bool,
    pub minimized: bool,
    pub w: i32,
    pub h: i32,
}

/// Shell surfaces that are never a sensible thing to share.
const SHELL_CLASSES: &[&str] = &[
    "Progman",
    "WorkerW",
    "Shell_TrayWnd",
    "Shell_SecondaryTrayWnd",
    "Windows.UI.Core.CoreWindow",
    "XamlExplorerHostIslandWindow",
    "TopLevelWindowForOverflowXamlIsland",
    "NotifyIconOverflowWindow",
];

/// Smaller than this (in physical pixels) is a helper window, not something to share.
const MIN_W: i32 = 120;
const MIN_H: i32 = 60;

/// Whether the picker lists the window. `own` holds our processes: our windows are never listed
/// (the launcher, the voice window, the share bar), so we can never be asked to capture ourselves.
pub fn keep_window(f: &WindowFacts, own: &HashSet<u32>) -> bool {
    if own.contains(&f.pid) || !f.visible || f.cloaked || f.title.trim().is_empty() {
        return false;
    }
    if f.tool_window && !f.app_window {
        return false;
    }
    if f.owned && !f.app_window {
        return false;
    }
    if SHELL_CLASSES.contains(&f.class.as_str()) {
        return false;
    }
    // A minimized window reports a tiny off-screen rectangle; keep it (it is shared once restored).
    f.minimized || (f.w >= MIN_W && f.h >= MIN_H)
}

/// The game's window among `windows`: the largest visible, titled window of `game_pid`.
pub fn game_window(windows: &[WindowFacts], game_pid: u32) -> Option<&WindowFacts> {
    windows
        .iter()
        .filter(|w| w.pid == game_pid && w.visible && !w.cloaked && !w.title.trim().is_empty())
        .max_by_key(|w| i64::from(w.w.max(0)) * i64::from(w.h.max(0)))
}

/// The picker's order: the game first, then windows in z-order (as enumerated, front to back),
/// minimized ones after visible ones, then monitors with the primary first.
pub fn order(list: &mut [SourceInfo]) {
    let rank = |s: &SourceInfo| match s.kind {
        SourceKind::Game => 0u8,
        SourceKind::Window if !s.minimized => 1,
        SourceKind::Window => 2,
        SourceKind::Monitor if s.primary => 3,
        SourceKind::Monitor => 4,
    };
    // Stable: z-order and display order survive inside each rank.
    list.sort_by_key(|s| {
        (
            rank(s),
            if s.kind == SourceKind::Monitor {
                s.index
            } else {
                0
            },
        )
    });
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::api::SourceId;

    fn window(pid: u32, title: &str) -> WindowFacts {
        WindowFacts {
            hwnd: pid * 10,
            pid,
            title: title.into(),
            class: "Chrome_WidgetWin_1".into(),
            app: "app".into(),
            visible: true,
            w: 1280,
            h: 720,
            ..Default::default()
        }
    }

    #[test]
    fn our_own_windows_are_never_listed() {
        let own: HashSet<u32> = [42].into();
        assert!(!keep_window(&window(42, "Projects Launcher"), &own));
        assert!(keep_window(&window(7, "Notepad"), &own));
    }

    #[test]
    fn hidden_helper_and_shell_windows_are_skipped() {
        let own = HashSet::new();
        let mut w = window(7, "x");
        w.visible = false;
        assert!(!keep_window(&w, &own));
        let mut w = window(7, "x");
        w.cloaked = true;
        assert!(!keep_window(&w, &own));
        let mut w = window(7, "   ");
        w.title = "   ".into();
        assert!(!keep_window(&w, &own));
        let mut w = window(7, "Tool");
        w.tool_window = true;
        assert!(!keep_window(&w, &own));
        w.app_window = true;
        assert!(keep_window(&w, &own), "WS_EX_APPWINDOW wins");
        let mut w = window(7, "Dialog");
        w.owned = true;
        assert!(!keep_window(&w, &own));
        let mut w = window(7, "Program Manager");
        w.class = "Progman".into();
        assert!(!keep_window(&w, &own));
        let mut w = window(7, "Tiny");
        w.w = 40;
        assert!(!keep_window(&w, &own));
        w.minimized = true;
        assert!(keep_window(&w, &own), "minimized windows stay listed");
    }

    #[test]
    fn the_game_is_its_largest_window() {
        let mut splash = window(9, "Minecraft splash");
        splash.w = 400;
        splash.h = 300;
        let game = window(9, "Minecraft 1.21.4");
        let other = window(3, "Other");
        let list = [splash, game.clone(), other];
        assert_eq!(game_window(&list, 9), Some(&game));
        assert_eq!(game_window(&list, 99), None);
    }

    fn info(
        kind: SourceKind,
        title: &str,
        primary: bool,
        index: u8,
        minimized: bool,
    ) -> SourceInfo {
        SourceInfo {
            id: SourceId::Monitor {
                device: title.into(),
            },
            kind,
            title: title.into(),
            app: String::new(),
            index,
            primary,
            w: 1,
            h: 1,
            minimized,
            thumb: None,
        }
    }

    #[test]
    fn order_puts_the_game_first_and_monitors_last() {
        let mut list = vec![
            info(SourceKind::Monitor, "m2", false, 2, false),
            info(SourceKind::Window, "w-min", false, 0, true),
            info(SourceKind::Window, "w1", false, 0, false),
            info(SourceKind::Monitor, "m1", true, 1, false),
            info(SourceKind::Game, "game", false, 0, false),
            info(SourceKind::Window, "w2", false, 0, false),
        ];
        order(&mut list);
        let titles: Vec<&str> = list.iter().map(|s| s.title.as_str()).collect();
        assert_eq!(titles, ["game", "w1", "w2", "w-min", "m1", "m2"]);
    }
}
