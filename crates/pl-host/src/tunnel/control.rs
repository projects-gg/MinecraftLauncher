//! Control messages (PROTOCOL.md §6.4): UTF-8 JSON objects carried in `ctrl` records, `t` naming
//! the message. Fields go out in the order the protocol lists them, so the bytes match the test
//! vectors; receivers ignore fields they do not know.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::ids::{GuestId, StreamId};
use super::invite::Paths;
use crate::manifest::Manifest;
use crate::transport::PathKind;

/// `hello.v` this launcher speaks.
pub const HELLO_VERSION: u32 = 1;

/// The guest's first message on every stream.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Hello {
    /// Opening a control stream (a second one from the same slot replaces the first).
    Control {
        /// The in-game name the guest will log in with.
        name: String,
        /// A Microsoft account: the game authenticates with Mojang.
        premium: bool,
        /// The guest's launcher version.
        launcher: String,
    },
    /// Opening a game stream, after the game connected locally.
    Game {
        name: String,
        premium: bool,
        /// From the install and the game's handshake packet; `proto` 0 when not known.
        mc: String,
        proto: i32,
    },
    /// Continuing a game stream on another stream.
    Resume {
        stream: StreamId,
        /// Bytes of the host's direction the guest received in order.
        recv: u64,
    },
}

/// The host's verdict on a control stream's name (`ok.name`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum NameVerdict {
    Ok,
    /// Not 3–16 of `[A-Za-z0-9_]`, the host's own name, or already in the world.
    Name,
    /// Bound to another friend in this world.
    NameTaken,
}

/// Why the host refused a stream (`no.reason`, PROTOCOL.md §6.9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Refusal {
    Full,
    Closed,
    Removed,
    Name,
    NameTaken,
    Version,
    Busy,
    Resume,
    /// `hello.v` or `hello.kind` unknown.
    Unsupported,
}

impl Refusal {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Full => "full",
            Self::Closed => "closed",
            Self::Removed => "removed",
            Self::Name => "name",
            Self::NameTaken => "name-taken",
            Self::Version => "version",
            Self::Busy => "busy",
            Self::Resume => "resume",
            Self::Unsupported => "unsupported",
        }
    }

    /// Reads `no.reason`; unknown reasons read as `busy`.
    pub fn from_wire(reason: &str) -> Self {
        match reason {
            "full" => Self::Full,
            "closed" => Self::Closed,
            "removed" => Self::Removed,
            "name" => Self::Name,
            "name-taken" => Self::NameTaken,
            "version" => Self::Version,
            "resume" => Self::Resume,
            "unsupported" => Self::Unsupported,
            _ => Self::Busy,
        }
    }

    /// The telemetry code (§6.9).
    pub fn code(self) -> &'static str {
        match self {
            Self::Full => "plj_full",
            Self::Closed => "plj_closed",
            Self::Removed => "plj_removed",
            Self::Name | Self::NameTaken => "plj_name",
            Self::Version => "plj_version",
            Self::Busy => "plj_busy",
            Self::Resume => "resume_failed",
            Self::Unsupported => "plj_error",
        }
    }
}

/// Why the host moves a guest's streams (`move.why`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum MoveWhy {
    /// The month's TURN budget (98 %).
    Budget,
    /// A session or player TURN limit.
    Cap,
    /// TURN credential renewal.
    Renew,
    /// The path failed.
    Failure,
    /// TURN switched off (or a leak).
    Off,
}

impl MoveWhy {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Budget => "budget",
            Self::Cap => "cap",
            Self::Renew => "renew",
            Self::Failure => "failure",
            Self::Off => "off",
        }
    }

    /// `turn_move.reason` (`budget`, `cap`, `off`).
    pub fn from_turn_move(reason: &str) -> Self {
        match reason {
            "budget" => Self::Budget,
            "cap" => Self::Cap,
            _ => Self::Off,
        }
    }
}

/// `bye.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ByeReason {
    /// The session ended (host).
    Closed,
    /// This guest was removed (host).
    Removed,
    /// The guest leaves.
    Leave,
}

/// `close.reason`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum CloseReason {
    Eof,
    Error,
}

/// The host's `ok`. Its fields depend on the stream: control (`guest`, `stream`, `name`, `caps`),
/// game (`guest`, `stream`, `caps`) or resume (`stream`, `recv`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Accept {
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub guest: Option<GuestId>,
    pub stream: StreamId,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub name: Option<NameVerdict>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub recv: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none", default)]
    pub caps: Option<Vec<String>>,
}

impl Accept {
    pub fn control(guest: GuestId, stream: StreamId, name: NameVerdict) -> Self {
        Self {
            guest: Some(guest),
            stream,
            name: Some(name),
            recv: None,
            caps: Some(Vec::new()),
        }
    }

    pub fn game(guest: GuestId, stream: StreamId) -> Self {
        Self {
            guest: Some(guest),
            stream,
            name: None,
            recv: None,
            caps: Some(Vec::new()),
        }
    }

    pub fn resume(stream: StreamId, recv: u64) -> Self {
        Self {
            guest: None,
            stream,
            name: None,
            recv: Some(recv),
            caps: None,
        }
    }
}

/// `state`: the world's guest count, limit and paths.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct WorldState {
    /// Guests in the world now, the host not counted.
    pub guests: u8,
    /// The guest limit (1–7).
    pub max: u8,
    pub paths: Paths,
}

/// One control message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Control {
    Hello(Hello),
    Ok(Accept),
    No(Refusal),
    Manifest(Box<Manifest>),
    State(WorldState),
    Move {
        to: PathKind,
        why: MoveWhy,
    },
    Bye(ByeReason),
    /// The local game connection ended: deliver up to `at` bytes, then close it.
    Close {
        reason: CloseReason,
        at: u64,
    },
}

/// A control message that cannot be used.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum ControlError {
    /// Not a JSON object with a string `t`.
    #[error("not a control message")]
    NotControl,
    /// A `t` this version does not know; the receiver ignores it.
    #[error("unknown control message")]
    Unknown(String),
    /// A `hello` with a `v` or `kind` this version does not speak (`no{unsupported}`).
    #[error("unsupported hello")]
    UnsupportedHello,
    /// A known message with missing or malformed fields.
    #[error("malformed {0} message")]
    Malformed(&'static str),
}

#[derive(Serialize)]
struct HelloControlOut<'a> {
    t: &'static str,
    v: u32,
    kind: &'static str,
    name: &'a str,
    premium: bool,
    launcher: &'a str,
    caps: [&'static str; 0],
}

#[derive(Serialize)]
struct HelloGameOut<'a> {
    t: &'static str,
    v: u32,
    kind: &'static str,
    name: &'a str,
    premium: bool,
    mc: &'a str,
    proto: i32,
    caps: [&'static str; 0],
}

#[derive(Serialize)]
struct HelloResumeOut {
    t: &'static str,
    v: u32,
    kind: &'static str,
    stream: StreamId,
    recv: u64,
    caps: [&'static str; 0],
}

#[derive(Serialize)]
struct Tagged<'a, T: Serialize> {
    t: &'static str,
    #[serde(flatten)]
    body: &'a T,
}

#[derive(Serialize)]
struct NoOut {
    t: &'static str,
    reason: &'static str,
}

#[derive(Serialize)]
struct MoveOut {
    t: &'static str,
    to: &'static str,
    why: MoveWhy,
}

#[derive(Serialize)]
struct ByeOut {
    t: &'static str,
    reason: ByeReason,
}

#[derive(Serialize)]
struct CloseOut {
    t: &'static str,
    reason: CloseReason,
    at: u64,
}

#[derive(Deserialize)]
struct HelloIn {
    v: Value,
    kind: Value,
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    premium: Option<bool>,
    #[serde(default)]
    launcher: Option<String>,
    #[serde(default)]
    mc: Option<String>,
    #[serde(default)]
    proto: Option<i32>,
    #[serde(default)]
    stream: Option<StreamId>,
    #[serde(default)]
    recv: Option<u64>,
}

#[derive(Deserialize)]
struct NoIn {
    reason: String,
}

#[derive(Deserialize)]
struct MoveIn {
    to: String,
    why: String,
}

#[derive(Deserialize)]
struct ByeIn {
    reason: String,
}

#[derive(Deserialize)]
struct CloseIn {
    reason: String,
    at: u64,
}

impl Control {
    /// The message as compact UTF-8 JSON, fields in protocol order.
    pub fn to_json(&self) -> Vec<u8> {
        let out = match self {
            Self::Hello(Hello::Control {
                name,
                premium,
                launcher,
            }) => serde_json::to_vec(&HelloControlOut {
                t: "hello",
                v: HELLO_VERSION,
                kind: "control",
                name,
                premium: *premium,
                launcher,
                caps: [],
            }),
            Self::Hello(Hello::Game {
                name,
                premium,
                mc,
                proto,
            }) => serde_json::to_vec(&HelloGameOut {
                t: "hello",
                v: HELLO_VERSION,
                kind: "game",
                name,
                premium: *premium,
                mc,
                proto: *proto,
                caps: [],
            }),
            Self::Hello(Hello::Resume { stream, recv }) => serde_json::to_vec(&HelloResumeOut {
                t: "hello",
                v: HELLO_VERSION,
                kind: "resume",
                stream: *stream,
                recv: *recv,
                caps: [],
            }),
            Self::Ok(accept) => serde_json::to_vec(&Tagged {
                t: "ok",
                body: accept,
            }),
            Self::No(reason) => serde_json::to_vec(&NoOut {
                t: "no",
                reason: reason.as_str(),
            }),
            Self::Manifest(manifest) => serde_json::to_vec(manifest.as_ref()),
            Self::State(state) => serde_json::to_vec(&Tagged {
                t: "state",
                body: state,
            }),
            Self::Move { to, why } => serde_json::to_vec(&MoveOut {
                t: "move",
                to: to.as_str(),
                why: *why,
            }),
            Self::Bye(reason) => serde_json::to_vec(&ByeOut {
                t: "bye",
                reason: *reason,
            }),
            Self::Close { reason, at } => serde_json::to_vec(&CloseOut {
                t: "close",
                reason: *reason,
                at: *at,
            }),
        };
        out.expect("control messages serialize")
    }

    /// Reads a control message.
    pub fn parse(json: &[u8]) -> Result<Self, ControlError> {
        let value: Value = serde_json::from_slice(json).map_err(|_| ControlError::NotControl)?;
        let kind = value
            .as_object()
            .and_then(|o| o.get("t"))
            .and_then(Value::as_str)
            .ok_or(ControlError::NotControl)?
            .to_owned();
        let malformed = |what| move |_| ControlError::Malformed(what);
        match kind.as_str() {
            "hello" => parse_hello(value).map(Self::Hello),
            "ok" => Accept::deserialize(value)
                .map(Self::Ok)
                .map_err(malformed("ok")),
            "no" => {
                let no = NoIn::deserialize(value).map_err(malformed("no"))?;
                Ok(Self::No(Refusal::from_wire(&no.reason)))
            }
            "manifest" => Manifest::deserialize(value)
                .map(|m| Self::Manifest(Box::new(m)))
                .map_err(malformed("manifest")),
            "state" => WorldState::deserialize(value)
                .map(Self::State)
                .map_err(malformed("state")),
            "move" => {
                let m = MoveIn::deserialize(value).map_err(malformed("move"))?;
                let to = match m.to.as_str() {
                    "turn" => PathKind::Turn,
                    "e4mc" => PathKind::E4mc,
                    _ => return Err(ControlError::Malformed("move")),
                };
                let why = match m.why.as_str() {
                    "budget" => MoveWhy::Budget,
                    "cap" => MoveWhy::Cap,
                    "renew" => MoveWhy::Renew,
                    "off" => MoveWhy::Off,
                    // Reasons added later read as a failure of the path.
                    _ => MoveWhy::Failure,
                };
                Ok(Self::Move { to, why })
            }
            "bye" => {
                let bye = ByeIn::deserialize(value).map_err(malformed("bye"))?;
                let reason = match bye.reason.as_str() {
                    "removed" => ByeReason::Removed,
                    "leave" => ByeReason::Leave,
                    _ => ByeReason::Closed,
                };
                Ok(Self::Bye(reason))
            }
            "close" => {
                let close = CloseIn::deserialize(value).map_err(malformed("close"))?;
                let reason = if close.reason == "eof" {
                    CloseReason::Eof
                } else {
                    CloseReason::Error
                };
                Ok(Self::Close {
                    reason,
                    at: close.at,
                })
            }
            _ => Err(ControlError::Unknown(kind)),
        }
    }
}

fn parse_hello(value: Value) -> Result<Hello, ControlError> {
    let hello = HelloIn::deserialize(value).map_err(|_| ControlError::UnsupportedHello)?;
    if hello.v.as_u64() != Some(u64::from(HELLO_VERSION)) {
        return Err(ControlError::UnsupportedHello);
    }
    let malformed = ControlError::Malformed("hello");
    match hello.kind.as_str() {
        Some("control") => Ok(Hello::Control {
            name: hello.name.ok_or(malformed.clone())?,
            premium: hello.premium.ok_or(malformed.clone())?,
            launcher: hello.launcher.unwrap_or_default(),
        }),
        Some("game") => Ok(Hello::Game {
            name: hello.name.ok_or(malformed.clone())?,
            premium: hello.premium.ok_or(malformed.clone())?,
            mc: hello.mc.unwrap_or_default(),
            proto: hello.proto.unwrap_or(0),
        }),
        Some("resume") => Ok(Hello::Resume {
            stream: hello.stream.ok_or(malformed.clone())?,
            recv: hello.recv.ok_or(malformed)?,
        }),
        _ => Err(ControlError::UnsupportedHello),
    }
}
