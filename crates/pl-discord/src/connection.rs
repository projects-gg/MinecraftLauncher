//! One IPC connection: handshake, commands and incoming frames.

use std::path::{Path, PathBuf};
use std::time::Duration;

use serde::{Deserialize, Serialize};
use serde_json::{Map, Value, json};
use tokio::io::{AsyncWriteExt, ReadHalf, WriteHalf};
use tokio::sync::mpsc;
use tokio::task::JoinHandle;

use crate::activity::Activity;
use crate::error::{Error, Result};
use crate::frame::{Frame, Opcode, read_frame, write_frame};
use crate::ipc::{BoxedStream, connect_any};

/// The Discord account the client is logged into (from `READY`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DiscordUser {
    pub id: String,
    pub username: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub global_name: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
}

/// A classified incoming message.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Incoming {
    Ready(Option<DiscordUser>),
    /// Discord answered a command; `nonce` echoes the one we sent.
    Ack {
        cmd: String,
        nonce: Option<String>,
    },
    /// An `ERROR` event (bad command, invalid activity field, …).
    Error {
        code: i64,
        message: String,
        nonce: Option<String>,
    },
    /// A ping that has already been answered.
    Ping,
    Other,
}

/// A connected, handshaken IPC session. Incoming frames are read by a background task and
/// delivered through [`DiscordIpc::next_frame`]; dropping the value closes the connection
/// (Discord then clears the presence of this process).
pub struct DiscordIpc {
    writer: WriteHalf<BoxedStream>,
    frames: mpsc::Receiver<Result<Frame>>,
    reader: JoinHandle<()>,
    user: Option<DiscordUser>,
    endpoint: PathBuf,
}

impl std::fmt::Debug for DiscordIpc {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DiscordIpc")
            .field("endpoint", &self.endpoint)
            .field("user", &self.user)
            .finish_non_exhaustive()
    }
}

impl Drop for DiscordIpc {
    fn drop(&mut self) {
        self.reader.abort();
    }
}

impl DiscordIpc {
    /// Connects to the first available endpoint, sends the handshake and waits (up to
    /// `timeout`) for `READY`.
    pub async fn connect(
        client_id: &str,
        endpoints: &[PathBuf],
        timeout: Duration,
    ) -> Result<Self> {
        let (stream, endpoint) = connect_any(endpoints).await?;
        let (read_half, writer) = tokio::io::split(stream);
        let (sender, frames) = mpsc::channel(16);
        let reader = tokio::spawn(read_loop(read_half, sender));
        let mut ipc = Self {
            writer,
            frames,
            reader,
            user: None,
            endpoint,
        };
        ipc.send(
            Opcode::Handshake,
            &json!({ "v": 1, "client_id": client_id }),
        )
        .await?;
        ipc.user = tokio::time::timeout(timeout, ipc.wait_ready())
            .await
            .map_err(|_| Error::Timeout)??;
        Ok(ipc)
    }

    pub fn user(&self) -> Option<&DiscordUser> {
        self.user.as_ref()
    }

    pub fn endpoint(&self) -> &Path {
        &self.endpoint
    }

    /// The next frame from Discord; `None` once the connection is gone. Cancel safe.
    pub async fn next_frame(&mut self) -> Option<Result<Frame>> {
        self.frames.recv().await
    }

    /// Answers pings, turns `CLOSE` into [`Error::Closed`] and classifies everything else.
    pub async fn process(&mut self, frame: Frame) -> Result<Incoming> {
        match frame.opcode {
            Opcode::Ping => {
                write_frame(&mut self.writer, &Frame::new(Opcode::Pong, frame.payload)).await?;
                Ok(Incoming::Ping)
            }
            Opcode::Close => {
                let value: Value = serde_json::from_slice(&frame.payload).unwrap_or(Value::Null);
                Err(Error::Closed {
                    code: value["code"].as_i64().unwrap_or_default(),
                    message: value["message"].as_str().unwrap_or_default().to_owned(),
                })
            }
            Opcode::Frame => Ok(classify(&serde_json::from_slice(&frame.payload)?)),
            Opcode::Handshake | Opcode::Pong => Ok(Incoming::Other),
        }
    }

    /// Sends `SET_ACTIVITY` for this process (`None` clears it). Returns the nonce; the
    /// answer arrives later as [`Incoming::Ack`] or [`Incoming::Error`].
    pub async fn set_activity(&mut self, activity: Option<&Activity>) -> Result<String> {
        let nonce = uuid::Uuid::new_v4().to_string();
        let mut args = Map::new();
        args.insert("pid".into(), json!(std::process::id()));
        if let Some(activity) = activity {
            args.insert("activity".into(), activity.to_wire());
        }
        self.send(
            Opcode::Frame,
            &json!({ "cmd": "SET_ACTIVITY", "args": args, "nonce": nonce }),
        )
        .await?;
        Ok(nonce)
    }

    /// Sends `CLOSE` and shuts the connection down.
    pub async fn close(mut self) {
        let _ = self.send(Opcode::Close, &json!({})).await;
        let _ = self.writer.shutdown().await;
    }

    async fn send(&mut self, opcode: Opcode, value: &Value) -> Result<()> {
        write_frame(&mut self.writer, &Frame::json(opcode, value)?).await
    }

    async fn wait_ready(&mut self) -> Result<Option<DiscordUser>> {
        loop {
            let frame = self.frames.recv().await.ok_or(Error::Disconnected)??;
            match self.process(frame).await? {
                Incoming::Ready(user) => return Ok(user),
                Incoming::Error { code, message, .. } => return Err(Error::Rpc { code, message }),
                _ => {}
            }
        }
    }
}

async fn read_loop(mut reader: ReadHalf<BoxedStream>, sender: mpsc::Sender<Result<Frame>>) {
    loop {
        let frame = read_frame(&mut reader).await;
        let failed = frame.is_err();
        if sender.send(frame).await.is_err() || failed {
            break;
        }
    }
}

fn classify(message: &Value) -> Incoming {
    let text = |value: &Value| value.as_str().map(str::to_owned);
    let cmd = message["cmd"].as_str().unwrap_or_default();
    let nonce = text(&message["nonce"]);
    let data = &message["data"];
    match message["evt"].as_str() {
        Some("READY") => Incoming::Ready(parse_user(&data["user"])),
        Some("ERROR") => Incoming::Error {
            code: data["code"].as_i64().unwrap_or_default(),
            message: data["message"].as_str().unwrap_or_default().to_owned(),
            nonce,
        },
        _ if !cmd.is_empty() && cmd != "DISPATCH" => Incoming::Ack {
            cmd: cmd.to_owned(),
            nonce,
        },
        _ => Incoming::Other,
    }
}

fn parse_user(user: &Value) -> Option<DiscordUser> {
    Some(DiscordUser {
        id: user["id"].as_str()?.to_owned(),
        username: user["username"].as_str()?.to_owned(),
        global_name: user["global_name"].as_str().map(str::to_owned),
        avatar: user["avatar"].as_str().map(str::to_owned),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classifies_discord_messages() {
        let ready = json!({
            "cmd": "DISPATCH",
            "evt": "READY",
            "data": { "v": 1, "user": { "id": "1", "username": "ejder", "global_name": "Ejder", "avatar": null } },
            "nonce": null
        });
        assert_eq!(
            classify(&ready),
            Incoming::Ready(Some(DiscordUser {
                id: "1".into(),
                username: "ejder".into(),
                global_name: Some("Ejder".into()),
                avatar: None,
            }))
        );
        let error = json!({
            "cmd": "SET_ACTIVITY",
            "evt": "ERROR",
            "data": { "code": 4000, "message": "child \"activity\" fails" },
            "nonce": "n1"
        });
        assert_eq!(
            classify(&error),
            Incoming::Error {
                code: 4000,
                message: "child \"activity\" fails".into(),
                nonce: Some("n1".into())
            }
        );
        let ack = json!({ "cmd": "SET_ACTIVITY", "data": {}, "evt": null, "nonce": "n2" });
        assert_eq!(
            classify(&ack),
            Incoming::Ack {
                cmd: "SET_ACTIVITY".into(),
                nonce: Some("n2".into())
            }
        );
        assert_eq!(
            classify(&json!({ "cmd": "DISPATCH", "evt": "ACTIVITY_JOIN" })),
            Incoming::Other
        );
        assert_eq!(classify(&json!("garbage")), Incoming::Other);
    }
}
