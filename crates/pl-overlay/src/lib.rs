//! Local bridge between Projects Launcher and the in-game overlay mod (`mods/projects-overlay`).
//!
//! For every game launch the launcher binds `127.0.0.1` on a port the OS picks and makes a fresh random
//! token ([`Overlay::start`], [`Bridge::bind`]). Both reach the game as JVM system properties
//! ([`Endpoint::jvm_args`]); the token's value must be masked wherever launch commands are logged
//! ([`Endpoint::secret`], [`is_secret_jvm_arg`]). The mod connects, proves it knows the token, and
//! then both sides exchange newline-delimited JSON frames ([`protocol`]).
//!
//! Nothing here can make the game do anything: the launcher only sends things to show, and the mod
//! only sends what the player did in its panel. What the mod hears is made from the friends
//! feature's views (`feed`); which social events become in-game toasts is decided by [`policy`], in
//! Rust, because the launcher's web view may rest while the window is hidden.
//!
//! Costs nothing while idle: between games every update is dropped at once, during a game the
//! tasks wait on their channels and sockets (no timers, no polling).
//!
//! The mod itself ships with the launcher, one jar per Minecraft version; [`install`] keeps the
//! right one in the instance a game starts from (OVL-B).

mod feed;
mod host;
mod hub;
pub mod install;
pub mod links;
pub mod policy;
pub mod protocol;
mod server;
mod session;
#[cfg(test)]
mod tests;
mod token;

pub use feed::{MAX_CONVS, MAX_FRIENDS};
pub use host::{HostSignal, HostSink, WorldsShown};
pub use hub::{Overlay, OverlayConfig, OverlayStatus};
pub use server::{
    BYE_TIMEOUT, Bridge, BridgeEvent, ConnId, Control, Endpoint, PORT_ARG, TOKEN_ARG,
    is_secret_jvm_arg,
};
pub use session::{CONNECT_WAIT, ConnectOutcome, PeopleChange, SocialActions};
pub use token::SessionToken;
