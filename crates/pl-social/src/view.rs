//! What the launcher UI receives: one [`SocialView`] after every change, and [`SocialNotice`]s
//! for moments worth a toast or a notification. camelCase JSON; the launcher exports the
//! TypeScript types with tauri-specta. Nothing here carries a token or an internal error.

use serde::{Deserialize, Serialize};

use crate::model::{
    CrewMute, FriendGroup, JoinMode, LimitedItem, LimitedKey, Limits, Me, NoticeKind, Privacy,
    PublicProfile, Relation, ReportReason, SettingsMeta, StatusMode, TeamExit, TeamPermissions,
    TeamRelation, TeamRole, discord_avatar_url,
};
use crate::protocol::{
    CallAnswer, CallKind, Game, LocalStatus, PlanAnswer, PresenceState, is_projects_host,
};
use crate::qr::QrPath;
use crate::state::Connection;

/// Sign-in state of this launcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AuthPhase {
    #[default]
    SignedOut,
    /// The browser is open on Discord; waiting for the `projects://auth/done` link.
    SigningIn,
    /// A saved session is being checked at start.
    Restoring,
    SignedIn,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SocialView {
    /// Grows with every view; a view older than the one shown is ignored.
    pub seq: u64,
    /// False while the feature is off: the launcher shows nothing of it.
    pub enabled: bool,
    pub auth: AuthPhase,
    pub connection: Connection,
    pub me: Option<MeView>,
    /// The friend list arrived since sign-in (skeletons until then).
    pub loaded: bool,
    pub friends: Vec<FriendView>,
    pub incoming: Vec<RequestView>,
    pub outgoing: Vec<RequestView>,
    pub blocked: Vec<ProfileView>,
    /// "Birlikte oyna" invites still worth answering, newest first.
    pub invites: Vec<InviteView>,
    pub limits: Option<Limits>,
    /// What this launcher tells friends right now.
    pub activity: ActivityView,
    /// The player's friend categories; None while the service does not offer them (an older
    /// service) or the list has not arrived.
    pub groups: Option<FriendGroupsView>,
    /// Teams are on (signed config `features.teams`) and the service offers them.
    pub teams: bool,
    /// What the service offers of crews beyond the first version (`look`, `calls`, …; only
    /// while `teams`). The launcher uses each part only while it is listed.
    pub team_features: Vec<String>,
    /// The player's crews in their order, the first being the primary crew (only while `teams`;
    /// at most one from a service without `multi`).
    pub crews: Vec<TeamView>,
    /// How many crews the player may be in (1 from a service without `multi`).
    pub crew_limit: u32,
    /// Invites to teams, newest first (only while `teams`).
    pub team_invites: Vec<TeamInviteView>,
    /// The player's own join requests that wait, newest first (only while `teams` and the
    /// service offers `requests`). They go by themselves 7 days after `sentAt`.
    pub team_requests: Vec<TeamRequestView>,
    /// The plans of the player's crews they see, soonest first, those under way included (only
    /// while `teams` and the service offers `plans` on this connection). They are over at
    /// `endsAt`: the launcher hides them then.
    pub plans: Vec<PlanView>,
    /// "Birlikte oynanan süre" is kept on this computer (the player may switch it off).
    pub together: bool,
    /// Open crew calls the player sees, their own included (only while `teams` and the service
    /// offers calls). They end by themselves at `endsAt`: the launcher hides them then.
    pub calls: Vec<CrewCallView>,
    /// Whether "Ekibi çağır" may start from where the player is right now (CALL1).
    pub call_place: CallPlace,
    /// The player's crew notification settings for all crews, kept by the service (only while
    /// `teams` and the service offers `mute`; otherwise the launcher's own switches decide).
    pub crew_notifications: Option<CrewNotificationsView>,
    /// Chat may show (signed config `features.chat`), whatever the service offers: the sign-in
    /// screen says what chat keeps.
    pub chat_allowed: bool,
    /// Crew chat and direct messages (CHAT1): only while `chat_allowed` and the service offers
    /// chat on this connection.
    pub chat: Option<ChatView>,
    /// One-time notes from moderators still to be shown, oldest first.
    pub notices: Vec<ModNoticeView>,
    /// Moderators suspended or banned the account (this run): the sign-in screen says so.
    pub restriction: Option<RestrictionView>,
    /// The settings backup waits until then (Unix seconds): the service protects its storage.
    pub backup_waits_until: Option<i64>,
}

/// A suspension (`until`, Unix seconds) or a ban (`until` None) from the social features.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RestrictionView {
    pub until: Option<i64>,
}

/// A one-time note from Projects' moderators.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ModNoticeView {
    pub id: String,
    pub kind: NoticeKind,
    /// Unix seconds.
    pub at: i64,
    /// What it was about (a report reason), when the moderators said.
    pub reason: Option<ReportReason>,
}

/// A limited account ("Sınırlı hesap", a new Discord account): until when, and what is limited
/// (only what this launcher knows how to say).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LimitedView {
    /// Unix seconds.
    pub until: i64,
    pub items: Vec<LimitedItem>,
}

impl LimitedView {
    pub(crate) fn of(limited: &crate::model::Limited) -> Self {
        Self {
            until: limited.until,
            items: limited
                .items
                .iter()
                .filter(|item| item.key != LimitedKey::Other)
                .copied()
                .collect(),
        }
    }
}

/// Crew chat and direct messages (CHAT1).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatView {
    /// The service's switch: off hides the composer; what this device holds stays readable.
    pub on: bool,
    /// Messages also wait for devices that are away; false: they reach only those online ("Mesajlar
    /// şu an yalnız çevrimiçi olanlara gidiyor.").
    pub mailbox: bool,
    /// The player's daily limit.
    pub per_day: u32,
    /// Unix seconds on this computer's clock: a moderator muted the player until then.
    pub muted_until: Option<i64>,
    /// The player's crews and every conversation this device holds, the newest first (crews
    /// without messages last).
    pub conversations: Vec<ConversationView>,
}

/// One conversation in the list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ConversationView {
    /// `c:<team id>` or `d:<id>:<id>`.
    pub id: String,
    /// The friend of a direct conversation.
    pub peer: Option<String>,
    /// The crew of a crew's chat.
    pub crew: Option<String>,
    /// Who or what it is with, as last known (the lists' name wins while they are in them).
    pub title: Option<String>,
    pub last: Option<LastMessageView>,
    pub unread: u32,
    /// The player may write here now (still friends, still in the crew).
    pub writable: bool,
    /// A message of the player's did not go.
    pub failed: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LastMessageView {
    pub from: String,
    pub mine: bool,
    /// Empty when it was deleted for everyone.
    pub text: String,
    /// Unix milliseconds on this computer's clock.
    pub at: i64,
    /// It was deleted for everyone, by this user id (`staff`: Projects' moderators; empty: not
    /// known).
    pub deleted_by: Option<String>,
}

/// Where one of the player's messages stands.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum MessageState {
    Sent,
    /// On its way (shown at once, settled by the service's answer).
    Sending,
    /// It did not go: `failure` says why.
    Failed,
}

/// Why one of the player's messages did not go.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SendFailure {
    /// A link ("Bağlantı gönderilemez.").
    Link,
    /// A phone number.
    Phone,
    /// A word the service does not let through (it never says which).
    Words,
    /// Too long, or nothing left after cleaning.
    Invalid,
    /// Too many messages in a row.
    TooFast,
    /// The day's messages are used up.
    DailyLimit,
    /// A moderator muted the player.
    Muted,
    /// Chat is switched off.
    Off,
    /// No longer friends, or no longer in the crew.
    NotAllowed,
    /// The connection: it may go again.
    Connection,
}

/// One message of a conversation.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessageView {
    /// The same from the moment it is typed to sent (`n:<nonce>` for the player's own, else
    /// `i:<id>`).
    pub key: String,
    /// The service's id (its clock, ms; the order key): None while on its way.
    pub id: Option<i64>,
    pub from: String,
    pub mine: bool,
    pub text: String,
    /// Unix milliseconds on this computer's clock.
    pub at: i64,
    /// "düzenlendi".
    pub edited: bool,
    pub state: MessageState,
    pub failure: Option<SendFailure>,
    /// Unix ms (this computer): the player may edit it until then (their own, sent; 15 minutes).
    pub edit_until: Option<i64>,
    /// Unix ms: the player may delete it for everyone until then (their own, or any in a crew
    /// they lead or moderate; 7 days).
    pub delete_until: Option<i64>,
    /// Deleted for everyone: a tombstone in the message's place (the same key; `text` is empty,
    /// nothing may be done with it).
    pub deleted: Option<ChatDeletedView>,
}

/// Who deleted a message for everyone, and when.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatDeletedView {
    /// A user id (the sender, or a crew's leader or moderator), `staff` for Projects' moderators,
    /// or empty when not known (that account is gone, or the service did not say).
    pub by: String,
    /// Unix milliseconds on this computer's clock, when known.
    pub at: Option<i64>,
}

/// A conversation's newest messages (or those before a given one), oldest first.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatPage {
    pub conv: String,
    pub messages: Vec<ChatMessageView>,
    /// Older messages remain on this device.
    pub more: bool,
}

/// What changed in one conversation (`social-chat`): messages to add or replace (by key), keys to
/// take away, or everything shown gone first (`reset`: the history was cleared here).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ChatChangeView {
    pub conv: String,
    pub upsert: Vec<ChatMessageView>,
    pub remove: Vec<String>,
    pub reset: bool,
}

/// What the player reports ("Rapor et"): a player, a crew (`code` when it was looked up by code)
/// or a message as this device holds it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ReportTarget {
    /// A player; from a plan card (FIX2) its maker, with the plan as the moderators' evidence.
    Player {
        id: String,
        #[serde(default)]
        plan: Option<ReportPlan>,
    },
    Crew {
        id: String,
        code: Option<String>,
    },
    Message {
        conv: String,
        id: i64,
    },
}

/// The crew plan a player is reported from: its crew's id and its own.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct ReportPlan {
    pub crew: String,
    pub id: String,
}

/// Where "Ekibi çağır" stands (CALL1): a call starts only while the player plays, in a game this
/// launcher started, on a multiplayer server: projects.gg always, another server only while its
/// address is shared with friends ("Başka sunuculardaysam adresini göster").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum CallPlace {
    /// On projects.gg, or on another server whose address is shared.
    Ok,
    /// In the launcher, in singleplayer, or the server is not known (menus, a LAN world).
    NeedsServer,
    /// On another server while its address is not shared.
    ServerHidden,
}

impl CallPlace {
    /// Where a call may start from this launcher's status.
    pub fn of(status: &LocalStatus, share_server: bool) -> Self {
        match status {
            LocalStatus::Playing { host: Some(host) } if is_projects_host(host) => Self::Ok,
            LocalStatus::Playing { host: Some(_) } if share_server => Self::Ok,
            LocalStatus::Playing { host: Some(_) } => Self::ServerHidden,
            LocalStatus::Launcher
            | LocalStatus::Singleplayer
            | LocalStatus::Playing { host: None } => Self::NeedsServer,
        }
    }
}

/// What the player hears of their crew ("Sessize al", "Ekip çağrıları"), the same on all their
/// devices.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrewNotificationsView {
    /// Crew call notifications are on.
    pub calls: bool,
    /// "Sessizde": every crew notification is silent until `mute_until`, or until the player
    /// unmutes (`mute_until` None).
    pub muted: bool,
    /// Unix seconds. A timed mute ends by itself (no view says so): compare it with the clock.
    pub mute_until: Option<i64>,
}

/// "Ekibi çağır": an open crew call.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrewCallView {
    pub id: String,
    /// The caller: a teammate, or the player (`mine`).
    pub from: String,
    pub mine: bool,
    /// The caller's name and picture as the crew shows them (None when not in the crew list).
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub avatar_url: Option<String>,
    /// `join`: "Geliyorum" can join the caller where `server` says; `ready`: "Hazır mısın?" (a
    /// call from before CALL1: an answer, nothing to join).
    pub kind: CallKind,
    /// Where the call is: projects.gg, or another server with its address (None for `ready`).
    pub server: Option<Game>,
    /// The player's crews the call names, in the player's order (every crew it names for their
    /// own call).
    pub crews: Vec<String>,
    /// Unix seconds on this computer's clock (the service's times moved by the difference
    /// between the two clocks when the call arrived).
    pub started_at: i64,
    pub ends_at: i64,
    /// Who answered what, in the crew's order.
    pub answers: Vec<CallAnswerView>,
    /// The player's own answer (any of their devices).
    pub my_answer: Option<CallAnswer>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CallAnswerView {
    pub id: String,
    pub answer: CallAnswer,
}

/// What "Ekibi çağır" led to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum CallOutcome {
    /// The crew is called (the call is in the view).
    Called,
    /// Called too recently (every 5 minutes, 10 a day): again in this many seconds.
    Wait { seconds: u32 },
    /// A call is open already (the player's own or a teammate's).
    Active,
    /// Not on a multiplayer server (CALL1): nothing was sent, or the service said so.
    NeedsServer,
    /// On another server whose address is not shared (CALL1).
    ServerHidden,
}

/// The player's friend categories, in their order, and who is in which.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FriendGroupsView {
    /// The saved version (0 before the first save).
    pub version: u64,
    pub groups: Vec<FriendGroup>,
    /// Friends without an entry are in the implicit "Arkadaşlar" category.
    pub members: Vec<GroupMember>,
}

/// One friend's category.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct GroupMember {
    pub friend: String,
    pub group: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamView {
    pub id: String,
    pub name: String,
    pub tag: String,
    /// `sword`, `shield`, … (the launcher draws a bundled icon).
    pub emblem: String,
    /// A key of the crew palette (`gold` from a service without looks).
    pub color: String,
    /// `cut`, `shield`, `banner` or `hex` (`cut` from a service without looks).
    pub frame: String,
    /// Empty when there is none.
    pub motto: String,
    /// `EKP-XXXX-XXXX`
    pub code: String,
    /// `YYYY-MM-DD`
    pub created_on: String,
    /// The player's own role.
    pub role: TeamRole,
    /// The player is one of the crew's moderators.
    pub moderator: bool,
    /// Who may invite, call, remove and edit (defaults from a service without roles: everyone
    /// invites, only the leader removes and edits).
    pub permissions: TeamPermissions,
    /// Founder first, then by joining time; the player is among them.
    pub members: Vec<TeamMemberView>,
    /// Friends the player invited who have not answered yet.
    pub invited: Vec<String>,
    /// How many may be in a team.
    pub limit: u32,
    /// The player's own mute of this crew as it reads now (never muted from a service without
    /// `multi`; `crewNotifications` covers all crews).
    pub notifications: CrewMute,
    /// Projects' moderators reset the crew's name, tag and motto; they stay locked until then
    /// (Unix seconds; None when not locked).
    pub text_locked_until: Option<i64>,
    /// How newcomers with the code or the invite link get in (`open` from a service without
    /// `requests`).
    pub join: JoinMode,
    /// The crew's invite link, `XXXX-XXXX-XXXX`, and its page (a service with `link`).
    pub link: Option<String>,
    pub link_url: Option<String>,
    /// Players who asked to join, oldest first: only for a player whose role may answer them.
    pub requests: Vec<JoinRequestView>,
    /// Seconds the player was on Projects this week with someone of this crew there too, as this
    /// computer kept it ("Birlikte oynanan süre"; 0 while it is off).
    pub together_week: u32,
}

/// Someone who asked to join one of the player's crews.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct JoinRequestView {
    pub id: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub avatar_url: Option<String>,
    /// Unix seconds.
    pub sent_at: i64,
}

/// One of the player's own join requests that waits.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamRequestView {
    pub team_id: String,
    pub name: String,
    pub tag: String,
    pub emblem: String,
    pub color: String,
    pub frame: String,
    pub motto: String,
    pub members: u32,
    /// Unix seconds.
    pub sent_at: i64,
}

/// "Planlar": a time to meet on Projects in one of the player's crews.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanView {
    pub id: String,
    /// The crew's id.
    pub crew: String,
    /// Who made it, and their name as the crew shows it (None when they are not in the list).
    pub by: String,
    pub by_name: Option<String>,
    /// The player made it.
    pub mine: bool,
    /// "Planı sil" may go: the player made it, or leads or moderates the crew.
    pub can_cancel: bool,
    /// The plan's time as the service keeps it (Unix seconds, a whole quarter of an hour): the
    /// time to show.
    pub at: i64,
    /// The same moment and the plan's end on this computer's clock (moved by the difference
    /// between the two clocks): for reminders and for hiding it.
    pub starts_at: i64,
    pub ends_at: i64,
    /// When it was made, on this computer's clock (Unix seconds).
    pub created_at: i64,
    /// Who answered what, in the crew's order.
    pub answers: Vec<PlanAnswerView>,
    pub my_answer: Option<PlanAnswer>,
    /// Its short note ("Bedwars, sesli gelin"; FIX2), when it has one.
    pub note: Option<String>,
    /// The player may write or change its note (they made it; a service with `planNotes`).
    pub can_write_note: bool,
    /// The player may clear its note (they made it, or lead or moderate the crew).
    pub can_clear_note: bool,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PlanAnswerView {
    pub id: String,
    pub answer: PlanAnswer,
}

/// What asking to join a crew led to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum JoinResult {
    /// In the crew now (it is in the view).
    Joined,
    /// The crew approves newcomers: the request waits for its leaders.
    Requested,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamMemberView {
    pub id: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub avatar_url: Option<String>,
    pub role: TeamRole,
    /// A moderator (never the leader).
    pub moderator: bool,
    /// The player themself.
    pub me: bool,
    /// Also on the friend list.
    pub friend: bool,
    /// The player blocked them (they see each other offline).
    pub blocked: bool,
    pub state: PresenceState,
    pub dnd: bool,
    /// Where they play; `projects` means "Katıl". Never another server's address.
    pub game: Option<Game>,
    pub discord_url: String,
    /// Unix seconds.
    pub joined_at: i64,
    /// Seconds the player and they were on Projects together this month, as this computer kept
    /// it ("Birlikte oynanan süre"; 0 while it is off).
    pub together_month: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamInviteView {
    pub team_id: String,
    pub name: String,
    pub tag: String,
    pub emblem: String,
    pub color: String,
    pub frame: String,
    pub motto: String,
    /// How many are in the team.
    pub members: u32,
    /// The friend who invited.
    pub from: ProfileView,
    /// Unix seconds.
    pub sent_at: i64,
}

/// What a team code leads to.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamLookupView {
    pub team_id: String,
    pub name: String,
    pub tag: String,
    pub emblem: String,
    pub color: String,
    pub frame: String,
    pub motto: String,
    pub members: u32,
    pub relation: TeamRelation,
    /// `EKP-XXXX-XXXX`, for joining (empty when it was looked up by its invite link).
    pub code: String,
    /// The invite link it was looked up by (`XXXX-XXXX-XXXX`), for joining.
    pub link: Option<String>,
    /// How newcomers get in: with `approval` "Katıl" sends a join request.
    pub join: JoinMode,
    /// The player's join request to it waits, since then (Unix seconds).
    pub requested_at: Option<i64>,
}

#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MeView {
    pub id: String,
    pub name: String,
    pub username: String,
    /// `discord`, `preset:<slug>` or `minecraft:<texture hash>`.
    pub avatar: String,
    /// Discord picture URL when `avatar` is `discord`.
    pub avatar_url: Option<String>,
    /// The Discord picture (or Discord's default one), for the picker's Discord choice.
    pub discord_avatar_url: String,
    /// `PRJ-XXXX-XXXX`
    pub friend_code: String,
    /// What the QR points to: `<service>/f/PRJ-XXXX-XXXX`.
    pub share_link: Option<String>,
    pub qr: Option<QrPath>,
    pub status: StatusMode,
    pub privacy: Privacy,
    /// The settings backup, when there is one.
    pub backup: Option<SettingsMeta>,
    /// A limited account ("Sınırlı hesap"): only the player sees it.
    pub limited: Option<LimitedView>,
}

/// Someone shown with name and picture (requests, blocked users, code lookups).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileView {
    pub id: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub avatar_url: Option<String>,
}

impl From<&PublicProfile> for ProfileView {
    fn from(profile: &PublicProfile) -> Self {
        Self {
            id: profile.id.clone(),
            name: profile.name().to_owned(),
            username: profile.username.clone(),
            avatar: profile.avatar.clone(),
            avatar_url: profile.discord_avatar_url(128),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FriendView {
    pub id: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub avatar_url: Option<String>,
    /// `YYYY-MM-DD`; empty when not known yet.
    pub since: String,
    /// Coarse "last seen" (`today`, `yesterday`, `week`, `long`); None when hidden.
    pub last_seen: Option<crate::model::LastSeen>,
    pub state: PresenceState,
    /// On do-not-disturb (only while online).
    pub dnd: bool,
    /// Where they play; `projects` means "Katıl". `host` of another server only when shared.
    pub game: Option<Game>,
    /// Their Discord profile, for "Discord'dan mesaj at".
    pub discord_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct RequestView {
    pub id: String,
    pub name: String,
    pub username: String,
    pub avatar: String,
    pub avatar_url: Option<String>,
    /// Unix seconds.
    pub sent_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct InviteView {
    pub id: String,
    /// The friend who sent it.
    pub from: String,
    /// Their name and picture when they are in the friend list.
    pub name: Option<String>,
    pub avatar: Option<String>,
    pub avatar_url: Option<String>,
    /// Unix seconds.
    pub at: i64,
    /// Their server when it can be shared (`projects` = "Katıl").
    pub server: Option<Game>,
    /// It waited (offline or do-not-disturb): shown quietly in the list, never as a toast.
    pub quiet: bool,
}

/// What this launcher tells friends (the player's own card).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "state", rename_all = "camelCase")]
pub enum ActivityView {
    Launcher,
    Playing { host: Option<String> },
    Singleplayer,
}

impl From<&LocalStatus> for ActivityView {
    fn from(status: &LocalStatus) -> Self {
        match status {
            LocalStatus::Launcher => Self::Launcher,
            LocalStatus::Playing { host } => Self::Playing { host: host.clone() },
            LocalStatus::Singleplayer => Self::Singleplayer,
        }
    }
}

/// Who owns a friend code, and how they relate to the player.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LookupView {
    pub profile: ProfileView,
    pub relation: Relation,
    /// The code as the service knows it (`PRJ-XXXX-XXXX`), for sending the request.
    pub code: String,
}

/// Result of sending a request by code.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SendOutcome {
    /// The request waits for them.
    Requested,
    /// They had asked first: friends now.
    Friends,
}

/// Result of a "Birlikte oyna" invite.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum InviteResult {
    /// They saw it right away.
    Delivered,
    /// Offline or on do-not-disturb: it waits for them (up to an hour).
    Queued,
}

impl From<crate::presence::InviteOutcome> for InviteResult {
    fn from(outcome: crate::presence::InviteOutcome) -> Self {
        match outcome {
            crate::presence::InviteOutcome::Delivered => Self::Delivered,
            crate::presence::InviteOutcome::Queued => Self::Queued,
        }
    }
}

/// Why a sign-in did not finish.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SignInFailure {
    /// The browser step took too long, or the link was already used: start again.
    Expired,
    /// The service could not be reached.
    Unavailable,
    Failed,
    /// The account is suspended or banned (`restriction` in the view says until when).
    Restricted,
    /// No new accounts are made right now; this Discord account has none yet.
    SignupsClosed,
}

/// A moment worth telling the player about (toasts, notifications). Views carry the state.
#[derive(Debug, Clone, PartialEq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum SocialNotice {
    /// A live invite (never while on do-not-disturb: those wait as quiet invites).
    Invite {
        invite: InviteView,
    },
    /// Someone sent a friend request.
    Request {
        request: RequestView,
    },
    /// A request was accepted (by them, or on another device): a new friend.
    FriendAdded {
        friend: FriendView,
    },
    /// A friend came online.
    FriendOnline {
        id: String,
    },
    /// A friend started playing on projects.gg.
    FriendOnProjects {
        id: String,
    },
    /// Signed in. `restore` is a settings backup saved by another computer, worth offering.
    SignedIn {
        restore: Option<SettingsMeta>,
    },
    /// A friend invited the player to their team (only while teams are on).
    TeamInvite {
        invite: Box<TeamInviteView>,
    },
    /// The founder removed the player from the team, or ended it.
    TeamGone {
        reason: TeamExit,
        /// The team's name, for the sentence.
        name: Option<String>,
        /// Projects' moderators did it (community rules).
        staff: bool,
    },
    /// A new chat message from someone else, live (never one of the player's own, nor one a sync
    /// brought): the launcher decides whether to say it.
    ChatMessage {
        conv: String,
        message: Box<ChatMessageView>,
        /// The sender's name as the lists show it.
        name: Option<String>,
        /// The crew of a crew's chat.
        crew: Option<String>,
    },
    /// Moderators suspended or banned the account: every session ended.
    Restricted {
        until: Option<i64>,
    },
    /// A teammate called the crew, live (never for the player's own call, on do-not-disturb or
    /// for a call that is only new to this launcher).
    CrewCall {
        call: Box<CrewCallView>,
    },
    /// The caller of a call the player said "Geliyorum" or "Birazdan" to moved: to projects.gg (a
    /// call that was "Hazır mısın?"), or to another server (the call's `server` says where).
    CrewCallJoin {
        call: Box<CrewCallView>,
    },
    /// A teammate (a friend or not) started playing on projects.gg.
    CrewmateOnProjects {
        id: String,
    },
    /// Someone asked to join one of the player's crews, live (only for a player who may answer,
    /// never while every crew or that crew is muted).
    JoinRequest {
        crew: String,
        request: JoinRequestView,
    },
    /// A join request of the player's was taken: they are in that crew now.
    RequestAccepted {
        crew: String,
        name: String,
    },
    /// A crewmate made a plan, live (never the player's own, never while that crew or every crew
    /// is muted).
    CrewPlan {
        plan: Box<PlanView>,
    },
    SignInFailed {
        reason: SignInFailure,
    },
    /// The session ended elsewhere (signed out, revoked, expired or the account deleted).
    SignedOutRemotely,
}

pub(crate) fn me_view(me: &Me, share: Option<(String, Option<QrPath>)>) -> MeView {
    let (share_link, qr) = match share {
        Some((link, qr)) => (Some(link), qr),
        None => (None, None),
    };
    MeView {
        id: me.profile.id.clone(),
        name: me.profile.name().to_owned(),
        username: me.profile.username.clone(),
        avatar: me.profile.avatar.clone(),
        avatar_url: me.profile.discord_avatar_url(128),
        discord_avatar_url: discord_avatar_url(
            &me.profile.id,
            me.discord_avatar_hash.as_deref(),
            128,
        ),
        friend_code: me.friend_code.clone(),
        share_link,
        qr,
        status: me.status,
        privacy: me.privacy,
        backup: me.settings.clone(),
        limited: me.limited.as_ref().map(LimitedView::of),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_call_starts_only_on_a_multiplayer_server() {
        let playing = |host: &str| LocalStatus::Playing {
            host: Some(host.to_owned()),
        };
        for share in [false, true] {
            assert_eq!(
                CallPlace::of(&playing("play.projects.gg"), share),
                CallPlace::Ok
            );
            assert_eq!(
                CallPlace::of(&LocalStatus::Launcher, share),
                CallPlace::NeedsServer
            );
            assert_eq!(
                CallPlace::of(&LocalStatus::Singleplayer, share),
                CallPlace::NeedsServer
            );
            // In the menus, or a LAN world: the server is not known.
            assert_eq!(
                CallPlace::of(&LocalStatus::Playing { host: None }, share),
                CallPlace::NeedsServer
            );
        }
        assert_eq!(
            CallPlace::of(&playing("mc.example.com"), false),
            CallPlace::ServerHidden
        );
        assert_eq!(
            CallPlace::of(&playing("mc.example.com:25570"), true),
            CallPlace::Ok
        );
    }
}
