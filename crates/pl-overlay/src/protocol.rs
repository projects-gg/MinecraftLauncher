//! Bridge frames, version 1. One JSON object per line; `t` names the frame. Both sides ignore unknown
//! frames and fields, so additions keep the version and only breaking changes bump [`PROTO`]. Every
//! addition is recorded in `docs/tasks/ovl2-protocol.md` (shared with the mod's side).

use std::collections::BTreeMap;
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
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(transparent)]
pub struct Secret(String);

impl Secret {
    pub fn new(value: impl Into<String>) -> Self {
        Self(value.into())
    }

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
    /// HOST1 (H3; from a mod whose hello lists `host`): the player entered (`open`) or left a
    /// singleplayer world. `folder` is its folder in `saves/`, `mode` the world's game mode, `lan`
    /// whether vanilla "LAN'a Aç" is on.
    HostWorld {
        open: bool,
        #[serde(default)]
        folder: String,
        #[serde(default)]
        mode: String,
        #[serde(default)]
        lan: bool,
    },
    /// HOST1 (H4): "Davet et" in the game: the mode guests play in, whether they may use commands,
    /// the friends to invite.
    HostRequest {
        #[serde(default)]
        mode: String,
        #[serde(default)]
        commands: bool,
        #[serde(default)]
        invite: Vec<String>,
    },
    /// HOST1 (H6): the answer to [`ToGame::HostPublish`]: the world's port on `127.0.0.1` and the
    /// game's protocol number, or why not (`busy`, `lan`, `not-world`, `error`).
    HostPublished {
        id: String,
        ok: bool,
        #[serde(default)]
        port: u16,
        #[serde(default)]
        proto: i32,
        #[serde(default)]
        reason: String,
    },
    /// HOST1 (H7): a guest got in (`in`), left (`out`) or was turned away at the gate (`refused`).
    HostGuest {
        #[serde(default)]
        conn: u64,
        #[serde(default)]
        guest: String,
        #[serde(default)]
        name: String,
        state: String,
        #[serde(default)]
        reason: String,
    },
    /// HOST1 (H9): more invites from the open world.
    HostInvite {
        #[serde(default)]
        invite: Vec<String>,
    },
    /// HOST1 (H10): the player pressed "Dünyadan çıkar" in the game.
    HostRemove { guest: String },
    /// HOST1 (H11): the player stopped sharing, or is leaving the world.
    HostStop {
        #[serde(default)]
        reason: String,
    },
    /// HOST1-M (H17): "Projects'te buluşun" in the game: these friends get a "Birlikte oyna" invite once the
    /// player is on Projects (the game goes there itself, after the player's confirmation in the game).
    HostMeet {
        #[serde(default)]
        invite: Vec<String>,
    },
    /// SKIN1-O (K3; from a mod whose hello lists `link`): a Projects server asked the game on
    /// `projects:link` whether its launcher vouches for the player; `name` is the game's own
    /// account name. The launcher answers with `link-offer` (a permit for that account) or
    /// `link-clear` (none).
    LinkWant {
        #[serde(default)]
        name: String,
    },
    /// OVL-UX (P3; the friend menu): "Birlikte oyna" to this friend. Answered with
    /// `friend-result`.
    Invite { friend: String },
    /// OVL-UX (P4): "Discord'dan mesaj at": the launcher opens the friend's Discord profile (in
    /// the app when it is installed, else on the web). Answered only when it could not.
    Discord { friend: String },
    /// OVL-UX (P5): "Arkadaşlıktan çıkar", after the mod's one-sentence question.
    Unfriend { friend: String },
    /// OVL-UX (P6): "Engelle", after the mod's question.
    Block { friend: String },
    /// OVL-UX (P7): "Rapor et": one of a player's reasons (`name`, `profile`, `harassment`,
    /// `spam`, `scam`, `other`), a note for moderators (200 characters at most, may be empty) and
    /// whether to block them too.
    Report {
        friend: String,
        reason: String,
        #[serde(default)]
        note: String,
        #[serde(default)]
        block: bool,
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

/// The `hello` feature of a mod that hosts worlds (HOST1, H1), and the `welcome` feature that says
/// this player may open their world now (H2).
pub const FEATURE_HOST: &str = "host";

/// The `hello` feature of a mod that draws crew tags (TEAM6-O, T1: it reads `badge` and hears the
/// `badges` and `note` frames), and the `welcome` feature that says crew tags are on now (T2: the
/// service offers them and Studio has them on).
pub const FEATURE_TAGS: &str = "tags";

/// The `hello` feature of a mod that carries the launcher's codeless linking permit to a Projects
/// server and draws its question (SKIN1-O, K1), and the `welcome` feature that says this launcher
/// can vouch for the player now (K2: signed in to friends, the service offers codeless linking,
/// the game was started from here).
pub const FEATURE_LINK: &str = "link";

/// The `hello` feature of a mod with the friend menu (OVL-UX, P1: it hears `friend-result`), and
/// the `welcome` feature that says this launcher takes the menu's frames (`invite`, `discord`,
/// `unfriend`, `block`, `report`; P2: signed in to friends).
pub const FEATURE_PEOPLE: &str = "people";

/// The `welcome` feature that says "Birlikte oyna" can go now (OVL-UX, P2): the player's game is
/// on Projects, or on a server they show their friends.
pub const FEATURE_INVITE: &str = "invite";

impl FromGame {
    /// One of HOST1's frames: they go to the launcher's world sharing, never to the friends
    /// feature.
    pub fn is_host(&self) -> bool {
        matches!(
            self,
            Self::HostWorld { .. }
                | Self::HostRequest { .. }
                | Self::HostPublished { .. }
                | Self::HostGuest { .. }
                | Self::HostInvite { .. }
                | Self::HostRemove { .. }
                | Self::HostStop { .. }
                | Self::HostMeet { .. }
        )
    }
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
    /// TEAM6-O (T3): the cards of the crew tags friends and crewmates wear, by crew id; friends,
    /// messages and toasts name a crew with `badge`. With `all` the list is the whole set (it
    /// replaces what the mod holds); otherwise it changes only the crews it names, `null` taking
    /// one away (hidden by its leader, worn by nobody the player sees, tags switched off). Only to
    /// a mod that said `tags`.
    Badges {
        all: bool,
        cards: BTreeMap<String, Option<BadgeView>>,
    },
    /// TEAM6-O (T7): a quiet line in a crew's chat ("Ece ekibe katıldı"; the mod words it). Lines
    /// are not kept anywhere: whoever came in while this launcher ran. Only to a mod that said
    /// `tags`.
    Note {
        conv: String,
        id: String,
        kind: NoteKind,
        name: String,
        /// The player themself came in ("Ekibe katıldın").
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        mine: bool,
        /// Unix milliseconds, this computer's clock.
        at: i64,
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
        /// HOST1 (H12): what the mod shows instead of the address ("Arkadaş dünyası: Ayşe"); with
        /// it `host` is `127.0.0.1`, the launcher's own way to a friend's world. Only to a mod that
        /// said `host`.
        #[serde(skip_serializing_if = "Option::is_none")]
        label: Option<String>,
    },
    /// HOST1 (H5): open the world on `127.0.0.1` only (no LAN announcement), its gate for this
    /// publish id. Only right after the player's "Davet et" or "Dünyayı aç".
    HostPublish {
        id: String,
        mode: String,
        commands: bool,
        max: u8,
        /// The name binding key's id (PROTOCOL.md §9).
        binding: String,
    },
    /// HOST1 (H8): the session as the mod's screens show it, whole, on every change.
    HostState {
        open: bool,
        max: u8,
        guests: Vec<HostGuestView>,
        /// `unavailable` or `limit`, only when the world did not open.
        #[serde(skip_serializing_if = "Option::is_none")]
        error: Option<String>,
    },
    /// HOST1 (H10): take this guest out now (`removed`, `blocked`, `restricted`; the mod says
    /// "Bu dünyadan çıkarıldın." for all).
    HostRemove {
        guest: String,
        reason: String,
    },
    /// HOST1 (H11): stop sharing (`player`, `left-world`, `restricted`, `launcher-exit`, `error`).
    HostStop {
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<String>,
    },
    /// SKIN1-O (K4): the answer to `link-want`: the codeless linking permit for game account
    /// `name`, valid until `expiresAt` (Unix seconds). The mod keeps it in memory only and hands
    /// it only to a Projects server that asks, while the game plays as `name`. Only to a mod that
    /// said `link`.
    LinkOffer {
        nonce: Secret,
        name: String,
        #[serde(rename = "expiresAt")]
        expires_at: i64,
    },
    /// SKIN1-O (K5): there is no permit (the answer to `link-want`), or the one the mod holds is
    /// no longer good (another friends account, codeless linking switched off): the mod forgets
    /// it and tells a server that asks that it has none. Only to a mod that said `link`.
    LinkClear,
    /// OVL-UX (P8): how a friend menu action went. `queued`: the invite waits for the friend
    /// (offline or on do-not-disturb); `reason` when it did not go. Only to a mod that said
    /// `people`.
    FriendResult {
        friend: String,
        action: FriendAction,
        ok: bool,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        queued: bool,
        #[serde(skip_serializing_if = "Option::is_none")]
        reason: Option<FriendRefusal>,
    },
    Bye {
        reason: ByeReason,
    },
}

/// A friend menu action, as `friend-result` names it (OVL-UX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FriendAction {
    Invite,
    Discord,
    Unfriend,
    Block,
    Report,
}

/// Why a friend menu action did not go (OVL-UX).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum FriendRefusal {
    /// "Birlikte oyna" to them waits for its five minutes (or the day's invites are used up).
    Wait,
    /// Too many actions from the game in a short time (a stuck key or a bad mod).
    Rate,
    /// The friends service cannot be reached now.
    Offline,
    /// Anything else: not a friend any more, the service refused, nothing to open.
    Failed,
}

/// A friend in the player's world session, for the mod's screens (H8).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HostGuestView {
    /// The guest id (`g1` …; empty until the friend's launcher connected).
    pub guest: String,
    /// The friend's user id.
    pub friend: String,
    /// The friend's shown name.
    pub label: String,
    /// The in-game name, once known.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `invited`, `joining` or `in`.
    pub state: &'static str,
}

/// A friend who invited the player to their open world (H13): shown as `guests + 1` of `max + 1`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct WorldMark {
    pub guests: u8,
    pub max: u8,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Me {
    pub name: String,
    /// OVL-UX (P9): the crew whose tag the player wears (its card comes in `badges`, to a mod that
    /// said `tags`), only while crew tags are on.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
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
    /// HOST1 (H13): they invited the player to their open world (then `joinable`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub world: Option<WorldMark>,
    /// TEAM6-O (T4): the crew whose tag they wear (its card came in `badges`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
    /// OVL-UX (P10): their direct conversation with the player, while chat is on: the panel opens
    /// it (a new one too) and messages go there.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub dm: Option<String>,
    /// OVL-UX (P11): their Discord profile is known ("Discord'dan mesaj at").
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub discord: bool,
}

/// A crew tag's card as the overlay shows it (TEAM6-O, T3): the chip (`tag` on the crew's colour)
/// and the bubble it opens (the mark, the name, the motto, "Kuruluş Eylül 2026 · 12 üye"). It never
/// names the crew's members, leader, code or link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct BadgeView {
    pub tag: String,
    /// A key of the crew palette (`gold`, `navy`, …; an unknown one draws gold).
    pub color: String,
    /// `cut`, `shield`, `banner` or `hex`.
    pub frame: String,
    /// `sword`, `shield`, … (an unknown one draws a shield).
    pub emblem: String,
    pub name: String,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub motto: String,
    /// `YYYY-MM`: the month the crew was founded (the mod says it in its own words).
    #[serde(skip_serializing_if = "String::is_empty")]
    pub since: String,
    pub members: u32,
    /// The player is in this crew ("Sen de bu ekiptesin").
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub mine: bool,
}

/// What a quiet line in a crew's chat says (TEAM6-O, T7).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum NoteKind {
    /// "Ece ekibe katıldı" (the player's own: "Ekibe katıldın").
    Joined,
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
    /// TEAM6-O (T5): the crew tag its sender wears, on someone else's message in a crew's chat
    /// (the chip before the name over their group).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
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
    /// TEAM6-O (T6): a message's sender wears this crew's tag (the chip before `title`).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<String>,
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
    /// HOST1 (H14): a world invite, `title` the friend ("%s seni dünyasına çağırıyor"). Only to a
    /// mod that said `host`.
    World,
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
            label: None,
        })
        .unwrap();
        assert_eq!(
            connect,
            serde_json::json!({"t":"connect","id":"1","host":"play.projects.gg","port":25565})
        );
    }

    #[test]
    fn host_frames_read_and_write_as_the_log_says() {
        let world: FromGame = serde_json::from_str(
            r#"{"t":"host-world","open":true,"folder":"Yeni Dünya","mode":"survival","lan":false}"#,
        )
        .unwrap();
        assert!(world.is_host());
        assert!(
            matches!(world, FromGame::HostWorld { open: true, ref folder, .. } if folder == "Yeni Dünya")
        );
        let left: FromGame = serde_json::from_str(r#"{"t":"host-world","open":false}"#).unwrap();
        assert!(matches!(left, FromGame::HostWorld { open: false, .. }));
        let request: FromGame = serde_json::from_str(
            r#"{"t":"host-request","mode":"creative","commands":true,"invite":["f1","f2"]}"#,
        )
        .unwrap();
        assert!(
            matches!(request, FromGame::HostRequest { commands: true, ref invite, .. } if invite.len() == 2)
        );
        let published: FromGame = serde_json::from_str(
            r#"{"t":"host-published","id":"p1","ok":true,"port":51234,"proto":774}"#,
        )
        .unwrap();
        assert_eq!(
            published,
            FromGame::HostPublished {
                id: "p1".into(),
                ok: true,
                port: 51234,
                proto: 774,
                reason: String::new()
            }
        );
        let refused: FromGame =
            serde_json::from_str(r#"{"t":"host-published","id":"p1","ok":false,"reason":"lan"}"#)
                .unwrap();
        assert!(
            matches!(refused, FromGame::HostPublished { ok: false, ref reason, .. } if reason == "lan")
        );
        let guest: FromGame = serde_json::from_str(
            r#"{"t":"host-guest","conn":3,"guest":"g1","name":"Notch","state":"in"}"#,
        )
        .unwrap();
        assert!(matches!(guest, FromGame::HostGuest { conn: 3, ref state, .. } if state == "in"));
        let more: FromGame =
            serde_json::from_str(r#"{"t":"host-invite","invite":["f3"]}"#).unwrap();
        assert!(matches!(more, FromGame::HostInvite { ref invite } if invite == &["f3"]));
        let remove: FromGame = serde_json::from_str(r#"{"t":"host-remove","guest":"g2"}"#).unwrap();
        assert_eq!(remove, FromGame::HostRemove { guest: "g2".into() });
        let stop: FromGame = serde_json::from_str(r#"{"t":"host-stop"}"#).unwrap();
        assert_eq!(
            stop,
            FromGame::HostStop {
                reason: String::new()
            }
        );
        let meet: FromGame =
            serde_json::from_str(r#"{"t":"host-meet","invite":["f1","f2"]}"#).unwrap();
        assert!(meet.is_host());
        assert!(matches!(meet, FromGame::HostMeet { ref invite } if invite == &["f1", "f2"]));
        let join: FromGame = serde_json::from_str(r#"{"t":"join","friend":"f1"}"#).unwrap();
        assert!(!join.is_host());

        let publish = serde_json::to_value(ToGame::HostPublish {
            id: "p1".into(),
            mode: "survival".into(),
            commands: false,
            max: 7,
            binding: "sW9hzU3c".into(),
        })
        .unwrap();
        assert_eq!(
            publish,
            serde_json::json!({"t":"host-publish","id":"p1","mode":"survival","commands":false,"max":7,"binding":"sW9hzU3c"})
        );
        let state = serde_json::to_value(ToGame::HostState {
            open: true,
            max: 7,
            guests: vec![HostGuestView {
                guest: "g1".into(),
                friend: "f1".into(),
                label: "Mert".into(),
                name: Some("Notch".into()),
                state: "in",
            }],
            error: None,
        })
        .unwrap();
        assert_eq!(state["t"], "host-state");
        assert_eq!(state["guests"][0]["label"], "Mert");
        assert!(state.get("error").is_none());
        let failed = serde_json::to_value(ToGame::HostState {
            open: false,
            max: 7,
            guests: Vec::new(),
            error: Some("limit".into()),
        })
        .unwrap();
        assert_eq!(failed["error"], "limit");
        assert_eq!(
            serde_json::to_value(ToGame::HostRemove {
                guest: "g1".into(),
                reason: "removed".into()
            })
            .unwrap(),
            serde_json::json!({"t":"host-remove","guest":"g1","reason":"removed"})
        );
        assert_eq!(
            serde_json::to_value(ToGame::HostStop {
                reason: Some("launcher-exit".into())
            })
            .unwrap(),
            serde_json::json!({"t":"host-stop","reason":"launcher-exit"})
        );
        let connect = serde_json::to_value(ToGame::Connect {
            id: "2".into(),
            host: "127.0.0.1".into(),
            port: 50123,
            label: Some("Arkadaş dünyası: Ayşe".into()),
        })
        .unwrap();
        assert_eq!(connect["label"], "Arkadaş dünyası: Ayşe");
        let toast = serde_json::to_value(ToGame::Toast(ToastView {
            kind: ToastKind::World,
            key: "world:s".into(),
            title: "Ayşe".into(),
            body: String::new(),
            conv: None,
            crew: None,
            sticker: None,
            badge: None,
        }))
        .unwrap();
        assert_eq!(toast["kind"], "world");
    }

    #[test]
    fn friend_menu_frames_read_and_write_as_the_log_says() {
        let invite: FromGame = serde_json::from_str(r#"{"t":"invite","friend":"f1"}"#).unwrap();
        assert_eq!(
            invite,
            FromGame::Invite {
                friend: "f1".into()
            }
        );
        let discord: FromGame = serde_json::from_str(r#"{"t":"discord","friend":"f1"}"#).unwrap();
        assert!(matches!(discord, FromGame::Discord { ref friend } if friend == "f1"));
        let unfriend: FromGame = serde_json::from_str(r#"{"t":"unfriend","friend":"f2"}"#).unwrap();
        assert!(matches!(unfriend, FromGame::Unfriend { ref friend } if friend == "f2"));
        let block: FromGame = serde_json::from_str(r#"{"t":"block","friend":"f3"}"#).unwrap();
        assert!(matches!(block, FromGame::Block { ref friend } if friend == "f3"));
        let report: FromGame = serde_json::from_str(
            r#"{"t":"report","friend":"f4","reason":"harassment","note":"sürekli","block":true}"#,
        )
        .unwrap();
        assert_eq!(
            report,
            FromGame::Report {
                friend: "f4".into(),
                reason: "harassment".into(),
                note: "sürekli".into(),
                block: true
            }
        );
        let bare: FromGame =
            serde_json::from_str(r#"{"t":"report","friend":"f4","reason":"spam"}"#).unwrap();
        assert!(matches!(bare, FromGame::Report { ref note, block: false, .. } if note.is_empty()));
        // A frame without its friend is not one of them.
        assert!(serde_json::from_str::<FromGame>(r#"{"t":"block"}"#).is_err());
        assert!(!invite.is_host());

        let sent = serde_json::to_value(ToGame::FriendResult {
            friend: "f1".into(),
            action: FriendAction::Invite,
            ok: true,
            queued: true,
            reason: None,
        })
        .unwrap();
        assert_eq!(
            sent,
            serde_json::json!({"t":"friend-result","friend":"f1","action":"invite","ok":true,"queued":true})
        );
        let refused = serde_json::to_value(ToGame::FriendResult {
            friend: "f2".into(),
            action: FriendAction::Unfriend,
            ok: false,
            queued: false,
            reason: Some(FriendRefusal::Offline),
        })
        .unwrap();
        assert_eq!(
            refused,
            serde_json::json!({"t":"friend-result","friend":"f2","action":"unfriend","ok":false,"reason":"offline"})
        );
        let me = serde_json::to_value(Me {
            name: "Ece".into(),
            badge: Some("t1".into()),
        })
        .unwrap();
        assert_eq!(me, serde_json::json!({"name":"Ece","badge":"t1"}));
        let plain = serde_json::to_value(Me {
            name: "Ece".into(),
            badge: None,
        })
        .unwrap();
        assert_eq!(plain, serde_json::json!({"name":"Ece"}));
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
            badge: None,
        });
        let json = serde_json::to_value(&toast).unwrap();
        assert_eq!(json["t"], "toast");
        assert_eq!(json["kind"], "call");
        assert!(json.get("conv").is_none() && json.get("crew").is_none());
        assert!(json.get("sticker").is_none() && json.get("badge").is_none());

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
            world: None,
            badge: None,
            dm: None,
            discord: false,
        }))
        .unwrap();
        assert_eq!(friend["where"], "projects");
        assert_eq!(friend["t"], "friend");
        assert!(friend.get("host").is_none() && friend.get("world").is_none());
        assert!(friend.get("badge").is_none());
        assert!(friend.get("dm").is_none() && friend.get("discord").is_none());
        let inviting = serde_json::to_value(ToGame::Friend(FriendView {
            id: "f2".into(),
            name: "Mert".into(),
            place: Place::Single,
            joinable: true,
            host: None,
            world: Some(WorldMark { guests: 1, max: 7 }),
            badge: None,
            dm: Some("d:f2:me".into()),
            discord: true,
        }))
        .unwrap();
        assert_eq!(inviting["world"], serde_json::json!({"guests":1,"max":7}));
        assert_eq!(inviting["dm"], "d:f2:me");
        assert_eq!(inviting["discord"], true);

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
        // An OVL2 message carries none of CHAT2's fields (nor TEAM6-O's).
        for field in [
            "sid",
            "sticker",
            "reply",
            "reactions",
            "links",
            "card",
            "badge",
        ] {
            assert!(msg.get(field).is_none(), "{field}");
        }
    }

    #[test]
    fn team6_frames_to_the_game_use_the_wire_names() {
        let card = BadgeView {
            tag: "GK".into(),
            color: "navy".into(),
            frame: "shield".into(),
            emblem: "moon".into(),
            name: "Gece Kartalları".into(),
            motto: String::new(),
            since: "2026-09".into(),
            members: 12,
            mine: false,
        };
        let badges = serde_json::to_value(ToGame::Badges {
            all: false,
            cards: BTreeMap::from([("t1".to_owned(), Some(card)), ("t2".to_owned(), None)]),
        })
        .unwrap();
        assert_eq!(
            badges,
            serde_json::json!({"t":"badges","all":false,"cards":{
                "t1":{"tag":"GK","color":"navy","frame":"shield","emblem":"moon",
                      "name":"Gece Kartalları","since":"2026-09","members":12},
                "t2":null}})
        );
        let note = serde_json::to_value(ToGame::Note {
            conv: "c:t1".into(),
            id: "j:u-ece:1800000000".into(),
            kind: NoteKind::Joined,
            name: "Ece".into(),
            mine: false,
            at: 1_800_000_000_000,
        })
        .unwrap();
        assert_eq!(
            note,
            serde_json::json!({"t":"note","conv":"c:t1","id":"j:u-ece:1800000000","kind":"joined",
                "name":"Ece","at":1_800_000_000_000_i64})
        );
        let msg = serde_json::to_value(ToGame::Msg(Box::new(MsgView {
            conv: Some("c:t1".into()),
            id: "i:9".into(),
            from: "Cem".into(),
            badge: Some("t1".into()),
            ..MsgView::default()
        })))
        .unwrap();
        assert_eq!(msg["badge"], "t1");
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
