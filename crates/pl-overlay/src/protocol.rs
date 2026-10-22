//! Bridge frames, version 1. One JSON object per line; `t` names the frame. Both sides ignore unknown
//! frames and fields, so additions keep the version and only breaking changes bump [`PROTO`]. Every
//! addition is recorded in `docs/tasks/ovl2-protocol.md` (shared with the mod's side).

use std::fmt;

use serde::{Deserialize, Serialize};

/// Protocol version this launcher speaks.
pub const PROTO: u32 = 1;
/// Longest frame the mod may send (player actions), in bytes without the newline.
pub const MAX_GAME_FRAME: usize = 16 * 1024;
/// Longest frame the launcher sends (a snapshot or a conversation history).
pub const MAX_LAUNCHER_FRAME: usize = 256 * 1024;
/// Longest chat message, the social service's rule.
pub const MAX_MESSAGE_CHARS: usize = 300;
/// Messages a `history` frame carries at most (the newest).
pub const MAX_HISTORY: usize = 100;

/// A value that never shows up in `Debug` output or logs.
#[derive(Clone, PartialEq, Eq, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("<redacted>")
    }
}

/// Frames from the mod.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum FromGame {
    /// First frame of every connection.
    Hello {
        proto: u32,
        token: Secret,
        #[serde(rename = "mod", default)]
        mod_version: String,
        #[serde(default)]
        mc: String,
        #[serde(default)]
        lang: String,
        /// What this mod can do beyond protocol 1 (`connect`: it switches servers on [`ToGame::Connect`]).
        #[serde(default)]
        features: Vec<String>,
    },
    /// The player opened or closed the panel.
    Panel { open: bool },
    /// The panel shows this conversation: send its history and count it as read.
    OpenConv { conv: String },
    /// The player sent a chat message; answer with `sent` or `send-failed` for `cid`.
    Send {
        cid: String,
        conv: String,
        text: String,
    },
    /// The player answered a crew call.
    Answer { call: String, answer: CallAnswer },
    /// The player pressed "Join" on a friend (the mod connects after its own confirmation).
    Join { friend: String },
    /// The answer to [`ToGame::Connect`]: the game is on its way to the server, or why not (`busy`,
    /// `refused`, `unsupported`, …).
    ConnectResult {
        id: String,
        ok: bool,
        #[serde(default)]
        reason: String,
    },
    /// Anything newer than this launcher.
    #[serde(other)]
    Unknown,
}

/// The `hello` feature of a mod that switches servers on [`ToGame::Connect`].
pub const FEATURE_CONNECT: &str = "connect";

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "kebab-case")]
pub enum CallAnswer {
    Coming,
    Soon,
    Cant,
}

/// Frames to the mod.
#[derive(Debug, Clone, PartialEq, Serialize)]
#[serde(tag = "t", rename_all = "kebab-case")]
pub enum ToGame {
    /// The handshake's answer. Comes again (followed by a `snapshot`) when the account, the
    /// features or the reduced-motion choice change mid-session.
    Welcome {
        proto: u32,
        launcher: String,
        me: Me,
        dnd: bool,
        #[serde(rename = "reducedMotion")]
        reduced_motion: bool,
        /// What works now: `friends` (signed in), `chat`, `calls`. Empty: not signed in.
        features: Vec<String>,
    },
    Snapshot {
        friends: Vec<FriendView>,
        convs: Vec<ConvView>,
        calls: Vec<CallView>,
        dnd: bool,
    },
    Friend(FriendView),
    FriendGone {
        id: String,
    },
    Conv(ConvView),
    /// A new message, or a known one again (same `id`) after an edit.
    Msg(MsgView),
    /// A message left the conversation (deleted for everyone, or dropped here).
    MsgGone {
        conv: String,
        id: String,
    },
    /// The newest messages of a conversation, oldest first: the answer to `open-conv`, or an
    /// empty list after its history was cleared on this computer.
    History {
        conv: String,
        msgs: Vec<MsgView>,
    },
    Sent {
        cid: String,
        id: String,
    },
    SendFailed {
        cid: String,
        reason: SendFailure,
    },
    Call(CallView),
    CallEnd {
        id: String,
    },
    Toast(ToastView),
    Dnd {
        on: bool,
    },
    /// The player chose "Bağlan" in the launcher (a call, an invite, a plan) and confirmed it:
    /// the game joins this server now. The one command the launcher gives the game; sent only
    /// right after that choice, only for Projects or an address a friend or crew shared, one at a
    /// time, and only to a mod that said `connect` in its hello. The mod answers with
    /// `connect-result`.
    Connect {
        id: String,
        host: String,
        port: u16,
    },
    Bye {
        reason: ByeReason,
    },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Me {
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct FriendView {
    pub id: String,
    pub name: String,
    #[serde(rename = "where")]
    pub place: Place,
    /// "Katıl" makes sense: on Projects, or on another server whose address they share.
    pub joinable: bool,
    /// The other server's address, only when they share it (`where: server`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Place {
    Projects,
    Launcher,
    Single,
    Server,
    /// In game, where is not known (the game's menus).
    Game,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConvView {
    pub id: String,
    pub kind: ConvKind,
    pub title: String,
    pub unread: u32,
    /// Muted on this computer, or its crew is muted: it never pops a toast.
    pub muted: bool,
    /// The player may write here now (still friends, still in the crew, chat on).
    pub writable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConvKind {
    Dm,
    Crew,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct MsgView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conv: Option<String>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    pub from: String,
    pub text: String,
    /// Unix time in milliseconds.
    pub at: i64,
    pub mine: bool,
    /// "düzenlendi".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub edited: bool,
    /// Deleted for everyone: `text` is empty and the mod shows a quiet "Bu mesaj silindi" line.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallView {
    pub id: String,
    /// The crew's name.
    pub crew: String,
    /// The caller's name.
    pub from: String,
    /// When the call started, Unix milliseconds.
    pub since: i64,
    /// When it ends by itself, Unix milliseconds (no frame says so then).
    pub until: i64,
    pub answers: AnswerCounts,
    pub mine: Option<CallAnswer>,
    /// Where the caller plays: `projects`, or `server` with `host` when they share it. Absent for
    /// an answer-only call.
    #[serde(rename = "where", skip_serializing_if = "Option::is_none")]
    pub place: Option<Place>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
    /// The player's own call (nothing to answer).
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub own: bool,
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize)]
pub struct AnswerCounts {
    pub coming: u32,
    pub soon: u32,
    pub cant: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ToastView {
    pub kind: ToastKind,
    /// Toasts with the same key update in place (one conversation, one call).
    pub key: String,
    pub title: String,
    pub body: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conv: Option<String>,
    /// The crew of a crew chat message (`title` is the sender).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub crew: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToastKind {
    /// `title` is the sender, `body` the text (the mod hides it when the player turned message
    /// text off in its settings).
    Message,
    /// A crew call: `title` is the crew, `body` the caller; the mod words it in the game's language.
    Call,
    /// "Play together": `title` is the friend; the mod words it.
    Invite,
    Info,
}

/// Why a message did not go. The mod shows a general "could not send" for a reason it does not
/// know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SendFailure {
    /// A link ("Bağlantı gönderilemez.").
    Link,
    /// A phone number.
    Phone,
    /// A word the service does not let through (it never says which).
    Words,
    /// Too many messages in a row.
    Rate,
    /// The day's messages are used up.
    DailyLimit,
    /// Empty, or too long after cleaning.
    TooLong,
    Blocked,
    /// No connection to the friends service: it may still go once the launcher reconnects.
    Offline,
    /// A moderator muted the player.
    Muted,
    /// Chat is switched off.
    Off,
    /// No longer friends, or no longer in the crew.
    NotAllowed,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ByeReason {
    /// The launcher is quitting.
    LauncherExit,
    /// The launcher closes because "on game start: close" is chosen.
    GameStartClose,
    /// The player turned the overlay off.
    Disabled,
    /// A newer connection from the same game took over.
    Replaced,
    /// Wrong token.
    Auth,
    /// No common protocol version.
    Proto,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hello_parses_and_hides_the_token() {
        let hello: FromGame = serde_json::from_str(
            r#"{"t":"hello","proto":1,"token":"abc","mod":"0.1.0","mc":"26.1.2","lang":"tr_tr","extra":1}"#,
        )
        .unwrap();
        let FromGame::Hello {
            proto,
            token,
            mod_version,
            ..
        } = &hello
        else {
            panic!("not a hello: {hello:?}");
        };
        assert_eq!(*proto, 1);
        assert_eq!(token.expose(), "abc");
        assert_eq!(mod_version, "0.1.0");
        assert!(!format!("{hello:?}").contains("abc"));
    }

    #[test]
    fn unknown_frames_parse_as_unknown() {
        let frame: FromGame = serde_json::from_str(r#"{"t":"screenshot-share","x":1}"#).unwrap();
        assert_eq!(frame, FromGame::Unknown);
    }

    #[test]
    fn player_actions_parse() {
        let send: FromGame =
            serde_json::from_str(r#"{"t":"send","cid":"c1","conv":"dm:1","text":"selam"}"#)
                .unwrap();
        assert!(matches!(send, FromGame::Send { ref text, .. } if text == "selam"));
        let answer: FromGame =
            serde_json::from_str(r#"{"t":"answer","call":"c1","answer":"cant"}"#).unwrap();
        assert!(matches!(
            answer,
            FromGame::Answer {
                answer: CallAnswer::Cant,
                ..
            }
        ));
        assert!(
            serde_json::from_str::<FromGame>(r#"{"t":"answer","call":"c1","answer":"x"}"#).is_err()
        );
        let join: FromGame = serde_json::from_str(r#"{"t":"join","friend":"f1"}"#).unwrap();
        assert_eq!(
            join,
            FromGame::Join {
                friend: "f1".into()
            }
        );
        let done: FromGame =
            serde_json::from_str(r#"{"t":"connect-result","id":"3","ok":true}"#).unwrap();
        assert_eq!(
            done,
            FromGame::ConnectResult {
                id: "3".into(),
                ok: true,
                reason: String::new()
            }
        );
        let busy: FromGame =
            serde_json::from_str(r#"{"t":"connect-result","id":"4","ok":false,"reason":"busy"}"#)
                .unwrap();
        assert!(
            matches!(busy, FromGame::ConnectResult { ok: false, ref reason, .. } if reason == "busy")
        );
    }

    #[test]
    fn hello_says_what_the_mod_can_do() {
        let old: FromGame = serde_json::from_str(r#"{"t":"hello","proto":1,"token":"t"}"#).unwrap();
        assert!(matches!(old, FromGame::Hello { ref features, .. } if features.is_empty()));
        let new: FromGame = serde_json::from_str(
            r#"{"t":"hello","proto":1,"token":"t","features":["connect","later"]}"#,
        )
        .unwrap();
        assert!(
            matches!(new, FromGame::Hello { ref features, .. } if features.iter().any(|f| f == FEATURE_CONNECT))
        );
        let connect = serde_json::to_value(ToGame::Connect {
            id: "1".into(),
            host: "play.projects.gg".into(),
            port: 25565,
        })
        .unwrap();
        assert_eq!(
            connect,
            serde_json::json!({"t":"connect","id":"1","host":"play.projects.gg","port":25565})
        );
    }

    #[test]
    fn frames_to_the_game_use_the_wire_names() {
        let toast = ToGame::Toast(ToastView {
            kind: ToastKind::Call,
            key: "call:c1".into(),
            title: "Gece Kuşları".into(),
            body: "Mert".into(),
            conv: None,
            crew: None,
        });
        let json = serde_json::to_value(&toast).unwrap();
        assert_eq!(json["t"], "toast");
        assert_eq!(json["kind"], "call");
        assert!(json.get("conv").is_none() && json.get("crew").is_none());

        let bye = serde_json::to_value(ToGame::Bye {
            reason: ByeReason::GameStartClose,
        })
        .unwrap();
        assert_eq!(
            bye,
            serde_json::json!({"t":"bye","reason":"game-start-close"})
        );

        let friend = serde_json::to_value(ToGame::Friend(FriendView {
            id: "f1".into(),
            name: "Ayşe".into(),
            place: Place::Projects,
            joinable: true,
            host: None,
        }))
        .unwrap();
        assert_eq!(friend["where"], "projects");
        assert_eq!(friend["t"], "friend");
        assert!(friend.get("host").is_none());

        let gone = serde_json::to_value(ToGame::MsgGone {
            conv: "d:1:2".into(),
            id: "i:5".into(),
        })
        .unwrap();
        assert_eq!(
            gone,
            serde_json::json!({"t":"msg-gone","conv":"d:1:2","id":"i:5"})
        );
        let failed = serde_json::to_value(ToGame::SendFailed {
            cid: "c".into(),
            reason: SendFailure::DailyLimit,
        })
        .unwrap();
        assert_eq!(failed["reason"], "daily-limit");
        let call = serde_json::to_value(ToGame::Call(CallView {
            id: "k".into(),
            crew: "Gece Kuşları".into(),
            from: "Mert".into(),
            since: 1,
            until: 2,
            answers: AnswerCounts::default(),
            mine: Some(CallAnswer::Coming),
            place: Some(Place::Server),
            host: Some("mc.example.net".into()),
            own: false,
        }))
        .unwrap();
        assert_eq!(call["where"], "server");
        assert!(call.get("own").is_none());
        assert_eq!(call["mine"], "coming");
        assert_eq!(call["answers"]["cant"], 0);
        let msg = serde_json::to_value(ToGame::Msg(MsgView {
            conv: Some("d:1:2".into()),
            id: "i:1".into(),
            cid: None,
            from: "Ayşe".into(),
            text: "selam".into(),
            at: 5,
            mine: false,
            edited: false,
            deleted: false,
        }))
        .unwrap();
        assert!(msg.get("edited").is_none() && msg.get("cid").is_none());
        assert!(msg.get("deleted").is_none());
    }
}
