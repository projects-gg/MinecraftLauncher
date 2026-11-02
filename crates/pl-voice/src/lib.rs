//! Voice chat on the launcher side (VOICE, design in the task note `voice.md`; the service's
//! contract is the friends service's README "Voice (VOICE-S1)").
//!
//! The voice engine runs in a WebView2 window of its own that is never shown (`voice`, the page
//! `apps/launcher/voice.html`): the main page may rest and be suspended during a call (OPT1,
//! `webview_rest`), and the call's audio work stays in a small page without React. Everything else
//! is here, free of Tauri so it is tested here:
//!
//! - [`manager`]: [`Voice`], what the launcher runs: the [`machine`] with its I/O;
//! - [`machine`]: the state machine (rings, calls, stays, reconnects, the window's life, input,
//!   the page's state), sans I/O;
//! - [`room`]: one room as the player sees it; [`wire`]: the room socket's frames; [`socket`]: the
//!   room socket's task;
//! - [`view`]: what the launcher's page sees and asks; [`settings`]: Settings → Sesli sohbet and
//!   the per-person choices; [`keys`]: shortcut keys and their clashes; [`input`]: the watched
//!   keys (Raw Input) and when anything is watched at all; [`idle`]: the inactivity rule;
//! - [`engine`]: the boundary with the voice page; [`guard`]: which window may call which command;
//! - [`permission`]: the microphone only for the voice window's own page, never saved in the
//!   profile all the launcher's windows share; [`csp`]: the voice page may compile WebAssembly
//!   (the noise suppressor), the main page's policy stays; [`lifecycle`]: the window opens on
//!   joining a room, closes a minute after leaving, comes back after a crash (a few times);
//! - [`gain`]: a person's volume, whose top depends on how the page plays people;
//! - [`probe`]: VOICE-0's measurement run (debug builds).
//!
//! Tickets, SDP, TURN credentials and session tokens never reach a log line.

pub mod csp;
pub mod engine;
pub mod gain;
pub mod guard;
pub mod idle;
pub mod input;
pub mod keys;
pub mod lifecycle;
pub mod machine;
pub mod manager;
pub mod permission;
pub mod probe;
pub mod room;
pub mod settings;
pub mod socket;
pub mod view;
pub mod wire;

#[cfg(test)]
mod machine_tests;

pub use manager::{CrewTicket, Platform, Service, Voice};

/// The voice window's label.
pub const VOICE_WINDOW: &str = "voice";
/// The voice page in the launcher's frontend (`apps/launcher/voice.html`).
pub const VOICE_PAGE: &str = "voice.html";
