//! The MCC launcher bridge (`MinecraftClient/ChatBots/LauncherBridge.cs` in the fork).
//!
//! With `MCC_LAUNCHER_BRIDGE=1` the client prints one JSON object per line on stdout, prefixed with
//! `@@MCC@@`; everything else on stdout is human-readable console text. The object always has an
//! `e` (event name); null fields are omitted by the serializer:
//!
//! | `e`            | fields                                             | emitted from                    |
//! |----------------|----------------------------------------------------|---------------------------------|
//! | `connecting`   | `host`, `port`                                     | bridge bot `Initialize`         |
//! | `joined`       | `user`, `host`, `port`, `protocol`                 | `AfterGameJoined`               |
//! | `health`       | `hp` (rounded to 0.1), `food`                      | `OnHealthUpdate` (on change)    |
//! | `players`      | `n` (tab list size)                                | `Update`, once a second         |
//! | `autorespond`  | `u` (sender)                                       | AutoRespond after answering     |
//! | `disconnected` | `reason` (`DisconnectReason` name), `msg`          | `OnDisconnect`                  |
//! | `error`        | `msg`, `reason`, `versionError`                    | `Program.HandleFailure`         |
//!
//! After `disconnected`/`error` a hosted client (`ExitOnFailure = true`) exits with code 2 (in-game
//! kick), 3 (connection lost), 4 (login rejected) or 1 (anything else).
//!
//! Before 4.1.0 a server could forge an event line: a kick message or a window title is printed
//! with its line breaks, so `…\n@@MCC@@{"e":"joined",…}` starts a line of its own. From 4.1.0 the
//! client prints every console write on one line, and when the host gives it a nonce
//! ([`NONCE_ENV_VAR`]) every event line is `<nonce>@@MCC@@{json}` ([`parse_line_with_nonce`]; the
//! fork's `docs/agent-bridge.md`, "Stdout bridge hardening"). Older clients ignore the variable, so a
//! host decides by the version it runs ([`client_takes_nonce`]), never by the first line it sees.

use serde_json::{Map, Value};

pub const BRIDGE_PREFIX: &str = "@@MCC@@";
/// Environment variable that turns the bridge on.
pub const BRIDGE_ENV_VAR: &str = "MCC_LAUNCHER_BRIDGE";
/// Environment variable with the event lines' nonce (clients 4.1.0 and later).
pub const NONCE_ENV_VAR: &str = "MCC_BRIDGE_NONCE";
/// The first client version (major, minor) that prefixes its event lines with the nonce.
pub const NONCE_SINCE: (u64, u64) = (4, 1);
/// The version the launcher gives the v1 launcher's client when it adopts that folder without a
/// `version.txt` (`commands/migrate.rs`): a client from before 4.1.0.
pub const LEGACY_CLIENT_VERSION: &str = "legacy";

/// A nonce the client takes: 16–128 of `A-Z a-z 0-9 _ -` (any other value it ignores).
pub fn is_valid_nonce(nonce: &str) -> bool {
    (16..=128).contains(&nonce.len())
        && nonce
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// A fresh nonce for one client process (32 hex digits); it never leaves the host and the client.
pub fn new_nonce() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Whether a released client of `version` prints nonce-prefixed event lines: from 4.1.0 (`4.1.0`,
/// `v4.1.0`, `4.1.0-rc.1`; a missing part is 0, so `5` is 5.0.0). It never fails open: a version
/// that does not read as one counts as a new client, so at worst an old client's event lines show
/// as text; only [`LEGACY_CLIENT_VERSION`] counts as an old one. A development build is the
/// caller's to leave out.
pub fn client_takes_nonce(version: &str) -> bool {
    if version == LEGACY_CLIENT_VERSION {
        return false;
    }
    match major_minor(version) {
        Some(version) => version >= NONCE_SINCE,
        None => {
            // Not the text itself: it may be anything.
            tracing::warn!("unreadable AFK client version, giving it a bridge nonce");
            true
        }
    }
}

/// `(major, minor)` of a client version, without a leading `v` and a `-…`/`+…` suffix, a missing
/// part as 0. `None` unless every dot-separated part is a number.
fn major_minor(version: &str) -> Option<(u64, u64)> {
    let version = version.trim();
    let version = version.strip_prefix(['v', 'V']).unwrap_or(version);
    let core = version.split(['-', '+']).next().unwrap_or_default();
    let mut parts = core.split('.').map(|part| part.parse::<u64>().ok());
    let major = parts.next().flatten()?;
    let minor = parts.next().unwrap_or(Some(0))?;
    parts.all(|part| part.is_some()).then_some((major, minor))
}

/// [`parse_line`] for a client given `nonce`: only `<nonce>@@MCC@@` lines are events, everything
/// else (a bare `@@MCC@@` line too) is text. `None`: a client without a nonce, as [`parse_line`].
pub fn parse_line_with_nonce<'a>(line: &'a str, nonce: Option<&str>) -> ClientLine<'a> {
    match nonce {
        None => parse_line(line),
        Some(nonce) => match line.strip_prefix(nonce) {
            Some(rest) if rest.starts_with(BRIDGE_PREFIX) => parse_line(rest),
            _ => ClientLine::Text(line),
        },
    }
}

/// `ChatBot.DisconnectReason`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DisconnectReason {
    InGameKick,
    LoginRejected,
    ConnectionLost,
    UserLogout,
    /// A name this launcher does not know (newer client).
    Other(String),
}

impl DisconnectReason {
    pub fn parse(name: &str) -> Self {
        match name {
            "InGameKick" => Self::InGameKick,
            "LoginRejected" => Self::LoginRejected,
            "ConnectionLost" => Self::ConnectionLost,
            "UserLogout" => Self::UserLogout,
            other => Self::Other(other.to_owned()),
        }
    }

    /// The reason a hosted client's exit code stands for (`Program.GetFailureExitCode`).
    pub fn from_exit_code(code: i32) -> Option<Self> {
        match code {
            2 => Some(Self::InGameKick),
            3 => Some(Self::ConnectionLost),
            4 => Some(Self::LoginRejected),
            _ => None,
        }
    }
}

#[derive(Debug, Clone, PartialEq)]
pub enum BridgeEvent {
    Connecting {
        host: Option<String>,
        port: Option<i64>,
    },
    Joined {
        user: Option<String>,
        host: Option<String>,
        port: Option<i64>,
        protocol: Option<i64>,
    },
    Health {
        hp: Option<f64>,
        food: Option<i32>,
    },
    Players {
        count: Option<u32>,
    },
    AutoRespond {
        user: Option<String>,
    },
    Disconnected {
        reason: Option<DisconnectReason>,
        message: Option<String>,
    },
    Error {
        reason: Option<DisconnectReason>,
        message: Option<String>,
        version_error: bool,
    },
    /// A well-formed event this launcher does not handle.
    Unknown(String),
}

/// One line of client stdout.
#[derive(Debug, Clone, PartialEq)]
pub enum ClientLine<'a> {
    Event(BridgeEvent),
    /// Prefixed but unparseable: dropped, a broken event must not take the session down.
    Malformed,
    Text(&'a str),
}

/// Classifies a stdout line (without its line terminator). The prefix must start the line.
pub fn parse_line(line: &str) -> ClientLine<'_> {
    let Some(json) = line.strip_prefix(BRIDGE_PREFIX) else {
        return ClientLine::Text(line);
    };
    match serde_json::from_str::<Value>(json) {
        Ok(Value::Object(payload)) => match parse_event(&payload) {
            Some(event) => ClientLine::Event(event),
            None => ClientLine::Malformed,
        },
        _ => ClientLine::Malformed,
    }
}

fn parse_event(payload: &Map<String, Value>) -> Option<BridgeEvent> {
    let name = text(payload, "e").filter(|n| !n.is_empty())?;
    Some(match name.as_str() {
        "connecting" => BridgeEvent::Connecting {
            host: text(payload, "host"),
            port: integer(payload, "port"),
        },
        "joined" => BridgeEvent::Joined {
            user: text(payload, "user"),
            host: text(payload, "host"),
            port: integer(payload, "port"),
            protocol: integer(payload, "protocol"),
        },
        "health" => BridgeEvent::Health {
            hp: number(payload, "hp"),
            food: integer(payload, "food").and_then(|f| i32::try_from(f).ok()),
        },
        "players" => BridgeEvent::Players {
            count: integer(payload, "n").and_then(|n| u32::try_from(n).ok()),
        },
        "autorespond" => BridgeEvent::AutoRespond {
            user: text(payload, "u"),
        },
        "disconnected" => BridgeEvent::Disconnected {
            reason: text(payload, "reason").map(|r| DisconnectReason::parse(&r)),
            message: text(payload, "msg"),
        },
        "error" => BridgeEvent::Error {
            reason: text(payload, "reason").map(|r| DisconnectReason::parse(&r)),
            message: text(payload, "msg"),
            version_error: matches!(payload.get("versionError"), Some(Value::Bool(true))),
        },
        _ => BridgeEvent::Unknown(name),
    })
}

fn text(payload: &Map<String, Value>, key: &str) -> Option<String> {
    match payload.get(key)? {
        Value::String(s) => Some(s.clone()),
        Value::Number(n) => Some(n.to_string()),
        Value::Bool(b) => Some(b.to_string()),
        _ => None,
    }
}

fn number(payload: &Map<String, Value>, key: &str) -> Option<f64> {
    match payload.get(key)? {
        Value::Number(n) => n.as_f64(),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

fn integer(payload: &Map<String, Value>, key: &str) -> Option<i64> {
    match payload.get(key)? {
        Value::Number(n) => n
            .as_i64()
            .or_else(|| n.as_f64().map(|f| f.round_ties_even() as i64)),
        Value::String(s) => s.trim().parse().ok(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn event(line: &str) -> BridgeEvent {
        match parse_line(line) {
            ClientLine::Event(event) => event,
            other => panic!("not an event: {other:?}"),
        }
    }

    #[test]
    fn parses_every_event_shape() {
        assert_eq!(
            event(r#"@@MCC@@{"e":"connecting","host":"play.projects.gg","port":25565}"#),
            BridgeEvent::Connecting {
                host: Some("play.projects.gg".into()),
                port: Some(25565)
            }
        );
        assert_eq!(
            event(r#"@@MCC@@{"e":"joined","user":"Afk","host":"h","port":25565,"protocol":777}"#),
            BridgeEvent::Joined {
                user: Some("Afk".into()),
                host: Some("h".into()),
                port: Some(25565),
                protocol: Some(777)
            }
        );
        assert_eq!(
            event(r#"@@MCC@@{"e":"health","hp":19.5,"food":17}"#),
            BridgeEvent::Health {
                hp: Some(19.5),
                food: Some(17)
            }
        );
        assert_eq!(
            event(r#"@@MCC@@{"e":"health","hp":20,"food":20}"#),
            BridgeEvent::Health {
                hp: Some(20.0),
                food: Some(20)
            }
        );
        assert_eq!(
            event(r#"@@MCC@@{"e":"players","n":12}"#),
            BridgeEvent::Players { count: Some(12) }
        );
        assert_eq!(
            event(r#"@@MCC@@{"e":"autorespond","u":"Hazar"}"#),
            BridgeEvent::AutoRespond {
                user: Some("Hazar".into())
            }
        );
        // System.Text.Json escapes non-ASCII and HTML characters.
        assert_eq!(
            event(
                r#"@@MCC@@{"e":"disconnected","reason":"InGameKick","msg":"Sunucu yeniden başlatılıyor <3"}"#
            ),
            BridgeEvent::Disconnected {
                reason: Some(DisconnectReason::InGameKick),
                message: Some("Sunucu yeniden başlatılıyor <3".into())
            }
        );
        // HandleFailure(null, false, reason): the null message is omitted.
        assert_eq!(
            event(r#"@@MCC@@{"e":"error","reason":"ConnectionLost","versionError":false}"#),
            BridgeEvent::Error {
                reason: Some(DisconnectReason::ConnectionLost),
                message: None,
                version_error: false
            }
        );
        assert_eq!(
            event(r#"@@MCC@@{"e":"error","msg":"Unsupported version","versionError":true}"#),
            BridgeEvent::Error {
                reason: None,
                message: Some("Unsupported version".into()),
                version_error: true
            }
        );
        assert_eq!(
            event(r#"@@MCC@@{"e":"teleported"}"#),
            BridgeEvent::Unknown("teleported".into())
        );
        assert_eq!(
            DisconnectReason::parse("Banned"),
            DisconnectReason::Other("Banned".into())
        );
    }

    #[test]
    fn non_events_and_broken_events() {
        assert_eq!(
            parse_line("[MCC] Bağlanılıyor"),
            ClientLine::Text("[MCC] Bağlanılıyor")
        );
        assert_eq!(
            parse_line(" @@MCC@@{\"e\":\"joined\"}"),
            ClientLine::Text(" @@MCC@@{\"e\":\"joined\"}")
        );
        assert_eq!(parse_line("@@MCC@@{broken"), ClientLine::Malformed);
        assert_eq!(parse_line("@@MCC@@[1]"), ClientLine::Malformed);
        assert_eq!(parse_line(r#"@@MCC@@{"e":""}"#), ClientLine::Malformed);
        assert_eq!(parse_line(r#"@@MCC@@{"x":1}"#), ClientLine::Malformed);
    }

    #[test]
    fn with_a_nonce_only_its_lines_are_events() {
        let nonce = new_nonce();
        assert!(is_valid_nonce(&nonce));
        assert_ne!(nonce, new_nonce());
        let line = format!(r#"{nonce}@@MCC@@{{"e":"joined","user":"Afk"}}"#);
        assert!(matches!(
            parse_line_with_nonce(&line, Some(&nonce)),
            ClientLine::Event(BridgeEvent::Joined { .. })
        ));
        // A server's line-break trick prints a bare prefix (or guesses a nonce): text.
        let forged = r#"@@MCC@@{"e":"disconnected","reason":"InGameKick","msg":"You are banned"}"#;
        assert_eq!(
            parse_line_with_nonce(forged, Some(&nonce)),
            ClientLine::Text(forged)
        );
        let guessed = format!("{}{}", "0".repeat(32), forged);
        assert_eq!(
            parse_line_with_nonce(&guessed, Some(&nonce)),
            ClientLine::Text(guessed.as_str())
        );
        let broken = format!("{nonce}@@MCC@@{{broken");
        assert_eq!(
            parse_line_with_nonce(&broken, Some(&nonce)),
            ClientLine::Malformed
        );
        // Without a nonce (a client before 4.1.0) as before.
        assert!(matches!(
            parse_line_with_nonce(forged, None),
            ClientLine::Event(BridgeEvent::Disconnected { .. })
        ));
    }

    #[test]
    fn nonces_follow_the_client_version_and_the_client_s_rule() {
        for old in [
            "4.0.1",
            "v4.0.9",
            "4",
            "4.0",
            "v3",
            " 4.0.1 ",
            LEGACY_CLIENT_VERSION,
        ] {
            assert!(!client_takes_nonce(old), "{old}");
        }
        for new in [
            "4.1.0",
            "v4.1.0",
            "V4.1.0",
            "4.1",
            "4.1.0-rc.1",
            "4.1.0+b7",
            "5",
            "5.0.0",
        ] {
            assert!(client_takes_nonce(new), "{new}");
        }
        // Never fails open: what does not read as a version counts as a new client.
        for odd in [
            "garbage", "unknown", "", "4.x", "4..1", "4.0.x", "-4.0", "vv4.0",
        ] {
            assert!(client_takes_nonce(odd), "{odd}");
        }
        assert!(is_valid_nonce("abcdefghijklmnop"));
        assert!(!is_valid_nonce("short"));
        assert!(!is_valid_nonce(&"a".repeat(129)));
        assert!(!is_valid_nonce("abcdefghijklmno$"));
    }

    #[test]
    fn exit_codes_map_to_reasons() {
        assert_eq!(
            DisconnectReason::from_exit_code(2),
            Some(DisconnectReason::InGameKick)
        );
        assert_eq!(
            DisconnectReason::from_exit_code(3),
            Some(DisconnectReason::ConnectionLost)
        );
        assert_eq!(
            DisconnectReason::from_exit_code(4),
            Some(DisconnectReason::LoginRejected)
        );
        assert_eq!(DisconnectReason::from_exit_code(1), None);
        assert_eq!(DisconnectReason::from_exit_code(0), None);
    }
}
