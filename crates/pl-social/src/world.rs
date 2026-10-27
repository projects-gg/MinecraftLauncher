//! World sessions on the presence socket (HOST1, "Dünyanı arkadaşlarına aç"): the frames of the
//! service README's "World sessions" and "TURN budget" sections, as the launcher sends and hears
//! them. The tunnel itself, the paths and what the frames mean for a session live in `pl-host`;
//! this crate only carries them, so it never reads an invite's keys or a path's addresses.
//!
//! - [`WorldFrame`]: client → service (`world`, `world_invite`, `world_revoke`, `world_peer`,
//!   `world_peer_ok`, `turn_creds`, `turn_release`, `turn_held`, `world_report`). Frames the service
//!   answers go through [`crate::SocialService::world_request`] (an `ack`: [`WorldAnswer`], or an
//!   `error`: [`WorldError`]); the others through [`crate::SocialService::world_send`].
//! - [`WorldEvent`]: service → client, plus the connection's own moments (the service took
//!   `worlds2` or not, Studio's switch, the socket dropped). The launcher's host hears them all
//!   ([`crate::SocialService::on_world`]).
//!
//! Secrets: an invite's `join` (its PSK), TURN usernames and credentials, relay addresses and
//! e4mc domains never show in `Debug` output ([`Hidden`]), so no log line carries them (PROTOCOL.md
//! §13).

use std::fmt;
use std::sync::{Arc, Mutex};

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};

/// The presence capability of world sessions: asked for while the signed config's
/// `features.worlds` is on, named in `hello.caps` when the service took it. Not `worlds`: launchers
/// before HOST1-E asked for that and crashed on "Katıl", so the service never gives it any more.
pub const WORLDS_CAP: &str = "worlds2";

/// Adds [`WORLDS_CAP`] to a presence address's `caps` query.
pub(crate) fn ask_for_worlds(url: &mut url::Url) {
    let caps = url
        .query_pairs()
        .find(|(name, _)| name == "caps")
        .map(|(_, value)| value.into_owned());
    let caps = match caps {
        Some(caps) if caps.split(',').any(|cap| cap == WORLDS_CAP) => return,
        Some(caps) if !caps.is_empty() => format!("{caps},{WORLDS_CAP}"),
        _ => WORLDS_CAP.to_owned(),
    };
    // Commas stay as they are, as the service reads them (like `presence_url_with`).
    url.set_query(Some(&format!("caps={caps}")));
}

/// A value that never shows in `Debug` output or logs.
#[derive(Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Hidden<T>(pub T);

impl<T> Hidden<T> {
    pub fn expose(&self) -> &T {
        &self.0
    }

    pub fn into_inner(self) -> T {
        self.0
    }
}

impl<T> fmt::Debug for Hidden<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Studio's switch for world sessions and the guest limit (`hello.worlds`, `world_state`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct WorldsStatus {
    /// `worldsOn`: new sessions, invites and TURN credentials are given.
    pub on: bool,
    /// `worldsMaxGuests` (1–7).
    pub max_guests: u8,
}

impl Default for WorldsStatus {
    fn default() -> Self {
        Self {
            on: false,
            max_guests: 7,
        }
    }
}

/// `turn_creds.role`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum WorldRole {
    Host,
    Guest,
}

impl WorldRole {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Host => "host",
            Self::Guest => "guest",
        }
    }
}

/// A client → service world frame. `paths` and `join` are the JSON `pl-host` makes
/// (`Paths`, `Join::to_value`); this crate passes them on unread.
#[derive(Clone)]
pub enum WorldFrame {
    /// Opens, updates (count, paths) and closes a session. `invited` only after the socket
    /// reconnected.
    World {
        open: bool,
        session: String,
        guests: Option<u8>,
        max: Option<u8>,
        paths: Option<Value>,
        invited: Option<Vec<String>>,
    },
    WorldInvite {
        session: String,
        to: String,
        join: Hidden<Value>,
    },
    WorldRevoke {
        session: String,
        to: String,
    },
    /// A guest's path signal: `path` `turn` with its own relay address, or `e4mc` without.
    WorldPeer {
        session: String,
        host: String,
        slot: String,
        path: &'static str,
        relay: Option<Hidden<String>>,
    },
    /// The host's answer to a guest's signal: its current paths.
    WorldPeerOk {
        session: String,
        to: String,
        slot: String,
        paths: Hidden<Value>,
    },
    /// Which path first and, when TURN is first (or `fallback`, `renew`), TURN credentials.
    /// `host` only from a guest.
    TurnCreds {
        session: String,
        host: Option<String>,
        role: WorldRole,
        reason: &'static str,
    },
    /// The launcher is done with these credentials.
    TurnRelease {
        username: Hidden<String>,
    },
    /// After a reconnect: credentials the launcher still uses (at most 4 a frame).
    TurnHeld {
        session: String,
        role: WorldRole,
        host: Option<String>,
        usernames: Vec<Hidden<String>>,
    },
    /// Telemetry, as `pl_host::report::batch::Reporter` made it (already a whole frame).
    Report(String),
}

impl fmt::Debug for WorldFrame {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        // Only the kind: sessions, slots, addresses and credentials stay out of logs.
        f.write_str(self.kind())
    }
}

impl WorldFrame {
    /// The frame's `t`.
    pub fn kind(&self) -> &'static str {
        match self {
            Self::World { .. } => "world",
            Self::WorldInvite { .. } => "world_invite",
            Self::WorldRevoke { .. } => "world_revoke",
            Self::WorldPeer { .. } => "world_peer",
            Self::WorldPeerOk { .. } => "world_peer_ok",
            Self::TurnCreds { .. } => "turn_creds",
            Self::TurnRelease { .. } => "turn_release",
            Self::TurnHeld { .. } => "turn_held",
            Self::Report(_) => "world_report",
        }
    }

    /// The frame as the socket sends it, with `rid` when it is a request.
    pub(crate) fn text(&self, rid: Option<u32>) -> String {
        if let Self::Report(frame) = self {
            return frame.clone();
        }
        let mut object = Map::new();
        object.insert("t".into(), Value::from(self.kind()));
        let mut put = |name: &str, value: Value| {
            object.insert(name.to_owned(), value);
        };
        match self {
            Self::World {
                open,
                session,
                guests,
                max,
                paths,
                invited,
            } => {
                put("open", json!(open));
                put("session", json!(session));
                if let Some(guests) = guests {
                    put("guests", json!(guests));
                }
                if let Some(max) = max {
                    put("max", json!(max));
                }
                if let Some(paths) = paths {
                    put("paths", paths.clone());
                }
                if let Some(invited) = invited {
                    put("invited", json!(invited));
                }
            }
            Self::WorldInvite { session, to, join } => {
                put("session", json!(session));
                put("to", json!(to));
                put("join", join.0.clone());
            }
            Self::WorldRevoke { session, to } => {
                put("session", json!(session));
                put("to", json!(to));
            }
            Self::WorldPeer {
                session,
                host,
                slot,
                path,
                relay,
            } => {
                put("session", json!(session));
                put("host", json!(host));
                put("slot", json!(slot));
                put("path", json!(path));
                if let Some(relay) = relay {
                    put("relay", json!(relay.0));
                }
            }
            Self::WorldPeerOk {
                session,
                to,
                slot,
                paths,
            } => {
                put("session", json!(session));
                put("to", json!(to));
                put("slot", json!(slot));
                put("paths", paths.0.clone());
            }
            Self::TurnCreds {
                session,
                host,
                role,
                reason,
            } => {
                put("session", json!(session));
                if let Some(host) = host {
                    put("host", json!(host));
                }
                put("role", json!(role.as_str()));
                put("reason", json!(reason));
            }
            Self::TurnRelease { username } => put("username", json!(username.0)),
            Self::TurnHeld {
                session,
                role,
                host,
                usernames,
            } => {
                put("session", json!(session));
                put("role", json!(role.as_str()));
                if let Some(host) = host {
                    put("host", json!(host));
                }
                let names: Vec<&str> = usernames.iter().map(|u| u.0.as_str()).collect();
                put("usernames", json!(names));
            }
            Self::Report(_) => {}
        }
        if let Some(rid) = rid {
            object.insert("rid".into(), json!(rid));
        }
        Value::Object(object).to_string()
    }
}

/// TURN credentials from a `turn_creds` answer (`ice`).
#[derive(Clone, PartialEq, Eq, Deserialize)]
pub struct TurnIce {
    #[serde(default)]
    pub urls: Vec<String>,
    pub username: Hidden<String>,
    pub credential: Hidden<String>,
    /// Seconds.
    #[serde(default)]
    pub ttl: u64,
}

impl fmt::Debug for TurnIce {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("TurnIce")
            .field("urls", &self.urls.len())
            .field("ttl", &self.ttl)
            .finish_non_exhaustive()
    }
}

/// The first answer to `world{open:true}`: what holds (`max` lowered to Studio's limit).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorldAck {
    #[serde(default)]
    pub session: String,
    #[serde(default)]
    pub max: Option<u8>,
}

/// `e4mc` in a host's `turn_creds` answer: Studio's relay region (`auto`, `eu`, `de`, `uk`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct E4mcHint {
    #[serde(default)]
    pub region: String,
}

/// The service's `ack` of a world request.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorldAnswer {
    /// `world_invite`, `world_peer`: a socket of the friend (the host) got it.
    pub delivered: Option<bool>,
    /// `world{open:true}`.
    pub world: Option<WorldAck>,
    /// `turn_creds`: the paths to try, best first (may be empty).
    pub order: Vec<String>,
    /// `turn_creds`: TURN credentials, when TURN is first or the reason was `fallback`/`renew`.
    pub ice: Option<TurnIce>,
    /// `turn_creds` (a host's, naming e4mc): the relay region.
    pub e4mc: Option<E4mcHint>,
}

/// A world request that got no `ack`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldError {
    /// The service's `error`: `code` (`worlds_off`, `not_hosting`, `friend_not_found`,
    /// `limit_world_invites`, `rate_limited`, `account_limited`, `turn_denied`, `turn_failed`,
    /// `bad_request`, `internal`), `reason` (`turn_denied`: `off`, `budget`, `session`, `player`,
    /// `full`, `day`), `item` (`rate_limited`: `world_opens`, `world_invites`, `turn_creds`;
    /// `account_limited`: `world_invites`) and the wait in seconds.
    Refused {
        code: String,
        reason: Option<String>,
        item: Option<String>,
        retry_after: Option<u64>,
    },
    /// This connection does not take world sessions (an older service, or not asked for).
    Off,
    /// No connection, or no answer in time.
    Unavailable,
}

impl WorldError {
    /// The service's error code, when it said one.
    pub fn code(&self) -> Option<&str> {
        match self {
            Self::Refused { code, .. } => Some(code),
            _ => None,
        }
    }

    /// `rate_limited` or `account_limited` with this `item`.
    pub fn limits(&self, item: &str) -> bool {
        matches!(self, Self::Refused { code, item: Some(what), .. }
            if (code == "rate_limited" || code == "account_limited") && what == item)
    }
}

impl fmt::Display for WorldError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::Refused { code, .. } => write!(f, "world request refused: {code}"),
            Self::Off => f.write_str("world sessions are not on this connection"),
            Self::Unavailable => f.write_str("the friends service did not answer"),
        }
    }
}

impl std::error::Error for WorldError {}

/// A friend's world invite (`world_invite.invite`). `join` is what the host's launcher made for
/// this player (its PSK among it): handed to `pl-host` unread, never kept beyond the session.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct WorldInvite {
    /// The service's id (16 hex).
    #[serde(default)]
    pub id: String,
    /// The friend who opened their world.
    pub from: String,
    /// Unix seconds.
    #[serde(default)]
    pub at: i64,
    pub session: String,
    pub join: Hidden<Value>,
}

/// What the presence socket says about world sessions.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum WorldEvent {
    /// After every connect: whether the service took `worlds2` on it, and Studio's switch.
    Hello { taken: bool, status: WorldsStatus },
    /// Studio's switch or the guest limit changed (`world_state`).
    Status(WorldsStatus),
    /// The presence connection dropped (it reconnects by itself, or the player signed out).
    Down,
    /// A friend invited the player; `quiet` on do-not-disturb (listed, never announced).
    Invite { invite: WorldInvite, quiet: bool },
    /// The world the player was invited to changed: its count, or new paths.
    Update {
        from: String,
        session: String,
        guests: u8,
        max: u8,
        paths: Option<Hidden<Value>>,
    },
    /// That world closed.
    Closed { from: String, session: String },
    /// The player's invite to that world was taken back (no reason given).
    Revoked { from: String, session: String },
    /// To the host: that friend may no longer be in the world; remove them.
    RevokedGuest { session: String, to: String },
    /// To the host: a guest's path signal (`path` `turn` with its relay address, or `e4mc`).
    Peer {
        session: String,
        from: String,
        slot: String,
        path: String,
        relay: Option<Hidden<String>>,
    },
    /// To a guest: the host's paths after its signal.
    PeerOk {
        session: String,
        from: String,
        slot: String,
        paths: Hidden<Value>,
    },
    /// Move this session's TURN streams to e4mc now; the credentials go after `grace` seconds.
    TurnMove {
        session: String,
        reason: String,
        grace: u64,
    },
}

pub type WorldSink = Arc<dyn Fn(WorldEvent) + Send + Sync>;

/// The service's side of world sessions: who hears them, and what the connection took.
#[derive(Default)]
pub(crate) struct WorldsHub {
    sink: Mutex<Option<WorldSink>>,
    /// The current connection took `worlds2`, with Studio's switch (None: not taken, or no
    /// connection).
    status: Mutex<Option<WorldsStatus>>,
}

impl WorldsHub {
    pub(crate) fn set_sink(&self, sink: WorldSink) {
        *self.sink.lock().unwrap_or_else(|p| p.into_inner()) = Some(sink);
    }

    pub(crate) fn status(&self) -> Option<WorldsStatus> {
        *self.status.lock().unwrap_or_else(|p| p.into_inner())
    }

    /// Keeps what the event says about the connection, then tells the launcher.
    pub(crate) fn deliver(&self, event: WorldEvent) {
        {
            let mut status = self.status.lock().unwrap_or_else(|p| p.into_inner());
            match &event {
                WorldEvent::Hello { taken, status: now } => {
                    *status = taken.then_some(*now);
                }
                WorldEvent::Status(now) => {
                    if status.is_some() {
                        *status = Some(*now);
                    }
                }
                WorldEvent::Down => *status = None,
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

    fn value(text: &str) -> Value {
        serde_json::from_str(text).unwrap()
    }

    #[test]
    fn frames_go_out_as_the_service_reads_them() {
        let open = WorldFrame::World {
            open: true,
            session: "stuiX9ATk0i2-2_h8YikXQ".into(),
            guests: Some(0),
            max: Some(7),
            paths: None,
            invited: None,
        };
        assert_eq!(
            value(&open.text(Some(1))),
            json!({"t":"world","open":true,"session":"stuiX9ATk0i2-2_h8YikXQ","guests":0,"max":7,"rid":1})
        );
        let update = WorldFrame::World {
            open: true,
            session: "s".into(),
            guests: Some(2),
            max: Some(7),
            paths: Some(json!({"e4mc":{"domain":"able-acid.eu.e4mc.link"}})),
            invited: Some(vec!["f1".into()]),
        };
        let sent = value(&update.text(None));
        assert_eq!(sent["paths"]["e4mc"]["domain"], "able-acid.eu.e4mc.link");
        assert_eq!(sent["invited"], json!(["f1"]));
        assert!(sent.get("rid").is_none());
        let close = WorldFrame::World {
            open: false,
            session: "s".into(),
            guests: None,
            max: None,
            paths: None,
            invited: None,
        };
        assert_eq!(
            value(&close.text(Some(3))),
            json!({"t":"world","open":false,"session":"s","rid":3})
        );
        let invite = WorldFrame::WorldInvite {
            session: "s".into(),
            to: "f1".into(),
            join: Hidden(json!({"v":1,"psk":"secret"})),
        };
        assert_eq!(
            value(&invite.text(Some(4))),
            json!({"t":"world_invite","session":"s","to":"f1","join":{"v":1,"psk":"secret"},"rid":4})
        );
        let peer = WorldFrame::WorldPeer {
            session: "s".into(),
            host: "h".into(),
            slot: "slot".into(),
            path: "turn",
            relay: Some(Hidden("162.159.207.10:40001".into())),
        };
        assert_eq!(
            value(&peer.text(Some(6))),
            json!({"t":"world_peer","session":"s","host":"h","slot":"slot","path":"turn","relay":"162.159.207.10:40001","rid":6})
        );
        let e4mc = WorldFrame::WorldPeer {
            session: "s".into(),
            host: "h".into(),
            slot: "slot".into(),
            path: "e4mc",
            relay: None,
        };
        assert!(value(&e4mc.text(Some(7))).get("relay").is_none());
        let creds = WorldFrame::TurnCreds {
            session: "s".into(),
            host: Some("h".into()),
            role: WorldRole::Guest,
            reason: "fallback",
        };
        assert_eq!(
            value(&creds.text(Some(9))),
            json!({"t":"turn_creds","session":"s","host":"h","role":"guest","reason":"fallback","rid":9})
        );
        let host_creds = WorldFrame::TurnCreds {
            session: "s".into(),
            host: None,
            role: WorldRole::Host,
            reason: "primary",
        };
        assert!(value(&host_creds.text(Some(8))).get("host").is_none());
        let release = WorldFrame::TurnRelease {
            username: Hidden("u1".into()),
        };
        assert_eq!(
            value(&release.text(None)),
            json!({"t":"turn_release","username":"u1"})
        );
        let held = WorldFrame::TurnHeld {
            session: "s".into(),
            role: WorldRole::Guest,
            host: Some("h".into()),
            usernames: vec![Hidden("u1".into()), Hidden("u2".into())],
        };
        assert_eq!(
            value(&held.text(None)),
            json!({"t":"turn_held","session":"s","role":"guest","host":"h","usernames":["u1","u2"]})
        );
        let ok = WorldFrame::WorldPeerOk {
            session: "s".into(),
            to: "f1".into(),
            slot: "slot".into(),
            paths: Hidden(json!({})),
        };
        assert_eq!(
            value(&ok.text(None)),
            json!({"t":"world_peer_ok","session":"s","to":"f1","slot":"slot","paths":{}})
        );
        let report = r#"{"t":"world_report","mc":"26.1.2","lv":"2.1.0","r":[]}"#;
        assert_eq!(WorldFrame::Report(report.into()).text(Some(5)), report);
    }

    #[test]
    fn nothing_secret_reaches_debug_output() {
        let frames = [
            WorldFrame::WorldInvite {
                session: "sess".into(),
                to: "f1".into(),
                join: Hidden(json!({"psk":"PSK-SECRET"})),
            },
            WorldFrame::TurnRelease {
                username: Hidden("USER-SECRET".into()),
            },
            WorldFrame::WorldPeer {
                session: "sess".into(),
                host: "h".into(),
                slot: "SLOT-SECRET".into(),
                path: "turn",
                relay: Some(Hidden("162.159.207.10:40001".into())),
            },
        ];
        for frame in &frames {
            let shown = format!("{frame:?}");
            for secret in [
                "PSK-SECRET",
                "USER-SECRET",
                "SLOT-SECRET",
                "162.159",
                "sess",
            ] {
                assert!(!shown.contains(secret), "{shown}");
            }
        }
        let ice: TurnIce = serde_json::from_value(json!({
            "urls": ["turn:turn.cloudflare.com:3478?transport=udp"],
            "username": "USER-SECRET",
            "credential": "PASS-SECRET",
            "ttl": 7200
        }))
        .unwrap();
        let shown = format!("{ice:?}");
        assert!(
            !shown.contains("SECRET") && !shown.contains("cloudflare"),
            "{shown}"
        );
        assert_eq!(ice.credential.expose(), "PASS-SECRET");
        let invite: WorldInvite = serde_json::from_value(json!({
            "id": "0123456789abcdef", "from": "f1", "at": 5, "session": "s",
            "join": {"psk": "PSK-SECRET"}
        }))
        .unwrap();
        assert!(!format!("{invite:?}").contains("PSK-SECRET"));
    }

    #[test]
    fn worlds_join_the_capabilities_once() {
        let mut url = url::Url::parse("wss://x.test/v1/presence?caps=callServer,crews").unwrap();
        ask_for_worlds(&mut url);
        assert_eq!(url.query(), Some("caps=callServer,crews,worlds2"));
        ask_for_worlds(&mut url);
        assert_eq!(url.query(), Some("caps=callServer,crews,worlds2"));
        let mut bare = url::Url::parse("wss://x.test/v1/presence").unwrap();
        ask_for_worlds(&mut bare);
        assert_eq!(bare.query(), Some("caps=worlds2"));
        // The name launchers before HOST1-E asked for is never asked for again.
        assert_ne!(WORLDS_CAP, "worlds");
    }

    #[test]
    fn errors_name_their_limits() {
        let daily = WorldError::Refused {
            code: "rate_limited".into(),
            reason: None,
            item: Some("world_invites".into()),
            retry_after: Some(60),
        };
        assert!(daily.limits("world_invites") && !daily.limits("world_opens"));
        assert_eq!(daily.code(), Some("rate_limited"));
        assert_eq!(WorldError::Off.code(), None);
    }

    #[test]
    fn the_hub_keeps_what_the_connection_took() {
        let hub = WorldsHub::default();
        let heard = Arc::new(Mutex::new(Vec::new()));
        let into = heard.clone();
        hub.set_sink(Arc::new(move |event| into.lock().unwrap().push(event)));
        assert_eq!(hub.status(), None);
        let on = WorldsStatus {
            on: true,
            max_guests: 5,
        };
        hub.deliver(WorldEvent::Hello {
            taken: true,
            status: on,
        });
        assert_eq!(hub.status(), Some(on));
        let off = WorldsStatus {
            on: false,
            max_guests: 5,
        };
        hub.deliver(WorldEvent::Status(off));
        assert_eq!(hub.status(), Some(off));
        hub.deliver(WorldEvent::Down);
        assert_eq!(hub.status(), None);
        // A switch after an older service's hello means nothing.
        hub.deliver(WorldEvent::Hello {
            taken: false,
            status: WorldsStatus::default(),
        });
        hub.deliver(WorldEvent::Status(on));
        assert_eq!(hub.status(), None);
        assert_eq!(heard.lock().unwrap().len(), 5);
    }
}
