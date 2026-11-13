//! SQOL-O: the voice room's own text chat in the game, the "Oda" chip (the lead's contract
//! `docs/tasks/sqol.md` §1.3, §1.4; the wire `docs/tasks/ovl2-protocol.md` → "SQOL").
//!
//! The launcher's `pl_voice::Voice` keeps the room's messages (at most [`LOG_MAX`], in memory, for
//! the stay). The app hands every `voice-chat` event to the hub ([`crate::Overlay::room_chat`]),
//! which keeps the same log for games that start later ([`RoomLog`]) and passes the event on to
//! every game's session. A session tells only a mod whose hello lists `voice` and `rchat`, and only
//! while the room's chat shows as its own chip: the contract's rule picks it ([`shown_room`]; a
//! friend room of two whose other one is a mutual friend keeps that friend's direct messages, as
//! the launcher's room panel does). The chip's messages travel as the chat's own frames (`msg`,
//! `history`, `sent`, `send-failed`) in a conversation of its own (`v:<room>`, the report path's
//! name for it), so the panel shows them with the chat it has.
//!
//! The player's presses (send, "Sil" on their own message, "Rapor Et") go to the app
//! ([`crate::Overlay::on_room_chat`]), which calls the same `Voice` methods as the page's commands.
//! A card for others' messages waits [`GATHER`] for more (CHAT-CALM's four seconds), then one card
//! says them all; none while the panel shows the chip.

use std::collections::VecDeque;
use std::future::Future;
use std::pin::Pin;
use std::time::Duration;

use pl_social::ReportReason;
use pl_voice::rchat::{
    LOG_MAX, RoomChatBy, RoomChatError, RoomChatMsg, RoomChatReason, RoomChatSend, VoiceChatEvent,
};
use pl_voice::view::{VoiceRoomKind, VoiceState};

use crate::links::{self, LinkLists};
use crate::protocol::{
    LinkView, MsgView, RoomChatShown, SendFailure, ToGame, ToastKind, ToastView,
};

/// A room chat's conversation in the game: this and the room's id.
pub const CONV_PREFIX: &str = "v:";

/// How long the first message the player has not seen waits for more before its card (the
/// tests' bridge waits less).
pub const GATHER: Duration = if cfg!(test) {
    Duration::from_millis(80)
} else {
    Duration::from_secs(4)
};

/// The player's own messages the room took that the session remembers (their message may come
/// before or after the answer), and the ones that came first.
const MAX_OWN: usize = 16;

/// The room chat's conversation for `room`.
pub fn conv_of(room: &str) -> String {
    format!("{CONV_PREFIX}{room}")
}

/// The room of a room chat's conversation, or none for any other conversation.
pub fn room_of(conv: &str) -> Option<&str> {
    conv.strip_prefix(CONV_PREFIX)
        .filter(|room| !room.is_empty())
}

/// A request from the game's room chat, for the launcher's voice chat (the page's commands of the
/// same name).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RoomChatAct {
    /// "Gönder": a text or a sticker (the room checks every rule).
    Send {
        room: String,
        text: Option<String>,
        sticker: Option<String>,
    },
    /// "Sil" on the player's own message.
    Delete { room: String, id: String },
    /// "Rapor Et" on someone else's message: the launcher sends it with the messages before it,
    /// as the room sealed them.
    Report {
        room: String,
        id: String,
        reason: ReportReason,
        note: Option<String>,
    },
}

/// What the app's voice chat answers a request with: a deletion and a report went when it is
/// [`RoomChatSend::Sent`] (see [`answer_of`]).
pub type RoomChatFuture = Pin<Box<dyn Future<Output = RoomChatSend> + Send>>;

/// A deletion's or a report's answer (`None`: it went) as a request's.
pub fn answer_of(error: Option<RoomChatError>) -> RoomChatSend {
    match error {
        None => RoomChatSend::Sent {
            id: String::new(),
            at: 0,
        },
        Some(error) => RoomChatSend::refused(error),
    }
}

/// A message's report reasons (the launcher's `REPORT_REASONS.message`), or none for any other.
pub fn message_reason(reason: &str) -> Option<ReportReason> {
    Some(match reason {
        "harassment" => ReportReason::Harassment,
        "inappropriate" => ReportReason::Inappropriate,
        "spam" => ReportReason::Spam,
        "scam" => ReportReason::Scam,
        "other" => ReportReason::Other,
        _ => return None,
    })
}

// --- the log ------------------------------------------------------------------------------------

/// One of the room's messages, and whether (and by whom) it was taken away: the game keeps a quiet
/// mark in its place, as the launcher's room panel does.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Held {
    pub msg: RoomChatMsg,
    pub gone: Option<RoomChatBy>,
}

/// The room chat's newest messages as the launcher told them, oldest first (the hub keeps one for
/// the games that start later, each session its own).
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct RoomLog {
    /// The room they are of.
    pub room: Option<String>,
    pub msgs: VecDeque<Held>,
}

/// What an event changed in a [`RoomLog`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LogChange {
    Nothing,
    /// A message came (or came again: it replaced the one with its id).
    Msg(Held),
    /// A message was taken away (it stays as a mark).
    Gone(Held),
    /// The whole log is new (joining, coming back, someone blocked, the chat switched off).
    Whole,
}

impl RoomLog {
    /// Takes the launcher's event in.
    pub fn apply(&mut self, event: &VoiceChatEvent) -> LogChange {
        match event {
            VoiceChatEvent::Log { room, msgs } => {
                self.room = Some(room.clone());
                self.msgs = msgs
                    .iter()
                    .rev()
                    .take(LOG_MAX)
                    .rev()
                    .map(|msg| Held {
                        msg: msg.clone(),
                        gone: None,
                    })
                    .collect();
                LogChange::Whole
            }
            VoiceChatEvent::Msg { room, msg } => {
                if self.room.as_deref() != Some(room.as_str()) {
                    self.room = Some(room.clone());
                    self.msgs.clear();
                }
                self.msgs.retain(|held| held.msg.id != msg.id);
                let held = Held {
                    msg: msg.clone(),
                    gone: None,
                };
                self.msgs.push_back(held.clone());
                while self.msgs.len() > LOG_MAX {
                    self.msgs.pop_front();
                }
                LogChange::Msg(held)
            }
            VoiceChatEvent::Del { room, id, by } => {
                if self.room.as_deref() != Some(room.as_str()) {
                    return LogChange::Nothing;
                }
                match self.msgs.iter_mut().find(|held| held.msg.id == *id) {
                    Some(held) if held.gone.is_none() => {
                        held.gone = Some(*by);
                        LogChange::Gone(held.clone())
                    }
                    _ => LogChange::Nothing,
                }
            }
        }
    }

    /// The player's room now (none: out of every room): a log of another room goes. Whether it
    /// did.
    pub fn keep_room(&mut self, room: Option<&str>) -> bool {
        if self.room.is_some() && self.room.as_deref() != room {
            self.room = None;
            self.msgs.clear();
            return true;
        }
        false
    }

    /// The message with `id` in `room`, while it is held and not taken away.
    pub fn get(&self, room: &str, id: &str) -> Option<&RoomChatMsg> {
        if self.room.as_deref() != Some(room) {
            return None;
        }
        self.msgs
            .iter()
            .find(|held| held.msg.id == id && held.gone.is_none())
            .map(|held| &held.msg)
    }
}

// --- which chat -----------------------------------------------------------------------------------

/// The friend whose direct messages stand in for the room's chat (the contract's "Hangi sohbet",
/// the launcher's `chatChoice`): a friend room that never held more than two (`grew`), whose other
/// one (or the friend the player's call still rings at) is a mutual friend, while chat is on and
/// nothing latched the room's chat for this stay. None otherwise.
pub fn direct_friend(
    state: &VoiceState,
    me: &str,
    is_friend: impl Fn(&str) -> bool,
    chat_on: bool,
    latched: bool,
) -> Option<String> {
    let room = state.room.as_ref()?;
    let grew = room.grew || room.people.len() > 1;
    if room.kind != VoiceRoomKind::Friend || grew || latched || !chat_on || me.is_empty() {
        return None;
    }
    let other = match room.people.as_slice() {
        [one] => Some(one.id.as_str()),
        [] => state
            .calling
            .as_ref()
            .map(|calling| calling.friend.as_str()),
        _ => None,
    }?;
    is_friend(other).then(|| other.to_owned())
}

/// The room whose own chat shows as the "Oda" chip now: the player's room while it has its chat
/// and no friend's direct messages stand in for it ([`direct_friend`]); none otherwise (voice chat
/// off, no room, a room without its chat).
pub fn shown_room(
    state: &VoiceState,
    me: &str,
    is_friend: impl Fn(&str) -> bool,
    chat_on: bool,
    latched: bool,
) -> Option<String> {
    if !(state.available && state.enabled) {
        return None;
    }
    let room = state.room.as_ref()?;
    if !room.chat || direct_friend(state, me, is_friend, chat_on, latched).is_some() {
        return None;
    }
    Some(room.id.clone())
}

// --- what the mod hears ---------------------------------------------------------------------------

/// How a message is shown: the words the lists give its sender, the player's own name, the
/// overlay's language and Studio's link lists.
pub struct Shown<'a> {
    pub me: &'a str,
    pub my_name: &'a str,
    pub english: bool,
    pub links: Option<&'a LinkLists>,
}

/// A room chat message as the panel shows it (`msg`): its sender's name (`name`; the player's own
/// for theirs), a sticker in words for a mod that cannot draw it, the links the player may press
/// (the friends' chat's rule table and Studio's lists; no cards), a quiet mark for one taken away.
/// `cid`: the panel's own key of the player's message it confirms.
pub fn msg_view(
    conv: &str,
    held: &Held,
    name: &str,
    shown: &Shown<'_>,
    cid: Option<String>,
) -> MsgView {
    let msg = &held.msg;
    let mine = !shown.me.is_empty() && msg.from == shown.me;
    let deleted = held.gone.is_some();
    let sticker = msg.sticker.clone().filter(|_| !deleted);
    let text = if deleted {
        String::new()
    } else if let Some(sticker) = &sticker {
        pl_social::sticker_text(sticker, shown.english)
    } else {
        msg.text.clone().unwrap_or_default()
    };
    let links = match shown.links {
        Some(lists) if !deleted && sticker.is_none() => links::pressable(&text, lists)
            .into_iter()
            .map(|link| LinkView {
                url: link.url,
                trusted: link.trusted,
            })
            .collect(),
        _ => Vec::new(),
    };
    MsgView {
        conv: Some(conv.to_owned()),
        id: msg.id.clone(),
        cid,
        from: if mine {
            shown.my_name.to_owned()
        } else {
            name.to_owned()
        },
        text,
        at: msg.at,
        mine,
        deleted,
        removed: matches!(held.gone, Some(RoomChatBy::Mod | RoomChatBy::Staff)),
        sticker,
        links,
        // Their name over a group opens their menu (the room page's).
        by: (!mine).then(|| msg.from.clone()),
        ..MsgView::default()
    }
}

/// `room`'s whole log as `history` (the answer to `open-conv`, and again when it is new); empty
/// while the log holds nothing of that room.
pub fn history_frame(
    room: &str,
    log: &RoomLog,
    name_of: impl Fn(&str) -> String,
    shown: &Shown<'_>,
) -> ToGame {
    let conv = conv_of(room);
    let msgs = if log.room.as_deref() == Some(room) {
        log.msgs
            .iter()
            .map(|held| MsgView {
                conv: None,
                ..msg_view(&conv, held, &name_of(&held.msg.from), shown, None)
            })
            .collect()
    } else {
        Vec::new()
    };
    ToGame::History { conv, msgs }
}

/// Why a message did not go, as the panel says it (the chat's own words), with the link rule that
/// refused it; none when it went.
pub fn send_failure(answer: &RoomChatSend) -> Option<(SendFailure, Option<String>)> {
    let RoomChatSend::Refused {
        error,
        reason,
        rule,
        ..
    } = answer
    else {
        return None;
    };
    Some(match error {
        RoomChatError::Rejected => match reason {
            Some(RoomChatReason::Phone) => (SendFailure::Phone, None),
            Some(RoomChatReason::Link) => (SendFailure::Link, rule.clone()),
            Some(RoomChatReason::Length) => (SendFailure::TooLong, None),
            Some(RoomChatReason::Words) | None => (SendFailure::Words, None),
        },
        RoomChatError::RateLimited => (SendFailure::Rate, None),
        RoomChatError::Muted => (SendFailure::Muted, None),
        RoomChatError::Limited => (SendFailure::LimitedLinks, None),
        RoomChatError::StickersOff => (SendFailure::FeatureOff, None),
        RoomChatError::Off => (SendFailure::Off, None),
        RoomChatError::NotAllowed => (SendFailure::NotAllowed, None),
        // The room or the launcher did not answer, or the player left: "Gönderilemedi."
        RoomChatError::Gone | RoomChatError::Failed => (SendFailure::Offline, None),
    })
}

/// Why a deletion or a report did not go, as `done` names it (the mod words it); none when it went.
pub fn done_reason(answer: &RoomChatSend) -> Option<&'static str> {
    match answer.error()? {
        RoomChatError::RateLimited => Some("rate"),
        RoomChatError::Gone => Some("gone"),
        _ => Some("failed"),
    }
}

/// The card for others' messages the player has not seen (`last` the newest of them): its sender
/// and words (a sticker in words; "Yeni mesaj" in streamer mode), the room chat's conversation (the
/// mod names the room chat under the sender and counts the chip's unread on it).
pub fn toast_of(
    room: &str,
    last: &RoomChatMsg,
    title: String,
    marks: Vec<String>,
    english: bool,
    streamer: bool,
    stickers: bool,
) -> ToastView {
    let body = if streamer {
        if english { "New message" } else { "Yeni mesaj" }.to_owned()
    } else if let Some(sticker) = &last.sticker {
        let word = if english { "Sticker" } else { "Çıkartma" };
        match pl_social::sticker(sticker) {
            Some(found) => format!(
                "{word} · {}",
                if english {
                    &found.name_en
                } else {
                    &found.name_tr
                }
            ),
            None => word.to_owned(),
        }
    } else {
        last.text.clone().unwrap_or_default()
    };
    ToastView {
        kind: ToastKind::Message,
        key: format!("room:{room}"),
        title,
        body,
        conv: Some(conv_of(room)),
        crew: None,
        sticker: last.sticker.clone().filter(|_| stickers && !streamer),
        badge: None,
        marks,
        by: (!last.from.is_empty()).then(|| last.from.clone()),
        at: None,
    }
}

// --- a session's side -----------------------------------------------------------------------------

/// A card being gathered: its number (a later one makes it stale), the room, how many came, the
/// newest.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Gather {
    pub id: u64,
    pub room: String,
    pub count: u32,
    pub last: RoomChatMsg,
}

/// What a session told its mod of the room's chat and keeps for the next differences: the chip's
/// room, the unread count, the latch, the player's messages on their way, the card being gathered.
#[derive(Debug, Default)]
pub struct RoomChatLink {
    /// The room whose chat shows as the chip now.
    shown: Option<String>,
    /// The room the latch belongs to: once someone else wrote in it while a friend's direct
    /// messages stood in for it, its own chat stays for the stay (the launcher's panel does so).
    latched: Option<String>,
    unread: u32,
    told: Option<RoomChatShown>,
    /// The panel's keys of the player's messages waiting for the room's answer.
    pending: Vec<String>,
    /// The player's messages the room took: their id and the panel's key, while their message has
    /// not come yet.
    taken: VecDeque<(String, String)>,
    /// The player's own messages that came before their answer.
    early: VecDeque<RoomChatMsg>,
    gather: Option<Gather>,
    gathers: u64,
}

/// What the room's answer to one of the player's messages makes the session say: `sent` or
/// `send-failed`, then the player's own messages to show now (with their line's key).
#[derive(Debug, Default)]
pub struct Answered {
    pub frames: Vec<ToGame>,
    pub show: Vec<(RoomChatMsg, Option<String>)>,
}

/// What became of one of the room's messages for the mod.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Arrival {
    /// Not shown here (no chip, another room).
    Quiet,
    /// The player's own, held back until its answer says which of the panel's lines it is.
    Held,
    /// Shown: with the panel's key when it confirms one of its lines.
    Shown { cid: Option<String> },
}

impl RoomChatLink {
    /// The room whose chat shows as the chip now.
    pub fn shown(&self) -> Option<&str> {
        self.shown.as_deref()
    }

    /// Whether someone else's message latched the room's chat for `room`'s stay.
    pub fn latched(&self, room: Option<&str>) -> bool {
        room.is_some() && self.latched.as_deref() == room
    }

    /// The chip's room now ([`shown_room`]); the room of the stay (for the latch). A new chip
    /// starts with nothing unread and no card on its way. Whether the chip changed.
    pub fn set_shown(&mut self, shown: Option<String>, stay: Option<&str>) -> bool {
        if self.latched.is_some() && self.latched.as_deref() != stay {
            self.latched = None;
        }
        if self.shown == shown {
            return false;
        }
        self.shown = shown;
        self.unread = 0;
        self.gather = None;
        true
    }

    /// Someone else wrote in `room` while its chat did not show as the chip (a friend's direct
    /// messages stood in): its chat shows for the rest of the stay. Whether that is new.
    pub fn latch(&mut self, room: &str) -> bool {
        if self.latched.as_deref() == Some(room) {
            return false;
        }
        self.latched = Some(room.to_owned());
        true
    }

    /// `room-chat` when what the mod has differs. A mod that heard nothing hears nothing while
    /// there is no chip; once it heard of one, an empty frame says it is gone.
    pub fn frame(&mut self) -> Option<ToGame> {
        let now = RoomChatShown {
            conv: self.shown.as_deref().map(conv_of),
            unread: if self.shown.is_some() { self.unread } else { 0 },
        };
        if self.told.as_ref() == Some(&now) {
            return None;
        }
        if self.told.is_none() && now.conv.is_none() {
            self.told = Some(now);
            return None;
        }
        self.told = Some(now.clone());
        Some(ToGame::RoomChat(now))
    }

    /// A new connection heard nothing yet.
    pub fn forget_told(&mut self) {
        self.told = None;
        self.pending.clear();
        self.taken.clear();
        self.early.clear();
        self.gather = None;
    }

    /// The panel shows the chip: everything in it is seen, no card waits.
    pub fn seen(&mut self) {
        self.unread = 0;
        self.gather = None;
    }

    pub fn unread(&self) -> u32 {
        self.unread
    }

    /// One of the player's messages left for the room (`cid` the panel's key of its line).
    pub fn sending(&mut self, cid: String) {
        self.pending.push(cid);
        if self.pending.len() > MAX_OWN {
            self.pending.remove(0);
        }
    }

    /// The room's answer to the player's message `cid`: the frame that says it (`sent` or
    /// `send-failed`), and the player's own messages that came before it, to show now (with
    /// their line's key when it is this one).
    pub fn answered(&mut self, cid: String, answer: &RoomChatSend) -> Answered {
        self.pending.retain(|waiting| *waiting != cid);
        let mut out = Answered::default();
        match answer {
            RoomChatSend::Sent { id, .. } => {
                out.frames.push(ToGame::Sent {
                    cid: cid.clone(),
                    id: id.clone(),
                    sid: None,
                });
                if let Some(msg) = self
                    .early
                    .iter()
                    .position(|msg| msg.id == *id)
                    .and_then(|at| self.early.remove(at))
                {
                    out.show.push((msg, Some(cid)));
                } else {
                    self.taken.push_back((id.clone(), cid));
                    while self.taken.len() > MAX_OWN {
                        self.taken.pop_front();
                    }
                }
            }
            RoomChatSend::Refused { .. } => {
                let (reason, rule) = send_failure(answer).unwrap_or((SendFailure::Offline, None));
                out.frames.push(ToGame::SendFailed { cid, reason, rule });
            }
        }
        if self.pending.is_empty() {
            // Nothing else waits for an answer: what came early is shown as it is.
            out.show.extend(self.early.drain(..).map(|msg| (msg, None)));
        }
        out
    }

    /// One of the room's messages came (`mine`: the player's own; `on_screen`: the panel shows the
    /// chip; `counts`: someone else's the player may hear of, not blocked nor ignored). Counts it
    /// as unread unless the chip is on screen; says how it goes to the mod.
    pub fn arrived(
        &mut self,
        room: &str,
        msg: &RoomChatMsg,
        mine: bool,
        on_screen: bool,
        counts: bool,
    ) -> Arrival {
        if self.shown.as_deref() != Some(room) {
            return Arrival::Quiet;
        }
        if mine {
            if let Some(at) = self.taken.iter().position(|(id, _)| *id == msg.id) {
                let cid = self.taken.remove(at).map(|(_, cid)| cid);
                return Arrival::Shown { cid };
            }
            if !self.pending.is_empty() {
                self.early.retain(|early| early.id != msg.id);
                self.early.push_back(msg.clone());
                while self.early.len() > MAX_OWN {
                    self.early.pop_front();
                }
                return Arrival::Held;
            }
            return Arrival::Shown { cid: None };
        }
        if counts && !on_screen {
            self.unread = self.unread.saturating_add(1);
        }
        Arrival::Shown { cid: None }
    }

    /// Someone else's message the player may get a card for: the card being gathered takes it, or
    /// a new one starts (its number is returned: the session wakes for it after [`GATHER`]).
    pub fn gather(&mut self, room: &str, msg: &RoomChatMsg) -> Option<u64> {
        match &mut self.gather {
            Some(gather) if gather.room == room => {
                gather.count = gather.count.saturating_add(1);
                gather.last = msg.clone();
                None
            }
            _ => {
                self.gathers += 1;
                self.gather = Some(Gather {
                    id: self.gathers,
                    room: room.to_owned(),
                    count: 1,
                    last: msg.clone(),
                });
                Some(self.gathers)
            }
        }
    }

    /// The card gathered under number `id`, when it is still the one (none: seen meanwhile, the
    /// chip went, a newer one took its place).
    pub fn take_gather(&mut self, id: u64) -> Option<Gather> {
        if self.gather.as_ref().is_some_and(|gather| gather.id == id) {
            let gather = self.gather.take()?;
            (self.shown.as_deref() == Some(gather.room.as_str()) && self.unread > 0)
                .then_some(gather)
        } else {
            None
        }
    }
}

#[cfg(test)]
mod tests {
    use pl_voice::view::{VoiceCalling, VoicePerson};

    use super::*;

    fn msg(id: &str, from: &str, text: &str) -> RoomChatMsg {
        RoomChatMsg {
            id: id.into(),
            from: from.into(),
            at: 1_800_000_000_000,
            text: Some(text.into()),
            sticker: None,
        }
    }

    fn room_state(kind: VoiceRoomKind, people: &[&str], chat: bool, grew: bool) -> VoiceState {
        let mut state = crate::voice::tests::in_room();
        let room = state.room.as_mut().unwrap();
        room.kind = kind;
        room.chat = chat;
        room.grew = grew;
        let template = room.people.first().cloned();
        room.people = people
            .iter()
            .map(|id| VoicePerson {
                id: (*id).into(),
                ..template.clone().unwrap()
            })
            .collect();
        state
    }

    #[test]
    fn the_rooms_chat_shows_by_the_contracts_rule() {
        let friends = |id: &str| id == "f-ayse" || id == "f-can";
        // A friend room of two with a mutual friend: the friend's direct messages stand in.
        let two = room_state(VoiceRoomKind::Friend, &["f-ayse"], true, false);
        assert_eq!(
            direct_friend(&two, "me", friends, true, false).as_deref(),
            Some("f-ayse")
        );
        assert_eq!(shown_room(&two, "me", friends, true, false), None);
        // Chat off, the latch, someone who is no friend, a third person, `grew`: the room's own.
        let room = two.room.as_ref().unwrap().id.clone();
        for (state, chat_on, latched) in [
            (two.clone(), false, false),
            (two.clone(), true, true),
            (
                room_state(VoiceRoomKind::Friend, &["x-other"], true, false),
                true,
                false,
            ),
            (
                room_state(VoiceRoomKind::Friend, &["f-ayse", "f-can"], true, false),
                true,
                false,
            ),
            (
                room_state(VoiceRoomKind::Friend, &["f-ayse"], true, true),
                true,
                false,
            ),
        ] {
            assert_eq!(
                shown_room(&state, "me", friends, chat_on, latched),
                Some(room.clone())
            );
        }
        // A crew room always has its own (while it has a chat at all).
        let crew = room_state(VoiceRoomKind::Crew, &["f-ayse"], true, false);
        assert_eq!(
            shown_room(&crew, "me", friends, true, false),
            Some(room.clone())
        );
        let quiet = room_state(VoiceRoomKind::Crew, &["f-ayse"], false, false);
        assert_eq!(shown_room(&quiet, "me", friends, true, false), None);
        // The player's call still rings at a friend: that friend's messages.
        let mut calling = room_state(VoiceRoomKind::Friend, &[], true, false);
        calling.calling = Some(VoiceCalling {
            friend: "f-can".into(),
            until: 0,
        });
        assert_eq!(
            direct_friend(&calling, "me", friends, true, false).as_deref(),
            Some("f-can")
        );
        // Voice chat off: nothing.
        let mut off = crew.clone();
        off.enabled = false;
        assert_eq!(shown_room(&off, "me", friends, true, false), None);
    }

    #[test]
    fn the_log_follows_the_launchers_events() {
        let mut log = RoomLog::default();
        let whole = VoiceChatEvent::Log {
            room: "r1".into(),
            msgs: vec![msg("1", "u1", "selam"), msg("2", "u2", "naber")],
        };
        assert_eq!(log.apply(&whole), LogChange::Whole);
        assert_eq!(log.msgs.len(), 2);
        // A repeat replaces its message; a message of another room starts that room's log.
        let again = VoiceChatEvent::Msg {
            room: "r1".into(),
            msg: msg("2", "u2", "naber?"),
        };
        assert!(matches!(log.apply(&again), LogChange::Msg(_)));
        assert_eq!(log.msgs.len(), 2);
        assert_eq!(log.msgs[1].msg.text.as_deref(), Some("naber?"));
        // A deletion leaves a mark; one of a message not held (or of another room) is nothing.
        let del = VoiceChatEvent::Del {
            room: "r1".into(),
            id: "1".into(),
            by: RoomChatBy::Mod,
        };
        assert!(matches!(
            log.apply(&del),
            LogChange::Gone(Held {
                gone: Some(RoomChatBy::Mod),
                ..
            })
        ));
        assert_eq!(log.apply(&del), LogChange::Nothing);
        assert_eq!(log.get("r1", "1"), None);
        assert!(log.get("r1", "2").is_some());
        // Out of the room: the log goes.
        assert!(!log.keep_room(Some("r1")));
        assert!(log.keep_room(None));
        assert!(log.msgs.is_empty());
        let other = VoiceChatEvent::Msg {
            room: "r2".into(),
            msg: msg("9", "u1", "hey"),
        };
        log.apply(&other);
        assert_eq!(log.room.as_deref(), Some("r2"));
        // At most the launcher's own log.
        let many = VoiceChatEvent::Log {
            room: "r2".into(),
            msgs: (0..80).map(|i| msg(&i.to_string(), "u1", "x")).collect(),
        };
        log.apply(&many);
        assert_eq!(log.msgs.len(), LOG_MAX);
        assert_eq!(log.msgs[0].msg.id, "30");
    }

    #[test]
    fn the_players_own_message_lands_on_its_line_whichever_comes_first() {
        let mut link = RoomChatLink::default();
        link.set_shown(Some("r1".into()), Some("r1"));
        // The message first (the room's answer carries it), then the answer.
        link.sending("c1".into());
        assert_eq!(
            link.arrived("r1", &msg("7", "me", "selam"), true, false, false),
            Arrival::Held
        );
        let out = link.answered(
            "c1".into(),
            &RoomChatSend::Sent {
                id: "7".into(),
                at: 1,
            },
        );
        assert!(
            matches!(&out.frames[..], [ToGame::Sent { cid, id, .. }] if cid == "c1" && id == "7")
        );
        assert!(matches!(&out.show[..], [(m, Some(cid))] if m.id == "7" && cid == "c1"));
        // The answer first, then the message.
        link.sending("c2".into());
        let out = link.answered(
            "c2".into(),
            &RoomChatSend::Sent {
                id: "8".into(),
                at: 2,
            },
        );
        assert!(out.show.is_empty());
        assert_eq!(
            link.arrived("r1", &msg("8", "me", "ikinci"), true, false, false),
            Arrival::Shown {
                cid: Some("c2".into())
            }
        );
        // From the launcher's page (nothing waits): shown as it is.
        assert_eq!(
            link.arrived("r1", &msg("9", "me", "sayfadan"), true, false, false),
            Arrival::Shown { cid: None }
        );
        // A refusal says why; what came early meanwhile is shown once nothing waits.
        link.sending("c3".into());
        assert_eq!(
            link.arrived("r1", &msg("10", "me", "?"), true, false, false),
            Arrival::Held
        );
        let out = link.answered(
            "c3".into(),
            &RoomChatSend::Refused {
                error: RoomChatError::Rejected,
                reason: Some(RoomChatReason::Link),
                rule: Some("shortener".into()),
                retry_after: None,
                until: None,
            },
        );
        assert!(matches!(
            &out.frames[..],
            [ToGame::SendFailed { cid, reason: SendFailure::Link, rule: Some(rule) }]
                if cid == "c3" && rule == "shortener"
        ));
        assert!(matches!(&out.show[..], [(m, None)] if m.id == "10"));
    }

    #[test]
    fn others_messages_count_until_seen_and_gather_into_one_card() {
        let mut link = RoomChatLink::default();
        // No chip: nothing shows, nothing counts.
        assert_eq!(
            link.arrived("r1", &msg("1", "u1", "a"), false, false, true),
            Arrival::Quiet
        );
        assert_eq!(link.frame(), None);
        link.set_shown(Some("r1".into()), Some("r1"));
        assert_eq!(
            link.frame(),
            Some(ToGame::RoomChat(RoomChatShown {
                conv: Some("v:r1".into()),
                unread: 0,
            }))
        );
        link.arrived("r1", &msg("2", "u1", "a"), false, false, true);
        let first = link.gather("r1", &msg("2", "u1", "a"));
        assert!(first.is_some());
        link.arrived("r1", &msg("3", "u2", "b"), false, false, true);
        assert_eq!(link.gather("r1", &msg("3", "u2", "b")), None);
        // Blocked or ignored ones, and anything while the chip is on screen, do not count.
        link.arrived("r1", &msg("4", "u3", "c"), false, false, false);
        link.arrived("r1", &msg("5", "u2", "d"), false, true, true);
        assert_eq!(link.unread(), 2);
        let gathered = link.take_gather(first.unwrap()).unwrap();
        assert_eq!((gathered.count, gathered.last.id.as_str()), (2, "3"));
        assert_eq!(
            link.frame(),
            Some(ToGame::RoomChat(RoomChatShown {
                conv: Some("v:r1".into()),
                unread: 2,
            }))
        );
        // Seen before its card: no card.
        let next = link.gather("r1", &msg("6", "u1", "e")).unwrap();
        link.seen();
        assert_eq!(link.take_gather(next), None);
        // The chip goes: an empty frame says so, once.
        link.set_shown(None, None);
        assert_eq!(
            link.frame(),
            Some(ToGame::RoomChat(RoomChatShown::default()))
        );
        assert_eq!(link.frame(), None);
    }

    #[test]
    fn the_latch_holds_for_its_stay_only() {
        let mut link = RoomChatLink::default();
        assert!(link.latch("r1"));
        assert!(!link.latch("r1"));
        assert!(link.latched(Some("r1")));
        link.set_shown(Some("r1".into()), Some("r1"));
        assert!(link.latched(Some("r1")));
        link.set_shown(None, Some("r2"));
        assert!(!link.latched(Some("r1")));
    }

    #[test]
    fn messages_go_as_the_chat_shows_them() {
        let shown = Shown {
            me: "me",
            my_name: "KorAtesi",
            english: false,
            links: None,
        };
        let held = Held {
            msg: msg("5", "u1", "selam"),
            gone: None,
        };
        let view = msg_view("v:r1", &held, "x7Ender", &shown, None);
        assert_eq!(
            (view.from.as_str(), view.mine, view.by.as_deref()),
            ("x7Ender", false, Some("u1"))
        );
        let mine = msg_view(
            "v:r1",
            &Held {
                msg: msg("6", "me", "bende"),
                gone: None,
            },
            "?",
            &shown,
            Some("c1".into()),
        );
        assert_eq!(
            (mine.from.as_str(), mine.mine, mine.by, mine.cid.as_deref()),
            ("KorAtesi", true, None, Some("c1"))
        );
        // A message taken away: a mark, its words gone; a moderator's says so.
        let removed = msg_view(
            "v:r1",
            &Held {
                gone: Some(RoomChatBy::Mod),
                ..held.clone()
            },
            "x7Ender",
            &shown,
            None,
        );
        assert!(removed.deleted && removed.removed && removed.text.is_empty());
        let own = msg_view(
            "v:r1",
            &Held {
                gone: Some(RoomChatBy::Sender),
                ..held
            },
            "x7Ender",
            &shown,
            None,
        );
        assert!(own.deleted && !own.removed);
        let json = serde_json::to_value(ToGame::Msg(Box::new(view))).unwrap();
        assert_eq!(json["conv"], "v:r1");
        assert!(json.get("removed").is_none() && json.get("sid").is_none());
    }

    #[test]
    fn refusals_use_the_chats_words() {
        let refused = |error, reason| RoomChatSend::Refused {
            error,
            reason,
            rule: None,
            retry_after: None,
            until: None,
        };
        for (answer, want) in [
            (
                refused(RoomChatError::Rejected, Some(RoomChatReason::Phone)),
                SendFailure::Phone,
            ),
            (
                refused(RoomChatError::Rejected, Some(RoomChatReason::Words)),
                SendFailure::Words,
            ),
            (
                refused(RoomChatError::Rejected, Some(RoomChatReason::Length)),
                SendFailure::TooLong,
            ),
            (refused(RoomChatError::RateLimited, None), SendFailure::Rate),
            (refused(RoomChatError::Muted, None), SendFailure::Muted),
            (
                refused(RoomChatError::Limited, None),
                SendFailure::LimitedLinks,
            ),
            (
                refused(RoomChatError::StickersOff, None),
                SendFailure::FeatureOff,
            ),
            (refused(RoomChatError::Off, None), SendFailure::Off),
            (refused(RoomChatError::Failed, None), SendFailure::Offline),
        ] {
            assert_eq!(send_failure(&answer).map(|(reason, _)| reason), Some(want));
        }
        assert_eq!(send_failure(&answer_of(None)), None);
        assert_eq!(done_reason(&answer_of(None)), None);
        assert_eq!(
            done_reason(&answer_of(Some(RoomChatError::Gone))),
            Some("gone")
        );
        assert_eq!(
            done_reason(&answer_of(Some(RoomChatError::RateLimited))),
            Some("rate")
        );
        assert_eq!(
            done_reason(&answer_of(Some(RoomChatError::NotAllowed))),
            Some("failed")
        );
        assert_eq!(
            message_reason("inappropriate"),
            Some(ReportReason::Inappropriate)
        );
        assert_eq!(message_reason("name"), None);
        assert_eq!(room_of("v:abc"), Some("abc"));
        assert_eq!(room_of("v:"), None);
        assert_eq!(room_of("d:1:2"), None);
    }

    #[test]
    fn a_card_says_the_newest_and_keeps_words_back_in_streamer_mode() {
        let card = toast_of(
            "r1",
            &msg("5", "u1", "selam"),
            "x7Ender".into(),
            Vec::new(),
            false,
            false,
            true,
        );
        assert_eq!(
            (
                card.key.as_str(),
                card.conv.as_deref(),
                card.body.as_str(),
                card.by.as_deref()
            ),
            ("room:r1", Some("v:r1"), "selam", Some("u1"))
        );
        let quiet = toast_of(
            "r1",
            &msg("5", "u1", "selam"),
            "x7Ender".into(),
            Vec::new(),
            true,
            true,
            true,
        );
        assert_eq!(quiet.body, "New message");
    }
}
