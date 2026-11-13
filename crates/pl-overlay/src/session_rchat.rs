//! SQOL-O: a session's side of the voice room's own chat (the "Oda" chip, [`crate::rchat`]): what
//! its mod hears of it, the player's requests from it and the cards for it. A child of
//! [`crate::session`] (the session's own fields, on its own task).
//!
//! Only a mod whose hello lists `voice` and `rchat` hears any of it, and only while the room's
//! chat shows as its own chip; anything such a mod sends about a room chat the chip does not show
//! is answered at once and goes nowhere.

use std::sync::Arc;
use std::time::Instant;

use pl_voice::rchat::{RoomChatSend, VoiceChatEvent};

use super::{
    PEOPLE_BURST, PEOPLE_WINDOW, SEND_BURST, SEND_WINDOW, Session, SocialActions, full,
    report_note, sticker_ref_ok,
};
use crate::protocol::{MAX_MESSAGE_CHARS, SendFailure, ToGame, ToastKind};
use crate::rchat::{self, Arrival, Held, LogChange, RoomChatAct, Shown};
use crate::voice;

/// What wakes a session for the room's chat: the launcher's answer to one of the player's requests,
/// or the end of a card's gathering time.
pub(crate) enum RoomWake {
    /// The answer to the player's message (`cid` its line's key in the panel).
    Sent { cid: String, answer: RoomChatSend },
    /// The answer to "Sil" or "Rapor Et" (`act` as `done` names it, `id` the message's).
    Done {
        act: &'static str,
        id: String,
        answer: RoomChatSend,
    },
    /// A card's four seconds are over.
    Gather { id: u64 },
}

/// `done`'s act for "Sil" on the player's own room chat message.
const DELETE: &str = "room-delete";
/// `done`'s act for "Rapor Et" on someone else's.
const REPORT: &str = "room-report";

impl<S: SocialActions> Session<S> {
    /// The live mod shows the room's chat (`voice` and `rchat` in its hello).
    fn shows_room_chat(&self) -> bool {
        self.live.as_ref().is_some_and(|live| live.rchat)
    }

    /// The player is in `room` now.
    fn in_room(&self, room: &str) -> bool {
        self.voice
            .as_ref()
            .and_then(|(state, _)| state.room.as_ref())
            .is_some_and(|here| here.id == room)
    }

    /// The panel shows the chip of `room` now (its messages are seen as they come).
    fn room_on_screen(&self, room: &str) -> bool {
        let conv = rchat::conv_of(room);
        self.live
            .as_ref()
            .is_some_and(|live| live.panel && live.open.as_deref() == Some(conv.as_str()))
    }

    /// The chip's room worked out again (voice chat's state, the friend list, chat, the latch), and
    /// `room-chat` when what the mod has differs.
    pub(super) fn refresh_room_chat(&mut self) {
        let state = self.voice.as_ref().map(|(state, _)| Arc::clone(state));
        let stay = state
            .as_ref()
            .and_then(|state| state.room.as_ref())
            .map(|room| room.id.clone());
        self.room_log.keep_room(stay.as_deref());
        let latched = self.room_chat.latched(stay.as_deref());
        let scene = &self.scene;
        let chat_on = scene.features.iter().any(|feature| feature == "chat");
        let shown = state.as_deref().and_then(|state| {
            rchat::shown_room(
                state,
                &scene.me_id,
                |id| scene.friend(id).is_some(),
                chat_on,
                latched,
            )
        });
        self.room_chat.set_shown(shown, stay.as_deref());
        if self.shows_room_chat()
            && let Some(frame) = self.room_chat.frame()
        {
            self.send(vec![frame]);
        }
    }

    /// A new connection: it heard nothing of the room's chat yet.
    pub(super) fn greet_room_chat(&mut self) {
        self.room_chat.forget_told();
        self.refresh_room_chat();
    }

    /// The launcher's news of the room's chat (the hub keeps the same log).
    pub(super) fn on_room_event(&mut self, event: &VoiceChatEvent) {
        let change = self.room_log.apply(event);
        let Some(room) = self.room_log.room.clone() else {
            return;
        };
        let told = self.shows_room_chat() && self.room_chat.shown() == Some(room.as_str());
        match change {
            LogChange::Nothing => {}
            LogChange::Whole if told => {
                let frame = self.room_history(&room);
                self.send(vec![frame]);
            }
            LogChange::Gone(held) if told => {
                let frame = self.room_msg(&room, &held, None);
                self.send(vec![frame]);
            }
            LogChange::Whole | LogChange::Gone(_) => {}
            LogChange::Msg(held) => self.on_room_msg(&room, &held),
        }
    }

    /// One of the room's messages came: to the panel (the player's own on its line), counted as
    /// unread unless the chip is on screen, gathered into a card.
    fn on_room_msg(&mut self, room: &str, held: &Held) {
        let msg = &held.msg;
        let mine = !self.scene.me_id.is_empty() && msg.from == self.scene.me_id;
        // Someone else wrote in the room while a friend's direct messages stood in for its chat:
        // the room's own chat shows for the rest of the stay (the launcher's panel does the same).
        if !mine
            && self.room_chat.shown() != Some(room)
            && self.in_room(room)
            && self.room_chat.latch(room)
        {
            self.refresh_room_chat();
        }
        if !self.shows_room_chat() {
            return;
        }
        let on_screen = self.room_on_screen(room);
        let counts =
            !mine && !self.scene.is_blocked(&msg.from) && !self.scene.is_ignored(&msg.from);
        let Arrival::Shown { cid } = self.room_chat.arrived(room, msg, mine, on_screen, counts)
        else {
            return;
        };
        let mut frames = vec![self.room_msg(room, held, cid)];
        // The chip's count moved.
        frames.extend(self.room_chat.frame());
        self.send(frames);
        if counts
            && !on_screen
            && self.room_cards()
            && let Some(id) = self.room_chat.gather(room, msg)
        {
            let wake = self.room_tx.clone();
            tokio::spawn(async move {
                tokio::time::sleep(rchat::GATHER).await;
                let _ = wake.send(RoomWake::Gather { id });
            });
        }
    }

    /// This game shows cards for the room's chat now: message cards on, not "Rahatsız etmeyin".
    fn room_cards(&self) -> bool {
        !self.scene.dnd
            && self
                .live
                .as_ref()
                .is_some_and(|live| live.rchat && live.shows(ToastKind::Message))
    }

    /// The card gathered under `id`: one card for the messages the player has not seen (the
    /// newest's sender and words; the chip's unread count on it), unless the panel shows the chip
    /// now, "Rahatsız etmeyin" holds or the player turned message cards off meanwhile. While the
    /// game is in the player's share it keeps its words to itself (OVL-SHARE).
    fn on_room_gather(&mut self, id: u64) {
        let Some(gather) = self.room_chat.take_gather(id) else {
            return;
        };
        if !self.room_cards() || self.room_on_screen(&gather.room) {
            return;
        }
        let from = gather.last.from.as_str();
        let title = self
            .scene
            .nickname_of(from)
            .map_or_else(|| self.room_name(from), str::to_owned);
        let listener = self.listener();
        let card = rchat::toast_of(
            &gather.room,
            &gather.last,
            title,
            self.scene.marks_of(from).to_vec(),
            listener.english,
            self.scene.streamer,
            listener.chat2,
        );
        let card = if self.private() {
            voice::keep_private(card)
        } else {
            Some(card)
        };
        let Some(card) = card else {
            return;
        };
        let mut frames: Vec<ToGame> = self.room_chat.frame().into_iter().collect();
        frames.push(ToGame::Toast(card));
        self.send(frames);
    }

    /// Someone's name in the room's chat: as the lists know them, else the room's card of them (a
    /// friend's friend), else "Biri".
    fn room_name(&self, id: &str) -> String {
        if let Some(name) = self.scene.name_of(id) {
            return name.to_owned();
        }
        let card = self.voice.as_ref().and_then(|(state, _)| {
            state
                .profiles
                .iter()
                .find(|card| card.id == id && !card.name.is_empty())
                .map(|card| card.name.clone())
        });
        card.unwrap_or_else(|| {
            if self.listener().english {
                "Someone"
            } else {
                "Biri"
            }
            .to_owned()
        })
    }

    /// One of `room`'s messages as `msg`.
    fn room_msg(&self, room: &str, held: &Held, cid: Option<String>) -> ToGame {
        let shown = Shown {
            me: &self.scene.me_id,
            my_name: &self.scene.me,
            english: self.listener().english,
            links: self.scene.links.as_ref(),
        };
        let name = self.room_name(&held.msg.from);
        ToGame::Msg(Box::new(rchat::msg_view(
            &rchat::conv_of(room),
            held,
            &name,
            &shown,
            cid,
        )))
    }

    /// `room`'s whole log as `history`.
    fn room_history(&self, room: &str) -> ToGame {
        let shown = Shown {
            me: &self.scene.me_id,
            my_name: &self.scene.me,
            english: self.listener().english,
            links: self.scene.links.as_ref(),
        };
        rchat::history_frame(room, &self.room_log, |id| self.room_name(id), &shown)
    }

    /// The panel shows the chip of the room in `conv`: its whole log; everything in it is seen and
    /// no card waits. A conversation of a room the chip does not show gets nothing (the panel hears
    /// the chip is gone).
    pub(super) fn on_room_open(&mut self, conv: String) {
        if let Some(live) = self.live.as_mut() {
            live.panel = true;
            live.open = Some(conv.clone());
        }
        let Some(room) = rchat::room_of(&conv).map(str::to_owned) else {
            return;
        };
        if !self.shows_room_chat() || self.room_chat.shown() != Some(room.as_str()) {
            return;
        }
        self.room_chat.seen();
        let mut frames = vec![self.room_history(&room)];
        frames.extend(self.room_chat.frame());
        self.send(frames);
    }

    /// "Gönder" in the room's chat: what can never go is answered here (the chip is gone, too
    /// fast, too long, not a sticker), the room decides the rest; its answer comes as a wake-up.
    pub(super) fn on_room_send(
        &mut self,
        cid: String,
        conv: &str,
        text: &str,
        sticker: Option<&str>,
    ) {
        let Some(room) = rchat::room_of(conv).map(str::to_owned) else {
            return;
        };
        if !self.shows_room_chat() {
            return;
        }
        let now = Instant::now();
        let refused = if self.room_chat.shown() != Some(room.as_str()) {
            Some(SendFailure::Off)
        } else if full(&mut self.recent_sends, now, SEND_WINDOW, SEND_BURST) {
            Some(SendFailure::Rate)
        } else if sticker.is_none() && text.chars().count() > MAX_MESSAGE_CHARS {
            Some(SendFailure::TooLong)
        } else if sticker.is_none() && text.trim().is_empty() {
            Some(SendFailure::Offline)
        } else if sticker.is_some_and(|sticker| !sticker_ref_ok(sticker)) {
            Some(SendFailure::FeatureOff)
        } else {
            None
        };
        if let Some(reason) = refused {
            self.send(vec![ToGame::SendFailed {
                cid,
                reason,
                rule: None,
            }]);
            return;
        }
        self.recent_sends.push_back(now);
        let request = RoomChatAct::Send {
            room,
            text: sticker.is_none().then(|| text.to_owned()),
            sticker: sticker.map(str::to_owned),
        };
        let Some(doing) = (self.room_act)(request) else {
            self.send(vec![ToGame::SendFailed {
                cid,
                reason: SendFailure::Offline,
                rule: None,
            }]);
            return;
        };
        self.room_chat.sending(cid.clone());
        let wake = self.room_tx.clone();
        tokio::spawn(async move {
            let answer = doing.await;
            let _ = wake.send(RoomWake::Sent { cid, answer });
        });
    }

    /// "Sil" on the player's own message in the room's chat (one the chip shows).
    pub(super) fn on_room_delete(&mut self, conv: &str, id: String) {
        let Some(room) = rchat::room_of(conv).map(str::to_owned) else {
            return;
        };
        if !self.shows_room_chat() {
            return;
        }
        let own = self.room_chat.shown() == Some(room.as_str())
            && self
                .room_log
                .get(&room, &id)
                .is_some_and(|msg| msg.from == self.scene.me_id);
        if self.room_press_allowed(DELETE, &id, own) {
            self.room_request(DELETE, id.clone(), RoomChatAct::Delete { room, id });
        }
    }

    /// "Rapor Et" on someone else's message in the room's chat (one the chip shows), with a
    /// message's reason and maybe a note.
    pub(super) fn on_room_report(&mut self, conv: &str, id: String, reason: &str, note: &str) {
        let Some(room) = rchat::room_of(conv).map(str::to_owned) else {
            return;
        };
        if !self.shows_room_chat() {
            return;
        }
        let theirs = self.room_chat.shown() == Some(room.as_str())
            && self
                .room_log
                .get(&room, &id)
                .is_some_and(|msg| msg.from != self.scene.me_id);
        let Some(reason) = rchat::message_reason(reason) else {
            self.done(REPORT, &id, Some("failed"));
            return;
        };
        if self.room_press_allowed(REPORT, &id, theirs) {
            let note = report_note(note);
            self.room_request(
                REPORT,
                id.clone(),
                RoomChatAct::Report {
                    room,
                    id,
                    reason,
                    note,
                },
            );
        }
    }

    /// Whether a deletion or a report may go: not too many (the person menu's ten seconds), about
    /// a message the chip shows (`known`). Otherwise the mod hears why at once.
    fn room_press_allowed(&mut self, act: &'static str, id: &str, known: bool) -> bool {
        let now = Instant::now();
        let refused = if full(&mut self.recent_room, now, PEOPLE_WINDOW, PEOPLE_BURST) {
            Some("rate")
        } else if known {
            None
        } else {
            Some("gone")
        };
        if let Some(reason) = refused {
            self.done(act, id, Some(reason));
            return false;
        }
        self.recent_room.push_back(now);
        true
    }

    /// Hands a deletion or a report to the launcher; its answer comes as a wake-up (`done`).
    fn room_request(&mut self, act: &'static str, id: String, request: RoomChatAct) {
        let Some(doing) = (self.room_act)(request) else {
            self.done(act, &id, Some("failed"));
            return;
        };
        tracing::debug!(act, "a room chat request from the in-game overlay");
        let wake = self.room_tx.clone();
        tokio::spawn(async move {
            let answer = doing.await;
            let _ = wake.send(RoomWake::Done { act, id, answer });
        });
    }

    /// A wake-up for the room's chat.
    pub(super) fn on_room_wake(&mut self, wake: RoomWake) {
        match wake {
            RoomWake::Sent { cid, answer } => {
                let out = self.room_chat.answered(cid, &answer);
                let room = self.room_log.room.clone().unwrap_or_default();
                let mut frames = out.frames;
                for (msg, cid) in out.show {
                    // As the log holds it now (it may have been taken away meanwhile).
                    let held = self
                        .room_log
                        .msgs
                        .iter()
                        .find(|held| held.msg.id == msg.id)
                        .cloned()
                        .unwrap_or(Held { msg, gone: None });
                    frames.push(self.room_msg(&room, &held, cid));
                }
                self.send(frames);
            }
            RoomWake::Done { act, id, answer } => {
                self.done(act, &id, rchat::done_reason(&answer));
            }
            RoomWake::Gather { id } => self.on_room_gather(id),
        }
    }
}
