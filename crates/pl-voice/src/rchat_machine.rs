//! The machine's room chat part (SQOL `rchat1`; [`crate::rchat`] holds the types). A child of
//! [`crate::machine`], so it shares the machine's stay, socket and requests.
//!
//! - The room's messages are kept for the stay (at most [`LOG_MAX`], with each one's seal for a
//!   report) and go to the page as `voice-chat`; the room's own log after each `hello` is merged
//!   in as the room says (from its first message on, the room's), so nothing is lost or doubled
//!   across a reconnect. The player's own message comes back in the `ack` (no `rchat` of its own).
//! - `room.grew`: the room had more than two people at some moment of the stay (the panel then
//!   shows the room's chat for good instead of the friend's private messages).

use std::collections::VecDeque;

use super::{Effect, Machine, Pending};
use crate::rchat::{
    ChatBody, LOG_MAX, REPORT_BEFORE, RoomChatBy, RoomChatError, RoomChatMsg, RoomChatSend,
    VoiceChatEvent, body_of, merge_log,
};
use crate::wire::{FromRoom, ToRoom};

/// One message as held: what the page sees, and the room's sealed form for a report.
#[derive(Debug, Clone)]
struct Held {
    msg: RoomChatMsg,
    raw: serde_json::Value,
}

/// Room chat's part of the machine.
#[derive(Debug, Default)]
pub(super) struct RoomChat {
    log: VecDeque<Held>,
    /// More than two people were in the room at some moment of the stay.
    grew: bool,
}

impl Pending {
    /// Requests room chat answers itself.
    fn is_chat(&self) -> bool {
        matches!(self, Self::Rchat | Self::RchatDel)
    }
}

impl Machine {
    /// The room the player is in has its chat on now (`hello.room.chat`).
    fn chat_on(&self, room: &str) -> bool {
        self.stay.as_ref().is_some_and(|stay| {
            stay.welcomed
                && stay
                    .room
                    .as_ref()
                    .is_some_and(|model| model.chat && model.id == room)
        })
    }

    fn chat_room(&self) -> Option<String> {
        self.stay
            .as_ref()
            .and_then(|stay| stay.room.as_ref())
            .map(|room| room.id.clone())
    }

    /// Whether `rid` waits for a room chat answer.
    pub(super) fn chat_pending(&self, rid: u32) -> bool {
        self.pending.get(&rid).is_some_and(Pending::is_chat)
    }

    /// Takes the frames only room chat reads (and the answers to its requests); hands back the
    /// rest.
    pub(super) fn chat_message(
        &mut self,
        message: FromRoom,
        out: &mut Vec<Effect>,
    ) -> Option<FromRoom> {
        match message {
            FromRoom::Rchat { m } => {
                self.chat_hold(m, out);
                None
            }
            FromRoom::RchatLog { ms } => {
                if let Some(room) = self.chat_room() {
                    let log: Vec<Held> = ms
                        .into_iter()
                        .filter_map(|raw| RoomChatMsg::of(&raw).map(|msg| Held { msg, raw }))
                        .collect();
                    merge_log(&mut self.chat.log, log, |held| held.msg.at);
                    out.push(Effect::Chat(VoiceChatEvent::Log {
                        room,
                        msgs: self.chat_msgs(),
                    }));
                }
                None
            }
            FromRoom::RchatDeleted { id, by } => {
                let before = self.chat.log.len();
                self.chat.log.retain(|held| held.msg.id != id);
                // A message not held (gone from the log, a staff deletion): nothing to tell.
                if self.chat.log.len() != before
                    && let Some(room) = self.chat_room()
                {
                    out.push(Effect::Chat(VoiceChatEvent::Del {
                        room,
                        id,
                        by: RoomChatBy::from_wire(&by),
                    }));
                }
                None
            }
            FromRoom::Ack {
                rid: Some(rid),
                id,
                at,
                m,
                ..
            } if self.chat_pending(rid) => {
                let pending = self.pending.remove(&rid);
                let answer = match (pending, id) {
                    (Some(Pending::Rchat), Some(id)) => {
                        // The sender gets no `rchat` of their own: the answer's message (as the
                        // room cleaned it) is the one to show.
                        if let Some(m) = m {
                            self.chat_hold(m, out);
                        }
                        RoomChatSend::Sent {
                            id,
                            at: at.unwrap_or(0),
                        }
                    }
                    // A deletion's answer carries nothing.
                    (Some(Pending::RchatDel), id) => RoomChatSend::Sent {
                        id: id.unwrap_or_default(),
                        at: at.unwrap_or(0),
                    },
                    _ => RoomChatSend::refused(RoomChatError::Failed),
                };
                out.push(Effect::ChatAnswer { rid, answer });
                None
            }
            FromRoom::Error {
                rid: Some(rid),
                code,
                retry_after,
                until,
                reason,
                rule,
                ..
            } if self.chat_pending(rid) => {
                self.pending.remove(&rid);
                out.push(Effect::ChatAnswer {
                    rid,
                    answer: RoomChatSend::of_error(
                        &code,
                        reason.as_deref(),
                        rule,
                        retry_after,
                        until,
                    ),
                });
                None
            }
            other => Some(other),
        }
    }

    /// A message the room sent (`m`), kept and told to the page; one that does not read is
    /// dropped. The room sends each message once; a repeat replaces it.
    fn chat_hold(&mut self, m: serde_json::Value, out: &mut Vec<Effect>) {
        let (Some(room), Some(msg)) = (self.chat_room(), RoomChatMsg::of(&m)) else {
            return;
        };
        self.chat.log.retain(|held| held.msg.id != msg.id);
        self.chat.log.push_back(Held {
            msg: msg.clone(),
            raw: m,
        });
        while self.chat.log.len() > LOG_MAX {
            self.chat.log.pop_front();
        }
        out.push(Effect::Chat(VoiceChatEvent::Msg { room, msg }));
    }

    /// After every room message: whether the room has had more than two people (as seen here, or
    /// as the room says: it keeps that across restarts); a chat switched off takes its messages
    /// with it (the room's are gone too).
    pub(super) fn chat_seen(&mut self, out: &mut Vec<Effect>) {
        let Some(room) = self.stay.as_ref().and_then(|stay| stay.room.as_ref()) else {
            return;
        };
        let (crowded, chat, id) = (
            room.grew || room.people.len() + 1 > 2,
            room.chat,
            room.id.clone(),
        );
        if crowded {
            self.chat.grew = true;
        }
        if !chat && !self.chat.log.is_empty() {
            self.chat.log.clear();
            out.push(Effect::Chat(VoiceChatEvent::Log {
                room: id,
                msgs: Vec::new(),
            }));
        }
    }

    /// The stay ended: the room's chat goes with it.
    pub(super) fn chat_end(&mut self) {
        self.chat = RoomChat::default();
    }

    /// `room.grew` for the view.
    pub(super) fn chat_grew(&self) -> bool {
        self.chat.grew
    }

    fn chat_msgs(&self) -> Vec<RoomChatMsg> {
        self.chat.log.iter().map(|held| held.msg.clone()).collect()
    }

    /// "Gönder" in the room's chat (`room`: the room the page shows): `rid` to wait on (its answer
    /// comes as [`Effect::ChatAnswer`]).
    pub fn chat_send(
        &mut self,
        room: &str,
        text: Option<&str>,
        sticker: Option<&str>,
    ) -> Result<(u32, Vec<Effect>), RoomChatSend> {
        if !self.chat_on(room) {
            return Err(RoomChatSend::refused(
                if self.chat_room().as_deref() == Some(room) {
                    RoomChatError::Off
                } else {
                    RoomChatError::Gone
                },
            ));
        }
        let (text, st) = match body_of(text, sticker)? {
            ChatBody::Text(text) => (Some(text), None),
            ChatBody::Sticker(sticker) => (None, Some(sticker)),
        };
        self.request(Pending::Rchat, |rid| ToRoom::Rchat { rid, text, st })
            .map_err(|_| RoomChatSend::refused(RoomChatError::Failed))
    }

    /// "Sil" on a message (the player's own; a leader's or moderator's anyone's: the room checks).
    pub fn chat_delete(
        &mut self,
        room: &str,
        id: &str,
    ) -> Result<(u32, Vec<Effect>), RoomChatSend> {
        if !self.chat_on(room) || !self.chat.log.iter().any(|held| held.msg.id == id) {
            return Err(RoomChatSend::refused(RoomChatError::Gone));
        }
        let id = id.to_owned();
        self.request(Pending::RchatDel, |rid| ToRoom::RchatDel { rid, id })
            .map_err(|_| RoomChatSend::refused(RoomChatError::Failed))
    }

    /// The room's messages held now (the page's start; none for another room).
    pub fn chat_log(&self, room: &str) -> Vec<RoomChatMsg> {
        if self.chat_room().as_deref() == Some(room) {
            self.chat_msgs()
        } else {
            Vec::new()
        }
    }

    /// "Rapor Et": the message as the room sealed it and at most [`REPORT_BEFORE`] before it, as
    /// held; none when it is not held (another room, gone).
    pub fn chat_evidence(
        &self,
        room: &str,
        id: &str,
    ) -> Option<(serde_json::Value, Vec<serde_json::Value>)> {
        if self.chat_room().as_deref() != Some(room) {
            return None;
        }
        let at = self.chat.log.iter().position(|held| held.msg.id == id)?;
        let before = self
            .chat
            .log
            .iter()
            .take(at)
            .rev()
            .take(REPORT_BEFORE)
            .rev()
            .map(|held| held.raw.clone())
            .collect();
        Some((self.chat.log[at].raw.clone(), before))
    }

    /// The player blocked `user`: their messages leave the page now (the room leaves them out of
    /// later logs).
    pub(super) fn chat_blocked(&mut self, user: &str, out: &mut Vec<Effect>) {
        let before = self.chat.log.len();
        self.chat.log.retain(|held| held.msg.from != user);
        if self.chat.log.len() != before
            && let Some(room) = self.chat_room()
        {
            out.push(Effect::Chat(VoiceChatEvent::Log {
                room,
                msgs: self.chat_msgs(),
            }));
        }
    }
}
