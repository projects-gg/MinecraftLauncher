//! HOST1's side of the bridge ("Dünyanı arkadaşlarına aç"): what the running game's mod says
//! about the player's world reaches the launcher's world sharing as [`HostSignal`]s; what world
//! sharing tells the mod goes through [`crate::Overlay::host_send`], only to a mod whose `hello`
//! lists `host` (rows H1–H16 of `docs/tasks/ovl2-protocol.md`).
//!
//! The launcher gives the game commands (`host-publish`, `host-stop`, `host-remove`, `connect`)
//! only right after the player's own press, or for a security event (a block, an unfriending, a
//! restriction, the launcher quitting). Nothing here decides that: world sharing does, in the
//! launcher; path moves never reach the game.

use std::collections::HashMap;
use std::sync::Arc;

use crate::protocol::{FromGame, WorldMark};

/// What world sharing hears from the running game's overlay.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum HostSignal {
    /// A mod connected (also again after a reconnect); `hosts`: its hello lists `host`.
    Connected { hosts: bool },
    /// The mod went: the game ended, the overlay went off, the launcher quits.
    Gone,
    /// One of the mod's `host-*` frames (only from a mod that lists `host`).
    Frame(FromGame),
    /// "Katıl" in the game on a friend who invited the player to their world (H15).
    Join { friend: String },
}

/// Hears [`HostSignal`]s; called from the session's task, so it must not block.
pub type HostSink = Arc<dyn Fn(HostSignal) + Send + Sync>;

/// What world sharing tells every overlay session: the friends who invited the player to their
/// open world (H13; only invitees learn that a world is open) and whether the player may open
/// their own world now (H2: `host` in `welcome`).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldsShown {
    pub marks: HashMap<String, WorldMark>,
    pub may_host: bool,
}
