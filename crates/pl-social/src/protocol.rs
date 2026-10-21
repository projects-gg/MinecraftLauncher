//! Presence WebSocket protocol (`GET /v1/presence`, JSON text frames).

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::model::{
    CrewMute, FriendGroups, LastSeen, PublicProfile, StatusMode, Team, TeamExit, TeamInvite,
    TeamNotifications,
};

/// What this launcher understands on the presence socket (`GET /v1/presence?caps=…`): crew calls
/// that carry their server and may end with `server` (`callServer`), and several crews (`crews`).
/// Older services ignore the query.
pub const PRESENCE_CAPS: &str = "callServer,crews";

/// projects.gg or one of its subdomains, with or without a port (`play.projects.gg:25565`).
pub fn is_projects_host(host: &str) -> bool {
    let name = host
        .rsplit_once(':')
        .map_or(host, |(name, port)| {
            if port.bytes().all(|b| b.is_ascii_digit()) {
                name
            } else {
                host
            }
        })
        .trim_end_matches('.')
        .to_ascii_lowercase();
    name == "projects.gg" || name.ends_with(".projects.gg")
}

/// What this launcher is doing. Sent only when it changes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LocalStatus {
    /// The launcher is open (or in the tray).
    #[default]
    Launcher,
    /// In game on a server. `host` is `host[:port]` from the game log, when known.
    Playing { host: Option<String> },
    /// In a singleplayer world.
    Singleplayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PresenceState {
    Offline,
    Launcher,
    Playing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum OnlineStatus {
    Online,
    Dnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum GameLocation {
    /// projects.gg (or a subdomain): show "Katıl".
    Projects,
    /// Another server; `host` is set only when the friend shares it.
    Other,
    Singleplayer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Game {
    #[serde(rename = "where")]
    pub location: GameLocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

impl Game {
    /// Whether the launcher can offer "Katıl" (join) for this game.
    pub fn joinable(&self) -> bool {
        self.location == GameLocation::Projects
    }
}

/// A friend's presence as the service shows it. Invisible friends are always offline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    pub id: String,
    pub state: PresenceState,
    #[serde(default)]
    pub status: Option<OnlineStatus>,
    /// While playing; None when the server is not known yet.
    #[serde(default)]
    pub game: Option<Game>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub avatar_hash: Option<String>,
    /// Set on a visible → offline transition when the friend shares it; otherwise keep the
    /// value from the friend list.
    #[serde(default)]
    pub last_seen: Option<LastSeen>,
}

/// "Birlikte oyna": a join-me invite without any text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub id: String,
    /// Sender's user id.
    pub from: String,
    /// Unix seconds.
    pub at: i64,
    /// The sender's server, when it can be shared.
    pub server: Option<Game>,
    /// It waited (the player was offline or on do-not-disturb): show it in a quiet list.
    pub quiet: bool,
}

/// What a crew call asks: come where the caller plays, or "are you ready?" (a call from before
/// CALL1, or one of another server for a launcher that does not know `callServer`). With
/// `callServer` it is always `Join` and the call carries its server. A kind this version does not
/// know reads as `Ready` (an answer, nothing to join).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CallKind {
    Join,
    Ready,
}

impl<'de> Deserialize<'de> for CallKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "join" => Self::Join,
            _ => Self::Ready,
        })
    }
}

/// An answer to a crew call: "Geliyorum", "Birazdan", "Gelemem".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CallAnswer {
    Yes,
    Soon,
    No,
}

impl CallAnswer {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "yes" => Some(Self::Yes),
            "soon" => Some(Self::Soon),
            "no" => Some(Self::No),
            _ => None,
        }
    }
}

/// "Ekibi çağır": an open call as the service shows it (`{"t":"call"}`, `hello.calls`). Times are
/// Unix seconds on the service's clock; the message that carries the call also says what time it
/// is there (`now`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CrewCall {
    pub id: String,
    /// The caller's user id (the player's own id for their own call).
    pub from: String,
    pub kind: CallKind,
    pub at: i64,
    /// The call ends by itself at this time; no message says so.
    pub until: i64,
    /// Who answered what (answers this version does not know are left out).
    #[serde(default, deserialize_with = "known_answers")]
    pub answers: BTreeMap<String, CallAnswer>,
    /// The player's crews the call names (the caller's own call: all it names). Empty from a
    /// service without several crews: the call is the one crew's.
    #[serde(default)]
    pub crews: Vec<String>,
    /// Where the call is (a service with `callServer`): projects.gg, or another server with its
    /// address; it follows the caller.
    #[serde(default)]
    pub server: Option<Game>,
}

fn known_answers<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, CallAnswer>, D::Error> {
    let raw = BTreeMap::<String, String>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|(id, answer)| CallAnswer::parse(&answer).map(|answer| (id, answer)))
        .collect())
}

/// Why a call ended: the caller ended it, it went away (the caller went offline, left the crew,
/// the crew ended, the player left it, or a block hid it), or the caller is no longer on a server
/// it allows (`server`, with `callServer`). Unknown reasons read as `Ended`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CallEndReason {
    #[default]
    Ended,
    Left,
    Server,
}

impl<'de> Deserialize<'de> for CallEndReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "left" => Self::Left,
            "server" => Self::Server,
            _ => Self::Ended,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(crate) enum ClientMessage {
    Status {
        state: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        host: Option<String>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        singleplayer: bool,
    },
    Invite {
        to: String,
        rid: u32,
    },
    /// "Ekibi çağır": the crews it calls (a service with several crews), else the primary one.
    Call {
        #[serde(skip_serializing_if = "Option::is_none")]
        crews: Option<Vec<String>>,
        rid: u32,
    },
    CallAnswer {
        call: String,
        answer: CallAnswer,
        rid: u32,
    },
    /// Ends the player's own call.
    CallEnd {
        call: String,
        rid: u32,
    },
}

impl ClientMessage {
    pub(crate) fn status(status: &LocalStatus) -> Self {
        match status {
            LocalStatus::Launcher => Self::Status {
                state: "launcher",
                host: None,
                singleplayer: false,
            },
            LocalStatus::Playing { host } => Self::Status {
                state: "playing",
                host: host.clone(),
                singleplayer: false,
            },
            LocalStatus::Singleplayer => Self::Status {
                state: "playing",
                host: None,
                singleplayer: true,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(crate) enum ServerMessage {
    Hello {
        status: StatusMode,
        #[serde(default, rename = "keepaliveSeconds")]
        keepalive_seconds: Option<u64>,
        #[serde(default)]
        friends: Vec<Presence>,
        /// Online teammates who are not friends.
        #[serde(default)]
        team: Vec<Presence>,
        #[serde(default)]
        invites: Vec<Invite>,
        /// Every open crew call the player sees, their own included (a service with calls).
        #[serde(default)]
        calls: Vec<CrewCall>,
        /// The service's clock (Unix seconds), for the calls' deadlines.
        #[serde(default)]
        now: Option<i64>,
        /// The capabilities of [`PRESENCE_CAPS`] the service took (none from an older one).
        #[serde(default)]
        caps: Vec<String>,
    },
    /// A crew call, new or changed (its server or kind, its crews, or answers that went away):
    /// replaces the one with the same id. `quiet`: show it, never announce it.
    Call {
        call: CrewCall,
        #[serde(default)]
        quiet: bool,
        #[serde(default)]
        now: Option<i64>,
    },
    /// Someone answered a call (the player too, from another device).
    CallAnswer {
        call: String,
        by: String,
        answer: String,
    },
    CallEnd {
        call: String,
        #[serde(default)]
        reason: CallEndReason,
    },
    /// One of the player's crews changed, or they are no longer in it (`team` null).
    Team {
        team: Option<Team>,
        /// The crew's id (none from a service with one crew per player).
        #[serde(default)]
        crew: Option<String>,
        #[serde(default)]
        reason: Option<TeamExit>,
        /// Who made the change.
        #[serde(default)]
        by: Option<String>,
    },
    TeamInvite {
        invite: TeamInvite,
    },
    /// The player's crew notification settings for all crews changed (on any of their devices).
    TeamNotifications {
        notifications: TeamNotifications,
    },
    /// The player's mute of one crew changed (on any of their devices).
    CrewNotifications {
        crew: String,
        notifications: CrewMute,
    },
    /// The player put their crews in another order on another device (the first is primary).
    CrewOrder {
        order: Vec<String>,
    },
    /// The player's friend categories changed on another device.
    Groups {
        groups: FriendGroups,
    },
    Presence {
        presence: Presence,
    },
    FriendAdded {
        friend: PublicProfile,
        presence: Presence,
    },
    FriendRemoved {
        id: String,
    },
    Request {
        from: PublicProfile,
    },
    RequestCancelled {
        id: String,
    },
    Invite {
        invite: Invite,
    },
    Invites {
        invites: Vec<Invite>,
    },
    Status {
        status: StatusMode,
    },
    Ack {
        #[serde(default)]
        rid: Option<u32>,
        #[serde(default)]
        delivered: Option<bool>,
    },
    Error {
        #[serde(default)]
        rid: Option<u32>,
        code: String,
        #[serde(default, rename = "retryAfter")]
        retry_after: Option<u64>,
    },
    Bye {
        #[serde(default)]
        reason: String,
    },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn status_messages() {
        let json = |s: &LocalStatus| serde_json::to_string(&ClientMessage::status(s)).unwrap();
        assert_eq!(
            json(&LocalStatus::Launcher),
            r#"{"t":"status","state":"launcher"}"#
        );
        assert_eq!(
            json(&LocalStatus::Playing {
                host: Some("projects.gg".into())
            }),
            r#"{"t":"status","state":"playing","host":"projects.gg"}"#
        );
        assert_eq!(
            json(&LocalStatus::Singleplayer),
            r#"{"t":"status","state":"playing","singleplayer":true}"#
        );
        assert_eq!(
            serde_json::to_string(&ClientMessage::Invite {
                to: "1".into(),
                rid: 7
            })
            .unwrap(),
            r#"{"t":"invite","to":"1","rid":7}"#
        );
    }

    #[test]
    fn server_messages() {
        let hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","keepaliveSeconds":45,
                "friends":[{"id":"1","state":"playing","status":"dnd",
                  "game":{"where":"projects","host":"projects.gg"},"avatar":"discord","avatarHash":null}],
                "invites":[{"id":"9","from":"1","at":5,"server":null,"quiet":true}]}"#,
        )
        .unwrap();
        match hello {
            ServerMessage::Hello {
                status,
                keepalive_seconds,
                friends,
                invites,
                ..
            } => {
                assert_eq!(status, StatusMode::Online);
                assert_eq!(keepalive_seconds, Some(45));
                assert!(friends[0].game.as_ref().unwrap().joinable());
                assert_eq!(friends[0].status, Some(OnlineStatus::Dnd));
                assert!(invites[0].quiet);
            }
            other => panic!("unexpected {other:?}"),
        }
        let offline: ServerMessage = serde_json::from_str(
            r#"{"t":"presence","presence":{"id":"1","state":"offline","lastSeen":"today"}}"#,
        )
        .unwrap();
        assert!(matches!(
            offline,
            ServerMessage::Presence { presence } if presence.last_seen == Some(LastSeen::Today)
        ));
        let hidden: ServerMessage = serde_json::from_str(
            r#"{"t":"presence","presence":{"id":"1","state":"playing","game":{"where":"other"}}}"#,
        )
        .unwrap();
        assert!(matches!(
            hidden,
            ServerMessage::Presence { presence } if presence.game == Some(Game { location: GameLocation::Other, host: None })
        ));
        let future: ServerMessage = serde_json::from_str(r#"{"t":"emoji","x":1}"#).unwrap();
        assert!(matches!(future, ServerMessage::Unknown));
        let gone: ServerMessage = serde_json::from_str(
            r#"{"t":"team","team":null,"reason":"kicked","by":"123456789012345678"}"#,
        )
        .unwrap();
        assert!(matches!(
            gone,
            ServerMessage::Team {
                team: None,
                crew: None,
                reason: Some(TeamExit::Kicked),
                by: Some(_)
            }
        ));
        let groups: ServerMessage = serde_json::from_str(
            r#"{"t":"groups","groups":{"version":2,"groups":[{"id":"a","name":"A"}],"members":{}}}"#,
        )
        .unwrap();
        assert!(matches!(groups, ServerMessage::Groups { groups } if groups.version == 2));
        let team_hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","friends":[],
                "team":[{"id":"7","state":"launcher","status":"online"}],"invites":[]}"#,
        )
        .unwrap();
        assert!(matches!(team_hello, ServerMessage::Hello { team, .. } if team.len() == 1));
        let error: ServerMessage =
            serde_json::from_str(r#"{"t":"error","rid":3,"code":"rate_limited","retryAfter":60}"#)
                .unwrap();
        assert!(matches!(
            error,
            ServerMessage::Error {
                rid: Some(3),
                retry_after: Some(60),
                ..
            }
        ));
    }

    #[test]
    fn crew_call_messages() {
        let json = |message: &ClientMessage| serde_json::to_string(message).unwrap();
        assert_eq!(
            json(&ClientMessage::Call {
                crews: None,
                rid: 8
            }),
            r#"{"t":"call","rid":8}"#
        );
        assert_eq!(
            json(&ClientMessage::Call {
                crews: Some(vec!["0a1b2c3d4e5f6a7b".into(), "9f8e7d6c5b4a3f2e".into()]),
                rid: 8
            }),
            r#"{"t":"call","crews":["0a1b2c3d4e5f6a7b","9f8e7d6c5b4a3f2e"],"rid":8}"#
        );
        assert_eq!(
            json(&ClientMessage::CallAnswer {
                call: "3f9c0a1b2c3d4e5f".into(),
                answer: CallAnswer::Yes,
                rid: 9
            }),
            r#"{"t":"call_answer","call":"3f9c0a1b2c3d4e5f","answer":"yes","rid":9}"#
        );
        assert_eq!(
            json(&ClientMessage::CallEnd {
                call: "3f9c0a1b2c3d4e5f".into(),
                rid: 10
            }),
            r#"{"t":"call_end","call":"3f9c0a1b2c3d4e5f","rid":10}"#
        );

        let call: ServerMessage = serde_json::from_str(
            r#"{"t":"call","call":{"id":"3f9c0a1b2c3d4e5f","from":"7","kind":"join","at":1792521000,
                "until":1792522200,"answers":{"8":"yes","9":"soon","10":"maybe"}},"quiet":false,
                "now":1792521005}"#,
        )
        .unwrap();
        match call {
            ServerMessage::Call { call, quiet, now } => {
                assert_eq!(call.kind, CallKind::Join);
                assert!(!quiet);
                assert_eq!(now, Some(1_792_521_005));
                // An answer this version does not know is left out, not the whole message.
                assert_eq!(call.answers.len(), 2);
                assert_eq!(call.answers["9"], CallAnswer::Soon);
            }
            other => panic!("unexpected {other:?}"),
        }
        let later: ServerMessage = serde_json::from_str(
            r#"{"t":"call","call":{"id":"a","from":"7","kind":"party","at":1,"until":2,"answers":{}}}"#,
        )
        .unwrap();
        assert!(
            matches!(later, ServerMessage::Call { call, quiet: false, now: None } if call.kind == CallKind::Ready)
        );
        let answer: ServerMessage = serde_json::from_str(
            r#"{"t":"call_answer","call":"3f9c0a1b2c3d4e5f","by":"8","answer":"soon"}"#,
        )
        .unwrap();
        assert!(matches!(answer, ServerMessage::CallAnswer { answer, .. } if answer == "soon"));
        let ended: ServerMessage =
            serde_json::from_str(r#"{"t":"call_end","call":"a","reason":"expired"}"#).unwrap();
        assert!(matches!(
            ended,
            ServerMessage::CallEnd {
                reason: CallEndReason::Ended,
                ..
            }
        ));
        let left: ServerMessage =
            serde_json::from_str(r#"{"t":"call_end","call":"a","reason":"left"}"#).unwrap();
        assert!(matches!(
            left,
            ServerMessage::CallEnd {
                reason: CallEndReason::Left,
                ..
            }
        ));
        let hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","keepaliveSeconds":45,"friends":[],"team":[],
                "invites":[],"calls":[{"id":"b","from":"7","kind":"ready","at":1792521000,
                "until":1792522200,"answers":{}}],"now":1792521300}"#,
        )
        .unwrap();
        assert!(matches!(
            hello,
            ServerMessage::Hello { calls, now: Some(1_792_521_300), .. } if calls.len() == 1
        ));
        // A service before calls: no calls, no clock.
        let older: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","friends":[],"invites":[]}"#,
        )
        .unwrap();
        assert!(matches!(older, ServerMessage::Hello { calls, now: None, .. } if calls.is_empty()));
    }

    #[test]
    fn crew_notification_settings_arrive_on_every_device() {
        let changed: ServerMessage = serde_json::from_str(
            r#"{"t":"team_notifications","notifications":{"calls":true,"muted":true,"muteUntil":1792600000}}"#,
        )
        .unwrap();
        assert!(matches!(
            changed,
            ServerMessage::TeamNotifications { notifications }
                if notifications.muted && notifications.mute_until == Some(1_792_600_000)
        ));
        let unmuted: ServerMessage = serde_json::from_str(
            r#"{"t":"team_notifications","notifications":{"calls":false,"muted":false,"muteUntil":null}}"#,
        )
        .unwrap();
        assert!(matches!(
            unmuted,
            ServerMessage::TeamNotifications { notifications }
                if !notifications.calls && !notifications.muted && notifications.mute_until.is_none()
        ));
    }

    #[test]
    fn calls_from_a_server_and_several_crews() {
        // A call with `callServer` and `crews`: always `join`, with where it is and its crews.
        let call: ServerMessage = serde_json::from_str(
            r#"{"t":"call","call":{"id":"3f9c0a1b2c3d4e5f","from":"7","kind":"join","at":1,
                "until":1201,"answers":{},"crews":["0a1b2c3d4e5f6a7b","9f8e7d6c5b4a3f2e"],
                "server":{"where":"other","host":"mc.example.com"}},"quiet":false,"now":5}"#,
        )
        .unwrap();
        match call {
            ServerMessage::Call { call, .. } => {
                assert_eq!(call.crews.len(), 2);
                assert_eq!(
                    call.server,
                    Some(Game {
                        location: GameLocation::Other,
                        host: Some("mc.example.com".into())
                    })
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        // From a service before them: no crews, no server.
        let older: ServerMessage = serde_json::from_str(
            r#"{"t":"call","call":{"id":"a","from":"7","kind":"ready","at":1,"until":2}}"#,
        )
        .unwrap();
        assert!(matches!(
            older,
            ServerMessage::Call { call, .. } if call.crews.is_empty() && call.server.is_none()
        ));
        let moved: ServerMessage =
            serde_json::from_str(r#"{"t":"call_end","call":"a","reason":"server"}"#).unwrap();
        assert!(matches!(
            moved,
            ServerMessage::CallEnd {
                reason: CallEndReason::Server,
                ..
            }
        ));
        let hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","friends":[],"invites":[],
                "caps":["callServer","crews"],"now":9}"#,
        )
        .unwrap();
        assert!(matches!(hello, ServerMessage::Hello { caps, .. } if caps.len() == 2));
        // A crew of several changed, or the player left it.
        let gone: ServerMessage = serde_json::from_str(
            r#"{"t":"team","team":null,"crew":"9f8e7d6c5b4a3f2e","reason":"left","by":"100"}"#,
        )
        .unwrap();
        assert!(matches!(
            gone,
            ServerMessage::Team { team: None, crew: Some(crew), .. } if crew == "9f8e7d6c5b4a3f2e"
        ));
        let muted: ServerMessage = serde_json::from_str(
            r#"{"t":"crew_notifications","crew":"0a1b2c3d4e5f6a7b",
                "notifications":{"muted":true,"muteUntil":1792600000}}"#,
        )
        .unwrap();
        assert!(matches!(
            muted,
            ServerMessage::CrewNotifications { crew, notifications }
                if crew == "0a1b2c3d4e5f6a7b" && notifications.mute_until == Some(1_792_600_000)
        ));
        let order: ServerMessage = serde_json::from_str(
            r#"{"t":"crew_order","order":["9f8e7d6c5b4a3f2e","0a1b2c3d4e5f6a7b"]}"#,
        )
        .unwrap();
        assert!(
            matches!(order, ServerMessage::CrewOrder { order } if order[0] == "9f8e7d6c5b4a3f2e")
        );
    }

    #[test]
    fn projects_hosts() {
        for host in [
            "projects.gg",
            "play.projects.gg",
            "PLAY.Projects.GG.",
            "play.projects.gg:25565",
        ] {
            assert!(is_projects_host(host), "{host}");
        }
        for host in [
            "notprojects.gg",
            "projects.gg.example.com",
            "mc.example.com",
            "projects.gg:abc",
            "",
        ] {
            assert!(!is_projects_host(host), "{host}");
        }
    }
}
