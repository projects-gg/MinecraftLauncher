//! The room chat's side of [`Voice`] (SQOL `rchat1`): the page's chat commands, each one step of
//! the machine (a child of [`crate::manager`]).

use tokio::sync::oneshot;

use super::{ANSWER_WAIT, Voice, lock};
use crate::machine::{Effect, Machine};
use crate::rchat::{RoomChatError, RoomChatMsg, RoomChatSend};

impl Voice {
    /// Sends `make`'s request and waits for the room's answer (at most [`ANSWER_WAIT`]).
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
        match tokio::time::timeout(ANSWER_WAIT, receiver).await {
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
