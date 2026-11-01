//! Voice chat's rules on the launcher side (VOICE, design in the task note `voice.md`).
//!
//! The voice engine runs in a WebView2 window of its own that is never shown (`voice`, the page
//! `apps/launcher/voice.html`): the main page may rest and be suspended during a call (OPT1,
//! `webview_rest`), and the call's audio work stays in a small page without React. What that
//! window obeys lives here, free of Tauri so it is tested here:
//!
//! - [`permission`]: the microphone only for the voice window's own page, nothing else, never
//!   saved in the profile all the launcher's windows share;
//! - [`csp`]: the voice page may compile WebAssembly (the noise suppressor); the main page's
//!   policy stays as it is;
//! - [`lifecycle`]: the window opens on joining a room, closes a minute after leaving, and comes
//!   back after a crash during a call (a few times, not forever);
//! - [`gain`]: a person's volume, whose top depends on how the page plays people;
//! - [`input`]: push-to-talk's key, watched through Raw Input without looking at other keys;
//! - [`probe`]: VOICE-0's measurement run (debug builds): its configuration and the summary of
//!   what it recorded.

pub mod csp;
pub mod gain;
pub mod input;
pub mod lifecycle;
pub mod permission;
pub mod probe;

/// The voice window's label.
pub const VOICE_WINDOW: &str = "voice";
/// The voice page in the launcher's frontend (`apps/launcher/voice.html`).
pub const VOICE_PAGE: &str = "voice.html";
