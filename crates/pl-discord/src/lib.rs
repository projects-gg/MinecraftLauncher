//! Minimal Discord Rich Presence client over local IPC (no Discord SDK).
//!
//! - [`PresenceManager`]: background actor the launcher talks to (`set`, `clear`,
//!   `set_enabled`, `shutdown`); lazy connection, reconnects, rate limiting.
//! - [`DiscordIpc`]: a single connection (handshake, `SET_ACTIVITY`, pings) for one-off use.
//! - [`Activity`]: the presence model with the modern fields (`type`, `status_display_type`,
//!   `details_url`/`state_url`, image URLs, party, buttons) and length validation.
//! - Transport: Windows named pipes `\\.\pipe\discord-ipc-{0..9}`; Unix sockets under
//!   `$XDG_RUNTIME_DIR`, `$TMPDIR`, `$TMP`, `$TEMP`, `/tmp` (including Flatpak, Snap and
//!   Vesktop sub-folders). Frames are `u32` LE opcode + `u32` LE length + JSON.

mod activity;
mod connection;
mod error;
mod frame;
mod ipc;
mod manager;

pub use activity::{
    Activity, ActivityType, Assets, BUTTON_LABEL_MAX, BUTTON_URL_MAX, Button, MAX_BUTTONS, Party,
    PartySize, StatusDisplayType, TEXT_MAX, TEXT_MIN, Timestamps, URL_MAX, fit_text,
};
pub use connection::{DiscordIpc, DiscordUser, Incoming};
pub use error::{Error, Result};
pub use frame::{Frame, HEADER_LEN, MAX_PAYLOAD, Opcode, read_frame, write_frame};
pub use ipc::default_endpoints;
pub use manager::{PresenceConfig, PresenceManager, PresenceStatus};
