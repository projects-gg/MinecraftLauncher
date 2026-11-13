//! SQOL: the voice room's own text chat (capability [`crate::share::RCHAT_CAP`], the lead's
//! contract `docs/tasks/sqol.md` §1.3, §2.2): what the page sees of it and asks, and the room's
//! messages read leniently. The machine's part is `rchat_machine.rs`.
//!
//! Messages live in memory for the stay only (at most [`LOG_MAX`], the room's own log on a new
//! socket replaces them); the room's seal stays with each one, never shown, for a report. The room
//! checks every rule (filters, links, a limited account, a chat mute, the rate); the launcher only
//! refuses what can never pass (an empty text, too long, too many lines).

use serde::Serialize;

/// The room's messages the launcher keeps for its page (the room sends at most this many on
/// `join`).
pub const LOG_MAX: usize = 50;
/// A text's length at most, in UTF-16 units as the room counts (the friends' chat's 300; more
/// than five lines the room joins into the fifth).
pub const TEXT_MAX_CHARS: usize = 300;
/// A report takes the reported message and at most this many before it.
pub const REPORT_BEFORE: usize = 20;
/// Ids (messages, people, stickers) longer than this are no ids.
const ID_MAX: usize = 64;

/// One message of the room's chat.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RoomChatMsg {
    pub id: String,
    /// Who sent it (the player too).
    pub from: String,
    /// The room's time of it (Unix milliseconds).
    pub at: i64,
    /// Its text, or none for a sticker.
    pub text: Option<String>,
    /// A sticker's id ("Küp" stickers), or none for a text.
    pub sticker: Option<String>,
}

fn id_like(value: &str) -> bool {
    !value.is_empty() && value.len() <= ID_MAX && value.chars().all(|c| c.is_ascii_graphic())
}

impl RoomChatMsg {
    /// A message as the room sent it (`m`): none when it does not read (an id, a sender and a body
    /// at least). A text keeps what the room let through, control characters other than line
    /// breaks taken out.
    pub fn of(raw: &serde_json::Value) -> Option<Self> {
        let id = raw.get("id")?.as_str().filter(|id| id_like(id))?;
        let from = raw.get("u")?.as_str().filter(|u| id_like(u))?;
        let at = raw
            .get("at")
            .and_then(serde_json::Value::as_i64)
            .unwrap_or(0);
        let text = raw
            .get("text")
            .and_then(serde_json::Value::as_str)
            .map(|text| {
                text.chars()
                    .filter(|c| *c == '\n' || !c.is_control())
                    .collect::<String>()
            })
            .filter(|text| !text.trim().is_empty());
        let sticker = raw
            .get("st")
            .and_then(serde_json::Value::as_str)
            .filter(|sticker| id_like(sticker))
            .map(str::to_owned);
        if text.is_none() && sticker.is_none() {
            return None;
        }
        Some(Self {
            id: id.to_owned(),
            from: from.to_owned(),
            at,
            text: if sticker.is_some() { None } else { text },
            sticker,
        })
    }
}

/// Why a message (or a deletion, or a report) did not go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RoomChatError {
    /// The room has no chat now (Studio, the chat switch, an older room).
    Off,
    /// Studio switched the chat's stickers off.
    StickersOff,
    /// The filters refused it (`reason`, `rule`).
    Rejected,
    /// Too many at once (`retryAfter`).
    RateLimited,
    /// A chat mute of Projects' moderators runs (`until`).
    Muted,
    /// A new (limited) account may not send that (a link; `until`).
    Limited,
    /// Deleting someone else's message without the right to.
    NotAllowed,
    /// Not in that room (any more), or no such message.
    Gone,
    /// The room did not answer, or did not take it.
    Failed,
}

impl RoomChatError {
    /// The room's error code.
    pub fn from_code(code: &str) -> Self {
        match code {
            "chat_off" => Self::Off,
            "chat_feature_off" => Self::StickersOff,
            "chat_rejected" => Self::Rejected,
            "rate_limited" => Self::RateLimited,
            "chat_muted" => Self::Muted,
            "limited" => Self::Limited,
            "not_allowed" => Self::NotAllowed,
            "not_found" | "gone" => Self::Gone,
            _ => Self::Failed,
        }
    }
}

/// What the filters found (`chat_rejected`'s `reason`, as the friends' chat names them), or the
/// launcher's own `length`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RoomChatReason {
    Phone,
    Link,
    Words,
    Length,
}

impl RoomChatReason {
    pub fn from_wire(reason: &str) -> Option<Self> {
        Some(match reason {
            "phone" => Self::Phone,
            "link" => Self::Link,
            "words" | "word" => Self::Words,
            "length" => Self::Length,
            _ => return None,
        })
    }
}

/// The answer to `voice_chat_send` (and, inside the launcher, to a deletion).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum RoomChatSend {
    /// The room took it: its id and time (the message itself comes as `voice-chat` too).
    Sent { id: String, at: i64 },
    Refused {
        error: RoomChatError,
        reason: Option<RoomChatReason>,
        /// The link rule that matched (`chat_rejected`, `link`).
        rule: Option<String>,
        /// Seconds to wait (`rateLimited`).
        #[serde(rename = "retryAfter")]
        retry_after: Option<u64>,
        /// A chat mute's end (Unix seconds, `muted`).
        until: Option<i64>,
    },
}

impl RoomChatSend {
    pub fn refused(error: RoomChatError) -> Self {
        Self::Refused {
            error,
            reason: None,
            rule: None,
            retry_after: None,
            until: None,
        }
    }

    /// The room's `error` frame as an answer.
    pub fn of_error(
        code: &str,
        reason: Option<&str>,
        rule: Option<String>,
        retry_after: Option<u64>,
        until: Option<i64>,
    ) -> Self {
        Self::Refused {
            error: RoomChatError::from_code(code),
            reason: reason.and_then(RoomChatReason::from_wire),
            rule,
            retry_after,
            until,
        }
    }

    /// As a deletion's or a report's answer: none when it went.
    pub fn error(&self) -> Option<RoomChatError> {
        match self {
            Self::Sent { .. } => None,
            Self::Refused { error, .. } => Some(*error),
        }
    }
}

/// Who deleted a message (`rchat_deleted.by`): its sender, the room's leader or a moderator, or
/// Projects' moderators.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum RoomChatBy {
    #[serde(rename = "self")]
    Sender,
    Mod,
    Staff,
}

impl RoomChatBy {
    pub fn from_wire(by: &str) -> Self {
        match by {
            "mod" => Self::Mod,
            "staff" => Self::Staff,
            _ => Self::Sender,
        }
    }
}

/// The room chat's news for the page (`voice-chat`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum VoiceChatEvent {
    /// A new message (the player's own too).
    Msg { room: String, msg: RoomChatMsg },
    /// A message went.
    Del {
        room: String,
        id: String,
        by: RoomChatBy,
    },
    /// The whole log anew (joining, coming back, someone blocked): replaces what the page shows.
    Log {
        room: String,
        msgs: Vec<RoomChatMsg>,
    },
}

/// What the player sends: a text or a sticker.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatBody {
    Text(String),
    Sticker(String),
}

/// The page's message, or why it can never go (the room checks everything else): a text is
/// trimmed and at most [`TEXT_MAX_CHARS`] long; a sticker is an id.
pub fn body_of(text: Option<&str>, sticker: Option<&str>) -> Result<ChatBody, RoomChatSend> {
    match (text.map(str::trim).filter(|text| !text.is_empty()), sticker) {
        (Some(_), Some(_)) | (None, None) => Err(RoomChatSend::refused(RoomChatError::Failed)),
        (None, Some(sticker)) if id_like(sticker) => Ok(ChatBody::Sticker(sticker.to_owned())),
        (None, Some(_)) => Err(RoomChatSend::refused(RoomChatError::Failed)),
        (Some(text), None) => {
            let text = text.replace("\r\n", "\n");
            if text.encode_utf16().count() > TEXT_MAX_CHARS {
                Err(RoomChatSend::Refused {
                    error: RoomChatError::Rejected,
                    reason: Some(RoomChatReason::Length),
                    rule: None,
                    retry_after: None,
                    until: None,
                })
            } else {
                Ok(ChatBody::Text(text))
            }
        }
    }
}

/// The room's log on a socket (`rchat_log`) merged into what is held (`held`, oldest first), as
/// the room says to: what is older than the log's first message stays, the rest is the log's; an
/// empty log keeps everything (the room's history may be gone, the page's is not).
pub fn merge_log<T>(held: &mut std::collections::VecDeque<T>, log: Vec<T>, at: impl Fn(&T) -> i64) {
    let Some(first) = log.first().map(&at) else {
        return;
    };
    held.retain(|item| at(item) < first);
    held.extend(log);
    while held.len() > LOG_MAX {
        held.pop_front();
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn messages_read_leniently() {
        let msg = RoomChatMsg::of(
            &json!({"id":"m1","u":"u2","at":1_731_500_000_123_i64,"text":"selam\u{7}\nnaber","seal":"x"}),
        )
        .unwrap();
        assert_eq!(msg.text.as_deref(), Some("selam\nnaber"));
        assert_eq!((msg.at, msg.sticker), (1_731_500_000_123, None));
        let sticker = RoomChatMsg::of(&json!({"id":"m2","u":"u2","st":"pl1/gg"})).unwrap();
        assert_eq!(sticker.sticker.as_deref(), Some("pl1/gg"));
        assert_eq!(sticker.text, None);
        for odd in [
            json!({"u":"u2","text":"no id"}),
            json!({"id":"m3","text":"no sender"}),
            json!({"id":"m3","u":"u2"}),
            json!({"id":"m3","u":"u2","text":"   "}),
            json!({"id":"m3","u":"u 2","text":"an odd sender"}),
            json!("not an object"),
        ] {
            assert_eq!(RoomChatMsg::of(&odd), None, "{odd}");
        }
    }

    #[test]
    fn only_what_can_pass_goes() {
        assert_eq!(
            body_of(Some("  selam  "), None),
            Ok(ChatBody::Text("selam".into()))
        );
        assert_eq!(
            body_of(None, Some("pl1/gg")),
            Ok(ChatBody::Sticker("pl1/gg".into()))
        );
        assert!(body_of(Some(" "), None).is_err());
        assert!(body_of(Some("a"), Some("pl1/gg")).is_err());
        assert!(body_of(None, Some("bad id")).is_err());
        let long = "ş".repeat(TEXT_MAX_CHARS + 1);
        assert!(matches!(
            body_of(Some(&long), None),
            Err(RoomChatSend::Refused {
                reason: Some(RoomChatReason::Length),
                ..
            })
        ));
        assert!(body_of(Some(&"ş".repeat(TEXT_MAX_CHARS)), None).is_ok());
        // Counted as the room counts (an emoji is two).
        assert!(body_of(Some(&"🎮".repeat(TEXT_MAX_CHARS / 2 + 1)), None).is_err());
        // More than five lines: the room joins them, nothing to refuse here.
        assert!(body_of(Some("1\n2\n3\n4\n5\n6"), None).is_ok());
    }

    #[test]
    fn the_rooms_log_merges_as_the_room_says() {
        use std::collections::VecDeque;
        let mut held: VecDeque<i64> = VecDeque::from([1, 2, 3, 4]);
        // Older than the log's first stays; the rest is the log's.
        merge_log(&mut held, vec![3, 5], |at| *at);
        assert_eq!(held, [1, 2, 3, 5]);
        // An empty log keeps what is held.
        merge_log(&mut held, Vec::new(), |at| *at);
        assert_eq!(held.len(), 4);
        // At most the log's size, the oldest going.
        merge_log(&mut held, (100..160).collect(), |at| *at);
        assert_eq!(held.len(), LOG_MAX);
        assert_eq!(held[0], 110);
    }

    #[test]
    fn written_as_the_page_reads_them() {
        assert_eq!(
            serde_json::to_value(RoomChatSend::of_error(
                "rate_limited",
                None,
                None,
                Some(3),
                None
            ))
            .unwrap(),
            json!({"kind":"refused","error":"rateLimited","reason":null,"rule":null,"retryAfter":3,"until":null})
        );
        assert_eq!(
            serde_json::to_value(VoiceChatEvent::Del {
                room: "f0".into(),
                id: "m1".into(),
                by: RoomChatBy::from_wire("self"),
            })
            .unwrap(),
            json!({"kind":"del","room":"f0","id":"m1","by":"self"})
        );
        assert_eq!(
            RoomChatSend::of_error("chat_rejected", Some("link"), Some("r".into()), None, None)
                .error(),
            Some(RoomChatError::Rejected)
        );
        assert_eq!(
            RoomChatReason::from_wire("words"),
            Some(RoomChatReason::Words)
        );
        assert_eq!(
            RoomChatError::from_code("chat_feature_off"),
            RoomChatError::StickersOff
        );
        assert_eq!(
            RoomChatError::from_code("not_allowed"),
            RoomChatError::NotAllowed
        );
        assert_eq!(RoomChatError::from_code("anything"), RoomChatError::Failed);
    }
}
