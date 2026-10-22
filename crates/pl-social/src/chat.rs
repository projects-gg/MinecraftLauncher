//! Chat (CHAT1, CHAT2) as the launcher shows it: the history's entries as views (times on this
//! computer's clock, what the player may still do with each, the message a reply answers as held
//! here, reactions, a link's card), why a message did not go, and the checks made before anything
//! is sent. The service checks everything again; the launcher's fuller text check (links, phone
//! numbers) runs in the composer as the player types.

use std::time::Duration;

use crate::cards::StoredCard;
use crate::error::{ErrorCode, SocialError};
use crate::history::{Change, Entry, History, Tombstone, message_key, same_emoji};
use crate::links::{first_link, host_of};
use crate::model::{
    ChatMessage, ChatQuota, ChatRejection, LimitedKey, LinkRule, PLAN_NOTE_MAX, Reaction,
};
use crate::view::{
    CardIcon, CardKind, CardState, ChatCardView, ChatChangeView, ChatDeletedView, ChatMessageView,
    ChatReactionView, ChatReplyView, MessageState, SendFailure,
};

/// A message may be edited this long after it was sent.
pub const EDIT_WINDOW_MS: i64 = 15 * 60_000;
/// A message may be deleted for everyone this long after it was sent.
pub const DELETE_WINDOW_MS: i64 = 7 * 86_400_000;
/// A message that waited longer than this is not sent by itself any more (the service remembers a
/// resend's nonce this long): it waits for "Tekrar dene".
pub const RESEND_WINDOW_MS: i64 = 15 * 60_000;
/// Reactions go only on messages younger than this (CHAT2: the service's mailbox keeps a week).
pub const REACT_WINDOW_MS: i64 = 7 * 86_400_000;
/// Cards are asked only for messages younger than this (CHAT2).
pub const CARD_WINDOW_MS: i64 = 7 * 86_400_000;
/// Different emoji one player puts on one message (CHAT2; the owner's choice, 22 October 2026).
pub const REACTIONS_PER_PLAYER: usize = 5;
/// Different emoji one message carries (CHAT2).
pub const REACTION_KINDS: usize = 10;
/// The longest reaction the service takes, in UTF-16 units.
const REACTION_MAX_UNITS: usize = 32;
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

/// A crew plan's note as it will go (FIX2): spaces collapsed and trimmed, as the service cleans
/// it. None when nothing is left (no note, or clearing it); refused when it is not one line or is
/// longer than the service takes. The service checks links, phone numbers and words itself (the
/// launcher's composer check runs as the player types).
pub fn prepare_plan_note(text: &str) -> Result<Option<String>, SocialError> {
    let refused = SocialError::PlanNoteRejected { reason: None };
    if text.len() > 1_000 {
        return Err(refused);
    }
    let trimmed = text.trim();
    if trimmed.contains(['\n', '\r']) {
        return Err(refused);
    }
    let note = trimmed.split_whitespace().collect::<Vec<_>>().join(" ");
    match note.chars().count() {
        0 => Ok(None),
        size if size <= PLAN_NOTE_MAX => Ok(Some(note)),
        _ => Err(refused),
    }
}

/// Why a send did not go, from the service's answer, and the link rule when a link was refused.
pub fn failure_of(error: &SocialError) -> (SendFailure, Option<LinkRule>) {
    let failure = match error {
        SocialError::ChatRejected { reason, rule } => {
            return match reason {
                ChatRejection::Link => (SendFailure::Link, *rule),
                ChatRejection::Phone => (SendFailure::Phone, None),
                ChatRejection::Words => (SendFailure::Words, None),
            };
        }
        SocialError::ChatMuted { .. } => SendFailure::Muted,
        // About one a second (bursts of 5) waits seconds; the day's limit waits until midnight,
        // and the day's stickers and messages with links have their own (CHAT2).
        SocialError::RateLimited { retry_after, item } => match item {
            Some(ChatQuota::Stickers) => SendFailure::Stickers,
            Some(ChatQuota::Links) => SendFailure::Links,
            _ => match retry_after {
                Some(wait) if *wait > Duration::from_secs(60) => SendFailure::DailyLimit,
                _ => SendFailure::TooFast,
            },
        },
        SocialError::Limited {
            item: LimitedKey::ChatLinks,
            ..
        } => SendFailure::LimitedLinks,
        SocialError::Refused { code, .. } => match code {
            ErrorCode::ChatOff => SendFailure::Off,
            ErrorCode::ChatFeatureOff => SendFailure::FeatureOff,
            ErrorCode::FriendNotFound
            | ErrorCode::TeamNotFound
            | ErrorCode::ChatNotPermitted
            | ErrorCode::MessageNotFound => SendFailure::NotAllowed,
            ErrorCode::BadRequest => SendFailure::Invalid,
            _ => SendFailure::Connection,
        },
        _ => SendFailure::Connection,
    };
    (failure, None)
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

/// What a view of an entry needs from elsewhere in the history (CHAT2): the message a reply
/// answers, as held here; the card of its first link, once one was asked for.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Context {
    pub reply: Option<ChatReplyView>,
    pub card: Option<ChatCardView>,
}

/// The message a reply answers, as this device holds it (`from`: its sender as the link says).
pub fn reply_view(history: &mut History, conv: &str, id: i64, from: &str) -> ChatReplyView {
    match history.entry(conv, id) {
        Some(Entry::Sent(parent)) => ChatReplyView {
            id,
            key: Some(message_key(&parent)),
            text: parent.sticker.is_none().then(|| parent.text.clone()),
            sticker: parent.sticker.clone(),
            from: parent.from,
            deleted: false,
            missing: false,
        },
        Some(Entry::Deleted(tombstone)) => ChatReplyView {
            id,
            key: Some(tombstone.key()),
            from: tombstone.from,
            text: None,
            sticker: None,
            deleted: true,
            missing: false,
        },
        Some(Entry::Outgoing(_)) | None => ChatReplyView {
            id,
            from: from.to_owned(),
            key: None,
            text: None,
            sticker: None,
            deleted: false,
            missing: true,
        },
    }
}

/// The launcher's own icon for a host.
fn icon_of(host: &str) -> CardIcon {
    let under = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    if under("projects.gg") {
        CardIcon::Projects
    } else if under("youtube.com") || under("youtu.be") {
        CardIcon::Youtube
    } else if under("twitch.tv") {
        CardIcon::Twitch
    } else if under("kick.com") {
        CardIcon::Kick
    } else if under("modrinth.com") {
        CardIcon::Modrinth
    } else if under("discord.gg") || under("discord.com") || under("discordapp.com") {
        CardIcon::Discord
    } else if under("instagram.com") {
        CardIcon::Instagram
    } else if under("tiktok.com") {
        CardIcon::Tiktok
    } else if under("x.com") || under("twitter.com") {
        CardIcon::X
    } else {
        CardIcon::Globe
    }
}

/// A card as the conversation draws it: the hub's card, or the plain one (the icon and the
/// domain) when there is nothing to show or the link leads somewhere the rules refuse.
pub fn card_view(url: &str, stored: &StoredCard) -> ChatCardView {
    let host = host_of(url);
    let plain = |state: CardState, rule: Option<LinkRule>| ChatCardView {
        state,
        url: url.to_owned(),
        site: host.strip_prefix("www.").unwrap_or(&host).to_owned(),
        icon: icon_of(&host),
        kind: CardKind::Page,
        title: None,
        author: None,
        desc: None,
        image: None,
        w: None,
        h: None,
        rule,
    };
    match stored {
        StoredCard::Ready { card } => {
            let base = plain(CardState::Ready, None);
            ChatCardView {
                site: if card.site.is_empty() {
                    base.site.clone()
                } else {
                    card.site.clone()
                },
                icon: if card.icon.is_empty() {
                    base.icon
                } else {
                    CardIcon::parse(&card.icon)
                },
                kind: CardKind::parse(&card.kind),
                title: card.title.clone(),
                author: card.author.clone(),
                desc: card.desc.clone(),
                image: card.image.clone(),
                w: card.w,
                h: card.h,
                ..base
            }
        }
        StoredCard::Plain => plain(CardState::Plain, None),
        StoredCard::Refused { rule } => plain(CardState::Refused, *rule),
    }
}

/// What the view of `entry` needs from the history: the message a reply answers, its first
/// link's card once asked for.
pub fn context_of(history: &mut History, conv: &str, entry: &Entry) -> Context {
    let (reply, text) = match entry {
        Entry::Sent(message) => (
            message
                .reply
                .as_ref()
                .map(|reply| (reply.id, reply.from.clone())),
            message.sticker.is_none().then_some(message.text.as_str()),
        ),
        Entry::Outgoing(outgoing) => (
            outgoing.reply.map(|id| (id, String::new())),
            outgoing.sticker.is_none().then_some(outgoing.text.as_str()),
        ),
        Entry::Deleted(_) => (None, None),
    };
    let card = text.and_then(first_link).and_then(|url| {
        let stored = history.cards().get(url)?;
        Some(card_view(url, &stored))
    });
    Context {
        reply: reply.map(|(id, from)| reply_view(history, conv, id, &from)),
        card,
    }
}

fn reactions_view(reactions: &[Reaction], me: &str) -> Vec<ChatReactionView> {
    reactions
        .iter()
        .filter(|reaction| !reaction.by.is_empty())
        .map(|reaction| ChatReactionView {
            e: reaction.e.clone(),
            mine: reaction.by.iter().any(|by| by == me),
            by: reaction.by.clone(),
        })
        .collect()
}

pub fn message_view(message: &ChatMessage, viewer: &Viewer, context: Context) -> ChatMessageView {
    let mine = message.from == viewer.me;
    let at = message.id + viewer.skew_ms;
    let open = |window: i64| Some(at + window).filter(|until| *until > viewer.now_ms);
    ChatMessageView {
        key: message_key(message),
        id: Some(message.id),
        from: message.from.clone(),
        mine,
        text: message.text.clone(),
        at,
        edited: message.edited_at.is_some(),
        state: MessageState::Sent,
        failure: None,
        failure_rule: None,
        // A sticker is never edited.
        edit_until: if mine && message.sticker.is_none() {
            open(EDIT_WINDOW_MS)
        } else {
            None
        },
        delete_until: if mine || viewer.moderates {
            open(DELETE_WINDOW_MS)
        } else {
            None
        },
        deleted: None,
        sticker: message.sticker.clone(),
        reply: context.reply,
        reactions: reactions_view(&message.reactions, &viewer.me),
        card: context.card,
    }
}

/// A message deleted for everyone: its place, its sender and time, who deleted it. Nothing may be
/// done with it (no edit, delete, copy, reply, reaction or report).
pub fn tombstone_view(tombstone: &Tombstone, viewer: &Viewer) -> ChatMessageView {
    ChatMessageView {
        key: tombstone.key(),
        id: Some(tombstone.id),
        from: tombstone.from.clone(),
        mine: tombstone.from == viewer.me,
        text: String::new(),
        at: tombstone.id + viewer.skew_ms,
        edited: false,
        state: MessageState::Sent,
        failure: None,
        failure_rule: None,
        edit_until: None,
        delete_until: None,
        deleted: Some(ChatDeletedView {
            by: tombstone.by.clone(),
            at: (tombstone.at > 0).then(|| tombstone.at + viewer.skew_ms),
        }),
        sticker: None,
        reply: None,
        reactions: Vec::new(),
        card: None,
    }
}

pub fn entry_view(entry: &Entry, viewer: &Viewer, context: Context) -> ChatMessageView {
    match entry {
        Entry::Sent(message) => message_view(message, viewer, context),
        Entry::Deleted(tombstone) => tombstone_view(tombstone, viewer),
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
            failure_rule: outgoing.rule,
            edit_until: None,
            delete_until: None,
            deleted: None,
            sticker: outgoing.sticker.clone(),
            reply: context.reply,
            reactions: Vec::new(),
            card: context.card,
        },
    }
}

/// The change as the UI takes it; `context` gives each entry what its view needs from the
/// history.
pub fn change_view(
    change: &Change,
    viewer: &Viewer,
    mut context: impl FnMut(&Entry) -> Context,
) -> ChatChangeView {
    ChatChangeView {
        conv: change.conv.clone(),
        upsert: change
            .upsert
            .iter()
            .map(|entry| entry_view(entry, viewer, context(entry)))
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

/// What a reaction asks of the service (CHAT2).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ReactionStep {
    /// Send it: `e` as the message carries that kind already when it does (❤ and ❤️ are one).
    Send { e: String },
    /// Nothing to change: the player gave it already, or has none to take back.
    Nothing,
}

/// Checks a reaction the way the service will (CHAT2): one emoji of at most 32 UTF-16 units, never
/// 🖕, at most [`REACTIONS_PER_PLAYER`] different ones of the player's on a message and
/// [`REACTION_KINDS`] on it; giving one it carries already ("Ben de") always goes.
pub fn reaction_step(
    e: &str,
    on: bool,
    reactions: &[Reaction],
    me: &str,
) -> Result<ReactionStep, SocialError> {
    let units = e.encode_utf16().count();
    if units == 0
        || units > REACTION_MAX_UNITS
        || e.chars().any(|c| c.is_whitespace() || c.is_control())
    {
        return Err(SocialError::InvalidInput("reaction"));
    }
    if e.contains('\u{1F595}') {
        return Err(SocialError::ChatRejected {
            reason: ChatRejection::Words,
            rule: None,
        });
    }
    let kind = reactions.iter().find(|reaction| same_emoji(&reaction.e, e));
    let given = kind.is_some_and(|kind| kind.by.iter().any(|by| by == me));
    let e = kind.map_or_else(|| e.to_owned(), |kind| kind.e.clone());
    if !on {
        return Ok(if given {
            ReactionStep::Send { e }
        } else {
            ReactionStep::Nothing
        });
    }
    if given {
        return Ok(ReactionStep::Nothing);
    }
    let full = |reason: &str| SocialError::Refused {
        status: 409,
        code: ErrorCode::ChatReactionsFull,
        message: String::new(),
        detail: Some(reason.to_owned()),
    };
    let mine = reactions
        .iter()
        .filter(|reaction| reaction.by.iter().any(|by| by == me))
        .count();
    if mine >= REACTIONS_PER_PLAYER {
        return Err(full("mine"));
    }
    if kind.is_none() && reactions.len() >= REACTION_KINDS {
        return Err(full("kinds"));
    }
    Ok(ReactionStep::Send { e })
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::history::Outgoing;
    use crate::model::{ChatReplyRef, LinkCard};

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
    fn a_plan_note_goes_on_one_line_of_80() {
        let note = |text: &str| prepare_plan_note(text).map_err(|e| e.code().cloned());
        assert_eq!(
            note("  Bedwars,   sesli  gelin "),
            Ok(Some("Bedwars, sesli gelin".into()))
        );
        assert_eq!(note("   "), Ok(None), "empty is none (or clears it)");
        assert_eq!(note(""), Ok(None));
        assert_eq!(
            note(&"ş".repeat(80)).map(|n| n.map(|n| n.chars().count())),
            Ok(Some(80))
        );
        let refused = Err(Some(ErrorCode::InvalidPlanNote));
        assert_eq!(note(&"ş".repeat(81)), refused);
        assert_eq!(note("iki\nsatır"), refused);
        assert_eq!(note("sonda satır\n"), Ok(Some("sonda satır".into())));
    }

    #[test]
    fn a_refusal_says_why_the_message_did_not_go() {
        let refused = |code: &str| SocialError::Refused {
            status: 403,
            code: ErrorCode::parse(code),
            message: String::new(),
            detail: None,
        };
        assert_eq!(
            failure_of(&SocialError::ChatRejected {
                reason: ChatRejection::Phone,
                rule: None
            }),
            (SendFailure::Phone, None)
        );
        assert_eq!(
            failure_of(&SocialError::ChatRejected {
                reason: ChatRejection::Link,
                rule: Some(LinkRule::Shortener)
            }),
            (SendFailure::Link, Some(LinkRule::Shortener))
        );
        assert_eq!(
            failure_of(&SocialError::RateLimited {
                retry_after: Some(Duration::from_secs(2)),
                item: None
            })
            .0,
            SendFailure::TooFast
        );
        assert_eq!(
            failure_of(&SocialError::RateLimited {
                retry_after: Some(Duration::from_secs(40_000)),
                item: None
            })
            .0,
            SendFailure::DailyLimit
        );
        // CHAT2's own daily limits, a new account's links, an extra switched off.
        assert_eq!(
            failure_of(&SocialError::RateLimited {
                retry_after: Some(Duration::from_secs(40_000)),
                item: Some(ChatQuota::Stickers)
            })
            .0,
            SendFailure::Stickers
        );
        assert_eq!(
            failure_of(&SocialError::RateLimited {
                retry_after: Some(Duration::from_secs(40_000)),
                item: Some(ChatQuota::Links)
            })
            .0,
            SendFailure::Links
        );
        assert_eq!(
            failure_of(&SocialError::Limited {
                item: LimitedKey::ChatLinks,
                until: Some(1),
                retry_after: None
            })
            .0,
            SendFailure::LimitedLinks
        );
        assert_eq!(
            failure_of(&refused("chat_feature_off")).0,
            SendFailure::FeatureOff
        );
        assert_eq!(failure_of(&refused("chat_off")).0, SendFailure::Off);
        assert_eq!(
            failure_of(&refused("friend_not_found")).0,
            SendFailure::NotAllowed
        );
        assert_eq!(failure_of(&refused("bad_request")).0, SendFailure::Invalid);
        assert_eq!(
            failure_of(&SocialError::Network("gone".into())).0,
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
            ..ChatMessage::default()
        };
        let viewer = Viewer {
            me: "1".into(),
            skew_ms: 500,
            now_ms: 1_000_500 + 60_000,
            moderates: false,
        };
        let view = message_view(&message, &viewer, Context::default());
        assert_eq!(view.key, "n:n");
        assert_eq!(view.at, 1_000_500);
        assert!(view.edited && view.mine);
        assert_eq!(view.edit_until, Some(1_000_500 + EDIT_WINDOW_MS));
        // Past the window: nothing to edit; a crewmate's message is the moderator's to delete.
        let later = Viewer {
            now_ms: 1_000_500 + EDIT_WINDOW_MS + 1,
            ..viewer.clone()
        };
        assert_eq!(
            message_view(&message, &later, Context::default()).edit_until,
            None
        );
        let theirs = ChatMessage {
            from: "2".into(),
            nonce: None,
            ..message.clone()
        };
        assert_eq!(
            message_view(&theirs, &viewer, Context::default()).delete_until,
            None
        );
        let moderator = Viewer {
            moderates: true,
            ..viewer.clone()
        };
        assert!(
            message_view(&theirs, &moderator, Context::default())
                .delete_until
                .is_some()
        );
        // A sticker of the player's own is never edited, only deleted.
        let sticker = ChatMessage {
            text: String::new(),
            sticker: Some("pl1/gg".into()),
            edited_at: None,
            ..message.clone()
        };
        let view = message_view(&sticker, &viewer, Context::default());
        assert_eq!(view.edit_until, None);
        assert!(view.delete_until.is_some());
        assert_eq!(view.sticker.as_deref(), Some("pl1/gg"));
        let waiting = Entry::Outgoing(Outgoing {
            conv: message.conv.clone(),
            nonce: "q".into(),
            text: "yolda".into(),
            sticker: None,
            reply: None,
            at: 7,
            failed: Some(SendFailure::Link),
            rule: Some(LinkRule::File),
        });
        let view = entry_view(&waiting, &viewer, Context::default());
        assert_eq!(view.state, MessageState::Failed);
        assert_eq!(view.failure_rule, Some(LinkRule::File));
        assert_eq!(view.at, 7);
        assert_eq!(view.id, None);
    }

    #[test]
    fn a_tombstone_shows_who_deleted_it_and_offers_nothing() {
        let viewer = Viewer {
            me: "1".into(),
            skew_ms: 500,
            now_ms: 1_000_500,
            moderates: true,
        };
        let tombstone = Tombstone {
            conv: "c:0123456789abcdef".into(),
            id: 1_000_000,
            from: "1".into(),
            nonce: Some("n".into()),
            by: "2".into(),
            at: 1_000_100,
        };
        let view = tombstone_view(&tombstone, &viewer);
        assert_eq!(view.key, "n:n", "the bubble keeps its key");
        assert!(view.mine && view.text.is_empty() && !view.edited);
        assert_eq!(view.at, 1_000_500);
        assert_eq!((view.edit_until, view.delete_until), (None, None));
        assert!(view.reactions.is_empty() && view.reply.is_none() && view.card.is_none());
        let deleted = view.deleted.unwrap();
        assert_eq!((deleted.by.as_str(), deleted.at), ("2", Some(1_000_600)));
        let unknown = Tombstone {
            at: 0,
            by: String::new(),
            ..tombstone
        };
        assert_eq!(tombstone_view(&unknown, &viewer).deleted.unwrap().at, None);
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
                ..ChatMessage::default()
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

    const ME: &str = "100000000000000001";
    const AYSE: &str = "100000000000000002";

    fn held(conv: &str, id: i64, from: &str, text: &str) -> ChatMessage {
        ChatMessage {
            conv: conv.to_owned(),
            id,
            from: from.to_owned(),
            text: text.to_owned(),
            mac: format!("mac{id}"),
            ..ChatMessage::default()
        }
    }

    #[test]
    fn a_reply_shows_what_it_answers_as_held_here() {
        let mut history = History::open(None, ME);
        let conv = crate::history::direct_conversation(ME, AYSE);
        history.put(&held(&conv, 10, AYSE, "Geliyor musun?"));
        history.put(&ChatMessage {
            sticker: Some("pl1/hi".into()),
            text: String::new(),
            ..held(&conv, 12, AYSE, "")
        });
        let reply = |to: i64| ChatMessage {
            reply: Some(ChatReplyRef {
                id: to,
                from: AYSE.into(),
            }),
            rmac: Some("r".into()),
            ..held(&conv, 20, ME, "evet")
        };
        let quote = |message: &ChatMessage, history: &mut History| {
            context_of(history, &conv, &Entry::Sent(message.clone())).reply
        };
        let shown = quote(&reply(10), &mut history).unwrap();
        assert_eq!(shown.text.as_deref(), Some("Geliyor musun?"));
        assert_eq!(shown.key.as_deref(), Some("i:10"));
        assert!(!shown.deleted && !shown.missing);
        let sticker = quote(&reply(12), &mut history).unwrap();
        assert_eq!(
            (sticker.text, sticker.sticker.as_deref()),
            (None, Some("pl1/hi"))
        );
        // Deleted since: "Mesaj silindi"; never held here: "Önceki bir mesaj".
        history.delete(&conv, 10, AYSE, 15);
        let gone = quote(&reply(10), &mut history).unwrap();
        assert!(gone.deleted && !gone.missing && gone.text.is_none());
        let missing = quote(&reply(5), &mut history).unwrap();
        assert!(missing.missing && missing.key.is_none());
        assert_eq!(missing.from, AYSE, "the link says whose it was");
    }

    #[test]
    fn a_card_comes_with_its_message_once_asked() {
        let mut history = History::open(None, ME);
        let conv = crate::history::direct_conversation(ME, AYSE);
        let url = "https://www.youtube.com/watch?v=dQw4w9WgXcQ";
        let message = held(&conv, 10, AYSE, &format!("bak {url} çok iyi"));
        let entry = Entry::Sent(message);
        assert_eq!(context_of(&mut history, &conv, &entry).card, None);
        history.cards().put(
            url,
            StoredCard::Ready {
                card: Box::new(LinkCard {
                    url: url.into(),
                    site: "YouTube".into(),
                    icon: "youtube".into(),
                    kind: "video".into(),
                    title: Some("Bir video".into()),
                    author: None,
                    desc: None,
                    image: None,
                    w: None,
                    h: None,
                }),
            },
        );
        let card = context_of(&mut history, &conv, &entry).card.unwrap();
        assert_eq!(card.state, CardState::Ready);
        assert_eq!((card.icon, card.kind), (CardIcon::Youtube, CardKind::Video));
        assert_eq!(card.title.as_deref(), Some("Bir video"));
        // Nothing to show: the icon and the domain; refused by the rules: plain text.
        let plain = card_view("https://www.modrinth.com/mod/sodium", &StoredCard::Plain);
        assert_eq!(
            (plain.site.as_str(), plain.icon),
            ("modrinth.com", CardIcon::Modrinth)
        );
        let refused = card_view(
            "https://grabify.link/x",
            &StoredCard::Refused {
                rule: Some(LinkRule::Grabber),
            },
        );
        assert_eq!(
            (refused.state, refused.icon, refused.rule),
            (CardState::Refused, CardIcon::Globe, Some(LinkRule::Grabber))
        );
        // An icon this version does not know is the globe.
        assert_eq!(CardIcon::parse("myspace"), CardIcon::Globe);
        assert_eq!(CardKind::parse("podcast"), CardKind::Page);
    }

    #[test]
    fn reactions_are_checked_as_the_service_checks_them() {
        let kinds = |list: &[(&str, &[&str])]| -> Vec<Reaction> {
            list.iter()
                .map(|(e, by)| Reaction {
                    e: (*e).to_owned(),
                    by: by.iter().map(|b| (*b).to_owned()).collect(),
                })
                .collect()
        };
        let none: Vec<Reaction> = Vec::new();
        assert_eq!(
            reaction_step("👍", true, &none, ME).unwrap(),
            ReactionStep::Send { e: "👍".into() }
        );
        // ❤ joins the ❤️ already there; giving it again or taking back what is not given is
        // nothing.
        let hearts = kinds(&[("❤️", &[AYSE])]);
        assert_eq!(
            reaction_step("❤", true, &hearts, ME).unwrap(),
            ReactionStep::Send { e: "❤️".into() }
        );
        assert_eq!(
            reaction_step("❤", false, &hearts, ME).unwrap(),
            ReactionStep::Nothing
        );
        let mine = kinds(&[("❤️", &[ME])]);
        assert_eq!(
            reaction_step("❤️", true, &mine, ME).unwrap(),
            ReactionStep::Nothing
        );
        // Never 🖕, in any tone; one emoji of at most 32 units.
        assert!(matches!(
            reaction_step("🖕🏽", true, &none, ME),
            Err(SocialError::ChatRejected {
                reason: ChatRejection::Words,
                ..
            })
        ));
        assert!(reaction_step("", true, &none, ME).is_err());
        assert!(reaction_step("👍 👍", true, &none, ME).is_err());
        assert!(reaction_step(&"👍".repeat(17), true, &none, ME).is_err());
        // Five of the player's own at most, ten kinds; "Ben de" always goes.
        let five: Vec<Reaction> = ["😀", "😂", "🔥", "👍", "😮"]
            .iter()
            .map(|e| Reaction {
                e: (*e).into(),
                by: vec![ME.into()],
            })
            .collect();
        let full = reaction_step("🎉", true, &five, ME).unwrap_err();
        assert_eq!(full.detail(), Some("mine"));
        let ten: Vec<Reaction> = ["😀", "😂", "🔥", "👍", "😮", "😢", "😡", "🎉", "💯", "👀"]
            .iter()
            .map(|e| Reaction {
                e: (*e).into(),
                by: vec![AYSE.into()],
            })
            .collect();
        assert_eq!(
            reaction_step("🙏", true, &ten, ME).unwrap_err().detail(),
            Some("kinds")
        );
        assert_eq!(
            reaction_step("👀", true, &ten, ME).unwrap(),
            ReactionStep::Send { e: "👀".into() }
        );
    }
}
