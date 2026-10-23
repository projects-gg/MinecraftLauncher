//! PLJ1, the end-to-end tunnel between a guest's and a host's launcher (PROTOCOL.md §6).
//!
//! Every stream on a path starts with a Noise `NKpsk0` handshake ([`noise`]): the guest proves it
//! holds its invite's PSK, the host that it holds the session's static key. After it, both sides
//! exchange encrypted frames of [`record`]s: control messages ([`control`]), and on game streams
//! the game connection's bytes with their offsets, so a game connection survives its stream
//! breaking and continues on another path from the last byte each side received.

pub(crate) mod channel;
pub mod control;
pub(crate) mod handshake;
pub mod ids;
pub mod invite;
pub(crate) mod link;
pub mod noise;
pub mod record;
pub mod varint;

#[cfg(test)]
mod vectors;

pub use control::{
    Accept, ByeReason, CloseReason, Control, ControlError, Hello, MoveWhy, NameVerdict, Refusal,
    WorldState,
};
pub use ids::{GuestId, HostKey, Psk, SessionId, Slot, StreamId};
pub use invite::{Join, JoinError, Paths, TurnAddress};
