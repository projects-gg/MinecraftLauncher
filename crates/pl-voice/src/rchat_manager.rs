//! The room chat's side of [`Voice`] (SQOL `rchat1`): the page's chat commands, each one step of
//! the machine (a child of [`crate::manager`]).

use std::time::Duration;

use tokio::sync::oneshot;

use super::{Voice, lock};
use crate::machine::{Effect, Machine};
use crate::rchat::{RoomChatError, RoomChatMsg, RoomChatSend};

/// SQOL-R3: how long a room chat request waits for its answer at most. The room answers every
/// request while its socket lives, and the machine answers at once when the socket goes; a slow
/// answer (past the usual 10 s) still settles the sender's own message instead of a failure the
/// player would send again (a duplicate in the room). Only a room that never answers is given up.
const CHAT_ANSWER_WAIT: Duration = Duration::from_secs(60);

impl Voice {
    /// Sends `make`'s request and waits for the room's answer (at most [`CHAT_ANSWER_WAIT`]).
    async fn chat_request(
        &self,
        make: impl FnOnce(&mut Machine) -> Result<(u32, Vec<Effect>), RoomChatSend>,
    ) -> RoomChatSend {
        let (sender, receiver) = oneshot::channel();
        let rid = self.step(|machine, _, _| match make(machine) {
            Ok((rid, effects)) => {
                lock(&self.inner.chat_waiters).insert(rid, sender);
                (Ok(rid), effects)
            }
            Err(answer) => (Err(answer), Vec::new()),
        });
        let rid = match rid {
            Ok(rid) => rid,
            Err(answer) => return answer,
        };
        match tokio::time::timeout(CHAT_ANSWER_WAIT, receiver).await {
            Ok(Ok(answer)) => answer,
            _ => {
                lock(&self.inner.chat_waiters).remove(&rid);
                lock(&self.inner.machine).forget(rid);
                RoomChatSend::refused(RoomChatError::Failed)
            }
        }
    }

    /// "Gönder" in the room's chat: a text or a sticker (`room`: the room the page shows).
    pub async fn chat_send(
        &self,
        room: String,
        text: Option<String>,
        sticker: Option<String>,
    ) -> RoomChatSend {
        self.chat_request(|machine| machine.chat_send(&room, text.as_deref(), sticker.as_deref()))
            .await
    }

    /// "Sil" on a message: none when it went.
    pub async fn chat_delete(&self, room: String, id: String) -> Option<RoomChatError> {
        self.chat_request(|machine| machine.chat_delete(&room, &id))
            .await
            .error()
    }

    /// The room's messages held now (the page's start).
    pub fn chat_log(&self, room: &str) -> Vec<RoomChatMsg> {
        lock(&self.inner.machine).chat_log(room)
    }

    /// "Rapor Et": the message as the room sealed it and the ones before it (the report's
    /// evidence), when it is held.
    pub fn chat_evidence(
        &self,
        room: &str,
        id: &str,
    ) -> Option<(serde_json::Value, Vec<serde_json::Value>)> {
        lock(&self.inner.machine).chat_evidence(room, id)
    }
}
