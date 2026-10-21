//! Chat (CHAT1) as the launcher shows it: the history's entries as views (times on this
//! computer's clock, what the player may still do with each), why a message did not go, and the
//! checks made before anything is sent. The service checks everything again; the launcher's
//! fuller text check (links, phone numbers) runs in the composer as the player types.

use std::time::Duration;

use crate::error::{ErrorCode, SocialError};
use crate::history::{Change, Entry};
use crate::model::{ChatMessage, ChatRejection};
use crate::view::{ChatChangeView, ChatMessageView, MessageState, SendFailure};

/// A message may be edited this long after it was sent.
pub const EDIT_WINDOW_MS: i64 = 15 * 60_000;
/// A message may be deleted for everyone this long after it was sent.
pub const DELETE_WINDOW_MS: i64 = 7 * 86_400_000;
/// A message that waited longer than this is not sent by itself any more (the service remembers a
/// resend's nonce this long): it waits for "Tekrar dene".
pub const RESEND_WINDOW_MS: i64 = 15 * 60_000;
/// Characters in a message (Unicode code points), as the service counts them after cleaning.
pub const CHAT_TEXT_MAX: usize = 300;
/// Lines in a message.
pub const CHAT_LINES_MAX: usize = 5;
/// Messages a page of a conversation brings.
pub const PAGE_SIZE: usize = 60;
/// Earlier messages a message report carries.
pub const REPORT_CONTEXT: usize = 20;
/// "Too fast" waits at most this long before the message goes again by itself (once).
pub const TOO_FAST_WAIT_MAX: Duration = Duration::from_secs(5);

const NONCE_CHARS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789";

/// A new nonce: this device's own key for a message (12 of `A-Z a-z 0-9`).
pub fn new_nonce() -> String {
    (0..12)
        .map(|_| char::from(NONCE_CHARS[rand::random_range(0..NONCE_CHARS.len())]))
        .collect()
}

/// The text as it will go: line breaks kept (at most five lines; more are joined into the fifth),
/// spaces collapsed, trimmed; None when nothing is left or it is longer than the service takes.
pub fn prepare_text(text: &str) -> Option<String> {
    if text.len() > 4_000 {
        return None;
    }
    let mut lines: Vec<String> = text
        .replace("\r\n", "\n")
        .replace('\r', "\n")
        .split('\n')
        .map(|line| line.split_whitespace().collect::<Vec<_>>().join(" "))
        .filter(|line| !line.is_empty())
        .collect();
    if lines.len() > CHAT_LINES_MAX {
        let rest = lines.split_off(CHAT_LINES_MAX - 1).join(" ");
        lines.push(rest);
    }
    let text = lines.join("\n");
    let size = text.chars().count();
    (1..=CHAT_TEXT_MAX).contains(&size).then_some(text)
}

/// Why a send did not go, from the service's answer.
pub fn failure_of(error: &SocialError) -> SendFailure {
    match error {
        SocialError::ChatRejected { reason } => match reason {
            ChatRejection::Link => SendFailure::Link,
            ChatRejection::Phone => SendFailure::Phone,
            ChatRejection::Words => SendFailure::Words,
        },
        SocialError::ChatMuted { .. } => SendFailure::Muted,
        // About one a second (bursts of 5) waits seconds; the day's limit waits until midnight.
        SocialError::RateLimited { retry_after } => match retry_after {
            Some(wait) if *wait > Duration::from_secs(60) => SendFailure::DailyLimit,
            _ => SendFailure::TooFast,
        },
        SocialError::Refused { code, .. } => match code {
            ErrorCode::ChatOff => SendFailure::Off,
            ErrorCode::FriendNotFound
            | ErrorCode::TeamNotFound
            | ErrorCode::ChatNotPermitted
            | ErrorCode::MessageNotFound => SendFailure::NotAllowed,
            ErrorCode::BadRequest => SendFailure::Invalid,
            _ => SendFailure::Connection,
        },
        _ => SendFailure::Connection,
    }
}

/// What the views need to know about one conversation.
#[derive(Debug, Clone)]
pub struct Viewer {
    pub me: String,
    /// This computer's clock minus the hub's, in milliseconds.
    pub skew_ms: i64,
    /// Unix ms on this computer.
    pub now_ms: i64,
    /// The player leads or moderates this crew (crew chat): any message may go.
    pub moderates: bool,
}

pub fn message_view(message: &ChatMessage, viewer: &Viewer) -> ChatMessageView {
    let mine = message.from == viewer.me;
    let at = message.id + viewer.skew_ms;
    let open = |window: i64| Some(at + window).filter(|until| *until > viewer.now_ms);
    ChatMessageView {
        key: crate::history::message_key(message),
        id: Some(message.id),
        from: message.from.clone(),
        mine,
        text: message.text.clone(),
        at,
        edited: message.edited_at.is_some(),
        state: MessageState::Sent,
        failure: None,
        edit_until: if mine { open(EDIT_WINDOW_MS) } else { None },
        delete_until: if mine || viewer.moderates {
            open(DELETE_WINDOW_MS)
        } else {
            None
        },
    }
}

pub fn entry_view(entry: &Entry, viewer: &Viewer) -> ChatMessageView {
    match entry {
        Entry::Sent(message) => message_view(message, viewer),
        Entry::Outgoing(outgoing) => ChatMessageView {
            key: outgoing.key(),
            id: None,
            from: viewer.me.clone(),
            mine: true,
            text: outgoing.text.clone(),
            at: outgoing.at,
            edited: false,
            state: if outgoing.failed.is_some() {
                MessageState::Failed
            } else {
                MessageState::Sending
            },
            failure: outgoing.failed,
            edit_until: None,
            delete_until: None,
        },
    }
}

pub fn change_view(change: &Change, viewer: &Viewer) -> ChatChangeView {
    ChatChangeView {
        conv: change.conv.clone(),
        upsert: change
            .upsert
            .iter()
            .map(|entry| entry_view(entry, viewer))
            .collect(),
        remove: change.remove.clone(),
        reset: change.reset,
    }
}

/// Several changes of one conversation as one (a sync answer): later ones win by key.
pub fn merge(changes: Vec<Change>) -> Option<Change> {
    let mut iter = changes.into_iter();
    let mut merged = iter.next()?;
    for change in iter {
        if change.reset {
            merged.upsert.clear();
            merged.remove.clear();
            merged.reset = true;
        }
        for key in &change.remove {
            merged.upsert.retain(|entry| entry.key() != *key);
        }
        merged.remove.extend(change.remove);
        for entry in change.upsert {
            let key = entry.key();
            merged.upsert.retain(|e| e.key() != key);
            merged.remove.retain(|k| *k != key);
            merged.upsert.push(entry);
        }
    }
    Some(merged)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::Outgoing;

    #[test]
    fn texts_go_as_the_service_will_take_them() {
        assert_eq!(
            prepare_text("  selam   naber  "),
            Some("selam naber".into())
        );
        assert_eq!(prepare_text("a\r\n\r\nb\n\n\nc"), Some("a\nb\nc".into()));
        assert_eq!(
            prepare_text("1\n2\n3\n4\n5\n6\n7"),
            Some("1\n2\n3\n4\n5 6 7".into()),
            "more lines join the fifth"
        );
        assert_eq!(prepare_text("   \n  "), None);
        assert_eq!(
            prepare_text(&"ş".repeat(300)).map(|t| t.chars().count()),
            Some(300)
        );
        assert_eq!(prepare_text(&"ş".repeat(301)), None);
        let emoji = "👍".repeat(300);
        assert!(prepare_text(&emoji).is_some(), "emoji count once each");
        let nonce = new_nonce();
        assert_eq!(nonce.len(), 12);
        assert!(nonce.bytes().all(|b| b.is_ascii_alphanumeric()));
        assert_ne!(nonce, new_nonce());
    }

    #[test]
    fn a_refusal_says_why_the_message_did_not_go() {
        let refused = |code: &str| SocialError::Refused {
            status: 403,
            code: ErrorCode::parse(code),
            message: String::new(),
        };
        assert_eq!(
            failure_of(&SocialError::ChatRejected {
                reason: ChatRejection::Phone
            }),
            SendFailure::Phone
        );
        assert_eq!(
            failure_of(&SocialError::RateLimited {
                retry_after: Some(Duration::from_secs(2))
            }),
            SendFailure::TooFast
        );
        assert_eq!(
            failure_of(&SocialError::RateLimited {
                retry_after: Some(Duration::from_secs(40_000))
            }),
            SendFailure::DailyLimit
        );
        assert_eq!(failure_of(&refused("chat_off")), SendFailure::Off);
        assert_eq!(
            failure_of(&refused("friend_not_found")),
            SendFailure::NotAllowed
        );
        assert_eq!(failure_of(&refused("bad_request")), SendFailure::Invalid);
        assert_eq!(
            failure_of(&SocialError::Network("gone".into())),
            SendFailure::Connection
        );
    }

    #[test]
    fn views_say_what_may_still_be_done_on_this_clock() {
        let message = ChatMessage {
            conv: "c:0123456789abcdef".into(),
            id: 1_000_000,
            from: "1".into(),
            text: "selam".into(),
            mac: "m".into(),
            edited_at: Some(1_000_100),
            nonce: Some("n".into()),
        };
        let viewer = Viewer {
            me: "1".into(),
            skew_ms: 500,
            now_ms: 1_000_500 + 60_000,
            moderates: false,
        };
        let view = message_view(&message, &viewer);
        assert_eq!(view.key, "n:n");
        assert_eq!(view.at, 1_000_500);
        assert!(view.edited && view.mine);
        assert_eq!(view.edit_until, Some(1_000_500 + EDIT_WINDOW_MS));
        // Past the window: nothing to edit; a crewmate's message is the moderator's to delete.
        let later = Viewer {
            now_ms: 1_000_500 + EDIT_WINDOW_MS + 1,
            ..viewer.clone()
        };
        assert_eq!(message_view(&message, &later).edit_until, None);
        let theirs = ChatMessage {
            from: "2".into(),
            nonce: None,
            ..message.clone()
        };
        assert_eq!(message_view(&theirs, &viewer).delete_until, None);
        let moderator = Viewer {
            moderates: true,
            ..viewer.clone()
        };
        assert!(message_view(&theirs, &moderator).delete_until.is_some());
        let waiting = Entry::Outgoing(Outgoing {
            conv: message.conv.clone(),
            nonce: "q".into(),
            text: "yolda".into(),
            at: 7,
            failed: Some(SendFailure::Link),
        });
        let view = entry_view(&waiting, &viewer);
        assert_eq!(view.state, MessageState::Failed);
        assert_eq!(view.at, 7);
        assert_eq!(view.id, None);
    }

    #[test]
    fn a_sync_of_one_conversation_is_one_change() {
        let sent = |id: i64| {
            Entry::Sent(ChatMessage {
                conv: "c:0123456789abcdef".into(),
                id,
                from: "1".into(),
                text: format!("{id}"),
                mac: "m".into(),
                edited_at: None,
                nonce: None,
            })
        };
        let change = |upsert: Vec<Entry>, remove: Vec<&str>| Change {
            conv: "c:0123456789abcdef".into(),
            upsert,
            remove: remove.into_iter().map(String::from).collect(),
            reset: false,
        };
        let merged = merge(vec![
            change(vec![sent(1)], vec![]),
            change(vec![sent(2)], vec![]),
            change(vec![], vec!["i:1"]),
        ])
        .unwrap();
        assert_eq!(merged.upsert, [sent(2)]);
        assert_eq!(merged.remove, ["i:1"]);
        assert!(merge(Vec::new()).is_none());
    }
}
