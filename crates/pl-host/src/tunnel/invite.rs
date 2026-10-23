//! The invite's `join` object and path addresses (PROTOCOL.md §4, §5.1).
//!
//! The service passes `join` along unread. A guest checks every address in it before connecting
//! anywhere: TURN relays must be public IPv4 addresses, e4mc domains must be `*.e4mc.link`, so a
//! host cannot point its guests at another server.

use std::fmt;
use std::net::{Ipv4Addr, SocketAddrV4};

use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::ids::{Psk, SessionId, Slot, b64u, from_b64u};
use crate::transport::{PathAddress, PathKind};

/// `join.v` this launcher speaks.
pub const JOIN_VERSION: u32 = 1;
/// Largest `join` as compact JSON.
pub const MAX_JOIN_BYTES: usize = 1_500;
/// The longest e4mc domain.
pub const MAX_DOMAIN_LEN: usize = 253;

/// The host's TURN path: its allocation's relay address and the SHA-256 of its QUIC certificate.
#[derive(Clone, Copy, PartialEq, Eq)]
pub struct TurnAddress {
    pub relay: SocketAddrV4,
    pub pin: [u8; 32],
}

/// A host's ready paths (`paths`), as invites, `world`, `world_peer_ok` and `state` carry them.
///
/// `Debug` hides the addresses: relay addresses and domains never reach a log (PROTOCOL.md §13).
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Paths {
    pub turn: Option<TurnAddress>,
    /// Lowercase `<name>.<region>.e4mc.link`.
    pub e4mc: Option<String>,
}

impl Paths {
    pub fn is_empty(&self) -> bool {
        self.turn.is_none() && self.e4mc.is_none()
    }

    pub fn has(&self, kind: PathKind) -> bool {
        match kind {
            PathKind::Turn => self.turn.is_some(),
            PathKind::E4mc => self.e4mc.is_some(),
        }
    }

    /// The kinds present, TURN first.
    pub fn kinds(&self) -> Vec<PathKind> {
        [PathKind::Turn, PathKind::E4mc]
            .into_iter()
            .filter(|k| self.has(*k))
            .collect()
    }

    /// One path's address.
    pub fn address(&self, kind: PathKind) -> Option<PathAddress> {
        match kind {
            PathKind::Turn => self.turn.map(|t| PathAddress::Turn {
                relay: t.relay,
                pin: t.pin,
            }),
            PathKind::E4mc => self.e4mc.clone().map(|domain| PathAddress::E4mc { domain }),
        }
    }

    /// Sets one path's address (replacing what was there).
    pub fn set(&mut self, address: PathAddress) {
        match address {
            PathAddress::Turn { relay, pin } => self.turn = Some(TurnAddress { relay, pin }),
            PathAddress::E4mc { domain } => self.e4mc = Some(domain),
        }
    }

    pub fn remove(&mut self, kind: PathKind) {
        match kind {
            PathKind::Turn => self.turn = None,
            PathKind::E4mc => self.e4mc = None,
        }
    }

    /// Whether every address passes a guest's checks (§5.1).
    pub fn is_valid(&self) -> bool {
        self.turn.is_none_or(|t| is_public_relay(t.relay))
            && self.e4mc.as_deref().is_none_or(is_e4mc_domain)
    }
}

impl fmt::Debug for Paths {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let mut list = f.debug_list();
        for kind in self.kinds() {
            list.entry(&kind.as_str());
        }
        list.finish()
    }
}

#[derive(Serialize, Deserialize)]
struct TurnWire {
    relay: String,
    pin: String,
}

#[derive(Serialize, Deserialize)]
struct E4mcWire {
    domain: String,
}

#[derive(Serialize, Deserialize)]
struct PathsWire {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    turn: Option<TurnWire>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    e4mc: Option<E4mcWire>,
}

impl Serialize for Paths {
    fn serialize<S: Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        PathsWire {
            turn: self.turn.map(|t| TurnWire {
                relay: t.relay.to_string(),
                pin: b64u(&t.pin),
            }),
            e4mc: self.e4mc.clone().map(|domain| E4mcWire { domain }),
        }
        .serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Paths {
    /// Reads `paths`, refusing any address a guest must not connect to.
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let wire = PathsWire::deserialize(deserializer)?;
        let turn = match wire.turn {
            Some(t) => Some(TurnAddress {
                relay: parse_relay(&t.relay).ok_or_else(|| D::Error::custom("bad relay"))?,
                pin: from_b64u(&t.pin).ok_or_else(|| D::Error::custom("bad pin"))?,
            }),
            None => None,
        };
        let e4mc = match wire.e4mc {
            Some(e) if is_e4mc_domain(&e.domain) => Some(e.domain),
            Some(_) => return Err(D::Error::custom("bad e4mc domain")),
            None => None,
        };
        Ok(Self { turn, e4mc })
    }
}

/// Reads a TURN relay address a guest may connect to: `a.b.c.d:port` written canonically (no
/// leading zeros, port 1–65535) and a public IPv4 address.
pub fn parse_relay(text: &str) -> Option<SocketAddrV4> {
    let addr: SocketAddrV4 = text.parse().ok()?;
    (addr.to_string() == text && is_public_relay(addr)).then_some(addr)
}

/// Whether a relay address is a public IPv4 address with a port (Cloudflare's relays are). Private,
/// loopback, link-local, shared, multicast, broadcast, unspecified, documentation and other
/// reserved ranges are refused.
pub fn is_public_relay(addr: SocketAddrV4) -> bool {
    addr.port() != 0 && is_public_ipv4(*addr.ip())
}

fn is_public_ipv4(ip: Ipv4Addr) -> bool {
    let [a, b, c, _] = ip.octets();
    !(ip.is_unspecified()
        || ip.is_loopback()
        || ip.is_private()
        || ip.is_link_local()
        || ip.is_multicast()
        || ip.is_broadcast()
        || ip.is_documentation()
        // 0.0.0.0/8 "this network"
        || a == 0
        // 100.64.0.0/10 shared address space
        || (a == 100 && (64..128).contains(&b))
        // 192.0.0.0/24 IETF protocol assignments, 192.88.99.0/24 6to4 relays
        || (a == 192 && b == 0 && c == 0)
        || (a == 192 && b == 88 && c == 99)
        // 198.18.0.0/15 benchmarking
        || (a == 198 && (b == 18 || b == 19))
        // 240.0.0.0/4 reserved
        || a >= 240)
}

/// Whether `domain` is an e4mc player address: `^[a-z0-9-]{1,63}\.[a-z0-9-]{1,63}\.e4mc\.link$`, at
/// most 253 characters.
pub fn is_e4mc_domain(domain: &str) -> bool {
    if domain.len() > MAX_DOMAIN_LEN {
        return false;
    }
    let Some(rest) = domain.strip_suffix(".e4mc.link") else {
        return false;
    };
    let mut labels = rest.split('.');
    let label_ok = |label: &str| {
        (1..=63).contains(&label.len())
            && label
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
    };
    matches!((labels.next(), labels.next(), labels.next()), (Some(name), Some(region), None)
        if label_ok(name) && label_ok(region))
}

/// What a host sends one invitee (`world_invite.join`). The service delivers it unread.
#[derive(Clone, PartialEq, Eq)]
pub struct Join {
    pub session: SessionId,
    /// The host session's X25519 public key.
    pub key: [u8; 32],
    pub slot: Slot,
    pub psk: Psk,
    /// The host's Minecraft version.
    pub mc: String,
    /// The host game's protocol number (`host-published.proto`).
    pub proto: i32,
    /// The official pack's family id the host's install follows.
    pub pack: Option<String>,
    /// At least one ready path.
    pub paths: Paths,
}

impl fmt::Debug for Join {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Join")
            .field("session", &self.session)
            .field("mc", &self.mc)
            .field("paths", &self.paths)
            .finish_non_exhaustive()
    }
}

#[derive(Serialize)]
struct JoinOut<'a> {
    v: u32,
    session: &'a SessionId,
    key: String,
    slot: &'a Slot,
    psk: &'a Psk,
    mc: &'a str,
    proto: i32,
    #[serde(skip_serializing_if = "Option::is_none")]
    pack: Option<&'a str>,
    paths: &'a Paths,
}

#[derive(Deserialize)]
struct JoinIn {
    v: u32,
    session: SessionId,
    key: String,
    slot: Slot,
    psk: Psk,
    mc: String,
    proto: i32,
    #[serde(default)]
    pack: Option<String>,
    paths: Paths,
}

/// Why a `join` was dropped.
#[derive(Debug, Clone, Copy, PartialEq, Eq, thiserror::Error)]
pub enum JoinError {
    /// `v` is not one this launcher knows: show nothing.
    #[error("unknown invite version")]
    UnknownVersion,
    /// Too long, not JSON, a field missing or malformed, or an address a guest must not use.
    #[error("malformed invite")]
    Malformed,
}

impl Join {
    /// The compact JSON the host puts into `world_invite.join`.
    pub fn to_value(&self) -> serde_json::Value {
        serde_json::to_value(self.wire()).expect("join serializes")
    }

    /// The compact JSON text.
    pub fn to_json(&self) -> String {
        serde_json::to_string(&self.wire()).expect("join serializes")
    }

    fn wire(&self) -> JoinOut<'_> {
        JoinOut {
            v: JOIN_VERSION,
            session: &self.session,
            key: b64u(&self.key),
            slot: &self.slot,
            psk: &self.psk,
            mc: &self.mc,
            proto: self.proto,
            pack: self.pack.as_deref(),
            paths: &self.paths,
        }
    }

    /// Reads a received `join` and checks it (§4, §5.1).
    pub fn parse(value: &serde_json::Value) -> Result<Self, JoinError> {
        let version = value.get("v").and_then(serde_json::Value::as_u64);
        if version != Some(u64::from(JOIN_VERSION)) {
            return Err(if value.is_object() {
                JoinError::UnknownVersion
            } else {
                JoinError::Malformed
            });
        }
        if serde_json::to_vec(value).map_or(true, |bytes| bytes.len() > MAX_JOIN_BYTES) {
            return Err(JoinError::Malformed);
        }
        let wire = JoinIn::deserialize(value).map_err(|_| JoinError::Malformed)?;
        debug_assert_eq!(wire.v, JOIN_VERSION);
        let key = from_b64u(&wire.key).ok_or(JoinError::Malformed)?;
        if wire.paths.is_empty() || wire.mc.is_empty() || wire.mc.len() > 32 {
            return Err(JoinError::Malformed);
        }
        Ok(Self {
            session: wire.session,
            key,
            slot: wire.slot,
            psk: wire.psk,
            mc: wire.mc,
            proto: wire.proto,
            pack: wire.pack,
            paths: wire.paths,
        })
    }
}
