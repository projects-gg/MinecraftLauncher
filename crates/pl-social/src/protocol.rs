//! Presence WebSocket protocol (`GET /v1/presence`, JSON text frames).

use serde::{Deserialize, Serialize};

use crate::model::{FriendGroups, LastSeen, PublicProfile, StatusMode, Team, TeamExit, TeamInvite};

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
    },
    /// The player's team changed, or they are no longer in one (`team` null).
    Team {
        team: Option<Team>,
        #[serde(default)]
        reason: Option<TeamExit>,
        /// Who made the change.
        #[serde(default)]
        by: Option<String>,
    },
    TeamInvite {
        invite: TeamInvite,
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
}
