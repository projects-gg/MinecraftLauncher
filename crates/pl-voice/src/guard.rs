//! Which window may call which of the launcher's commands (VOICE-0's L1 decision).
//!
//! Tauri lets every window call every app command unless an app manifest lists them all; the
//! launcher has hundreds, and a missed one would break the main page. So one check in front of the
//! command handler does it instead: the hidden voice window may call only its own two commands,
//! and no other window may call those. Plugin commands follow the windows' capabilities as before
//! (the voice window's grant nothing in release builds).

use crate::VOICE_WINDOW;

/// The voice page's commands (`voice_page_attach`, `voice_page_report`).
pub const PAGE_COMMANDS: [&str; 2] = ["voice_page_attach", "voice_page_report"];

/// SHARE: what the "Yayın" window calls: its stage's commands and the state to draw it from
/// (SQOL: and what it plays streams with, its watching choices).
pub const SHARE_WINDOW_COMMANDS: [&str; 14] = [
    "voice_state",
    "share_watch",
    "share_unwatch",
    "share_watch_layer",
    "share_watch_attach",
    "share_watch_report",
    "share_watch_visible",
    "share_watch_still",
    "share_popout",
    "share_popout_on_top",
    "share_fullscreen",
    "share_stream_volume",
    "share_popout_audio",
    "share_view_prefs",
];

/// Whether `window` may call the app command `command`.
pub fn allowed(window: &str, command: &str) -> bool {
    let page_command = PAGE_COMMANDS.contains(&command);
    if window == VOICE_WINDOW {
        page_command
    } else if window == crate::share::SHARE_WINDOW {
        SHARE_WINDOW_COMMANDS.contains(&command)
    } else {
        !page_command
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_voice_window_reaches_only_its_own_commands() {
        assert!(allowed(VOICE_WINDOW, "voice_page_report"));
        assert!(allowed(VOICE_WINDOW, "voice_page_attach"));
        for main_command in [
            "voice_call",
            "voice_state",
            "social_send_request",
            "account_remove",
            "app_quit",
            "",
        ] {
            assert!(!allowed(VOICE_WINDOW, main_command), "{main_command}");
        }
    }

    #[test]
    fn no_other_window_reaches_the_voice_pages_commands() {
        assert!(!allowed("main", "voice_page_report"));
        assert!(!allowed("main", "voice_page_attach"));
        assert!(allowed("main", "voice_call"));
        assert!(allowed("main", "social_state"));
        // A window label that only resembles the voice window's.
        assert!(!allowed("voice2", "voice_page_report"));
        assert!(!allowed("Voice", "voice_page_report"));
    }

    #[test]
    fn the_share_window_reaches_only_its_stage() {
        for command in SHARE_WINDOW_COMMANDS {
            assert!(allowed(crate::share::SHARE_WINDOW, command), "{command}");
            assert!(allowed("main", command), "{command}");
        }
        for other in [
            "share_start",
            "share_sources",
            "share_switch",
            "voice_chat_send",
            "voice_save_settings",
            "voice_call",
            "voice_page_report",
            "social_send_request",
            "app_quit",
        ] {
            assert!(!allowed(crate::share::SHARE_WINDOW, other), "{other}");
        }
        assert!(allowed("main", "share_start"));
    }
}
