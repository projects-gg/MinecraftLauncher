//! The launcher's world sharing (HOST1, "Dünyanı arkadaşlarına aç"): a player opens their
//! singleplayer world to friends they invite, and the invited friends' launchers join it through a
//! relay. The tunnel, the paths and install matching are `pl-host`'s; the presence socket's frames
//! are `pl-social`'s (`world`); the game's side is the overlay bridge (`pl-overlay`, rows H1–H16 of
//! the OVL2 protocol log). This crate holds them together for the launcher ([`Worlds`]) and keeps
//! the page's view of it ([`view`]).
//!
//! Nothing here logs an address, a port of a relay, a domain, a slot, a key or a credential
//! (PROTOCOL.md §13).

mod link;
mod manager;
pub mod manifest;
mod mismatch;
/// The end-to-end rig's relays and light guest (HOST1-E): never in a release build.
#[cfg(debug_assertions)]
pub mod rig;
pub mod view;

#[cfg(test)]
mod tests;

pub use link::{
    CREDS_WAIT, CredsTable, GuestLink, HostLink, PEER_WAIT, PeerWaiters, Service, Transports,
};
pub use manager::{
    Account, Bridge, HostInstall, Launcher, PlayTogether, Progress, UiEvent, UiSink, Worlds,
    WorldsConfig, refusal_text, world_label, world_label_in,
};
pub use view::{
    HostNotice, HostPlace, HostSessionView, HostView, WorldGameMode, WorldGuest, WorldGuestState,
    WorldInviteOutcome, WorldInviteRefusal, WorldInviteView, WorldJoinAnswer, WorldJoinAsk,
    WorldJoinFailure, WorldJoinStage, WorldNameVerdict, WorldOpenFailure, WorldOpenOutcome,
    WorldOpenRequest, WorldOpeningStep,
};
