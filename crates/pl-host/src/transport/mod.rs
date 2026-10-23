//! Paths a PLJ1 stream rides on (PROTOCOL.md §5, §11). Implementations: `e4mc`, `turn`.

pub mod e4mc;
pub mod turn;

// HOST1-R2 (additive): what both paths share. `net` is public for its configuration types.
mod mc;
pub mod net;
mod quic;

use std::borrow::Cow;
use std::future::Future;
use std::net::SocketAddrV4;
use std::pin::Pin;

use tokio::io::{AsyncRead, AsyncWrite};

pub type BoxFuture<'a, T> = Pin<Box<dyn Future<Output = T> + Send + 'a>>;

/// Which relay a path goes through.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum PathKind {
    Turn,
    E4mc,
}

impl PathKind {
    /// The wire name (`paths`, `order`, telemetry).
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Turn => "turn",
            Self::E4mc => "e4mc",
        }
    }
}

/// What a host announces for one of its paths (an entry of `paths`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathAddress {
    /// The host allocation's relay address and the SHA-256 of its QUIC certificate.
    Turn { relay: SocketAddrV4, pin: [u8; 32] },
    /// The e4mc domain, lowercase.
    E4mc { domain: String },
}

/// A step of a path attempt (PROTOCOL.md §12.2).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Step {
    Creds,
    Alloc,
    Peer,
    Broker,
    Quic,
    Domain,
    Tcp,
    Plj,
    Resume,
}

impl Step {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Creds => "creds",
            Self::Alloc => "alloc",
            Self::Peer => "peer",
            Self::Broker => "broker",
            Self::Quic => "quic",
            Self::Domain => "domain",
            Self::Tcp => "tcp",
            Self::Plj => "plj",
            Self::Resume => "resume",
        }
    }
}

/// A path failure: the step and one code of PROTOCOL.md §12.2. Never carries an address, a domain
/// or a credential.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PathError {
    pub step: Step,
    pub code: Cow<'static, str>,
}

/// One reliable, ordered, bidirectional byte stream: a QUIC stream (TURN) or a TCP connection
/// through the e4mc relay. Shutting down the write half ends the stream for the peer.
pub trait PathStream: AsyncRead + AsyncWrite + Send + Unpin + 'static {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin + 'static> PathStream for T {}

pub type BoxStream = Box<dyn PathStream>;

/// The host's side of one path.
pub trait HostPath: Send + Sync {
    fn kind(&self) -> PathKind;
    /// What invites, `world` and `world_peer_ok` announce for this path.
    fn address(&self) -> PathAddress;
    /// TURN: permit and bind a guest's relay address until `forget_peer`. e4mc: nothing.
    fn admit_peer(&self, relay: SocketAddrV4) -> BoxFuture<'_, Result<(), PathError>>;
    fn forget_peer(&self, relay: SocketAddrV4);
    /// The next stream a guest opened, positioned at the PLJ1 opening (e4mc: the relay's handshake
    /// packet already read and checked).
    fn accept(&self) -> BoxFuture<'_, Result<BoxStream, PathError>>;
    /// Resolves once the path is gone for good (relay closed it, allocation or credentials lost).
    fn closed(&self) -> BoxFuture<'_, PathError>;
    /// Bytes received from the relay so far, transport overhead included (TURN: what is billed).
    fn rx_bytes(&self) -> u64;
    fn close(&self) -> BoxFuture<'_, ()>;
}

/// The guest's side of one path to one host.
pub trait GuestPath: Send + Sync {
    fn kind(&self) -> PathKind;
    /// A new stream to the host (e4mc: a TCP connection with the handshake packet written; TURN: a
    /// QUIC stream, connecting first if needed).
    fn open(&self) -> BoxFuture<'_, Result<BoxStream, PathError>>;
    /// The opening got a reply that is not msg2 (`packet`: the bytes after its `VarInt` length):
    /// the path's own name for it (e4mc: `e4mc_unknown_domain`, `e4mc_rate_limited`), if any.
    fn explain_reply(&self, packet: &[u8]) -> Option<Cow<'static, str>>;
    fn closed(&self) -> BoxFuture<'_, PathError>;
    fn rx_bytes(&self) -> u64;
    fn close(&self) -> BoxFuture<'_, ()>;
}
