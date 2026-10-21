//! Bridge frames, version 1. One JSON object per line; `t` names the frame. Both sides ignore unknown
//! frames and fields, so additions keep the version and only breaking changes bump [`PROTO`].

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
    /// The player pressed "Join" on a friend.
    Join { friend: String },
    /// Anything newer than this launcher.
    #[serde(other)]
    Unknown,
}

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
    Welcome {
        proto: u32,
        launcher: String,
        me: Me,
        dnd: bool,
        #[serde(rename = "reducedMotion")]
        reduced_motion: bool,
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
    Msg(MsgView),
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
    pub joinable: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum Place {
    Projects,
    Launcher,
    Single,
    Server,
    Offline,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ConvView {
    pub id: String,
    pub kind: ConvKind,
    pub title: String,
    pub unread: u32,
    pub muted: bool,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct CallView {
    pub id: String,
    pub crew: String,
    pub from: String,
    pub since: i64,
    pub answers: AnswerCounts,
    pub mine: Option<CallAnswer>,
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
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ToastKind {
    /// `title` is the sender, `body` the text (empty when the player hides message text).
    Message,
    /// A crew call: `title` is the crew, `body` the caller; the mod words it in the game's language.
    Call,
    /// "Play together": `title` is the friend; the mod words it.
    Invite,
    Info,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum SendFailure {
    Link,
    Rate,
    TooLong,
    Blocked,
    Offline,
    Muted,
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
    }

    #[test]
    fn frames_to_the_game_use_the_wire_names() {
        let toast = ToGame::Toast(ToastView {
            kind: ToastKind::Call,
            key: "call:c1".into(),
            title: "Gece Kuşları".into(),
            body: "Mert ekibi çağırıyor".into(),
            conv: None,
        });
        let json = serde_json::to_value(&toast).unwrap();
        assert_eq!(json["t"], "toast");
        assert_eq!(json["kind"], "call");
        assert!(json.get("conv").is_none());

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
        }))
        .unwrap();
        assert_eq!(friend["where"], "projects");
        assert_eq!(friend["t"], "friend");
    }
}
