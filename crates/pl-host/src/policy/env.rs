//! What the path state machines need from the launcher: the friends service's answers (over the
//! presence socket) and the path constructors (`transport`). HOST1-L1 implements these traits;
//! the tests use fakes.
//!
//! TURN credentials stay with the launcher: this crate only hands back the [`Creds`] handle it was
//! given, so no TURN username or password ever passes through here.

use std::borrow::Cow;
use std::net::SocketAddrV4;
use std::sync::Arc;
use std::time::Duration;

use crate::transport::{BoxFuture, GuestPath, HostPath, PathError, PathKind};
use crate::tunnel::Paths;

/// `turn_creds.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum CredsReason {
    /// Which path to try first, with credentials when TURN is first.
    Primary,
    /// The first path failed: TURN now.
    Fallback,
    /// An open connection's credentials end soon.
    Renew,
}

impl CredsReason {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Primary => "primary",
            Self::Fallback => "fallback",
            Self::Renew => "renew",
        }
    }
}

/// TURN credentials the launcher holds (from a `turn_creds` answer's `ice`): an opaque handle and
/// how long they last.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct Creds {
    /// The launcher's own id for them.
    pub id: u64,
    /// `ice.ttl`.
    pub ttl: Duration,
}

/// A `turn_creds` answer.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CredsAnswer {
    /// `order`: the paths to try, best first (may be empty).
    pub order: Vec<PathKind>,
    /// Present when the answer carried `ice`.
    pub creds: Option<Creds>,
}

/// A `turn_creds` request that brought no credentials.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CredsError {
    /// `turn_denied` with its `reason` (`off`, `budget`, `session`, `player`, `full`, `day`).
    Denied(String),
    /// `turn_failed` (Cloudflare did not answer), or no answer within 4 s: the other path may work.
    Failed,
    /// The invite or session is not valid any more (`not_hosting`, `worlds_off`, …): stop.
    Gone,
}

impl CredsError {
    /// The telemetry code (§12.2).
    pub fn code(&self) -> Cow<'static, str> {
        match self {
            Self::Denied(reason) => match reason.as_str() {
                "off" => "turn_denied_off",
                "budget" => "turn_denied_budget",
                "session" => "turn_denied_session",
                "player" => "turn_denied_player",
                "full" => "turn_denied_full",
                "day" => "turn_denied_day",
                _ => "turn_creds_error",
            },
            Self::Failed | Self::Gone => "turn_creds_error",
        }
        .into()
    }
}

/// A guest's TURN allocation (HOST1-R2's `turn::allocate`).
pub trait TurnAllocation: Send + Sync {
    /// The allocation's relay address; the host permits it after `world_peer`.
    fn relay(&self) -> SocketAddrV4;
    /// Whether it works only over TCP or TLS (the attempt then reports `turn_tcp_only`).
    fn tcp_only(&self) -> bool {
        false
    }
    /// QUIC to the host's relay address, accepting only the certificate with this pin. The same
    /// allocation connects again when the host's address changes (its credential renewal).
    fn connect(
        &self,
        host_relay: SocketAddrV4,
        pin: [u8; 32],
    ) -> BoxFuture<'_, Result<Arc<dyn GuestPath>, PathError>>;
}

/// The guest launcher's side, for one invite.
pub trait GuestPaths: Send + Sync {
    /// `turn_creds{session, host, role:"guest", reason}` and its answer.
    fn turn_creds(&self, reason: CredsReason) -> BoxFuture<'_, Result<CredsAnswer, CredsError>>;
    /// `turn_release{username}` for credentials no longer used.
    fn turn_release(&self, creds: Creds);
    /// `world_peer{session, host, slot, path, relay?}`; the host's `paths` from `world_peer_ok`, or
    /// `None` when no answer comes.
    fn world_peer(
        &self,
        path: PathKind,
        relay: Option<SocketAddrV4>,
    ) -> BoxFuture<'_, Option<Paths>>;
    /// A TURN allocation with these credentials.
    fn turn_allocate(
        &self,
        creds: Creds,
    ) -> BoxFuture<'_, Result<Arc<dyn TurnAllocation>, PathError>>;
    /// A path to the host's e4mc domain (the handshake packet names `proto`).
    fn e4mc_connect(
        &self,
        domain: &str,
        proto: i32,
    ) -> BoxFuture<'_, Result<Arc<dyn GuestPath>, PathError>>;
}

/// The result of opening the host's e4mc path.
pub struct E4mcOpened {
    pub result: Result<Arc<dyn HostPath>, PathError>,
    /// The relay's id (`eu`, …), for telemetry.
    pub region: Option<String>,
    /// The broker failed (its code) and a fallback relay was tried instead.
    pub broker_failure: Option<Cow<'static, str>>,
}

/// The host launcher's side, for one session.
pub trait HostPaths: Send + Sync {
    /// `turn_creds{session, role:"host", reason}` and its answer.
    fn turn_creds(&self, reason: CredsReason) -> BoxFuture<'_, Result<CredsAnswer, CredsError>>;
    /// `turn_release{username}` for credentials no longer used.
    fn turn_release(&self, creds: Creds);
    /// The host's TURN path with these credentials (HOST1-R2's `turn::open_host`).
    fn turn_open(&self, creds: Creds) -> BoxFuture<'_, Result<Arc<dyn HostPath>, PathError>>;
    /// The host's e4mc path (HOST1-R2's `e4mc::open_host`).
    fn e4mc_open(&self) -> BoxFuture<'_, E4mcOpened>;
}
