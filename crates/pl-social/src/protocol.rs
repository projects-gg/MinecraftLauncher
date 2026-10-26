//! Presence WebSocket protocol (`GET /v1/presence`, JSON text frames).

use std::collections::BTreeMap;

use serde::{Deserialize, Deserializer, Serialize};

use crate::afk::AfkNotice;
use crate::minecraft::McMessage;
use crate::model::{
    ChatCaps, ChatDeletion, ChatMessage, ChatReacted, ChatStatus, CrewBadge, CrewMute,
    FriendGroups, JoinRequest, LastSeen, LinkCard, ModNotice, Privacy, PublicProfile, Reaction,
    ReadMark, StatusMode, Team, TeamExit, TeamInvite, TeamNotifications,
};

/// What this launcher understands on the presence socket (`GET /v1/presence?caps=…`): crew calls
/// that carry their server and may end with `server` (`callServer`), several crews (`crews`),
/// crew plans (`plans`, TEAM5), crew tag changes (`badges`, TEAM6), SKIN1's game account link
/// and Projects skin news (`mc`), Cloud AFK's notices (`afk`, AFKPLUS) and the player's own privacy
/// switches and picture from their other devices (`profile`, SOC-LAG). Older services ignore the
/// query, and names they do not know.
pub const PRESENCE_CAPS: &str = "callServer,crews,plans,badges,mc,afk,profile";

/// Crew plans (TEAM5): the capability the service names in `hello.caps` when it took it.
pub const PLANS_CAP: &str = "plans";

/// Crew tag changes (TEAM6): `badge` messages flow only when the service named it in `hello.caps`.
pub const BADGES_CAP: &str = "badges";

/// SKIN1: `mc` messages (a job closed, the link changed elsewhere, staff reset the skin).
pub const MC_CAP: &str = "mc";

/// AFKPLUS: Cloud AFK's notices (`afk_notice`) come to this socket instead of the phone; the
/// launcher answers `afk_seen` once the player saw one.
pub const AFK_CAP: &str = "afk";

/// Crew chat and direct messages (CHAT1): asked for only while the signed content allows chat.
pub const CHAT_CAP: &str = "chat";

/// CHAT2's chat capabilities, asked for beside `chat`, one each (the launcher, the phone app and
/// the overlay update at different times): replies, stickers, reactions, "yazıyor…", read marks.
pub const REPLIES_CAP: &str = "replies";
pub const STICKERS_CAP: &str = "stickers";
pub const REACTIONS_CAP: &str = "reactions";
pub const TYPING_CAP: &str = "typing";
pub const READS_CAP: &str = "reads";
pub const CHAT_EXTRA_CAPS: [&str; 5] = [
    REPLIES_CAP,
    STICKERS_CAP,
    REACTIONS_CAP,
    TYPING_CAP,
    READS_CAP,
];

/// The capabilities to ask for: [`PRESENCE_CAPS`], with chat and its extras when it may show.
pub fn presence_caps(chat: bool) -> String {
    if chat {
        format!("{PRESENCE_CAPS},{CHAT_CAP},{}", CHAT_EXTRA_CAPS.join(","))
    } else {
        PRESENCE_CAPS.to_owned()
    }
}

/// The chat capabilities the service took (`hello.caps`, CHAT2).
pub(crate) fn chat_caps(caps: &[String]) -> ChatCaps {
    let has = |name: &str| caps.iter().any(|cap| cap == name);
    ChatCaps {
        replies: has(REPLIES_CAP),
        stickers: has(STICKERS_CAP),
        reactions: has(REACTIONS_CAP),
        typing: has(TYPING_CAP),
        reads: has(READS_CAP),
    }
}

/// projects.gg or one of its subdomains, with or without a port (`play.projects.gg:25565`).
pub fn is_projects_host(host: &str) -> bool {
    let name = host
        .rsplit_once(':')
        .map_or(host, |(name, port)| {
            if port.bytes().all(|b| b.is_ascii_digit()) {
                name
            } else {
                host
            }
        })
        .trim_end_matches('.')
        .to_ascii_lowercase();
    name == "projects.gg" || name.ends_with(".projects.gg")
}

/// What this launcher is doing. Sent only when it changes.
#[derive(Debug, Clone, PartialEq, Eq, Default)]
pub enum LocalStatus {
    /// The launcher is open (or in the tray).
    #[default]
    Launcher,
    /// In game on a server. `host` is `host[:port]` from the game log, when known.
    Playing { host: Option<String> },
    /// In a singleplayer world.
    Singleplayer,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PresenceState {
    Offline,
    Launcher,
    Playing,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum OnlineStatus {
    Online,
    Dnd,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum GameLocation {
    /// projects.gg (or a subdomain): show "Katıl".
    Projects,
    /// Another server; `host` is set only when the friend shares it.
    Other,
    Singleplayer,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct Game {
    #[serde(rename = "where")]
    pub location: GameLocation,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub host: Option<String>,
}

impl Game {
    /// Whether the launcher can offer "Katıl" (join) for this game.
    pub fn joinable(&self) -> bool {
        self.location == GameLocation::Projects
    }
}

/// A friend's presence as the service shows it. Invisible friends are always offline.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Presence {
    pub id: String,
    pub state: PresenceState,
    #[serde(default)]
    pub status: Option<OnlineStatus>,
    /// While playing; None when the server is not known yet.
    #[serde(default)]
    pub game: Option<Game>,
    #[serde(default)]
    pub avatar: Option<String>,
    #[serde(default)]
    pub avatar_hash: Option<String>,
    /// Only on a visible → offline transition: `Some(Some(_))` when the friend shares it,
    /// `Some(None)` (`lastSeen: null`) when they hide it now, so the friend list's goes too
    /// (SOC-LAG); None (absent): keep the friend list's.
    #[serde(default, deserialize_with = "present")]
    pub last_seen: Option<Option<LastSeen>>,
}

/// A field that may be absent (None), `null` (`Some(None)`) or a value: the outer `Option` says
/// whether it was there at all (with `#[serde(default)]`).
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<Option<T>>, D::Error> {
    Option::<T>::deserialize(deserializer).map(Some)
}

/// "Birlikte oyna": a join-me invite without any text.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Invite {
    pub id: String,
    /// Sender's user id.
    pub from: String,
    /// Unix seconds.
    pub at: i64,
    /// The sender's server, when it can be shared.
    pub server: Option<Game>,
    /// It waited (the player was offline or on do-not-disturb): show it in a quiet list.
    pub quiet: bool,
}

/// What a crew call asks: come where the caller plays, or "are you ready?" (a call from before
/// CALL1, or one of another server for a launcher that does not know `callServer`). With
/// `callServer` it is always `Join` and the call carries its server. A kind this version does not
/// know reads as `Ready` (an answer, nothing to join).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CallKind {
    Join,
    Ready,
}

impl<'de> Deserialize<'de> for CallKind {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "join" => Self::Join,
            _ => Self::Ready,
        })
    }
}

/// An answer to a crew call: "Geliyorum", "Birazdan", "Gelemem".
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CallAnswer {
    Yes,
    Soon,
    No,
}

impl CallAnswer {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "yes" => Some(Self::Yes),
            "soon" => Some(Self::Soon),
            "no" => Some(Self::No),
            _ => None,
        }
    }
}

/// "Ekibi çağır": an open call as the service shows it (`{"t":"call"}`, `hello.calls`). Times are
/// Unix seconds on the service's clock; the message that carries the call also says what time it
/// is there (`now`).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CrewCall {
    pub id: String,
    /// The caller's user id (the player's own id for their own call).
    pub from: String,
    pub kind: CallKind,
    pub at: i64,
    /// The call ends by itself at this time; no message says so.
    pub until: i64,
    /// Who answered what (answers this version does not know are left out).
    #[serde(default, deserialize_with = "known_answers")]
    pub answers: BTreeMap<String, CallAnswer>,
    /// The player's crews the call names (the caller's own call: all it names). Empty from a
    /// service without several crews: the call is the one crew's.
    #[serde(default)]
    pub crews: Vec<String>,
    /// Where the call is (a service with `callServer`): projects.gg, or another server with its
    /// address; it follows the caller.
    #[serde(default)]
    pub server: Option<Game>,
}

fn known_answers<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, CallAnswer>, D::Error> {
    let raw = BTreeMap::<String, String>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|(id, answer)| CallAnswer::parse(&answer).map(|answer| (id, answer)))
        .collect())
}

/// An answer to a crew plan: "Geliyorum", "Belki", "Gelemem" (TEAM5).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PlanAnswer {
    Yes,
    Maybe,
    No,
}

impl PlanAnswer {
    pub(crate) fn parse(value: &str) -> Option<Self> {
        match value {
            "yes" => Some(Self::Yes),
            "maybe" => Some(Self::Maybe),
            "no" => Some(Self::No),
            _ => None,
        }
    }
}

/// "Planlar": a time to meet on Projects in one of the player's crews (`hello.plans`,
/// `{"t":"plan"}`, `{"t":"plans"}`), with an optional short note (`planNotes`, FIX2: its only
/// text). Times are Unix seconds on the service's clock; `at` is a whole quarter of an hour, and
/// the plan is over at `until` (`at` + 2 hours), without a message.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrewPlan {
    pub id: String,
    /// The crew's id.
    pub crew: String,
    /// Who made it.
    pub by: String,
    pub at: i64,
    pub until: i64,
    #[serde(default)]
    pub created_at: i64,
    /// The answers the player may see (theirs and those of crewmates who see them there); answers
    /// this version does not know are left out.
    #[serde(default, deserialize_with = "known_plan_answers")]
    pub answers: BTreeMap<String, PlanAnswer>,
    /// One line of at most 80 characters ("Bedwars, sesli gelin"); None (or empty) is none.
    #[serde(default, deserialize_with = "plan_note")]
    pub note: Option<String>,
}

/// A plan's note: kept only when it is a string with something in it.
fn plan_note<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .and_then(|value| value.as_str().map(str::trim).map(str::to_owned))
        .filter(|note| !note.is_empty()))
}

fn known_plan_answers<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<BTreeMap<String, PlanAnswer>, D::Error> {
    let raw = BTreeMap::<String, String>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|(id, answer)| PlanAnswer::parse(&answer).map(|answer| (id, answer)))
        .collect())
}

/// A list of plans where one this version cannot read is left out (it never costs the rest, nor
/// the `hello` that carries them).
fn readable_plans<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Vec<CrewPlan>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect())
}

fn readable_plans_opt<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<CrewPlan>>, D::Error> {
    let raw = Option::<Vec<serde_json::Value>>::deserialize(deserializer)?;
    Ok(raw.map(|list| {
        list.into_iter()
            .filter_map(|value| serde_json::from_value(value).ok())
            .collect()
    }))
}

/// Join requests where one this version cannot read is left out.
fn readable_requests<'de, D: Deserializer<'de>>(
    deserializer: D,
) -> Result<Vec<JoinRequest>, D::Error> {
    let raw = Vec::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .into_iter()
        .filter_map(|value| serde_json::from_value(value).ok())
        .collect())
}

/// Why a call ended: the caller ended it, it went away (the caller went offline, left the crew,
/// the crew ended, the player left it, or a block hid it), or the caller is no longer on a server
/// it allows (`server`, with `callServer`). Unknown reasons read as `Ended`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum CallEndReason {
    #[default]
    Ended,
    Left,
    Server,
}

impl<'de> Deserialize<'de> for CallEndReason {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "left" => Self::Left,
            "server" => Self::Server,
            _ => Self::Ended,
        })
    }
}

#[derive(Debug, Serialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(crate) enum ClientMessage {
    Status {
        state: &'static str,
        #[serde(skip_serializing_if = "Option::is_none")]
        host: Option<String>,
        #[serde(skip_serializing_if = "std::ops::Not::not")]
        singleplayer: bool,
    },
    Invite {
        to: String,
        rid: u32,
    },
    /// "Ekibi çağır": the crews it calls (a service with several crews), else the primary one.
    Call {
        #[serde(skip_serializing_if = "Option::is_none")]
        crews: Option<Vec<String>>,
        rid: u32,
    },
    CallAnswer {
        call: String,
        answer: CallAnswer,
        rid: u32,
    },
    /// Ends the player's own call.
    CallEnd {
        call: String,
        rid: u32,
    },
    /// A chat message: a text, or a sticker (CHAT2), maybe answering another (`reply`:
    /// `{"msg": <that message as held>}`, CHAT2). `nonce` makes a resend after a lost answer safe.
    Chat {
        conv: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        text: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        sticker: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        reply: Option<serde_json::Value>,
        nonce: String,
        rid: u32,
    },
    /// The player's own message, as held (its seal), and its new text.
    ChatEdit {
        msg: serde_json::Value,
        text: String,
        rid: u32,
    },
    /// "Herkesten sil": a message as held (its seal).
    ChatDelete {
        msg: serde_json::Value,
        rid: u32,
    },
    /// What changed after `since` (ms); `after` only to continue with the cursor just given.
    ChatSync {
        since: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        after: Option<String>,
        rid: u32,
    },
    /// A reaction given (`on`) or taken back (CHAT2).
    ChatReact {
        conv: String,
        id: i64,
        e: String,
        on: bool,
        rid: u32,
    },
    /// "Yazıyor…" (CHAT2): no `rid`, never answered.
    ChatTyping {
        conv: String,
    },
    /// The player has read the conversation up to message `id` (CHAT2): no `rid`, never answered.
    ChatRead {
        conv: String,
        id: i64,
    },
    /// The card of a link as written in a message or the composer (CHAT2).
    ChatUnfurl {
        url: String,
        rid: u32,
    },
    /// A new crew plan: a time (Unix seconds, a whole quarter of an hour) in one of the player's
    /// crews, and its note (only to a service with `planNotes`).
    Plan {
        crew: String,
        at: i64,
        #[serde(skip_serializing_if = "Option::is_none")]
        note: Option<String>,
        rid: u32,
    },
    /// A plan's new note, or `""` to clear it (its maker; the crew's leader and moderators may
    /// only clear it).
    PlanNote {
        crew: String,
        id: String,
        note: String,
        rid: u32,
    },
    /// "Geliyorum", "Belki" or "Gelemem" to a plan.
    PlanAnswer {
        plan: String,
        answer: PlanAnswer,
        rid: u32,
    },
    /// "Planı sil" (its maker, the crew's leader or a moderator).
    PlanCancel {
        plan: String,
        rid: u32,
    },
    /// The player saw Cloud AFK's notice `id` (AFKPLUS): no `rid`, never answered.
    AfkSeen {
        id: String,
    },
}

impl ClientMessage {
    pub(crate) fn status(status: &LocalStatus) -> Self {
        match status {
            LocalStatus::Launcher => Self::Status {
                state: "launcher",
                host: None,
                singleplayer: false,
            },
            LocalStatus::Playing { host } => Self::Status {
                state: "playing",
                host: host.clone(),
                singleplayer: false,
            },
            LocalStatus::Singleplayer => Self::Status {
                state: "playing",
                host: None,
                singleplayer: true,
            },
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(tag = "t", rename_all = "snake_case")]
pub(crate) enum ServerMessage {
    Hello {
        status: StatusMode,
        #[serde(default, rename = "keepaliveSeconds")]
        keepalive_seconds: Option<u64>,
        #[serde(default)]
        friends: Vec<Presence>,
        /// Online teammates who are not friends.
        #[serde(default)]
        team: Vec<Presence>,
        #[serde(default)]
        invites: Vec<Invite>,
        /// Every open crew call the player sees, their own included (a service with calls).
        #[serde(default)]
        calls: Vec<CrewCall>,
        /// The service's clock (Unix seconds), for the calls' deadlines.
        #[serde(default)]
        now: Option<i64>,
        /// The capabilities of [`PRESENCE_CAPS`] the service took (none from an older one).
        #[serde(default)]
        caps: Vec<String>,
        /// The chat state (only with `chat` in the capabilities).
        #[serde(default)]
        chat: Option<ChatStatus>,
        /// Every plan of the player's crews they see (only with `plans` in the capabilities).
        #[serde(default, deserialize_with = "readable_plans_opt")]
        plans: Option<Vec<CrewPlan>>,
        /// World sessions (HOST1): Studio's switch and the guest limit (only with `worlds` in
        /// the capabilities).
        #[serde(default)]
        worlds: Option<crate::world::WorldsStatus>,
    },
    /// A crew call, new or changed (its server or kind, its crews, or answers that went away):
    /// replaces the one with the same id. `quiet`: show it, never announce it.
    Call {
        call: CrewCall,
        #[serde(default)]
        quiet: bool,
        #[serde(default)]
        now: Option<i64>,
    },
    /// Someone answered a call (the player too, from another device).
    CallAnswer {
        call: String,
        by: String,
        answer: String,
    },
    CallEnd {
        call: String,
        #[serde(default)]
        reason: CallEndReason,
    },
    /// One of the player's crews changed, or they are no longer in it (`team` null).
    Team {
        team: Option<Team>,
        /// The crew's id (none from a service with one crew per player).
        #[serde(default)]
        crew: Option<String>,
        #[serde(default)]
        reason: Option<TeamExit>,
        /// Who made the change.
        #[serde(default)]
        by: Option<String>,
        /// Who just came in (TEAM6: "Hoş geldin" in the crew's chat; none from an older service).
        #[serde(default)]
        joined: Vec<String>,
    },
    /// The crew tag a friend or crewmate wears (or the player's own, from another device) changed
    /// (TEAM6, with `badges` in the capabilities): replace their card wherever they show.
    Badge {
        id: String,
        #[serde(default, deserialize_with = "crate::model::readable_badge")]
        badge: Option<CrewBadge>,
    },
    TeamInvite {
        invite: TeamInvite,
    },
    /// The waiting join requests of a crew the player may answer for changed (or the player
    /// gained the right): replace them. `added` is the sender of a new one.
    TeamRequests {
        crew: String,
        #[serde(default, deserialize_with = "readable_requests")]
        requests: Vec<JoinRequest>,
        #[serde(default)]
        added: Option<String>,
    },
    /// A plan the player sees, new or changed (answers): replaces the one with the same id.
    /// `quiet`: show it, never announce it.
    Plan {
        plan: CrewPlan,
        #[serde(default)]
        quiet: bool,
    },
    /// Everything the player sees of one crew's plans now (also empty): replace that crew's.
    Plans {
        crew: String,
        #[serde(default, deserialize_with = "readable_plans")]
        plans: Vec<CrewPlan>,
    },
    /// A plan was cancelled (`by`: its maker, or the crew's leader or a moderator).
    PlanRemoved {
        plan: String,
        #[serde(default)]
        crew: String,
        #[serde(default)]
        by: String,
    },
    /// The player's crew notification settings for all crews changed (on any of their devices).
    TeamNotifications {
        notifications: TeamNotifications,
    },
    /// The player's mute of one crew changed (on any of their devices).
    CrewNotifications {
        crew: String,
        notifications: CrewMute,
    },
    /// The player put their crews in another order on another device (the first is primary).
    CrewOrder {
        order: Vec<String>,
    },
    /// The player's friend categories changed on another device.
    Groups {
        groups: FriendGroups,
    },
    Presence {
        presence: Presence,
    },
    FriendAdded {
        friend: PublicProfile,
        presence: Presence,
    },
    FriendRemoved {
        id: String,
    },
    Request {
        from: PublicProfile,
    },
    RequestCancelled {
        id: String,
    },
    Invite {
        invite: Invite,
    },
    Invites {
        invites: Vec<Invite>,
    },
    Status {
        status: StatusMode,
    },
    /// The player's privacy switches or picture changed, on any of their devices (SOC-LAG; only
    /// with `profile` in the capabilities): what they are now.
    Profile {
        privacy: Privacy,
        #[serde(default)]
        avatar: Option<String>,
        #[serde(default, rename = "avatarHash")]
        avatar_hash: Option<String>,
    },
    /// A new chat message in one of the player's conversations (their own too, from another
    /// device).
    Chat {
        msg: ChatMessage,
    },
    ChatEdited {
        msg: ChatMessage,
    },
    ChatDeleted {
        conv: String,
        id: i64,
        #[serde(default)]
        by: String,
        /// When (the hub's clock, ms).
        #[serde(default)]
        at: i64,
    },
    /// One conversation of a sync answer; CHAT2 adds the messages whose reactions alone changed and
    /// a direct conversation's read marks.
    ChatSync {
        conv: String,
        #[serde(default)]
        messages: Vec<ChatMessage>,
        #[serde(default)]
        deleted: Vec<ChatDeletion>,
        #[serde(default)]
        reacted: Vec<ChatReacted>,
        #[serde(default)]
        reads: Vec<ReadMark>,
    },
    /// A reaction was given (`on`) or taken back, also the player's own from another device
    /// (CHAT2).
    ChatReacted {
        conv: String,
        id: i64,
        by: String,
        e: String,
        #[serde(default)]
        on: bool,
    },
    /// Someone writes in that conversation (CHAT2).
    ChatTyping {
        conv: String,
        by: String,
    },
    /// A read mark moved (CHAT2): the player's own from another device, or a friend's.
    ChatRead {
        conv: String,
        by: String,
        id: i64,
    },
    /// The end of a sync answer: `now` is the hub's clock (ms), `more` the cursor to go on with.
    ChatSynced {
        #[serde(default)]
        rid: Option<u32>,
        #[serde(default)]
        now: Option<i64>,
        #[serde(default)]
        more: Option<String>,
    },
    ChatState {
        chat: ChatStatus,
    },
    /// A one-time note from moderators.
    Notice {
        notice: ModNotice,
    },
    /// World sessions (HOST1, `crate::world`; only with `worlds` in the capabilities): a friend's
    /// invite, `quiet` on do-not-disturb.
    WorldInvite {
        invite: crate::world::WorldInvite,
        #[serde(default)]
        quiet: bool,
    },
    /// The world the player was invited to changed (its count, or new `paths`).
    WorldUpdate {
        #[serde(default)]
        from: String,
        session: String,
        #[serde(default)]
        guests: u8,
        #[serde(default)]
        max: u8,
        #[serde(default)]
        paths: Option<crate::world::Hidden<serde_json::Value>>,
    },
    WorldClosed {
        #[serde(default)]
        from: String,
        session: String,
    },
    /// The invite was taken back: to the guest with `from`, to the host with `to`.
    WorldRevoked {
        #[serde(default)]
        from: Option<String>,
        session: String,
        #[serde(default)]
        to: Option<String>,
    },
    /// To the host: a guest's path signal.
    WorldPeer {
        session: String,
        from: String,
        slot: String,
        path: String,
        #[serde(default)]
        relay: Option<crate::world::Hidden<String>>,
    },
    /// To a guest: the host's paths after its signal.
    WorldPeerOk {
        session: String,
        #[serde(default)]
        from: String,
        #[serde(default)]
        slot: String,
        paths: crate::world::Hidden<serde_json::Value>,
    },
    /// Move this session's TURN streams to e4mc; the credentials go after `grace` seconds.
    TurnMove {
        session: String,
        #[serde(default)]
        reason: String,
        #[serde(default)]
        grace: u64,
    },
    /// Studio's switch for world sessions or the guest limit changed.
    WorldState {
        worlds: crate::world::WorldsStatus,
    },
    /// SKIN1 (with `mc` in the capabilities): a job of the player's closed, or ProBot reported a
    /// link change or a staff reset of the Projects skin.
    Mc(Box<McMessage>),
    /// Cloud AFK's notice about one of the player's cloud accounts (with `afk`, AFKPLUS).
    AfkNotice {
        notice: AfkNotice,
    },
    Ack {
        #[serde(default)]
        rid: Option<u32>,
        #[serde(default)]
        delivered: Option<bool>,
        /// A chat message as stored (sending and editing).
        #[serde(default)]
        msg: Option<ChatMessage>,
        /// A plan as stored (making one).
        #[serde(default)]
        plan: Option<CrewPlan>,
        /// A message's reactions as the player sees them (a reaction, CHAT2).
        #[serde(default)]
        reactions: Option<Vec<Reaction>>,
        /// A link's card (CHAT2).
        #[serde(default)]
        card: Option<Box<LinkCard>>,
        /// The first answer to `world{open:true}` (HOST1).
        #[serde(default)]
        world: Option<crate::world::WorldAck>,
        /// A `turn_creds` answer (HOST1): the paths to try, TURN credentials, the e4mc region.
        #[serde(default)]
        order: Option<Vec<String>>,
        #[serde(default)]
        ice: Option<crate::world::TurnIce>,
        #[serde(default)]
        e4mc: Option<crate::world::E4mcHint>,
    },
    Error {
        #[serde(default)]
        rid: Option<u32>,
        code: String,
        #[serde(default, rename = "retryAfter")]
        retry_after: Option<u64>,
        /// `chat_rejected`: `link`, `phone` or `words`; `chat_reactions_full`: `mine` or `kinds`.
        #[serde(default)]
        reason: Option<String>,
        /// `chat_rejected` with `link`, `unfurl_failed`: the link rule (CHAT2).
        #[serde(default)]
        rule: Option<String>,
        /// `chat_muted`, `account_limited`: Unix seconds.
        #[serde(default)]
        until: Option<i64>,
        /// `account_limited`: what is limited (`crew_plans`, `chat_links`); `rate_limited`: a
        /// chat quota of its own (`stickers`, …); `chat_feature_off`: the extra (CHAT2).
        #[serde(default)]
        item: Option<String>,
    },
    Bye {
        #[serde(default)]
        reason: String,
        /// `restricted`: until when (Unix seconds; null: for good).
        #[serde(default)]
        until: Option<i64>,
    },
    #[serde(other)]
    Unknown,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::ChatReplyRef;

    #[test]
    fn status_messages() {
        let json = |s: &LocalStatus| serde_json::to_string(&ClientMessage::status(s)).unwrap();
        assert_eq!(
            json(&LocalStatus::Launcher),
            r#"{"t":"status","state":"launcher"}"#
        );
        assert_eq!(
            json(&LocalStatus::Playing {
                host: Some("projects.gg".into())
            }),
            r#"{"t":"status","state":"playing","host":"projects.gg"}"#
        );
        assert_eq!(
            json(&LocalStatus::Singleplayer),
            r#"{"t":"status","state":"playing","singleplayer":true}"#
        );
        assert_eq!(
            serde_json::to_string(&ClientMessage::Invite {
                to: "1".into(),
                rid: 7
            })
            .unwrap(),
            r#"{"t":"invite","to":"1","rid":7}"#
        );
    }

    #[test]
    fn server_messages() {
        let hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","keepaliveSeconds":45,
                "friends":[{"id":"1","state":"playing","status":"dnd",
                  "game":{"where":"projects","host":"projects.gg"},"avatar":"discord","avatarHash":null}],
                "invites":[{"id":"9","from":"1","at":5,"server":null,"quiet":true}]}"#,
        )
        .unwrap();
        match hello {
            ServerMessage::Hello {
                status,
                keepalive_seconds,
                friends,
                invites,
                ..
            } => {
                assert_eq!(status, StatusMode::Online);
                assert_eq!(keepalive_seconds, Some(45));
                assert!(friends[0].game.as_ref().unwrap().joinable());
                assert_eq!(friends[0].status, Some(OnlineStatus::Dnd));
                assert!(invites[0].quiet);
            }
            other => panic!("unexpected {other:?}"),
        }
        let offline: ServerMessage = serde_json::from_str(
            r#"{"t":"presence","presence":{"id":"1","state":"offline","lastSeen":"today"}}"#,
        )
        .unwrap();
        assert!(matches!(
            offline,
            ServerMessage::Presence { presence } if presence.last_seen == Some(Some(LastSeen::Today))
        ));
        // SOC-LAG: `null` hides the friend list's last seen; absent keeps it.
        let hidden_seen: ServerMessage = serde_json::from_str(
            r#"{"t":"presence","presence":{"id":"1","state":"offline","lastSeen":null}}"#,
        )
        .unwrap();
        assert!(matches!(
            hidden_seen,
            ServerMessage::Presence { presence } if presence.last_seen == Some(None)
        ));
        let quiet_seen: ServerMessage =
            serde_json::from_str(r#"{"t":"presence","presence":{"id":"1","state":"offline"}}"#)
                .unwrap();
        assert!(matches!(
            quiet_seen,
            ServerMessage::Presence { presence } if presence.last_seen.is_none()
        ));
        // SOC-LAG: the player's own switches and picture from another device.
        let profile: ServerMessage = serde_json::from_str(
            r#"{"t":"profile","privacy":{"shareServer":true,"shareLastSeen":false,"shareRead":true,"shareTyping":false},"avatar":"preset:fox","avatarHash":null}"#,
        )
        .unwrap();
        match profile {
            ServerMessage::Profile {
                privacy,
                avatar,
                avatar_hash,
            } => {
                assert!(privacy.share_server && !privacy.share_last_seen);
                assert_eq!(
                    (privacy.share_read, privacy.share_typing),
                    (Some(true), Some(false))
                );
                assert_eq!(avatar.as_deref(), Some("preset:fox"));
                assert_eq!(avatar_hash, None);
            }
            other => panic!("unexpected {other:?}"),
        }
        let hidden: ServerMessage = serde_json::from_str(
            r#"{"t":"presence","presence":{"id":"1","state":"playing","game":{"where":"other"}}}"#,
        )
        .unwrap();
        assert!(matches!(
            hidden,
            ServerMessage::Presence { presence } if presence.game == Some(Game { location: GameLocation::Other, host: None })
        ));
        let future: ServerMessage = serde_json::from_str(r#"{"t":"emoji","x":1}"#).unwrap();
        assert!(matches!(future, ServerMessage::Unknown));
        let gone: ServerMessage = serde_json::from_str(
            r#"{"t":"team","team":null,"reason":"kicked","by":"123456789012345678"}"#,
        )
        .unwrap();
        assert!(matches!(
            gone,
            ServerMessage::Team {
                team: None,
                crew: None,
                reason: Some(TeamExit::Kicked),
                by: Some(_),
                ..
            }
        ));
        let groups: ServerMessage = serde_json::from_str(
            r#"{"t":"groups","groups":{"version":2,"groups":[{"id":"a","name":"A"}],"members":{}}}"#,
        )
        .unwrap();
        assert!(matches!(groups, ServerMessage::Groups { groups } if groups.version == 2));
        let team_hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","friends":[],
                "team":[{"id":"7","state":"launcher","status":"online"}],"invites":[]}"#,
        )
        .unwrap();
        assert!(matches!(team_hello, ServerMessage::Hello { team, .. } if team.len() == 1));
        let error: ServerMessage =
            serde_json::from_str(r#"{"t":"error","rid":3,"code":"rate_limited","retryAfter":60}"#)
                .unwrap();
        assert!(matches!(
            error,
            ServerMessage::Error {
                rid: Some(3),
                retry_after: Some(60),
                ..
            }
        ));
    }

    #[test]
    fn crew_call_messages() {
        let json = |message: &ClientMessage| serde_json::to_string(message).unwrap();
        assert_eq!(
            json(&ClientMessage::Call {
                crews: None,
                rid: 8
            }),
            r#"{"t":"call","rid":8}"#
        );
        assert_eq!(
            json(&ClientMessage::Call {
                crews: Some(vec!["0a1b2c3d4e5f6a7b".into(), "9f8e7d6c5b4a3f2e".into()]),
                rid: 8
            }),
            r#"{"t":"call","crews":["0a1b2c3d4e5f6a7b","9f8e7d6c5b4a3f2e"],"rid":8}"#
        );
        assert_eq!(
            json(&ClientMessage::CallAnswer {
                call: "3f9c0a1b2c3d4e5f".into(),
                answer: CallAnswer::Yes,
                rid: 9
            }),
            r#"{"t":"call_answer","call":"3f9c0a1b2c3d4e5f","answer":"yes","rid":9}"#
        );
        assert_eq!(
            json(&ClientMessage::CallEnd {
                call: "3f9c0a1b2c3d4e5f".into(),
                rid: 10
            }),
            r#"{"t":"call_end","call":"3f9c0a1b2c3d4e5f","rid":10}"#
        );

        let call: ServerMessage = serde_json::from_str(
            r#"{"t":"call","call":{"id":"3f9c0a1b2c3d4e5f","from":"7","kind":"join","at":1792521000,
                "until":1792522200,"answers":{"8":"yes","9":"soon","10":"maybe"}},"quiet":false,
                "now":1792521005}"#,
        )
        .unwrap();
        match call {
            ServerMessage::Call { call, quiet, now } => {
                assert_eq!(call.kind, CallKind::Join);
                assert!(!quiet);
                assert_eq!(now, Some(1_792_521_005));
                // An answer this version does not know is left out, not the whole message.
                assert_eq!(call.answers.len(), 2);
                assert_eq!(call.answers["9"], CallAnswer::Soon);
            }
            other => panic!("unexpected {other:?}"),
        }
        let later: ServerMessage = serde_json::from_str(
            r#"{"t":"call","call":{"id":"a","from":"7","kind":"party","at":1,"until":2,"answers":{}}}"#,
        )
        .unwrap();
        assert!(
            matches!(later, ServerMessage::Call { call, quiet: false, now: None } if call.kind == CallKind::Ready)
        );
        let answer: ServerMessage = serde_json::from_str(
            r#"{"t":"call_answer","call":"3f9c0a1b2c3d4e5f","by":"8","answer":"soon"}"#,
        )
        .unwrap();
        assert!(matches!(answer, ServerMessage::CallAnswer { answer, .. } if answer == "soon"));
        let ended: ServerMessage =
            serde_json::from_str(r#"{"t":"call_end","call":"a","reason":"expired"}"#).unwrap();
        assert!(matches!(
            ended,
            ServerMessage::CallEnd {
                reason: CallEndReason::Ended,
                ..
            }
        ));
        let left: ServerMessage =
            serde_json::from_str(r#"{"t":"call_end","call":"a","reason":"left"}"#).unwrap();
        assert!(matches!(
            left,
            ServerMessage::CallEnd {
                reason: CallEndReason::Left,
                ..
            }
        ));
        let hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","keepaliveSeconds":45,"friends":[],"team":[],
                "invites":[],"calls":[{"id":"b","from":"7","kind":"ready","at":1792521000,
                "until":1792522200,"answers":{}}],"now":1792521300}"#,
        )
        .unwrap();
        assert!(matches!(
            hello,
            ServerMessage::Hello { calls, now: Some(1_792_521_300), .. } if calls.len() == 1
        ));
        // A service before calls: no calls, no clock.
        let older: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","friends":[],"invites":[]}"#,
        )
        .unwrap();
        assert!(matches!(older, ServerMessage::Hello { calls, now: None, .. } if calls.is_empty()));
    }

    #[test]
    fn crew_notification_settings_arrive_on_every_device() {
        let changed: ServerMessage = serde_json::from_str(
            r#"{"t":"team_notifications","notifications":{"calls":true,"muted":true,"muteUntil":1792600000}}"#,
        )
        .unwrap();
        assert!(matches!(
            changed,
            ServerMessage::TeamNotifications { notifications }
                if notifications.muted && notifications.mute_until == Some(1_792_600_000)
        ));
        let unmuted: ServerMessage = serde_json::from_str(
            r#"{"t":"team_notifications","notifications":{"calls":false,"muted":false,"muteUntil":null}}"#,
        )
        .unwrap();
        assert!(matches!(
            unmuted,
            ServerMessage::TeamNotifications { notifications }
                if !notifications.calls && !notifications.muted && notifications.mute_until.is_none()
        ));
    }

    #[test]
    fn calls_from_a_server_and_several_crews() {
        // A call with `callServer` and `crews`: always `join`, with where it is and its crews.
        let call: ServerMessage = serde_json::from_str(
            r#"{"t":"call","call":{"id":"3f9c0a1b2c3d4e5f","from":"7","kind":"join","at":1,
                "until":1201,"answers":{},"crews":["0a1b2c3d4e5f6a7b","9f8e7d6c5b4a3f2e"],
                "server":{"where":"other","host":"mc.example.com"}},"quiet":false,"now":5}"#,
        )
        .unwrap();
        match call {
            ServerMessage::Call { call, .. } => {
                assert_eq!(call.crews.len(), 2);
                assert_eq!(
                    call.server,
                    Some(Game {
                        location: GameLocation::Other,
                        host: Some("mc.example.com".into())
                    })
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        // From a service before them: no crews, no server.
        let older: ServerMessage = serde_json::from_str(
            r#"{"t":"call","call":{"id":"a","from":"7","kind":"ready","at":1,"until":2}}"#,
        )
        .unwrap();
        assert!(matches!(
            older,
            ServerMessage::Call { call, .. } if call.crews.is_empty() && call.server.is_none()
        ));
        let moved: ServerMessage =
            serde_json::from_str(r#"{"t":"call_end","call":"a","reason":"server"}"#).unwrap();
        assert!(matches!(
            moved,
            ServerMessage::CallEnd {
                reason: CallEndReason::Server,
                ..
            }
        ));
        let hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","friends":[],"invites":[],
                "caps":["callServer","crews"],"now":9}"#,
        )
        .unwrap();
        assert!(matches!(hello, ServerMessage::Hello { caps, .. } if caps.len() == 2));
        // A crew of several changed, or the player left it.
        let gone: ServerMessage = serde_json::from_str(
            r#"{"t":"team","team":null,"crew":"9f8e7d6c5b4a3f2e","reason":"left","by":"100"}"#,
        )
        .unwrap();
        assert!(matches!(
            gone,
            ServerMessage::Team { team: None, crew: Some(crew), .. } if crew == "9f8e7d6c5b4a3f2e"
        ));
        let muted: ServerMessage = serde_json::from_str(
            r#"{"t":"crew_notifications","crew":"0a1b2c3d4e5f6a7b",
                "notifications":{"muted":true,"muteUntil":1792600000}}"#,
        )
        .unwrap();
        assert!(matches!(
            muted,
            ServerMessage::CrewNotifications { crew, notifications }
                if crew == "0a1b2c3d4e5f6a7b" && notifications.mute_until == Some(1_792_600_000)
        ));
        let order: ServerMessage = serde_json::from_str(
            r#"{"t":"crew_order","order":["9f8e7d6c5b4a3f2e","0a1b2c3d4e5f6a7b"]}"#,
        )
        .unwrap();
        assert!(
            matches!(order, ServerMessage::CrewOrder { order } if order[0] == "9f8e7d6c5b4a3f2e")
        );
    }

    #[test]
    fn projects_hosts() {
        for host in [
            "projects.gg",
            "play.projects.gg",
            "PLAY.Projects.GG.",
            "play.projects.gg:25565",
        ] {
            assert!(is_projects_host(host), "{host}");
        }
        for host in [
            "notprojects.gg",
            "projects.gg.example.com",
            "mc.example.com",
            "projects.gg:abc",
            "",
        ] {
            assert!(!is_projects_host(host), "{host}");
        }
    }

    #[test]
    fn chat_frames_both_ways() {
        assert_eq!(
            presence_caps(false),
            "callServer,crews,plans,badges,mc,afk,profile"
        );
        assert_eq!(
            presence_caps(true),
            "callServer,crews,plans,badges,mc,afk,profile,chat,replies,stickers,reactions,typing,reads"
        );
        let json = |message: &ClientMessage| serde_json::to_string(message).unwrap();
        assert_eq!(
            json(&ClientMessage::Chat {
                conv: "c:0a1b2c3d4e5f6a7b".into(),
                text: Some("Geliyorum".into()),
                sticker: None,
                reply: None,
                nonce: "q8Zr3kT0aWbc".into(),
                rid: 11
            }),
            r#"{"t":"chat","conv":"c:0a1b2c3d4e5f6a7b","text":"Geliyorum","nonce":"q8Zr3kT0aWbc","rid":11}"#
        );
        let held = ChatMessage {
            conv: "d:1:2".into(),
            id: 1_792_521_000_123,
            from: "1".into(),
            text: "Geliyrum".into(),
            mac: "m".repeat(22),
            nonce: Some("own".into()),
            ..ChatMessage::default()
        };
        // The nonce stays home: the service wants the sealed message back as it sent it.
        let sealed: serde_json::Value = serde_json::from_str(&json(&ClientMessage::ChatEdit {
            msg: held.sealed(),
            text: "Geliyorum".into(),
            rid: 12,
        }))
        .unwrap();
        assert_eq!(sealed["t"], "chat_edit");
        assert_eq!(
            sealed["msg"],
            serde_json::json!({"conv": "d:1:2", "id": 1_792_521_000_123_i64, "from": "1",
                "text": "Geliyrum", "mac": "m".repeat(22)})
        );
        let edited = ChatMessage {
            edited_at: Some(1_792_521_060_000),
            ..held.clone()
        };
        assert_eq!(edited.sealed()["editedAt"], 1_792_521_060_000_i64);
        assert_eq!(
            json(&ClientMessage::ChatSync {
                since: 1_792_521_000_000,
                after: None,
                rid: 14
            }),
            r#"{"t":"chat_sync","since":1792521000000,"rid":14}"#
        );
        assert_eq!(
            json(&ClientMessage::ChatSync {
                since: 5,
                after: Some("k".into()),
                rid: 15
            }),
            r#"{"t":"chat_sync","since":5,"after":"k","rid":15}"#
        );

        let hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","friends":[],"invites":[],"caps":["callServer","crews","chat"],
                "chat":{"on":true,"mailbox":false,"mailboxBack":1792540800,"perDay":100,"mutedUntil":null},"now":9}"#,
        )
        .unwrap();
        match hello {
            ServerMessage::Hello {
                chat: Some(chat), ..
            } => {
                assert!(chat.on && !chat.mailbox);
                assert_eq!(chat.mailbox_back, Some(1_792_540_800));
                assert_eq!(chat.per_day, 100);
                assert_eq!(chat.muted_until, None);
            }
            other => panic!("unexpected {other:?}"),
        }
        let sync: ServerMessage = serde_json::from_str(
            r#"{"t":"chat_sync","conv":"c:0a1b2c3d4e5f6a7b","messages":[{"id":7,"from":"1","text":"a","mac":"x",
                "editedAt":9,"nonce":"n1"}],"deleted":[{"id":3,"by":"staff","at":8}]}"#,
        )
        .unwrap();
        assert!(matches!(
            sync,
            ServerMessage::ChatSync { conv, messages, deleted, reacted, reads }
                if conv.starts_with("c:") && messages[0].edited_at == Some(9)
                    && messages[0].conv.is_empty() && deleted[0].by == "staff"
                    && reacted.is_empty() && reads.is_empty()
        ));
        let synced: ServerMessage =
            serde_json::from_str(r#"{"t":"chat_synced","rid":14,"now":1792521300000,"more":"k"}"#)
                .unwrap();
        assert!(matches!(
            synced,
            ServerMessage::ChatSynced {
                rid: Some(14),
                more: Some(_),
                ..
            }
        ));
        let ack: ServerMessage = serde_json::from_str(
            r#"{"t":"ack","rid":11,"msg":{"conv":"c:0a1b2c3d4e5f6a7b","id":7,"from":"1","text":"a","mac":"x","nonce":"n1"}}"#,
        )
        .unwrap();
        assert!(
            matches!(ack, ServerMessage::Ack { msg: Some(msg), .. } if msg.nonce.as_deref() == Some("n1"))
        );
        let refused: ServerMessage = serde_json::from_str(
            r#"{"t":"error","rid":11,"code":"chat_rejected","reason":"link"}"#,
        )
        .unwrap();
        assert!(matches!(refused, ServerMessage::Error { reason: Some(r), .. } if r == "link"));
        let muted: ServerMessage = serde_json::from_str(
            r#"{"t":"error","rid":11,"code":"chat_muted","until":1792600000}"#,
        )
        .unwrap();
        assert!(matches!(
            muted,
            ServerMessage::Error {
                until: Some(1_792_600_000),
                ..
            }
        ));
        let deleted: ServerMessage =
            serde_json::from_str(r#"{"t":"chat_deleted","conv":"d:1:2","id":7,"by":"","at":9}"#)
                .unwrap();
        assert!(matches!(
            deleted,
            ServerMessage::ChatDeleted { id: 7, at: 9, .. }
        ));
        let notice: ServerMessage = serde_json::from_str(
            r#"{"t":"notice","notice":{"id":"w3","kind":"warning","at":1792520000,"reason":"harassment"}}"#,
        )
        .unwrap();
        assert!(matches!(notice, ServerMessage::Notice { notice } if notice.id == "w3"));
        let bye: ServerMessage =
            serde_json::from_str(r#"{"t":"bye","reason":"restricted","until":null}"#).unwrap();
        assert!(matches!(bye, ServerMessage::Bye { until: None, .. }));
    }

    #[test]
    fn crew_tag_frames_and_who_just_came_in() {
        let worn: ServerMessage = serde_json::from_str(
            r#"{"t":"badge","id":"223456789012345678","badge":{"crew":"0a1b2c3d4e5f6a7b",
                "tag":"GK","color":"navy","frame":"shield","emblem":"bolt","name":"Gece Kartalları",
                "motto":"","createdOn":"2026-09-20","members":12,"requests":false}}"#,
        )
        .unwrap();
        assert!(matches!(
            worn,
            ServerMessage::Badge { id, badge: Some(badge) }
                if id == "223456789012345678" && badge.tag == "GK" && badge.members == 12
        ));
        let off: ServerMessage =
            serde_json::from_str(r#"{"t":"badge","id":"223456789012345678","badge":null}"#)
                .unwrap();
        assert!(matches!(off, ServerMessage::Badge { badge: None, .. }));
        // A card this version cannot read is no card, not an unreadable message.
        let odd: ServerMessage =
            serde_json::from_str(r#"{"t":"badge","id":"223456789012345678","badge":"GK"}"#)
                .unwrap();
        assert!(matches!(odd, ServerMessage::Badge { badge: None, .. }));
        let joined: ServerMessage = serde_json::from_str(
            r#"{"t":"team","crew":"0a1b2c3d4e5f6a7b","by":"123456789012345678",
                "joined":["223456789012345678"],"team":{"id":"0a1b2c3d4e5f6a7b","name":"Gece",
                "tag":"GK","emblem":"bolt","code":"TKM-ABCD-EFGH","createdOn":"2026-09-20",
                "hidden":true,"badgeRequests":false,"members":[]}}"#,
        )
        .unwrap();
        match joined {
            ServerMessage::Team {
                team: Some(team),
                joined,
                ..
            } => {
                assert_eq!(joined, ["223456789012345678"]);
                assert!(team.hidden && !team.badge_requests);
            }
            other => panic!("unexpected {other:?}"),
        }
        // An older service names nobody.
        let older: ServerMessage =
            serde_json::from_str(r#"{"t":"team","team":null,"reason":"left"}"#).unwrap();
        assert!(matches!(older, ServerMessage::Team { joined, .. } if joined.is_empty()));
    }

    #[test]
    fn plan_frames_and_join_requests_both_ways() {
        let json = |message: &ClientMessage| serde_json::to_string(message).unwrap();
        assert_eq!(
            json(&ClientMessage::Plan {
                crew: "0a1b2c3d4e5f6a7b".into(),
                at: 1_792_609_200,
                note: None,
                rid: 15
            }),
            r#"{"t":"plan","crew":"0a1b2c3d4e5f6a7b","at":1792609200,"rid":15}"#
        );
        // FIX2: the note goes with the plan, and on its own to change or clear it.
        assert_eq!(
            json(&ClientMessage::Plan {
                crew: "0a1b2c3d4e5f6a7b".into(),
                at: 1_792_609_200,
                note: Some("Bedwars, sesli gelin".into()),
                rid: 18
            }),
            r#"{"t":"plan","crew":"0a1b2c3d4e5f6a7b","at":1792609200,"note":"Bedwars, sesli gelin","rid":18}"#
        );
        assert_eq!(
            json(&ClientMessage::PlanNote {
                crew: "0a1b2c3d4e5f6a7b".into(),
                id: "3f9c0a1b2c3d4e5f".into(),
                note: String::new(),
                rid: 19
            }),
            r#"{"t":"plan_note","crew":"0a1b2c3d4e5f6a7b","id":"3f9c0a1b2c3d4e5f","note":"","rid":19}"#
        );
        assert_eq!(
            json(&ClientMessage::PlanAnswer {
                plan: "3f9c0a1b2c3d4e5f".into(),
                answer: PlanAnswer::Maybe,
                rid: 16
            }),
            r#"{"t":"plan_answer","plan":"3f9c0a1b2c3d4e5f","answer":"maybe","rid":16}"#
        );
        assert_eq!(
            json(&ClientMessage::PlanCancel {
                plan: "3f9c0a1b2c3d4e5f".into(),
                rid: 17
            }),
            r#"{"t":"plan_cancel","plan":"3f9c0a1b2c3d4e5f","rid":17}"#
        );

        // `hello.plans` keeps what it can read: an answer or a plan from a newer service never
        // costs the rest (nor the hello).
        let hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","caps":["callServer","crews","plans"],"now":9,
                "plans":[{"id":"p1","crew":"0a1b2c3d4e5f6a7b","by":"1","at":1792609200,
                  "until":1792616400,"createdAt":1792520000,"answers":{"1":"yes","2":"maybe","3":"later"}},
                  {"id":"broken"}]}"#,
        )
        .unwrap();
        match hello {
            ServerMessage::Hello {
                plans: Some(plans),
                caps,
                ..
            } => {
                assert!(caps.iter().any(|cap| cap == PLANS_CAP));
                assert_eq!(plans.len(), 1);
                assert_eq!(plans[0].until - plans[0].at, 7_200);
                assert_eq!(plans[0].answers.len(), 2);
                assert_eq!(plans[0].answers["2"], PlanAnswer::Maybe);
            }
            other => panic!("unexpected {other:?}"),
        }
        // Without the capability there are none at all (not an empty list).
        let older: ServerMessage =
            serde_json::from_str(r#"{"t":"hello","v":1,"status":"online","now":9}"#).unwrap();
        assert!(matches!(older, ServerMessage::Hello { plans: None, .. }));

        let plan: ServerMessage = serde_json::from_str(
            r#"{"t":"plan","plan":{"id":"p1","crew":"c","by":"1","at":900,"until":8100,
                "createdAt":1,"answers":{}},"quiet":false}"#,
        )
        .unwrap();
        assert!(
            matches!(plan, ServerMessage::Plan { quiet: false, plan } if plan.id == "p1" && plan.note.is_none())
        );
        // A note shows when it says something; an empty one or one that is not text is none.
        for (note, expected) in [
            (r#""Bedwars, sesli gelin""#, Some("Bedwars, sesli gelin")),
            (r#""""#, None),
            ("null", None),
            ("7", None),
        ] {
            let plan: CrewPlan = serde_json::from_str(&format!(
                r#"{{"id":"p1","crew":"c","by":"1","at":900,"until":8100,"note":{note}}}"#
            ))
            .unwrap();
            assert_eq!(plan.note.as_deref(), expected, "{note}");
        }
        let empty: ServerMessage =
            serde_json::from_str(r#"{"t":"plans","crew":"c","plans":[]}"#).unwrap();
        assert!(matches!(empty, ServerMessage::Plans { plans, .. } if plans.is_empty()));
        let removed: ServerMessage =
            serde_json::from_str(r#"{"t":"plan_removed","plan":"p1","crew":"c","by":"2"}"#)
                .unwrap();
        assert!(matches!(removed, ServerMessage::PlanRemoved { by, .. } if by == "2"));
        let ack: ServerMessage = serde_json::from_str(
            r#"{"t":"ack","rid":15,"plan":{"id":"p2","crew":"c","by":"1","at":900,"until":8100,
                "createdAt":1,"answers":{"1":"yes"}}}"#,
        )
        .unwrap();
        assert!(matches!(ack, ServerMessage::Ack { plan: Some(plan), .. } if plan.id == "p2"));
        let limited: ServerMessage = serde_json::from_str(
            r#"{"t":"error","rid":15,"code":"account_limited","item":"crew_plans","until":1792600000}"#,
        )
        .unwrap();
        assert!(matches!(
            limited,
            ServerMessage::Error { item: Some(item), until: Some(1_792_600_000), .. } if item == "crew_plans"
        ));

        let requests: ServerMessage = serde_json::from_str(
            r#"{"t":"team_requests","crew":"c","added":"2","requests":[{"from":{"id":"2","username":"b",
                "displayName":null,"avatar":"discord","avatarHash":null},"sentAt":5},{"from":7}]}"#,
        )
        .unwrap();
        match requests {
            ServerMessage::TeamRequests {
                crew,
                requests,
                added,
            } => {
                assert_eq!((crew.as_str(), added.as_deref()), ("c", Some("2")));
                assert_eq!(requests.len(), 1);
                assert_eq!(requests[0].sent_at, 5);
            }
            other => panic!("unexpected {other:?}"),
        }
    }

    #[test]
    fn chat_extras_frames_both_ways() {
        let json = |message: &ClientMessage| serde_json::to_string(message).unwrap();
        // A sticker answering a reply: the message answered goes back as held, with both seals.
        let answered = ChatMessage {
            conv: "d:1:2".into(),
            id: 1_792_521_000_123,
            from: "2".into(),
            text: "Geliyor musun?".into(),
            mac: "a".repeat(22),
            reply: Some(ChatReplyRef {
                id: 1_792_521_000_100,
                from: "1".into(),
            }),
            rmac: Some("r".repeat(22)),
            reactions: vec![Reaction {
                e: "👍".into(),
                by: vec!["1".into()],
            }],
            ..ChatMessage::default()
        };
        let sealed = answered.sealed();
        assert_eq!(
            sealed["reply"],
            serde_json::json!({"id": 1_792_521_000_100_i64, "from": "1"})
        );
        assert_eq!(sealed["rmac"], "r".repeat(22));
        assert!(sealed.get("reactions").is_none() && sealed.get("nonce").is_none());
        let frame: serde_json::Value = serde_json::from_str(&json(&ClientMessage::Chat {
            conv: "d:1:2".into(),
            text: None,
            sticker: Some("pl1/gg".into()),
            reply: Some(serde_json::json!({ "msg": sealed })),
            nonce: "Hq2Vw9Lm3Pzt".into(),
            rid: 19,
        }))
        .unwrap();
        assert_eq!(frame["t"], "chat");
        assert_eq!(frame["sticker"], "pl1/gg");
        assert!(frame.get("text").is_none());
        assert_eq!(frame["reply"]["msg"]["text"], "Geliyor musun?");
        // A sticker is sealed over its id, in place of the text.
        let sticker = ChatMessage {
            conv: "c:0a1b2c3d4e5f6a7b".into(),
            id: 5,
            from: "1".into(),
            mac: "s".repeat(22),
            sticker: Some("pl1/gg".into()),
            ..ChatMessage::default()
        };
        assert_eq!(
            sticker.sealed(),
            serde_json::json!({"conv": "c:0a1b2c3d4e5f6a7b", "id": 5, "from": "1",
                "sticker": "pl1/gg", "mac": "s".repeat(22)})
        );
        assert_eq!(
            json(&ClientMessage::ChatReact {
                conv: "c:0a1b2c3d4e5f6a7b".into(),
                id: 1_792_521_000_123,
                e: "👍".into(),
                on: true,
                rid: 20
            }),
            r#"{"t":"chat_react","conv":"c:0a1b2c3d4e5f6a7b","id":1792521000123,"e":"👍","on":true,"rid":20}"#
        );
        assert_eq!(
            json(&ClientMessage::ChatTyping {
                conv: "d:1:2".into()
            }),
            r#"{"t":"chat_typing","conv":"d:1:2"}"#
        );
        assert_eq!(
            json(&ClientMessage::ChatRead {
                conv: "d:1:2".into(),
                id: 7
            }),
            r#"{"t":"chat_read","conv":"d:1:2","id":7}"#
        );
        assert_eq!(
            json(&ClientMessage::ChatUnfurl {
                url: "https://youtu.be/dQw4w9WgXcQ".into(),
                rid: 21
            }),
            r#"{"t":"chat_unfurl","url":"https://youtu.be/dQw4w9WgXcQ","rid":21}"#
        );

        // The state: which extras are on, the sticker sets, the link rules; the caps say which
        // the socket took.
        let hello: ServerMessage = serde_json::from_str(
            r#"{"t":"hello","v":1,"status":"online","caps":["callServer","crews","chat","replies","stickers","typing"],
                "chat":{"on":true,"mailbox":true,"mailboxBack":null,"perDay":500,"mutedUntil":null,
                  "extras":{"replies":true,"stickers":true,"reactions":true,"typing":false,"reads":true,"links":true},
                  "stickers":["pl1"],"links":{"rules":2,"trusted":["projects.gg"],"blocked":[],"invites":false}}}"#,
        )
        .unwrap();
        match hello {
            ServerMessage::Hello {
                chat: Some(mut chat),
                caps,
                ..
            } => {
                assert_eq!(chat.stickers, ["pl1"]);
                assert_eq!(
                    chat.links.as_ref().map(|l| (l.rules, l.invites)),
                    Some((2, false))
                );
                chat.caps = chat_caps(&caps);
                let offered = chat.offered().unwrap();
                // Typing is off in Studio; reactions and reads were not taken by the socket.
                assert!(offered.replies && offered.stickers && offered.links);
                assert!(!offered.typing && !offered.reactions && !offered.reads);
            }
            other => panic!("unexpected {other:?}"),
        }
        // An older service: no extras at all.
        let older: ChatStatus =
            serde_json::from_str(r#"{"on":true,"mailbox":true,"perDay":500}"#).unwrap();
        assert!(older.offered().is_none() && older.stickers.is_empty());

        let reacted: ServerMessage = serde_json::from_str(
            r#"{"t":"chat_reacted","conv":"c:0a1b2c3d4e5f6a7b","id":9,"by":"7","e":"😂","on":false}"#,
        )
        .unwrap();
        assert!(
            matches!(reacted, ServerMessage::ChatReacted { id: 9, ref by, on: false, .. } if by == "7")
        );
        let typing: ServerMessage =
            serde_json::from_str(r#"{"t":"chat_typing","conv":"d:1:2","by":"2"}"#).unwrap();
        assert!(matches!(typing, ServerMessage::ChatTyping { ref by, .. } if by == "2"));
        let read: ServerMessage =
            serde_json::from_str(r#"{"t":"chat_read","conv":"d:1:2","by":"2","id":7}"#).unwrap();
        assert!(matches!(read, ServerMessage::ChatRead { id: 7, .. }));
        let sync: ServerMessage = serde_json::from_str(
            r#"{"t":"chat_sync","conv":"d:1:2","messages":[{"id":8,"from":"2","sticker":"pl1/hi","mac":"x",
                "reactions":[{"e":"❤️","by":["1","2"]}]}],"deleted":[],
                "reacted":[{"id":3,"reactions":[]}],"reads":[{"by":"1","id":8},{"by":"2","id":5}]}"#,
        )
        .unwrap();
        match sync {
            ServerMessage::ChatSync {
                messages,
                reacted,
                reads,
                ..
            } => {
                assert_eq!(messages[0].sticker.as_deref(), Some("pl1/hi"));
                assert!(messages[0].text.is_empty());
                assert_eq!(messages[0].reactions[0].by, ["1", "2"]);
                assert!(reacted[0].reactions.is_empty());
                assert_eq!(
                    reads[1],
                    ReadMark {
                        by: "2".into(),
                        id: 5
                    }
                );
            }
            other => panic!("unexpected {other:?}"),
        }
        let ack: ServerMessage =
            serde_json::from_str(r#"{"t":"ack","rid":20,"reactions":[{"e":"👍","by":["7","1"]}]}"#)
                .unwrap();
        assert!(
            matches!(ack, ServerMessage::Ack { reactions: Some(ref r), msg: None, .. } if r[0].by.len() == 2)
        );
        let card: ServerMessage = serde_json::from_str(
            r#"{"t":"ack","rid":21,"card":{"url":"https://youtu.be/x","site":"YouTube","icon":"youtube",
                "kind":"video","title":"Bir video","image":"data:image/jpeg;base64,AAAA","w":320,"h":180}}"#,
        )
        .unwrap();
        assert!(matches!(card, ServerMessage::Ack { card: Some(ref c), .. } if c.w == Some(320)));
        let refused: ServerMessage = serde_json::from_str(
            r#"{"t":"error","rid":11,"code":"chat_rejected","reason":"link","rule":"shortener"}"#,
        )
        .unwrap();
        assert!(
            matches!(refused, ServerMessage::Error { rule: Some(ref r), .. } if r == "shortener")
        );
    }
}
