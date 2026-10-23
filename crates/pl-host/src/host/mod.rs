//! The host's side of a world session (PROTOCOL.md §3.1, §6.8, §7, §9).
//!
//! [`HostSession`] accepts the streams guests open on the session's paths, admits them, connects
//! game streams to the world on loopback behind a [`gate`] header, and moves guests between paths.
//! [`binding`] is the name binding file the control stream's name verdict reads.

pub(crate) mod admission;
pub mod binding;
pub mod gate;
mod session;
#[cfg(test)]
mod tests;

pub use session::{
    GameEndpoint, GuestStatus, GuestUpdate, HostConfig, HostEnv, HostEvent, HostSession,
    InviteError, OpenError,
};
