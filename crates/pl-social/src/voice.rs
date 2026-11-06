//! Voice chat on the presence socket (VOICE, the service README's "Voice (VOICE-S1)" → Switches
//! and Calling) and its crew room routes, as the launcher sends and hears them. The rooms
//! themselves (their socket, the SFU, the state) are `pl-voice`'s; this crate only carries the
//! hub's part, so it never reads a ticket.
//!
//! - [`VoiceFrame`]: client → hub (`voice_call`, answered by `rid` with the room and a ticket;
//!   `voice_decline`, never answered).
//! - [`VoiceEvent`]: hub → client (`hello.voice`, `hello.voiceRooms`, `voice_state`, `voice_ring`,
//!   `voice_ring_end`, `voice_rooms`), plus the connection's own moments (the service took `voice1`
//!   or not, the socket dropped). The launcher hears them all ([`crate::SocialService::on_voice`]).
//! - `status.voice` (the player's switch) and `presence.voice` ([`PresenceVoice`]).
//!
//! Tickets never show in `Debug` output ([`Hidden`]): no log line carries them.

use std::fmt;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};

use crate::world::Hidden;

/// The presence capability of voice chat: asked for while the signed config's `features.voice`
/// is on, named in `hello.caps` when the service took it.
pub const VOICE_CAP: &str = "voice1";

/// Adds [`VOICE_CAP`] to a presence address's `caps` query.
pub(crate) fn ask_for_voice(url: &mut url::Url) {
    crate::world::ask_for_cap(url, VOICE_CAP);
}

/// A room id as the service makes them (`[fgc]` + 16 hex digits).
pub(crate) fn is_room_id(room: &str) -> bool {
    let bytes = room.as_bytes();
    bytes.len() == 17
        && matches!(bytes[0], b'f' | b'g' | b'c')
        && bytes[1..]
            .iter()
            .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(b))
}

/// `hello.voice` and `voice_state.voice`: Studio's switch and what launchers apply themselves.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceStatus {
    pub on: bool,
    pub friend_max: u16,
    pub crew_max: u16,
    /// Rooms a crew may have, "Genel" included.
    pub crew_rooms: u16,
    pub ring_seconds: u32,
    pub idle_minutes: u32,
    pub kbps: u16,
    /// VOICE-ROOMS: crews arrange their rooms (Studio's "Oda yönetimi"): rooms stay until deleted
    /// and the crew page changes them ([`VoiceCrewChange`]). Absent (an older service): no.
    pub manage: bool,
    /// OVL-NEXT: a crew's leader and moderators may make a kept room push-to-talk only (Studio's
    /// "Yalnız bas-konuş odalar", only with "Oda yönetimi"). Absent (an older service): no.
    pub ptt: bool,
    /// SHARE-S: screen sharing in voice rooms, only while Studio has it on (`hello.voice.share`,
    /// `voice_state.voice.share`). None: no sharing (an older service, or switched off).
    pub share: Option<VoiceShareStatus>,
}

/// SHARE-S: the best quality a new share may use (`maxPreset`). Anything unknown reads as `Low`
/// (a newer service's step the launcher does not know never lets it send more).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum SharePreset {
    #[default]
    Normal,
    High,
    // Last: `other` must be (also reads "low").
    #[serde(other)]
    Low,
}

/// SHARE-S: what the service allows of screen sharing now: shares a player and a room may have,
/// viewers per share, the best quality, and whether a new share (`open`) and a new watch (`watch`)
/// may start (the month's budget). Studio's switch and the budget's steps change it; the hub sends
/// `voice_state` again when it does.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceShareStatus {
    pub on: bool,
    pub per_user: u8,
    pub per_room: u8,
    pub viewers: u8,
    pub max_preset: SharePreset,
    pub open: bool,
    pub watch: bool,
}

impl Default for VoiceShareStatus {
    fn default() -> Self {
        Self {
            on: false,
            per_user: 2,
            per_room: 3,
            viewers: 10,
            max_preset: SharePreset::Normal,
            open: true,
            watch: true,
        }
    }
}

impl Default for VoiceStatus {
    fn default() -> Self {
        Self {
            on: false,
            friend_max: 10,
            crew_max: 15,
            crew_rooms: 3,
            ring_seconds: 30,
            idle_minutes: 30,
            kbps: 24,
            manage: false,
            ptt: false,
            share: None,
        }
    }
}

/// One of a crew's rooms, in the order the service lists them ("Genel" first).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VoiceCrewRoom {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Who is in it now (the player's blocks left out).
    #[serde(default)]
    pub ids: Vec<String>,
    /// VOICE-ROOMS: people at most, when the crew set fewer than the crew room maximum.
    #[serde(default)]
    pub max: Option<u16>,
    /// VOICE-ROOMS: the player opened this room (they may change and delete it).
    #[serde(default)]
    pub own: bool,
    /// SHARE-S: only the crew's leader and moderators may share in it (`shareBy: "mods"`; absent:
    /// everyone).
    #[serde(default, rename = "shareBy", deserialize_with = "mods_only")]
    pub share_mods: bool,
    /// OVL-NEXT: only push-to-talk in it (`ptt: true`; absent or anything else: no).
    #[serde(default, deserialize_with = "only_true")]
    pub ptt: bool,
    /// SHARE-LIVE: who of `ids` shares their screen now (the player's blocks left out; absent from
    /// an older service and while sharing is off). An entry that is no text is dropped.
    #[serde(default, deserialize_with = "texts")]
    pub live: Vec<String>,
}

fn mods_only<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<bool, D::Error> {
    let value = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(value.as_ref().and_then(serde_json::Value::as_str) == Some("mods"))
}

/// A list of texts where anything else is dropped (and a list that is no list is empty).
fn texts<'de, D: serde::Deserializer<'de>>(deserializer: D) -> Result<Vec<String>, D::Error> {
    Ok(
        match Option::<serde_json::Value>::deserialize(deserializer)? {
            Some(serde_json::Value::Array(items)) => items
                .into_iter()
                .filter_map(|item| match item {
                    serde_json::Value::String(text) => Some(text),
                    _ => None,
                })
                .take(64)
                .collect(),
            _ => Vec::new(),
        },
    )
}

/// SHARE-LIVE: `true` reads as yes; absent, `null` or anything else as no.
pub(crate) fn only_true<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<bool, D::Error> {
    Ok(Option::<serde_json::Value>::deserialize(deserializer)?
        == Some(serde_json::Value::Bool(true)))
}

/// VOICE-ROOMS: a change of the crew's rooms from the crew page (README "Voice (VOICE-ROOMS)").
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceCrewChange {
    /// A room's name and people at most (`Some(None)`: the crew room maximum again); SHARE-S:
    /// who may share in it (`Some(true)`: its leader and moderators, `Some(false)`: everyone);
    /// OVL-NEXT: push-to-talk only (`None`: as it is).
    Edit {
        crew: String,
        room: String,
        name: Option<String>,
        max: Option<Option<u16>>,
        share_mods: Option<bool>,
        ptt: Option<bool>,
    },
    /// The crew's rooms ("Genel" aside) in this order.
    Order { crew: String, rooms: Vec<String> },
    /// Deletes a room (its people go to "Genel"); `empty`: only while nobody is in it.
    Remove {
        crew: String,
        room: String,
        empty: bool,
    },
    /// Moves a crewmate from the crew room they are in to `room`.
    Move {
        crew: String,
        person: String,
        room: String,
    },
}

impl VoiceCrewChange {
    /// The route and its body.
    pub(crate) fn request(&self) -> (&'static str, serde_json::Value) {
        match self {
            Self::Edit {
                crew,
                room,
                name,
                max,
                share_mods,
                ptt,
            } => {
                let mut body = serde_json::json!({ "crew": crew, "room": room });
                if let Some(name) = name {
                    body["name"] = name.as_str().into();
                }
                if let Some(max) = max {
                    body["max"] = max.map_or(serde_json::Value::Null, Into::into);
                }
                if let Some(mods) = share_mods {
                    body["shareBy"] = if *mods { "mods" } else { "all" }.into();
                }
                if let Some(ptt) = ptt {
                    body["ptt"] = (*ptt).into();
                }
                ("v1/voice/crew/room", body)
            }
            Self::Order { crew, rooms } => (
                "v1/voice/crew/order",
                serde_json::json!({ "crew": crew, "rooms": rooms }),
            ),
            Self::Remove { crew, room, empty } => {
                let mut body = serde_json::json!({ "crew": crew, "room": room });
                if *empty {
                    body["empty"] = true.into();
                }
                ("v1/voice/crew/remove", body)
            }
            Self::Move { crew, person, room } => (
                "v1/voice/crew/move",
                serde_json::json!({ "crew": crew, "person": person, "room": room }),
            ),
        }
    }
}

/// One crew's live rooms (`hello.voiceRooms[]`, `voice_rooms`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VoiceCrewRooms {
    pub crew: String,
    #[serde(default)]
    pub rooms: Vec<VoiceCrewRoom>,
}

/// A ring for the player (`voice_ring.ring`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VoiceRing {
    pub room: String,
    pub from: String,
    /// Unix seconds.
    #[serde(default)]
    pub at: i64,
    #[serde(default)]
    pub until: i64,
    pub ticket: Hidden<String>,
    /// Others in the room (at most 4).
    #[serde(default)]
    pub with: Vec<String>,
}

/// `presence.voice`: `ready` may be called, `in` is in a room (a call still rings). Absent: "Ara"
/// is not offered.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PresenceVoice {
    Ready,
    In,
}

/// SHARE-LIVE: a live share's still picture as the friends service hands it out
/// (`GET /v1/voice/thumb`, [`crate::Session::voice_thumb`]).
#[derive(Clone, PartialEq, Eq)]
pub enum VoiceThumb {
    /// The JPEG.
    Image(Vec<u8>),
    /// The player sees the room but may not join it: no picture ("a blurred lock").
    Locked,
    /// Nothing to show: no such share, no picture yet, or nothing the player may see.
    None,
}

impl VoiceThumb {
    /// The service's pictures are at most this long (the room keeps none larger).
    pub const MAX_BYTES: usize = 24 * 1024;

    /// A JPEG (`FF D8 FF`) of at most [`Self::MAX_BYTES`].
    pub fn is_picture(bytes: &[u8]) -> bool {
        bytes.len() > 3 && bytes.len() <= Self::MAX_BYTES && bytes.starts_with(&[0xFF, 0xD8, 0xFF])
    }
}

impl fmt::Debug for VoiceThumb {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Image(jpg) => write!(f, "Image(<{} bytes>)", jpg.len()),
            Self::Locked => f.write_str("Locked"),
            Self::None => f.write_str("None"),
        }
    }
}

/// A 403 body that says `{"locked": true}`.
pub(crate) fn says_locked(body: &[u8]) -> bool {
    serde_json::from_slice::<serde_json::Value>(body)
        .is_ok_and(|value| value.get("locked") == Some(&serde_json::Value::Bool(true)))
}

/// A client → hub voice frame.
#[derive(Clone, PartialEq, Eq)]
pub enum VoiceFrame {
    /// Opens a friend room led by the player and rings `to`.
    Call { to: String },
    /// "Reddet" (never answered).
    Decline { room: String },
}

impl fmt::Debug for VoiceFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Call { .. } => f.write_str("voice_call"),
            Self::Decline { .. } => f.write_str("voice_decline"),
        }
    }
}

/// `voice_call`'s answer: the room and the player's ticket to it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct VoiceAck {
    pub room: String,
    pub ticket: Hidden<String>,
}

/// A voice request that got no `ack`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceError {
    /// The service's `error`: `code` (`voice_off`, `friend_not_found`, `rate_limited`,
    /// `account_limited`, `voice_muted`, `bad_request`, …), the wait (`retryAfter`) and `until`.
    Refused {
        code: String,
        retry_after: Option<u64>,
        until: Option<i64>,
    },
    /// This connection does not take voice chat (an older service, or not asked for).
    Off,
    /// No connection, or no answer in time.
    Unavailable,
}

impl fmt::Display for VoiceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { code, .. } => write!(f, "voice request refused: {code}"),
            Self::Off => f.write_str("voice chat is not on this connection"),
            Self::Unavailable => f.write_str("the friends service did not answer"),
        }
    }
}

impl std::error::Error for VoiceError {}

/// A crew room's ticket (`POST /v1/voice/crew`, `…/crew/new`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VoiceCrewTicket {
    pub room: String,
    #[serde(default)]
    pub name: String,
    pub ticket: Hidden<String>,
}

/// What the presence socket says about voice chat.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoiceEvent {
    /// After every connect: whether the service took `voice1` on it, Studio's switch and limits
    /// (only while voice chat is on), and the live rooms of the player's crews.
    Hello {
        taken: bool,
        status: Option<VoiceStatus>,
        rooms: Vec<VoiceCrewRooms>,
    },
    /// Studio's settings changed (`voice_state`).
    Status(VoiceStatus),
    /// A call or invite rings for the player; `quiet` on do-not-disturb (a missed call only).
    Ring { ring: VoiceRing, quiet: bool },
    /// `voice_ring_end`: `joined` or `declined` on another device, or `ended`.
    RingEnd { room: String, reason: String },
    /// One crew's live rooms changed: they replace that crew's.
    Rooms(VoiceCrewRooms),
    /// The presence connection dropped (it reconnects by itself, or the player signed out).
    Down,
}

pub type VoiceSink = Arc<dyn Fn(VoiceEvent) + Send + Sync>;

/// The service's side of voice chat: who hears it, and whether the connection took it.
#[derive(Default)]
pub(crate) struct VoiceHub {
    sink: Mutex<Option<VoiceSink>>,
    taken: Mutex<bool>,
}

impl VoiceHub {
    pub(crate) fn set_sink(&self, sink: VoiceSink) {
        *self.sink.lock().unwrap_or_else(|p| p.into_inner()) = Some(sink);
    }

    pub(crate) fn taken(&self) -> bool {
        *self.taken.lock().unwrap_or_else(|p| p.into_inner())
    }

    pub(crate) fn deliver(&self, event: VoiceEvent) {
        {
            let mut taken = self.taken.lock().unwrap_or_else(|p| p.into_inner());
            match &event {
                VoiceEvent::Hello { taken: now, .. } => *taken = *now,
                VoiceEvent::Down => *taken = false,
                _ => {}
            }
        }
        let sink = self.sink.lock().unwrap_or_else(|p| p.into_inner()).clone();
        if let Some(sink) = sink {
            sink(event);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_hubs_words_read_as_it_writes_them() {
        let status: VoiceStatus = serde_json::from_str(
            r#"{"on":true,"friendMax":10,"crewMax":15,"crewRooms":3,"ringSeconds":30,"idleMinutes":30,"kbps":24}"#,
        )
        .unwrap();
        assert!(status.on);
        assert_eq!(status.crew_rooms, 3);
        assert_eq!(status.kbps, 24);
        // An older or partial one keeps the defaults.
        let partial: VoiceStatus = serde_json::from_str(r#"{"on":true}"#).unwrap();
        assert_eq!(partial.ring_seconds, 30);
        let ring: VoiceRing = serde_json::from_str(
            r#"{"room":"f0123456789abcdef","from":"7","at":100,"until":130,"ticket":"v1.secret.mac","with":["8"]}"#,
        )
        .unwrap();
        assert_eq!(ring.with, vec!["8".to_owned()]);
        assert_eq!(ring.ticket.expose(), "v1.secret.mac");
        assert!(!format!("{ring:?}").contains("secret"));
        let rooms: VoiceCrewRooms = serde_json::from_str(
            r#"{"crew":"0123456789abcdef","rooms":[{"id":"g0123456789abcdef","name":"Genel","ids":["7"]}]}"#,
        )
        .unwrap();
        assert_eq!(rooms.rooms[0].name, "Genel");
        assert_eq!(
            serde_json::from_str::<PresenceVoice>(r#""in""#).unwrap(),
            PresenceVoice::In
        );
    }

    /// SHARE-S: `hello.voice.share` as the hub writes it; absent (an older service, or sharing
    /// off) and odd values keep the launcher on the safe side.
    #[test]
    fn reads_screen_sharing_in_hello_voice() {
        let status: VoiceStatus = serde_json::from_str(
            r#"{"on":true,"share":{"on":true,"perUser":2,"perRoom":3,"viewers":5,"maxPreset":"normal","open":true,"watch":false}}"#,
        )
        .unwrap();
        let share = status.share.unwrap();
        assert!(share.on && share.open && !share.watch);
        assert_eq!((share.per_user, share.per_room, share.viewers), (2, 3, 5));
        assert_eq!(share.max_preset, SharePreset::Normal);
        assert_eq!(
            serde_json::from_str::<VoiceStatus>(r#"{"on":true}"#)
                .unwrap()
                .share,
            None
        );
        assert_eq!(VoiceStatus::default().share, None);
        let newer: VoiceShareStatus =
            serde_json::from_str(r#"{"on":true,"maxPreset":"ultra","later":1}"#).unwrap();
        assert_eq!(newer.max_preset, SharePreset::Low);
        assert_eq!(newer.viewers, 10);
    }

    /// VOICE-ROOMS: the switch, a room's own limit and opener, and the crew page's changes as the
    /// service's README writes them.
    #[test]
    fn reads_kept_rooms_and_writes_the_crew_pages_changes() {
        let status: VoiceStatus = serde_json::from_str(r#"{"on":true,"manage":true}"#).unwrap();
        assert!(status.manage);
        assert!(!VoiceStatus::default().manage);
        // OVL-NEXT: push-to-talk-only rooms, off unless the service says so.
        let status: VoiceStatus = serde_json::from_str(r#"{"on":true,"ptt":true}"#).unwrap();
        assert!(status.ptt);
        assert!(!VoiceStatus::default().ptt);
        let rooms: VoiceCrewRooms = serde_json::from_str(
            r#"{"crew":"0123456789abcdef","rooms":[{"id":"g0123456789abcdef","name":"Genel","ids":[]},
                {"id":"c0123456789abcdef","name":"Taş Ocağı","ids":["7"],"max":4,"own":true}]}"#,
        )
        .unwrap();
        assert_eq!(rooms.rooms[0].max, None);
        assert!(!rooms.rooms[0].own);
        assert_eq!(rooms.rooms[1].max, Some(4));
        assert!(rooms.rooms[1].own);
        let crew = "0123456789abcdef".to_owned();
        let room = "c0123456789abcdef".to_owned();
        let cases = [
            (
                VoiceCrewChange::Edit {
                    crew: crew.clone(),
                    room: room.clone(),
                    name: Some("Antrenman".into()),
                    max: Some(None),
                    share_mods: None,
                    ptt: None,
                },
                "v1/voice/crew/room",
                serde_json::json!({"crew": crew, "room": room, "name": "Antrenman", "max": null}),
            ),
            (
                VoiceCrewChange::Edit {
                    crew: crew.clone(),
                    room: room.clone(),
                    name: None,
                    max: Some(Some(6)),
                    share_mods: None,
                    ptt: None,
                },
                "v1/voice/crew/room",
                serde_json::json!({"crew": crew, "room": room, "max": 6}),
            ),
            // SHARE-S: who may share in a kept room.
            (
                VoiceCrewChange::Edit {
                    crew: crew.clone(),
                    room: room.clone(),
                    name: None,
                    max: None,
                    share_mods: Some(true),
                    ptt: None,
                },
                "v1/voice/crew/room",
                serde_json::json!({"crew": crew, "room": room, "shareBy": "mods"}),
            ),
            // OVL-NEXT: push-to-talk only.
            (
                VoiceCrewChange::Edit {
                    crew: crew.clone(),
                    room: room.clone(),
                    name: None,
                    max: None,
                    share_mods: None,
                    ptt: Some(false),
                },
                "v1/voice/crew/room",
                serde_json::json!({"crew": crew, "room": room, "ptt": false}),
            ),
            (
                VoiceCrewChange::Order {
                    crew: crew.clone(),
                    rooms: vec![room.clone()],
                },
                "v1/voice/crew/order",
                serde_json::json!({"crew": crew, "rooms": [room]}),
            ),
            (
                VoiceCrewChange::Remove {
                    crew: crew.clone(),
                    room: room.clone(),
                    empty: true,
                },
                "v1/voice/crew/remove",
                serde_json::json!({"crew": crew, "room": room, "empty": true}),
            ),
            (
                VoiceCrewChange::Move {
                    crew: crew.clone(),
                    person: "7".into(),
                    room: room.clone(),
                },
                "v1/voice/crew/move",
                serde_json::json!({"crew": crew, "person": "7", "room": room}),
            ),
        ];
        for (change, path, body) in cases {
            assert_eq!(change.request(), (path, body));
        }
    }

    #[test]
    fn the_capability_is_asked_once() {
        let mut url = url::Url::parse("wss://social.projects.gg/v1/presence?caps=chat").unwrap();
        ask_for_voice(&mut url);
        ask_for_voice(&mut url);
        assert_eq!(url.query(), Some("caps=chat,voice1"));
    }

    #[test]
    fn the_hub_keeps_whether_the_connection_took_it() {
        let hub = VoiceHub::default();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let sink = heard.clone();
        hub.set_sink(Arc::new(move |event| sink.lock().unwrap().push(event)));
        hub.deliver(VoiceEvent::Hello {
            taken: true,
            status: Some(VoiceStatus::default()),
            rooms: Vec::new(),
        });
        assert!(hub.taken());
        hub.deliver(VoiceEvent::Down);
        assert!(!hub.taken());
        assert_eq!(heard.lock().unwrap().len(), 2);
    }

    /// SHARE-LIVE: who shares, in a crew room's list and a friend's presence, read leniently (an
    /// older service says nothing); the picture's answers.
    #[test]
    fn who_shares_reads_leniently() {
        let rooms: VoiceCrewRooms = serde_json::from_str(
            r#"{"crew":"0123456789abcdef","rooms":[
                {"id":"g0123456789abcdef","ids":["7","8"],"live":["8",9,null]},
                {"id":"c0123456789abcdef","ids":["9"],"live":"9"},
                {"id":"c0123456789abcdee","ids":["9"]}]}"#,
        )
        .unwrap();
        assert_eq!(rooms.rooms[0].live, ["8"]);
        assert!(rooms.rooms[1].live.is_empty() && rooms.rooms[2].live.is_empty());
        let presence = |extra: &str| -> crate::protocol::Presence {
            serde_json::from_str(&format!(
                r#"{{"id":"7","state":"launcher","voice":"in"{extra}}}"#
            ))
            .unwrap()
        };
        assert!(presence(r#","live":true"#).live);
        for odd in [
            "",
            r#","live":false"#,
            r#","live":"yes""#,
            r#","live":1"#,
            r#","live":null"#,
        ] {
            assert!(!presence(odd).live, "{odd}");
        }
        assert!(says_locked(br#"{"locked":true}"#));
        assert!(!says_locked(br#"{"error":"forbidden"}"#) && !says_locked(b"locked"));
        assert!(VoiceThumb::is_picture(&[0xFF, 0xD8, 0xFF, 0xE0]));
        assert!(!VoiceThumb::is_picture(b"\x89PNG"));
        let mut huge = vec![0xFF, 0xD8, 0xFF];
        huge.resize(VoiceThumb::MAX_BYTES + 1, 0);
        assert!(!VoiceThumb::is_picture(&huge));
        assert_eq!(
            format!("{:?}", VoiceThumb::Image(vec![0; 9])),
            "Image(<9 bytes>)"
        );
    }

    /// SHARE-S: a crew room's "Kim paylaşabilir" (`shareBy`), everyone unless it says "mods".
    #[test]
    fn crew_rooms_say_who_may_share() {
        let room =
            |json: serde_json::Value| -> VoiceCrewRoom { serde_json::from_value(json).unwrap() };
        let id = "c0123456789abcdef";
        assert!(room(serde_json::json!({"id": id, "shareBy": "mods"})).share_mods);
        assert!(!room(serde_json::json!({"id": id, "shareBy": "all"})).share_mods);
        assert!(!room(serde_json::json!({"id": id, "shareBy": 3})).share_mods);
        assert!(!room(serde_json::json!({"id": id, "shareBy": null})).share_mods);
        assert!(!room(serde_json::json!({"id": id})).share_mods);
    }

    /// OVL-NEXT: a crew room is push-to-talk only when it says `ptt: true`, nothing else.
    #[test]
    fn crew_rooms_say_push_to_talk_only() {
        let room =
            |json: serde_json::Value| -> VoiceCrewRoom { serde_json::from_value(json).unwrap() };
        let id = "c0123456789abcdef";
        assert!(room(serde_json::json!({"id": id, "ptt": true})).ptt);
        for odd in [
            serde_json::json!({"id": id, "ptt": false}),
            serde_json::json!({"id": id, "ptt": 1}),
            serde_json::json!({"id": id, "ptt": "true"}),
            serde_json::json!({"id": id, "ptt": null}),
            serde_json::json!({"id": id}),
        ] {
            assert!(!room(odd.clone()).ptt, "{odd}");
        }
    }
}
