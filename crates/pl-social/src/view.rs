//! What the launcher UI receives: one [`SocialView`] after every change, and [`SocialNotice`]s
//! for moments worth a toast or a notification. camelCase JSON; the launcher exports the
//! TypeScript types with tauri-specta. Nothing here carries a token or an internal error.

use serde::{Deserialize, Serialize};

use crate::model::{
    FriendGroup, Limits, Me, Privacy, PublicProfile, Relation, SettingsMeta, StatusMode, TeamExit,
    TeamRelation, TeamRole, discord_avatar_url,
};
use crate::protocol::{Game, LocalStatus, PresenceState};
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
    /// The player's team (only while `teams`).
    pub team: Option<TeamView>,
    /// Invites to teams, newest first (only while `teams`).
    pub team_invites: Vec<TeamInviteView>,
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
    /// `TKM-XXXX-XXXX`
    pub code: String,
    /// `YYYY-MM-DD`
    pub created_on: String,
    /// The player's own role.
    pub role: TeamRole,
    /// Founder first, then by joining time; the player is among them.
    pub members: Vec<TeamMemberView>,
    /// Friends the player invited who have not answered yet.
    pub invited: Vec<String>,
    /// How many may be in a team.
    pub limit: u32,
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
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamInviteView {
    pub team_id: String,
    pub name: String,
    pub tag: String,
    pub emblem: String,
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
    pub members: u32,
    pub relation: TeamRelation,
    /// `TKM-XXXX-XXXX`, for joining.
    pub code: String,
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
    }
}
