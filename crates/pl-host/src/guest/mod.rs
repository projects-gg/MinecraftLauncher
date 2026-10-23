//! The guest's side of a world session (PROTOCOL.md §3.2, §6.7, §8).
//!
//! [`GuestSession`] joins through the service's order, keeps a control stream to the host, relays
//! the game's connection from the [`local`] endpoint, and moves streams between paths without the
//! game noticing.

pub mod local;
mod session;

pub use session::{GuestConfig, GuestEnd, GuestEnv, GuestEvent, GuestSession, RefusalText};
