//! The room socket's frames (`GET /v1/voice?ticket=…`; the friends service's README "Voice
//! (VOICE-S1)" → Room socket, its types in `services/social/src/voice-wire.ts`), as the launcher
//! sends and reads them.
//!
//! - [`ToRoom`]: client → room (`join`, `sync`, `answer`, `set`, `q`, `turn`, `invite`, `cancel`,
//!   `mod`, `leave`). The first frame is `join` or `sync`.
//! - [`FromRoom`]: room → client. A message this launcher does not know reads as
//!   [`FromRoom::Unknown`] and changes nothing (a newer service may say more).
//!
//! Tickets and SDP never show in `Debug` output ([`Ticket`], [`Sdp`]): no log line holds them.

use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Deserializer, Serialize};

/// A room ticket (`v1.<payload>.<mac>`): opaque, handed back as it came, never logged.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Ticket(String);

impl Ticket {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Ticket {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<ticket>")
    }
}

/// An SDP offer or answer: passed between the voice page and the room unread, never logged.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Sdp(String);

impl Sdp {
    pub fn new(text: impl Into<String>) -> Self {
        Self(text.into())
    }

    pub fn expose(&self) -> &str {
        &self.0
    }

    pub fn into_inner(self) -> String {
        self.0
    }
}

impl fmt::Debug for Sdp {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<sdp {} chars>", self.0.len())
    }
}

/// A role in a room: `l` leader, `m` moderator, `u` member.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum Role {
    #[serde(rename = "l")]
    Leader,
    #[serde(rename = "m")]
    Moderator,
    #[default]
    #[serde(rename = "u", other)]
    Member,
}

/// `f` a friend room, `c` a crew room.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum RoomKind {
    #[default]
    #[serde(rename = "f")]
    Friend,
    #[serde(rename = "c")]
    Crew,
}

impl RoomKind {
    /// A room's kind from its id (`f…` friend; `g…` and `c…` crew).
    pub fn of(room: &str) -> Self {
        if room.starts_with('f') {
            Self::Friend
        } else {
            Self::Crew
        }
    }
}

/// Whether `room` is a room id the service gives (`[fgc]` + 16 hex digits).
pub fn is_room_id(room: &str) -> bool {
    let bytes = room.as_bytes();
    bytes.len() == 17
        && matches!(bytes[0], b'f' | b'g' | b'c')
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}

/// The id of a crew's "Genel" room.
pub fn general_room_of(crew: &str) -> String {
    format!("g{crew}")
}

/// A connection quality as launchers report it: 0 good, 1 weak, 2 bad.
pub fn quality(value: u8) -> u8 {
    value.min(2)
}

/// `mod.act`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModAct {
    Mute,
    Unmute,
    Kick,
    Unkick,
    Move,
    Mod,
    Unmod,
    Name,
    Close,
    /// SHARE: stop someone's share for everyone.
    Unshare,
}

/// Client → room.
#[derive(Clone, PartialEq, Eq, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum ToRoom {
    /// A fresh peer connection: `sdp` publishes the microphone as one track (none: listen only);
    /// `mid` the audio m-line's mid.
    Join {
        #[serde(skip_serializing_if = "Option::is_none")]
        sdp: Option<Sdp>,
        #[serde(skip_serializing_if = "Option::is_none")]
        mid: Option<String>,
        /// SHARE: what this launcher understands beyond voice (`share1`); an older room ignores it.
        #[serde(skip_serializing_if = "Vec::is_empty")]
        caps: Vec<String>,
    },
    /// Back with the resume ticket, the peer connection still working.
    Sync {
        /// SHARE: as `join`'s (a new socket is a new attachment for the room).
        #[serde(skip_serializing_if = "Vec::is_empty")]
        caps: Vec<String>,
    },
    /// The answer to the room's `offer`.
    Answer {
        sdp: Sdp,
    },
    /// The microphone and deafen state (also after a `sync`); never answered.
    Set {
        mute: bool,
        deaf: bool,
    },
    /// The connection's quality when it changes; never answered.
    Q {
        v: u8,
    },
    /// Voice's own TURN credentials (only after ICE failed for 5 s).
    Turn {
        rid: u32,
    },
    Invite {
        to: String,
        rid: u32,
    },
    /// Takes the player's ringing invite back.
    Cancel {
        to: String,
        rid: u32,
    },
    Mod {
        act: ModAct,
        #[serde(skip_serializing_if = "Option::is_none")]
        to: Option<String>,
        /// `move`: another live room of the crew.
        #[serde(skip_serializing_if = "Option::is_none")]
        room: Option<String>,
        /// `name`: the new name.
        #[serde(skip_serializing_if = "Option::is_none")]
        name: Option<String>,
        /// SHARE: `unshare`: which of their shares.
        #[serde(skip_serializing_if = "Option::is_none")]
        n: Option<u8>,
        rid: u32,
    },
    Leave,
    // SHARE (`share1`, the lead's contract share.md §4) -----------------------------------------
    /// A new share: the publisher's offer for its own SFU session (`ack.sdp` answers it).
    ShareOpen {
        n: u8,
        src: crate::share::ShareKind,
        audio: bool,
        preset: crate::share::SharePreset,
        fps: u8,
        sdp: Sdp,
        rid: u32,
    },
    /// Nobody watches: encoding rests (or goes on).
    ShareSet {
        n: u8,
        paused: bool,
    },
    ShareClose {
        n: u8,
    },
    /// Watch someone's share (`woffer` follows).
    Watch {
        u: String,
        n: u8,
        layer: crate::share::ShareLayer,
        rid: u32,
    },
    Layer {
        u: String,
        n: u8,
        layer: crate::share::ShareLayer,
    },
    Unwatch {
        u: String,
        n: u8,
    },
    /// The watching page's answer to `woffer`.
    Wanswer {
        sdp: Sdp,
    },
}

impl fmt::Debug for ToRoom {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only the kind: frames carry SDP and ids.
        f.write_str(self.kind())
    }
}

impl ToRoom {
    pub fn kind(&self) -> &'static str {
        match self {
            Self::Join { .. } => "join",
            Self::Sync { .. } => "sync",
            Self::Answer { .. } => "answer",
            Self::Set { .. } => "set",
            Self::Q { .. } => "q",
            Self::Turn { .. } => "turn",
            Self::Invite { .. } => "invite",
            Self::Cancel { .. } => "cancel",
            Self::Mod { .. } => "mod",
            Self::Leave => "leave",
            Self::ShareOpen { .. } => "share_open",
            Self::ShareSet { .. } => "share_set",
            Self::ShareClose { .. } => "share_close",
            Self::Watch { .. } => "watch",
            Self::Layer { .. } => "layer",
            Self::Unwatch { .. } => "unwatch",
            Self::Wanswer { .. } => "wanswer",
        }
    }

    /// The frame's `rid`, for frames the room answers.
    pub fn rid(&self) -> Option<u32> {
        match self {
            Self::Turn { rid } => Some(*rid),
            Self::Invite { rid, .. }
            | Self::Cancel { rid, .. }
            | Self::Mod { rid, .. }
            | Self::ShareOpen { rid, .. }
            | Self::Watch { rid, .. } => Some(*rid),
            _ => None,
        }
    }

    /// The JSON text frame.
    pub fn text(&self) -> String {
        serde_json::to_string(self).unwrap_or_default()
    }
}

/// A person in a room as others see them (`hello.people` holds the player too).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PersonWire {
    pub u: String,
    /// When they came in (Unix seconds): the order of "longest present".
    #[serde(default)]
    pub j: i64,
    #[serde(default)]
    pub mute: bool,
    #[serde(default)]
    pub deaf: bool,
    /// Muted for everyone: nobody plays them.
    #[serde(default)]
    pub sm: bool,
    #[serde(default, deserialize_with = "quality_value")]
    pub q: u8,
    /// VOICE-S2: their card as the friends list shows one, as it was when they came in (no crew
    /// tag), so a friend's friend is named; only people the player sees are ever sent. None from
    /// an older room, and for a card this launcher cannot read (the person still counts).
    #[serde(default, deserialize_with = "card_value")]
    pub card: Option<Box<pl_social::PublicProfile>>,
    /// SHARE: their live shares (only to `share1` sockets).
    #[serde(default, deserialize_with = "crate::share::shares_list")]
    pub sh: Vec<crate::share::ShareWire>,
}

/// `hello.room` (the `room` message carries the name and roles again).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RoomWire {
    pub id: String,
    #[serde(default)]
    pub kind: RoomKind,
    #[serde(default)]
    pub crew: Option<String>,
    /// Crew rooms: always; friend rooms: none until the leader names it.
    #[serde(default)]
    pub name: Option<String>,
    /// Crew rooms: the crew's leader (none while it has none); friend rooms: none for a viewer
    /// blocked with the leader.
    #[serde(default)]
    pub leader: Option<String>,
    #[serde(default)]
    pub mods: Vec<String>,
    #[serde(default)]
    pub max: u16,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
pub struct MeWire {
    #[serde(default)]
    pub role: Role,
}

/// One of the player's own invites still ringing.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct RingWire {
    pub to: String,
    /// Unix seconds.
    #[serde(default)]
    pub until: i64,
}

/// Voice's own TURN credentials (`turn` → `ack.ice`): never logged.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct IceWire {
    #[serde(default)]
    pub urls: Vec<String>,
    #[serde(default)]
    pub username: String,
    #[serde(default)]
    pub credential: String,
    #[serde(default)]
    pub ttl: u64,
}

impl fmt::Debug for IceWire {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<ice {} urls>", self.urls.len())
    }
}

/// `left.why`: someone else left.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum LeftWhy {
    /// Their connection did not come back.
    Gone,
    Kicked,
    Moved,
    /// Themselves (or a block that cannot be told); anything unknown too.
    #[default]
    #[serde(other)]
    Left,
}

/// `closed.why`: the player's stay ended. Anything unknown reads like `off`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClosedWhy {
    Leader,
    Alone,
    NoAnswer,
    Elsewhere,
    Crew,
    Budget,
    Staff,
    Account,
    /// BAN-TYPES: Projects' moderators restricted the player's voice chat (until `closed.until`).
    Muted,
    #[serde(other)]
    Off,
}

/// `ring_end.why` (to the one who invited); anything unknown reads like `no_answer`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RingEndWhy {
    Joined,
    Cancelled,
    #[default]
    #[serde(other)]
    NoAnswer,
}

/// Room → client.
#[derive(Debug, Clone, PartialEq, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub enum FromRoom {
    Hello {
        room: RoomWire,
        #[serde(default)]
        me: MeWire,
        #[serde(default)]
        people: Vec<PersonWire>,
        #[serde(default)]
        rings: Vec<RingWire>,
        /// The SFU's answer to `join`'s offer (none after `sync`, or a listen-only join).
        #[serde(default)]
        sdp: Option<Sdp>,
        /// The resume ticket for this seat.
        #[serde(default)]
        resume: Option<Ticket>,
        /// The service's clock (Unix seconds).
        #[serde(default)]
        now: Option<i64>,
        /// SHARE: sharing in this room (only to `share1` sockets).
        #[serde(default, deserialize_with = "crate::share::room_share")]
        share: Option<crate::share::RoomShareWire>,
    },
    /// The SFU's offer: people to receive (`mids`: mid → person) or receivers closed.
    Offer {
        sdp: Sdp,
        #[serde(default)]
        mids: BTreeMap<String, String>,
    },
    Joined {
        person: PersonWire,
    },
    Left {
        u: String,
        #[serde(default)]
        why: LeftWhy,
    },
    /// Someone's `mute`, `deaf`, `sm` or `q` changed (the player's too).
    State {
        u: String,
        #[serde(default)]
        mute: Option<bool>,
        #[serde(default)]
        deaf: Option<bool>,
        #[serde(default)]
        sm: Option<bool>,
        #[serde(default, deserialize_with = "quality_option")]
        q: Option<u8>,
        /// SHARE: their whole list of live shares, when it changed.
        #[serde(default, deserialize_with = "crate::share::shares_option")]
        sh: Option<Vec<crate::share::ShareWire>>,
    },
    /// The room's name or roles changed (VOICE-ROOMS: a crew room's `max` too, which the crew may
    /// change; none from an older service).
    Room {
        #[serde(default)]
        name: Option<String>,
        #[serde(default)]
        leader: Option<String>,
        #[serde(default)]
        mods: Vec<String>,
        #[serde(default)]
        max: Option<u16>,
    },
    /// The player's role changed.
    Me {
        #[serde(default)]
        role: Role,
    },
    /// The player's invite rings.
    Ring {
        to: String,
        #[serde(default)]
        until: i64,
    },
    /// The player's invite ended.
    RingEnd {
        to: String,
        #[serde(default)]
        why: RingEndWhy,
    },
    Kicked,
    /// Join that room of the crew with this ticket now.
    Moved {
        room: String,
        ticket: Ticket,
    },
    Closed {
        why: ClosedWhy,
        /// BAN-TYPES: `muted`'s end (Unix seconds).
        #[serde(default)]
        until: Option<i64>,
    },
    Ack {
        #[serde(default)]
        rid: Option<u32>,
        /// `turn`.
        #[serde(default)]
        ice: Option<IceWire>,
        /// `invite`.
        #[serde(default)]
        until: Option<i64>,
        /// SHARE: `share_open`: the SFU's answer, for share `n`.
        #[serde(default)]
        sdp: Option<Sdp>,
        #[serde(default)]
        n: Option<u8>,
    },
    Error {
        #[serde(default)]
        rid: Option<u32>,
        code: String,
        #[serde(default, rename = "retryAfter")]
        retry_after: Option<u64>,
        #[serde(default)]
        until: Option<i64>,
        #[serde(default)]
        item: Option<String>,
    },
    // SHARE --------------------------------------------------------------------------------------
    /// Who watches one of the player's shares.
    Watchers {
        n: u8,
        #[serde(default)]
        ids: Vec<String>,
    },
    /// The SFU's offer for the player's watch session (`mids`: mid → share and kind).
    Woffer {
        sdp: Sdp,
        #[serde(default, deserialize_with = "crate::share::watch_mids")]
        mids: BTreeMap<String, crate::share::WatchMidWire>,
    },
    /// One of the player's shares ended without them.
    ShareClosed {
        n: u8,
        #[serde(default)]
        why: crate::share::ShareClosedWhy,
    },
    #[serde(other)]
    Unknown,
}

impl FromRoom {
    /// A text frame from the room; `pong` (the keep-alive's answer) and anything unreadable read
    /// as [`FromRoom::Unknown`].
    pub fn parse(text: &str) -> Self {
        serde_json::from_str(text).unwrap_or(Self::Unknown)
    }
}

/// The room socket's close codes (`ROOM_CLOSE`; 1000 follows `kicked`, `moved` and `closed`).
pub mod close {
    /// The same device came back with its resume ticket: do not reconnect this one.
    pub const REPLACED: u16 = 4000;
    /// The session ended (signed out, revoked) or the account was restricted or deleted.
    pub const SESSION_ENDED: u16 = 4001;
    /// A malformed frame, or a frame before `join`/`sync`.
    pub const PROTOCOL: u16 = 4003;
    /// Too many frames: wait at least a minute.
    pub const RATE_LIMITED: u16 = 4008;
}

/// The room socket's limits (`ROOM_LIMITS`).
pub mod limits {
    /// Client frames: at most this many characters (SDP).
    pub const MAX_FRAME_CHARS: usize = 16_384;
    /// A dropped socket keeps its seat this long; then the stay is over.
    pub const SEAT_SECONDS: u64 = 120;
    /// After a 4008 close, the wait before the socket comes back.
    pub const RATE_LIMITED_WAIT_SECONDS: u64 = 60;
}

fn quality_value<'de, D: Deserializer<'de>>(deserializer: D) -> Result<u8, D::Error> {
    Ok(quality(u8::deserialize(deserializer).unwrap_or(0)))
}

fn quality_option<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<u8>, D::Error> {
    Ok(Option::<u8>::deserialize(deserializer)?.map(quality))
}

/// A card that does not read (a newer shape, a wrong type) is no card: it never costs the person
/// or the whole `hello`.
fn card_value<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Box<pl_social::PublicProfile>>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .and_then(|value| serde_json::from_value(value).ok())
        .map(Box::new))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn value(frame: &ToRoom) -> Value {
        serde_json::from_str(&frame.text()).unwrap()
    }

    /// The client frames exactly as the README's "Client → room" writes them.
    #[test]
    fn client_frames_as_the_contract_writes_them() {
        let join = ToRoom::Join {
            sdp: Some(Sdp::new("v=0")),
            mid: Some("0".into()),
            caps: Vec::new(),
        };
        assert_eq!(value(&join), json!({"t":"join","sdp":"v=0","mid":"0"}));
        let listen = ToRoom::Join {
            sdp: None,
            mid: None,
            caps: Vec::new(),
        };
        assert_eq!(value(&listen), json!({"t":"join"}));
        assert_eq!(
            value(&ToRoom::Sync { caps: Vec::new() }),
            json!({"t":"sync"})
        );
        assert_eq!(
            value(&ToRoom::Answer {
                sdp: Sdp::new("v=0 a")
            }),
            json!({"t":"answer","sdp":"v=0 a"})
        );
        assert_eq!(
            value(&ToRoom::Set {
                mute: true,
                deaf: false
            }),
            json!({"t":"set","mute":true,"deaf":false})
        );
        assert_eq!(value(&ToRoom::Q { v: 1 }), json!({"t":"q","v":1}));
        assert_eq!(value(&ToRoom::Turn { rid: 2 }), json!({"t":"turn","rid":2}));
        assert_eq!(
            value(&ToRoom::Invite {
                to: "u1".into(),
                rid: 3
            }),
            json!({"t":"invite","to":"u1","rid":3})
        );
        assert_eq!(
            value(&ToRoom::Cancel {
                to: "u1".into(),
                rid: 4
            }),
            json!({"t":"cancel","to":"u1","rid":4})
        );
        assert_eq!(
            value(&ToRoom::Mod {
                act: ModAct::Mute,
                to: Some("u2".into()),
                room: None,
                name: None,
                n: None,
                rid: 5
            }),
            json!({"t":"mod","act":"mute","to":"u2","rid":5})
        );
        assert_eq!(
            value(&ToRoom::Mod {
                act: ModAct::Move,
                to: Some("u2".into()),
                room: Some("c0123456789abcdef".into()),
                name: None,
                n: None,
                rid: 6
            }),
            json!({"t":"mod","act":"move","to":"u2","room":"c0123456789abcdef","rid":6})
        );
        assert_eq!(
            value(&ToRoom::Mod {
                act: ModAct::Name,
                to: None,
                room: None,
                name: Some("Gece".into()),
                n: None,
                rid: 7
            }),
            json!({"t":"mod","act":"name","name":"Gece","rid":7})
        );
        assert_eq!(
            value(&ToRoom::Mod {
                act: ModAct::Close,
                to: None,
                room: None,
                name: None,
                n: None,
                rid: 8
            }),
            json!({"t":"mod","act":"close","rid":8})
        );
        assert_eq!(value(&ToRoom::Leave), json!({"t":"leave"}));
        assert_eq!(ToRoom::Turn { rid: 9 }.rid(), Some(9));
        assert_eq!(ToRoom::Leave.rid(), None);
    }

    #[test]
    fn hello_reads_as_the_room_sends_it() {
        let text = r#"{"t":"hello","room":{"id":"f0123456789abcdef","kind":"f","name":null,
            "leader":"u1","mods":["u2"],"max":10},"me":{"role":"m"},"people":[
            {"u":"u1","j":100,"mute":false,"deaf":false,"sm":false,"q":0},
            {"u":"u2","j":101,"mute":true,"deaf":false,"sm":true,"q":2}],
            "rings":[{"to":"u3","until":130}],"sdp":"v=0 answer","resume":"v1.a.b","now":105}"#;
        let FromRoom::Hello {
            room,
            me,
            people,
            rings,
            sdp,
            resume,
            now,
            ..
        } = FromRoom::parse(text)
        else {
            panic!("not a hello");
        };
        assert_eq!(room.id, "f0123456789abcdef");
        assert_eq!(room.kind, RoomKind::Friend);
        assert_eq!(room.name, None);
        assert_eq!(room.leader.as_deref(), Some("u1"));
        assert_eq!(room.mods, vec!["u2".to_owned()]);
        assert_eq!(room.max, 10);
        assert_eq!(me.role, Role::Moderator);
        assert_eq!(people.len(), 2);
        assert!(people[1].sm && people[1].mute);
        assert_eq!(people[1].q, 2);
        assert_eq!(
            rings,
            vec![RingWire {
                to: "u3".into(),
                until: 130
            }]
        );
        assert_eq!(sdp.map(Sdp::into_inner).as_deref(), Some("v=0 answer"));
        assert_eq!(resume.as_ref().map(Ticket::expose), Some("v1.a.b"));
        assert_eq!(now, Some(105));
        // A crew room after `sync`: no SDP.
        let crew = r#"{"t":"hello","room":{"id":"gabc","kind":"c","crew":"abc","name":"Genel",
            "leader":null,"mods":[],"max":15},"me":{"role":"u"},"people":[],"rings":[],
            "resume":"v1.c.d","now":1}"#;
        let FromRoom::Hello { room, sdp, .. } = FromRoom::parse(crew) else {
            panic!("not a hello");
        };
        assert_eq!(room.kind, RoomKind::Crew);
        assert_eq!(room.crew.as_deref(), Some("abc"));
        assert_eq!(room.leader, None);
        assert_eq!(sdp, None);
    }

    #[test]
    fn room_messages_read_as_the_room_sends_them() {
        assert_eq!(
            FromRoom::parse(r#"{"t":"offer","sdp":"v=0 o","mids":{"1":"u2","2":"u3"}}"#),
            FromRoom::Offer {
                sdp: Sdp::new("v=0 o"),
                mids: BTreeMap::from([("1".into(), "u2".into()), ("2".into(), "u3".into())]),
            }
        );
        assert!(matches!(
            FromRoom::parse(r#"{"t":"joined","person":{"u":"u4","j":9,"mute":false,"deaf":true,"sm":false,"q":1}}"#),
            FromRoom::Joined { person } if person.u == "u4" && person.deaf && person.q == 1 && person.card.is_none()
        ));
        // VOICE-S2's card, as the friends list's.
        let FromRoom::Joined { person } = FromRoom::parse(
            r#"{"t":"joined","person":{"u":"77","j":9,"mute":false,"deaf":false,"sm":false,"q":0,
            "card":{"id":"77","username":"kumfirtinasi","displayName":"KumFirtinasi",
            "avatar":"discord","avatarHash":"abc","discordId":"80351110224678912","marks":["verified"]}}}"#,
        ) else {
            panic!("not a joined");
        };
        let card = person.card.expect("a card");
        assert_eq!(card.name(), "KumFirtinasi");
        assert!(card.discord_avatar_url(128).is_some());
        // A card this launcher cannot read costs nothing but the card.
        assert!(matches!(
            FromRoom::parse(r#"{"t":"joined","person":{"u":"u5","j":9,"card":{"id":7}}}"#),
            FromRoom::Joined { person } if person.u == "u5" && person.card.is_none()
        ));
        assert!(matches!(
            FromRoom::parse(r#"{"t":"joined","person":{"u":"u6","j":9,"card":"KumFirtinasi"}}"#),
            FromRoom::Joined { person } if person.u == "u6" && person.card.is_none()
        ));
        assert_eq!(
            FromRoom::parse(r#"{"t":"left","u":"u4","why":"gone"}"#),
            FromRoom::Left {
                u: "u4".into(),
                why: LeftWhy::Gone
            }
        );
        // A reason this launcher does not know reads as `left`.
        assert_eq!(
            FromRoom::parse(r#"{"t":"left","u":"u4","why":"vanished"}"#),
            FromRoom::Left {
                u: "u4".into(),
                why: LeftWhy::Left
            }
        );
        assert_eq!(
            FromRoom::parse(r#"{"t":"state","u":"u2","sm":true}"#),
            FromRoom::State {
                u: "u2".into(),
                mute: None,
                deaf: None,
                sm: Some(true),
                q: None,
                sh: None,
            }
        );
        assert_eq!(
            FromRoom::parse(r#"{"t":"state","u":"u2","mute":true,"deaf":false}"#),
            FromRoom::State {
                u: "u2".into(),
                mute: Some(true),
                deaf: Some(false),
                sm: None,
                q: None,
                sh: None,
            }
        );
        assert_eq!(
            FromRoom::parse(r#"{"t":"state","u":"u2","q":7}"#),
            FromRoom::State {
                u: "u2".into(),
                mute: None,
                deaf: None,
                sm: None,
                q: Some(2),
                sh: None,
            }
        );
        assert_eq!(
            FromRoom::parse(r#"{"t":"room","name":"Gece","leader":null,"mods":["u2"]}"#),
            FromRoom::Room {
                name: Some("Gece".into()),
                leader: None,
                mods: vec!["u2".into()],
                max: None,
            }
        );
        // VOICE-ROOMS: a crew room's limit the crew changed.
        assert_eq!(
            FromRoom::parse(r#"{"t":"room","name":"Gece","leader":"u1","mods":[],"max":4}"#),
            FromRoom::Room {
                name: Some("Gece".into()),
                leader: Some("u1".into()),
                mods: Vec::new(),
                max: Some(4),
            }
        );
        assert_eq!(
            FromRoom::parse(r#"{"t":"me","role":"l"}"#),
            FromRoom::Me { role: Role::Leader }
        );
        assert_eq!(
            FromRoom::parse(r#"{"t":"ring","to":"u5","until":200}"#),
            FromRoom::Ring {
                to: "u5".into(),
                until: 200
            }
        );
        assert_eq!(
            FromRoom::parse(r#"{"t":"ring_end","to":"u5","why":"cancelled"}"#),
            FromRoom::RingEnd {
                to: "u5".into(),
                why: RingEndWhy::Cancelled
            }
        );
        assert_eq!(FromRoom::parse(r#"{"t":"kicked"}"#), FromRoom::Kicked);
        assert_eq!(
            FromRoom::parse(r#"{"t":"moved","room":"c0123456789abcdef","ticket":"v1.x.y"}"#),
            FromRoom::Moved {
                room: "c0123456789abcdef".into(),
                ticket: Ticket::new("v1.x.y")
            }
        );
        for (why, read) in [
            ("leader", ClosedWhy::Leader),
            ("alone", ClosedWhy::Alone),
            ("no_answer", ClosedWhy::NoAnswer),
            ("elsewhere", ClosedWhy::Elsewhere),
            ("crew", ClosedWhy::Crew),
            ("off", ClosedWhy::Off),
            ("budget", ClosedWhy::Budget),
            ("staff", ClosedWhy::Staff),
            ("account", ClosedWhy::Account),
            ("muted", ClosedWhy::Muted),
            // Anything else like `off`.
            ("sunset", ClosedWhy::Off),
        ] {
            assert_eq!(
                FromRoom::parse(&format!(r#"{{"t":"closed","why":"{why}"}}"#)),
                FromRoom::Closed {
                    why: read,
                    until: None
                },
                "{why}"
            );
        }
        // BAN-TYPES: a voice chat ban says until when.
        assert_eq!(
            FromRoom::parse(r#"{"t":"closed","why":"muted","until":1700000000}"#),
            FromRoom::Closed {
                why: ClosedWhy::Muted,
                until: Some(1_700_000_000)
            }
        );
        assert!(matches!(
            FromRoom::parse(r#"{"t":"ack","rid":2,"ice":{"urls":["turn:turn.cloudflare.com:443?transport=tcp"],"username":"x","credential":"y","ttl":7200}}"#),
            FromRoom::Ack { rid: Some(2), ice: Some(ice), until: None, .. } if ice.urls.len() == 1 && ice.ttl == 7200
        ));
        assert_eq!(
            FromRoom::parse(r#"{"t":"ack","rid":3,"until":130}"#),
            FromRoom::Ack {
                rid: Some(3),
                ice: None,
                until: Some(130),
                sdp: None,
                n: None,
            }
        );
        assert_eq!(
            FromRoom::parse(
                r#"{"t":"error","rid":3,"code":"rate_limited","retryAfter":40,"item":"voice_invites"}"#
            ),
            FromRoom::Error {
                rid: Some(3),
                code: "rate_limited".into(),
                retry_after: Some(40),
                until: None,
                item: Some("voice_invites".into())
            }
        );
        assert_eq!(FromRoom::parse(r#"{"t":"news"}"#), FromRoom::Unknown);
        assert_eq!(FromRoom::parse("pong"), FromRoom::Unknown);
    }

    #[test]
    fn secrets_stay_out_of_debug_output() {
        let hello = FromRoom::parse(
            r#"{"t":"hello","room":{"id":"f0","kind":"f","max":2},"sdp":"v=0 secret-sdp","resume":"v1.secret.ticket"}"#,
        );
        let text = format!("{hello:?}");
        assert!(!text.contains("secret"), "{text}");
        let join = ToRoom::Join {
            sdp: Some(Sdp::new("v=0 secret")),
            mid: None,
            caps: Vec::new(),
        };
        assert_eq!(format!("{join:?}"), "join");
        let ice: IceWire = serde_json::from_str(
            r#"{"urls":["turn:a"],"username":"secret","credential":"secret"}"#,
        )
        .unwrap();
        assert!(!format!("{ice:?}").contains("secret"));
    }

    #[test]
    fn room_ids() {
        assert!(is_room_id("f0123456789abcdef"));
        assert!(is_room_id("g0123456789abcdef"));
        assert!(is_room_id("c0123456789abcdef"));
        assert!(!is_room_id("x0123456789abcdef"));
        assert!(!is_room_id("f0123456789ABCDEF"));
        assert!(!is_room_id("f0123"));
        assert_eq!(RoomKind::of("g0123456789abcdef"), RoomKind::Crew);
        assert_eq!(RoomKind::of("f0123456789abcdef"), RoomKind::Friend);
        assert_eq!(general_room_of("0123456789abcdef"), "g0123456789abcdef");
    }
}
