//! Local bridge between Projects Launcher and the in-game overlay mod (`mods/projects-overlay`).
//!
//! For every game launch the launcher binds `127.0.0.1` on a port the OS picks and makes a fresh random
//! token ([`Bridge::bind`]). Both reach the game as JVM system properties ([`Endpoint::jvm_args`]); the
//! token argument must be redacted wherever launch commands are logged ([`is_secret_jvm_arg`]). The mod
//! connects, proves it knows the token, and then both sides exchange newline-delimited JSON frames
//! ([`protocol`]).
//!
//! Nothing here can make the game do anything: the launcher only sends things to show, and the mod only
//! sends what the player did in its panel. Which social events become in-game toasts is decided by
//! [`policy`], in Rust, because the launcher's web view sleeps while the window is hidden.
//!
//! This is the OVL1 spike sketch; it is not wired into the app yet.

pub mod policy;
pub mod protocol;
mod server;
mod token;

pub use server::{Bridge, BridgeEvent, Endpoint, PORT_ARG, TOKEN_ARG, is_secret_jvm_arg};
pub use token::SessionToken;
