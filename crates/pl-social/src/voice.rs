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
        }
    }
}

/// One of a crew's live rooms.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct VoiceCrewRoom {
    pub id: String,
    #[serde(default)]
    pub name: String,
    /// Who is in it now (the player's blocks left out).
    #[serde(default)]
    pub ids: Vec<String>,
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
}
