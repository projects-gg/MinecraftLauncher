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
        /// Empty for a sticker.
        #[serde(default)]
        text: String,
        /// CHAT2: the message it answers, by its `sid`.
        #[serde(default)]
        reply: Option<i64>,
        /// CHAT2: a Projects sticker (`pl1/gg`) instead of text.
        #[serde(default)]
        sticker: Option<String>,
    },
    /// CHAT2: the player gives (`on`) or takes back their reaction `e` on message `sid`. The message
    /// comes back with its reactions; a refusal is `react-failed`.
    React {
        conv: String,
        sid: i64,
        e: String,
        on: bool,
    },
    /// CHAT2: the player writes in the panel's field ("yazıyor…"; the mod sends it at most every
    /// 4 s, only while the field has text).
    Typing { conv: String },
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
/// The `hello` feature of a mod that knows CHAT2's frames and fields (`docs/tasks/ovl2-protocol.md`,
/// CHAT2-O): it hears "yazıyor…" and the precise reasons, and its panel asks for link cards.
pub const FEATURE_CHAT2: &str = "chat2";

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
    /// A new message, or a known one again (same `id`) after an edit. (Boxed: a message with
    /// CHAT2's fields is the largest frame by far.)
    Msg(Box<MsgView>),
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
        /// CHAT2: the service's id of the message (replies and reactions name it).
        #[serde(skip_serializing_if = "Option::is_none")]
        sid: Option<i64>,
    },
    SendFailed {
        cid: String,
        reason: SendFailure,
        /// CHAT2, with `link`: the link rule that refused it, when the service said.
        #[serde(skip_serializing_if = "Option::is_none")]
        rule: Option<String>,
    },
    /// CHAT2: someone writes in a conversation now (`by` an opaque person key, `name` as the lists
    /// show it). Shown for 6 s after the last; a message of theirs ends it. Sent only while the panel
    /// is open, and only to a mod that said `chat2`.
    Typing {
        conv: String,
        by: String,
        name: String,
    },
    /// CHAT2: the player's reaction did not go (the message comes back as it was).
    ReactFailed {
        conv: String,
        sid: i64,
        e: String,
        reason: ReactFailure,
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
    /// CHAT2, direct conversations: the friend has seen the messages up to this `sid` ("Görüldü";
    /// only while both keep "Görüldü bilgisi" on).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub seen: Option<i64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConvKind {
    Dm,
    Crew,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct MsgView {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub conv: Option<String>,
    pub id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cid: Option<String>,
    pub from: String,
    /// A sticker's words ("[Çıkartma: GG]", in the game's language) for a mod that cannot draw it.
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
    /// CHAT2: the service's id (replies and reactions name a message by it; "Görüldü" compares it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sid: Option<i64>,
    /// CHAT2: a Projects sticker (`pl1/gg`); the mod draws it from its own atlas.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticker: Option<String>,
    /// CHAT2: the message it answers, as this computer holds it.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub reply: Option<ReplyView>,
    /// CHAT2: its reactions, kinds in the order first given.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<ReactionView>,
    /// CHAT2: the links of its text the player may press now (they pass the rule table and
    /// Studio's lists); any other stays plain text. Only `https`.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<LinkView>,
    /// CHAT2: its first link's card, made by the launcher ("YouTube · <başlık>").
    #[serde(skip_serializing_if = "Option::is_none")]
    pub card: Option<CardView>,
}

/// The message a reply answers (CHAT2): the service keeps only the link, the words come from this
/// computer's own history.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ReplyView {
    /// Its `id` in the conversation, when this computer holds it (the mod scrolls to it).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub id: Option<String>,
    pub sid: i64,
    /// Its sender as the lists show them (empty when `missing`).
    pub from: String,
    /// The player's own message.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mine: bool,
    /// What it said, at most [`MAX_QUOTE_CHARS`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticker: Option<String>,
    /// Deleted for everyone: "Mesaj silindi".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub deleted: bool,
    /// Not on this computer, or from someone no longer seen: "Önceki bir mesaj".
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub missing: bool,
}

/// Characters of a quoted message the bridge carries (the mod shows one line of it).
pub const MAX_QUOTE_CHARS: usize = 120;

/// One kind of reaction on a message (CHAT2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct ReactionView {
    /// One emoji, as the service keeps it (send it back as it is to give or take the same one).
    pub e: String,
    /// How many people gave it.
    pub n: u32,
    /// The player is one of them.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mine: bool,
    /// The first few of the others, by name (the mod's "Ayşe, Mert ve 2 kişi daha").
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub who: Vec<String>,
}

/// A link the player may press (CHAT2).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct LinkView {
    /// As written in the text.
    pub url: String,
    /// A site Studio trusts.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub trusted: bool,
}

/// A link's card as the overlay shows it: one quiet line (CHAT2; no picture in game).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct CardView {
    /// "YouTube", else the domain.
    pub site: String,
    pub title: String,
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
    /// CHAT2: a sticker message (`body` says "Çıkartma · GG"); the mod may draw a small picture.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sticker: Option<String>,
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
    /// CHAT2: the day's stickers are used up.
    Stickers,
    /// CHAT2: the day's messages with links are used up.
    Links,
    /// CHAT2: a new Discord account sends no links yet.
    LimitedLinks,
    /// CHAT2: Studio switched this extra (stickers, replies) off.
    FeatureOff,
}

/// Why a reaction did not go (CHAT2). The mod says something general for one it does not know.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ReactFailure {
    /// The player has 5 reactions on that message already.
    Full,
    /// The message carries 10 kinds already.
    Kinds,
    /// The message is older than a week (or no longer here).
    Old,
    /// That emoji is never a reaction (🖕).
    Words,
    Rate,
    Muted,
    /// Reactions are switched off.
    Off,
    /// No longer friends, or no longer in the crew.
    NotAllowed,
    /// No connection to the friends service.
    Offline,
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
            sticker: None,
        });
        let json = serde_json::to_value(&toast).unwrap();
        assert_eq!(json["t"], "toast");
        assert_eq!(json["kind"], "call");
        assert!(json.get("conv").is_none() && json.get("crew").is_none());
        assert!(json.get("sticker").is_none());

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
            rule: None,
        })
        .unwrap();
        assert_eq!(failed["reason"], "daily-limit");
        assert!(failed.get("rule").is_none());
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
        let msg = serde_json::to_value(ToGame::Msg(Box::new(MsgView {
            conv: Some("d:1:2".into()),
            id: "i:1".into(),
            cid: None,
            from: "Ayşe".into(),
            text: "selam".into(),
            at: 5,
            mine: false,
            edited: false,
            deleted: false,
            ..MsgView::default()
        })))
        .unwrap();
        assert!(msg.get("edited").is_none() && msg.get("cid").is_none());
        assert!(msg.get("deleted").is_none());
        // An OVL2 message carries none of CHAT2's fields.
        for field in ["sid", "sticker", "reply", "reactions", "links", "card"] {
            assert!(msg.get(field).is_none(), "{field}");
        }
    }

    #[test]
    fn chat2_actions_of_the_panel_parse() {
        // An older mod's send still reads (no reply, no sticker).
        let plain: FromGame =
            serde_json::from_str(r#"{"t":"send","cid":"c1","conv":"d:1","text":"selam"}"#).unwrap();
        assert!(
            matches!(plain, FromGame::Send { reply: None, sticker: None, ref text, .. } if text == "selam")
        );
        let sticker: FromGame = serde_json::from_str(
            r#"{"t":"send","cid":"c2","conv":"d:1","sticker":"pl1/gg","reply":1800000000123}"#,
        )
        .unwrap();
        assert!(matches!(
            sticker,
            FromGame::Send { ref text, reply: Some(1_800_000_000_123), sticker: Some(ref s), .. }
                if text.is_empty() && s == "pl1/gg"
        ));
        let react: FromGame =
            serde_json::from_str(r#"{"t":"react","conv":"c:t1","sid":42,"e":"👍","on":true}"#)
                .unwrap();
        assert_eq!(
            react,
            FromGame::React {
                conv: "c:t1".into(),
                sid: 42,
                e: "👍".into(),
                on: true
            }
        );
        let typing: FromGame = serde_json::from_str(r#"{"t":"typing","conv":"c:t1"}"#).unwrap();
        assert_eq!(
            typing,
            FromGame::Typing {
                conv: "c:t1".into()
            }
        );
        // A reaction without its emoji is not one.
        assert!(
            serde_json::from_str::<FromGame>(r#"{"t":"react","conv":"c","sid":1,"on":true}"#)
                .is_err()
        );
    }

    #[test]
    fn chat2_frames_to_the_game_use_the_wire_names() {
        let typing = serde_json::to_value(ToGame::Typing {
            conv: "c:t1".into(),
            by: "u-cem".into(),
            name: "Cem".into(),
        })
        .unwrap();
        assert_eq!(
            typing,
            serde_json::json!({"t":"typing","conv":"c:t1","by":"u-cem","name":"Cem"})
        );
        let failed = serde_json::to_value(ToGame::ReactFailed {
            conv: "c:t1".into(),
            sid: 7,
            e: "👍".into(),
            reason: ReactFailure::NotAllowed,
        })
        .unwrap();
        assert_eq!(failed["t"], "react-failed");
        assert_eq!(failed["reason"], "not-allowed");
        let sent = serde_json::to_value(ToGame::Sent {
            cid: "c".into(),
            id: "n:x".into(),
            sid: Some(9),
        })
        .unwrap();
        assert_eq!(
            sent,
            serde_json::json!({"t":"sent","cid":"c","id":"n:x","sid":9})
        );
        let refused = serde_json::to_value(ToGame::SendFailed {
            cid: "c".into(),
            reason: SendFailure::Link,
            rule: Some("shortener".into()),
        })
        .unwrap();
        assert_eq!(refused["rule"], "shortener");
        assert_eq!(
            serde_json::to_value(SendFailure::LimitedLinks).unwrap(),
            "limited-links"
        );
        let msg = serde_json::to_value(ToGame::Msg(Box::new(MsgView {
            conv: Some("c:t1".into()),
            id: "i:8".into(),
            from: "Cem".into(),
            text: "bak https://youtu.be/x".into(),
            at: 8,
            sid: Some(8),
            reply: Some(ReplyView {
                id: Some("i:7".into()),
                sid: 7,
                from: "Ece".into(),
                mine: true,
                text: Some("geliyor musun".into()),
                ..ReplyView::default()
            }),
            reactions: vec![ReactionView {
                e: "👍".into(),
                n: 2,
                mine: true,
                who: vec!["Ayşe".into()],
            }],
            links: vec![LinkView {
                url: "https://youtu.be/x".into(),
                trusted: true,
            }],
            card: Some(CardView {
                site: "YouTube".into(),
                title: "Rekor".into(),
            }),
            ..MsgView::default()
        })))
        .unwrap();
        assert_eq!(msg["sid"], 8);
        assert_eq!(
            msg["reply"],
            serde_json::json!({"id":"i:7","sid":7,"from":"Ece","mine":true,"text":"geliyor musun"})
        );
        assert_eq!(
            msg["reactions"][0],
            serde_json::json!({"e":"👍","n":2,"mine":true,"who":["Ayşe"]})
        );
        assert_eq!(
            msg["links"][0],
            serde_json::json!({"url":"https://youtu.be/x","trusted":true})
        );
        assert_eq!(
            msg["card"],
            serde_json::json!({"site":"YouTube","title":"Rekor"})
        );
    }
}
