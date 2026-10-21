//! API data types (JSON is camelCase).

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};

/// What other players may see about someone.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PublicProfile {
    /// Discord user id.
    pub id: String,
    pub username: String,
    pub display_name: Option<String>,
    /// Profile picture choice; see [`Avatar`].
    pub avatar: String,
    /// Discord avatar hash, only when `avatar` is `discord` (None = Discord's default avatar).
    pub avatar_hash: Option<String>,
}

impl PublicProfile {
    /// Display name, falling back to the username.
    pub fn name(&self) -> &str {
        self.display_name.as_deref().unwrap_or(&self.username)
    }

    /// The Discord profile, for the "Discord'dan mesaj at" button.
    pub fn discord_profile_url(&self) -> String {
        format!("https://discord.com/users/{}", self.id)
    }

    pub fn avatar_choice(&self) -> Avatar {
        Avatar::parse(&self.avatar).unwrap_or(Avatar::Discord)
    }

    /// Discord CDN URL of the avatar (or of Discord's default avatar) when `avatar` is `discord`.
    pub fn discord_avatar_url(&self, size: u32) -> Option<String> {
        if self.avatar_choice() != Avatar::Discord {
            return None;
        }
        Some(discord_avatar_url(
            &self.id,
            self.avatar_hash.as_deref(),
            size,
        ))
    }
}

/// `https://cdn.discordapp.com/...` for a user's avatar hash, or the default avatar.
pub fn discord_avatar_url(user_id: &str, avatar_hash: Option<&str>, size: u32) -> String {
    let size = size.clamp(16, 1024).next_power_of_two();
    match avatar_hash {
        Some(hash) => {
            let extension = if hash.starts_with("a_") { "gif" } else { "png" };
            format!("https://cdn.discordapp.com/avatars/{user_id}/{hash}.{extension}?size={size}")
        }
        None => {
            // Default avatars for the new username system: (id >> 22) % 6.
            let index = user_id.parse::<u64>().map_or(0, |id| (id >> 22) % 6);
            format!("https://cdn.discordapp.com/embed/avatars/{index}.png")
        }
    }
}

/// Profile picture choice. Players cannot upload their own images (no moderation needed).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Avatar {
    /// The Discord avatar (from the stored avatar hash).
    Discord,
    /// A picture bundled with the launcher (`preset:<slug>`, slug `[a-z0-9-]{1,32}`).
    Preset(String),
    /// The player's Minecraft head from textures.minecraft.net (`minecraft:<texture hash>`).
    Minecraft(String),
}

impl Avatar {
    pub fn parse(value: &str) -> Option<Self> {
        if value == "discord" {
            return Some(Self::Discord);
        }
        if let Some(slug) = value.strip_prefix("preset:") {
            let valid = (1..=32).contains(&slug.len())
                && slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-');
            return valid.then(|| Self::Preset(slug.to_owned()));
        }
        if let Some(hash) = value.strip_prefix("minecraft:") {
            let valid = (40..=64).contains(&hash.len())
                && hash
                    .bytes()
                    .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b));
            return valid.then(|| Self::Minecraft(hash.to_owned()));
        }
        None
    }

    pub fn to_wire(&self) -> String {
        match self {
            Self::Discord => "discord".to_owned(),
            Self::Preset(slug) => format!("preset:{slug}"),
            Self::Minecraft(hash) => format!("minecraft:{hash}"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum StatusMode {
    Online,
    /// Do not disturb: friends see it; the launcher shows no notifications and invites wait.
    Dnd,
    /// Friends see the player as offline; the player still sees friends.
    Invisible,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Privacy {
    /// Show a server other than projects.gg to friends (default off: "başka sunucuda").
    pub share_server: bool,
    /// Show a coarse "last seen" to friends (default on).
    pub share_last_seen: bool,
}

/// Settings backup metadata, enough for "Bu bilgisayara ayarlarını getireyim mi?".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SettingsMeta {
    pub version: u64,
    /// Unix seconds.
    pub updated_at: i64,
    /// Label of the device that saved it.
    pub device: String,
    /// Bytes.
    pub size: u64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Me {
    #[serde(flatten)]
    pub profile: PublicProfile,
    /// The Discord avatar hash even when another picture is chosen (for the picker).
    pub discord_avatar_hash: Option<String>,
    /// `PRJ-XXXX-XXXX`
    pub friend_code: String,
    pub status: StatusMode,
    pub privacy: Privacy,
    /// `YYYY-MM-DD`
    pub created_on: String,
    pub settings: Option<SettingsMeta>,
    /// A limited account ("Sınırlı hesap"): what is limited, until when (SPAM1). None from an
    /// older service or for an account that is not limited.
    #[serde(default)]
    pub limited: Option<Limited>,
    /// One-time notes from moderators that wait to be shown (MOD1).
    #[serde(default)]
    pub notices: Vec<ModNotice>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum LastSeen {
    Today,
    Yesterday,
    /// Within the last 7 days.
    Week,
    /// Longer ago.
    Long,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Friend {
    #[serde(flatten)]
    pub profile: PublicProfile,
    /// `YYYY-MM-DD`
    pub since: String,
    /// None when hidden by the friend or unknown.
    pub last_seen: Option<LastSeen>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendRequest {
    #[serde(flatten)]
    pub profile: PublicProfile,
    /// Unix seconds.
    pub sent_at: i64,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BlockedUser {
    #[serde(flatten)]
    pub profile: PublicProfile,
    pub since: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Limits {
    pub friends: u32,
    pub outgoing_requests: u32,
    /// Members of a team (None from a service without teams).
    #[serde(default)]
    pub team_members: Option<u32>,
    /// Friend categories (None from a service without them).
    #[serde(default)]
    pub groups: Option<u32>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct FriendsOverview {
    pub friends: Vec<Friend>,
    pub incoming: Vec<FriendRequest>,
    pub outgoing: Vec<FriendRequest>,
    pub blocked: Vec<BlockedUser>,
    pub limits: Limits,
    /// The player's friend categories. None: the service is older than categories (the launcher
    /// hides them); a service with them always sends the document (version 0 before a save).
    #[serde(default)]
    pub groups: Option<FriendGroups>,
    /// The player's team.
    #[serde(default)]
    pub team: Option<Team>,
    /// Friends the player invited to their team who have not answered yet.
    #[serde(default)]
    pub team_invited: Vec<String>,
    /// Invites to other teams, newest first.
    #[serde(default)]
    pub team_invites: Vec<TeamInvite>,
    /// What this service offers of crews beyond the first version (`look`, `calls`, …). An older
    /// service sends none; unknown values are kept but mean nothing to this launcher.
    #[serde(default)]
    pub team_features: Vec<String>,
    /// The player's own crew notification settings (a service with [`TEAM_FEATURE_MUTE`]; also
    /// outside a crew).
    #[serde(default)]
    pub team_notifications: Option<TeamNotifications>,
    /// Every crew of the player in their order, the first being the primary crew (a service with
    /// [`TEAM_FEATURE_MULTI`]). None from an older service: its one crew is `team`.
    #[serde(default)]
    pub crews: Option<Vec<CrewEntry>>,
    /// How many crews a player may be in (a service with [`TEAM_FEATURE_MULTI`]).
    #[serde(default)]
    pub crew_limit: Option<u32>,
    /// What the service offers besides friends and crews: `push`, `chat` (none from an older one).
    #[serde(default)]
    pub features: Vec<String>,
}

/// `features`: crew chat and direct messages (CHAT1).
pub const FEATURE_CHAT: &str = "chat";

/// `teamFeatures`: crew looks (colour, frame, motto, the newer emblems).
pub const TEAM_FEATURE_LOOK: &str = "look";
/// `teamFeatures`: "Ekibi çağır" on the presence socket.
pub const TEAM_FEATURE_CALLS: &str = "calls";
/// `teamFeatures`: moderators and the crew's permissions.
pub const TEAM_FEATURE_ROLES: &str = "roles";
/// `teamFeatures`: the player's crew notification settings ("Sessize al", crew calls on or off).
pub const TEAM_FEATURE_MUTE: &str = "mute";
/// `teamFeatures`: several crews per player (`crews`, `crewLimit`, `multi: true`, the crew order,
/// a mute per crew, `crews` in calls).
pub const TEAM_FEATURE_MULTI: &str = "multi";
/// `teamFeatures`: crew calls start only on a multiplayer server and carry it (CALL1).
pub const TEAM_FEATURE_CALL_SERVER: &str = "callServer";

/// How many crews a player may be in when a service with several crews does not say.
pub const DEFAULT_CREW_LIMIT: u32 = 5;

/// The longest timed mute the service takes; longer is "until I unmute".
pub const MAX_MUTE_SECS: i64 = 30 * 86_400;

/// The player's own mute of one crew (`crews[].notifications`, a service with
/// [`TEAM_FEATURE_MULTI`]): silent until `mute_until`, or until unmuted when that is None. Nobody
/// else sees it; a timed mute ends by itself.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrewMute {
    #[serde(default)]
    pub muted: bool,
    /// Unix seconds (the service's clock).
    #[serde(default)]
    pub mute_until: Option<i64>,
}

impl CrewMute {
    /// Muted at `now` (Unix seconds).
    pub fn muted_at(&self, now: i64) -> bool {
        self.muted && self.mute_until.is_none_or(|until| until > now)
    }

    /// As it reads at `now`: a timed mute that ran out is no mute.
    pub fn at(self, now: i64) -> Self {
        if self.muted_at(now) {
            self
        } else {
            Self::default()
        }
    }
}

/// One of the player's crews as `crews` lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CrewEntry {
    /// The crew as its members see it.
    pub team: Team,
    /// Friends the player invited to this crew who have not answered yet.
    #[serde(default)]
    pub invited: Vec<String>,
    /// The player's own mute of this crew.
    #[serde(default)]
    pub notifications: CrewMute,
}

/// The player's crews in their order: `crews` from a service with several crews, else the one
/// crew an older service sends as `team` (with its `invited`).
pub(crate) fn crews_of(
    crews: Option<Vec<CrewEntry>>,
    team: Option<Team>,
    invited: Vec<String>,
) -> Vec<CrewEntry> {
    crews.unwrap_or_else(|| {
        team.map(|team| CrewEntry {
            team,
            invited,
            notifications: CrewMute::default(),
        })
        .into_iter()
        .collect()
    })
}

/// `PATCH /v1/teams/:id/notifications`: "Sessize al" for one crew until `mute_until` (or until
/// unmuted without it), or "Sesi aç" (`muted: false`).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CrewMuteEdit {
    pub muted: bool,
    /// Unix seconds, in the next 30 days; only with `muted: true`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mute_until: Option<i64>,
}

impl CrewMuteEdit {
    /// A timed mute needs `muted: true` and an end in the next 30 days (the service's rule).
    pub fn valid_at(&self, now: i64) -> bool {
        match self.mute_until {
            None => true,
            Some(until) => self.muted && until > now && until <= now + MAX_MUTE_SECS,
        }
    }
}

/// `teamNotifications`: what the player hears of their crew, on every device of theirs and in
/// any crew. Nobody else sees it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamNotifications {
    /// Crew call notifications ("Ekip çağrıları"), on unless turned off.
    #[serde(default = "calls_on")]
    pub calls: bool,
    /// "Sessize al": every crew notification is silent, until `mute_until` or until the player
    /// unmutes (`mute_until` None).
    #[serde(default)]
    pub muted: bool,
    /// Unix seconds (the service's clock) when a timed mute ends; nothing is sent then.
    #[serde(default)]
    pub mute_until: Option<i64>,
}

fn calls_on() -> bool {
    true
}

impl Default for TeamNotifications {
    fn default() -> Self {
        Self {
            calls: true,
            muted: false,
            mute_until: None,
        }
    }
}

impl TeamNotifications {
    /// Muted at `now` (Unix seconds): a timed mute ends by itself.
    pub fn muted_at(&self, now: i64) -> bool {
        self.muted && self.mute_until.is_none_or(|until| until > now)
    }

    /// Crew calls are announced at `now`: switched on and not muted.
    pub fn calls_at(&self, now: i64) -> bool {
        self.calls && !self.muted_at(now)
    }

    /// As it reads at `now`: a timed mute that ran out is no mute.
    pub fn at(self, now: i64) -> Self {
        if self.muted_at(now) {
            self
        } else {
            Self {
                muted: false,
                mute_until: None,
                ..self
            }
        }
    }
}

/// `PATCH /v1/teams/notifications`: what changes. `mute_until` goes only with `muted: true`
/// (without it the mute lasts until the player unmutes).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TeamNotificationsEdit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub calls: Option<bool>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub muted: Option<bool>,
    /// Unix seconds, in the next 30 days.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub mute_until: Option<i64>,
}

impl TeamNotificationsEdit {
    pub fn is_empty(&self) -> bool {
        self.calls.is_none() && self.muted.is_none() && self.mute_until.is_none()
    }

    /// A timed mute needs `muted: true` and an end in the next 30 days (the service's rule).
    pub fn valid_at(&self, now: i64) -> bool {
        match self.mute_until {
            None => true,
            Some(until) => self.muted == Some(true) && until > now && until <= now + MAX_MUTE_SECS,
        }
    }
}

/// One of the player's friend categories ("Okul", "PvP").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct FriendGroup {
    /// 1-12 lower case letters or digits, made by the launcher.
    pub id: String,
    /// 1-24 characters.
    pub name: String,
}

/// The player's friend categories, in their order, and who is in which (friend id → group id).
/// Friends without an entry are in the implicit "Arkadaşlar" category. Seen by nobody else.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct FriendGroups {
    /// Grows with every save; 0 before the first.
    #[serde(default)]
    pub version: u64,
    pub groups: Vec<FriendGroup>,
    pub members: BTreeMap<String, String>,
}

/// At most this many categories (the service refuses more).
pub const MAX_FRIEND_GROUPS: usize = 20;

/// The wire keeps the first names: players see "Lider" (leader) for `founder`. A role this
/// version does not know reads as `Member`, so a newer service never breaks the friend list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    /// The leader: edits the crew, renews its code, removes members, hands the role on,
    /// disbands it.
    Founder,
    /// Invites friends, may leave.
    Member,
}

impl<'de> Deserialize<'de> for TeamRole {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "founder" => Self::Founder,
            _ => Self::Member,
        })
    }
}

/// A crew's colour, frame and motto before looks existed (and from a service without them).
pub const DEFAULT_TEAM_COLOR: &str = "gold";
pub const DEFAULT_TEAM_FRAME: &str = "cut";

/// The emblems every service accepts; the other 16 came with crew looks ([`TEAM_FEATURE_LOOK`]),
/// and a service without them refuses those.
pub const FIRST_EMBLEMS: [&str; 20] = [
    "sword", "shield", "crown", "flame", "bolt", "star", "skull", "diamond", "heart", "rocket",
    "tree", "paw", "ghost", "cube", "axe", "trophy", "moon", "flag", "target", "planet",
];

fn default_team_color() -> String {
    DEFAULT_TEAM_COLOR.to_owned()
}

fn default_team_frame() -> String {
    DEFAULT_TEAM_FRAME.to_owned()
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMember {
    #[serde(flatten)]
    pub profile: PublicProfile,
    pub role: TeamRole,
    /// A moderator ("Moderatör"): a member the leader gave more rights ([`TEAM_FEATURE_ROLES`];
    /// at most 4, never the leader).
    #[serde(default)]
    pub moderator: bool,
    /// Unix seconds.
    pub joined_at: i64,
}

/// Who may do something in a crew: the lowest role that may (every role above it may too).
/// A level this version does not know reads as `Leader`: the button stays away and the service
/// decides anyway.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum PermissionLevel {
    Members,
    Moderators,
    Leader,
}

impl<'de> Deserialize<'de> for PermissionLevel {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "members" => Self::Members,
            "moderators" => Self::Moderators,
            _ => Self::Leader,
        })
    }
}

/// A crew's permissions (a service with [`TEAM_FEATURE_ROLES`]). A service without them sends
/// none: everyone invites (and would call), only the leader removes and edits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct TeamPermissions {
    /// Invite friends to the crew (`members`, `moderators` or `leader`).
    #[serde(default = "PermissionLevel::members")]
    pub invite: PermissionLevel,
    /// "Ekibi çağır" (`members`, `moderators` or `leader`).
    #[serde(default = "PermissionLevel::members")]
    pub call: PermissionLevel,
    /// Remove plain members (`moderators` or `leader`; the code is renewed with it).
    #[serde(default = "PermissionLevel::leader")]
    pub kick: PermissionLevel,
    /// Edit the emblem, colour, frame and motto (`moderators` or `leader`; the name and the tag
    /// are always the leader's).
    #[serde(default = "PermissionLevel::leader")]
    pub edit: PermissionLevel,
}

impl PermissionLevel {
    fn members() -> Self {
        Self::Members
    }

    fn leader() -> Self {
        Self::Leader
    }

    /// Whether a crew member with this role has the right.
    pub fn allows(self, role: TeamRole, moderator: bool) -> bool {
        match self {
            Self::Members => true,
            Self::Moderators => role == TeamRole::Founder || moderator,
            Self::Leader => role == TeamRole::Founder,
        }
    }
}

impl Default for TeamPermissions {
    fn default() -> Self {
        Self {
            invite: PermissionLevel::Members,
            call: PermissionLevel::Members,
            kick: PermissionLevel::Leader,
            edit: PermissionLevel::Leader,
        }
    }
}

/// A change of permissions (`PATCH /v1/teams/:id {"permissions": {…}}`, leader); unset rights
/// stay as they are.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
pub struct PermissionsEdit {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub invite: Option<PermissionLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub call: Option<PermissionLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kick: Option<PermissionLevel>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edit: Option<PermissionLevel>,
}

impl PermissionsEdit {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }
}

/// A team ("Ekip" to players) as its members see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    pub name: String,
    /// 2-4 letters or digits, upper case.
    pub tag: String,
    /// One of the service's emblem names (`sword`, `shield`, …), drawn with a bundled icon.
    pub emblem: String,
    /// A key of the launcher's crew palette (`gold`, `teal`, …).
    #[serde(default = "default_team_color")]
    pub color: String,
    /// The shape around the emblem (`cut`, `shield`, `banner`, `hex`).
    #[serde(default = "default_team_frame")]
    pub frame: String,
    /// Up to 40 characters; empty when there is none.
    #[serde(default)]
    pub motto: String,
    /// `TKM-XXXX-XXXX` (the launcher shows `EKP-XXXX-XXXX`).
    pub code: String,
    /// `YYYY-MM-DD`
    pub created_on: String,
    /// Founder first, then by joining time.
    pub members: Vec<TeamMember>,
    /// Who may invite, call, remove and edit ([`TeamPermissions::default`] from a service
    /// without roles).
    #[serde(default)]
    pub permissions: TeamPermissions,
    /// Projects' moderators reset the crew's name, tag and motto: they stay locked until then
    /// (Unix seconds; MOD1).
    #[serde(default)]
    pub text_locked_until: Option<i64>,
}

/// What an invite or a code shows of a team before joining it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSummary {
    pub id: String,
    pub name: String,
    pub tag: String,
    pub emblem: String,
    #[serde(default = "default_team_color")]
    pub color: String,
    #[serde(default = "default_team_frame")]
    pub frame: String,
    #[serde(default)]
    pub motto: String,
    /// How many are in it.
    pub members: u32,
}

/// A crew's colour, frame and motto, sent only to a service that offers looks
/// ([`TEAM_FEATURE_LOOK`]): an older one refuses keys it does not know.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct TeamLook {
    pub color: String,
    pub frame: String,
    /// Empty: no motto.
    pub motto: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamInvite {
    pub team: TeamSummary,
    pub from: PublicProfile,
    /// Unix seconds.
    pub sent_at: i64,
}

/// How a team code relates to the player.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum TeamRelation {
    None,
    /// Already in this team.
    Member,
    /// In another team: leave it first.
    Elsewhere,
    /// The player blocked someone in it.
    Blocked,
    /// No room left.
    Full,
    /// The player is in as many crews as they may be (only when the lookup asked with
    /// `multi: true`).
    Limit,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamLookup {
    pub team: TeamSummary,
    pub relation: TeamRelation,
}

/// What team calls that change the player's team answer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamAnswer {
    pub team: Option<Team>,
    #[serde(default)]
    pub invited: Vec<String>,
}

/// `GET /v1/teams`: the team part of the friend list.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamsOverview {
    pub team: Option<Team>,
    #[serde(default)]
    pub invited: Vec<String>,
    #[serde(default)]
    pub team_invites: Vec<TeamInvite>,
    #[serde(default)]
    pub team_features: Vec<String>,
    #[serde(default)]
    pub team_notifications: Option<TeamNotifications>,
    /// Every crew in the player's order (a service with [`TEAM_FEATURE_MULTI`]).
    #[serde(default)]
    pub crews: Option<Vec<CrewEntry>>,
    #[serde(default)]
    pub crew_limit: Option<u32>,
}

/// Why the player is no longer in their team (presence `{"t":"team","team":null}`). A reason this
/// version does not know reads as `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum TeamExit {
    Left,
    /// The founder removed them.
    Kicked,
    /// The founder ended the team.
    Disbanded,
    Other,
}

impl<'de> Deserialize<'de> for TeamExit {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "left" => Self::Left,
            "kicked" => Self::Kicked,
            "disbanded" => Self::Disbanded,
            _ => Self::Other,
        })
    }
}

/// `PATCH /v1/teams/:id`; unset fields stay as they are. Colour, frame and motto only go to a
/// service that offers looks, permissions to one that offers roles; `motto: Some("")` takes the
/// motto away.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TeamEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emblem: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub color: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub frame: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub motto: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub permissions: Option<PermissionsEdit>,
}

impl TeamEdit {
    /// Nothing would change.
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    /// It changes a text the service counts (name, tag, motto: 10 a day per crew).
    pub fn changes_text(&self) -> bool {
        self.name.is_some() || self.tag.is_some() || self.motto.is_some()
    }

    /// Without what a service would refuse: the look without looks, permissions without roles
    /// (and an empty permissions change).
    pub fn limited_to(self, look: bool, roles: bool) -> Self {
        let mut edit = self;
        if !look {
            edit.color = None;
            edit.frame = None;
            edit.motto = None;
        }
        if !roles || edit.permissions.is_some_and(|p| p.is_empty()) {
            edit.permissions = None;
        }
        edit
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Relation {
    None,
    Friend,
    /// You already sent them a request.
    Outgoing,
    /// They sent you a request.
    Incoming,
    /// You blocked them.
    Blocked,
    /// Your own code.
    #[serde(rename = "self")]
    Myself,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CodeLookup {
    pub user: PublicProfile,
    pub relation: Relation,
}

/// Result of sending a request by code.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "status", rename_all = "lowercase")]
pub enum RequestOutcome {
    /// The request is pending.
    Requested {
        user: PublicProfile,
        #[serde(rename = "sentAt")]
        sent_at: i64,
    },
    /// They had already asked you: you are friends now.
    Friends { friend: Friend },
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SessionInfo {
    /// Public id, for revoking.
    pub id: String,
    pub device: String,
    pub created_on: String,
    pub last_used_on: String,
    /// This device.
    pub current: bool,
}

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsBackup {
    pub version: u64,
    pub updated_at: i64,
    pub device: String,
    pub data: serde_json::Value,
}

/// Which settings version a write expects to replace.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SettingsPrecondition {
    /// First upload: fails if a backup already exists.
    Create,
    /// Replace exactly this version: fails if another device saved in between.
    Replace(u64),
}

/// `PATCH /v1/me`; unset fields stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct ProfileUpdate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub avatar: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub status: Option<StatusMode>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_server: Option<bool>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_last_seen: Option<bool>,
}

impl ProfileUpdate {
    pub fn avatar(mut self, avatar: &Avatar) -> Self {
        self.avatar = Some(avatar.to_wire());
        self
    }

    pub fn status(mut self, status: StatusMode) -> Self {
        self.status = Some(status);
        self
    }

    pub fn share_server(mut self, share: bool) -> Self {
        self.share_server = Some(share);
        self
    }

    pub fn share_last_seen(mut self, share: bool) -> Self {
        self.share_last_seen = Some(share);
        self
    }
}

// --- limited accounts, notices (SPAM1, MOD1) ---------------------------------------------------

/// What a limited account ("Sınırlı hesap", a Discord account younger than the service's
/// threshold) may not do, or does less of. A key this version does not know reads as `Other` and
/// is left out of the view.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LimitedKey {
    /// Founding crews (joining one is fine).
    CrewFound,
    /// Crew invites a day.
    CrewInvites,
    /// Friend and crew code lookups a day (requests and joins by code included).
    CodeLookups,
    /// The settings backup.
    SettingsBackup,
    /// Chat messages a day.
    ChatMessages,
    Other,
}

impl LimitedKey {
    pub fn parse(value: &str) -> Self {
        match value {
            "crew_found" => Self::CrewFound,
            "crew_invites" => Self::CrewInvites,
            "code_lookups" => Self::CodeLookups,
            "settings_backup" => Self::SettingsBackup,
            "chat_messages" => Self::ChatMessages,
            _ => Self::Other,
        }
    }
}

impl<'de> Deserialize<'de> for LimitedKey {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

/// One limit of a limited account (`perDay` for the daily ones).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LimitedItem {
    pub key: LimitedKey,
    #[serde(default)]
    pub per_day: Option<u32>,
}

/// `me.limited`: the account is limited until `until` (Unix seconds), in these ways.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Limited {
    pub until: i64,
    #[serde(default)]
    pub items: Vec<LimitedItem>,
}

/// What a moderators' one-time note says. A kind this version does not know reads as `Other`
/// and is not shown.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum NoticeKind {
    /// "Topluluk kurallarına aykırı davranış nedeniyle uyarı aldın."
    Warning,
    /// The profile picture went back to the default.
    ProfileReset,
    Other,
}

impl<'de> Deserialize<'de> for NoticeKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "warning" => Self::Warning,
            "profile_reset" => Self::ProfileReset,
            _ => Self::Other,
        })
    }
}

/// A report reason, as players pick it and the service names it. One this version does not know
/// reads as `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ReportReason {
    /// An inappropriate name, tag or motto.
    Name,
    /// An inappropriate profile picture.
    Profile,
    /// Harassment or bullying.
    Harassment,
    /// An inappropriate message.
    Inappropriate,
    Spam,
    Scam,
    Other,
}

impl ReportReason {
    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Name => "name",
            Self::Profile => "profile",
            Self::Harassment => "harassment",
            Self::Inappropriate => "inappropriate",
            Self::Spam => "spam",
            Self::Scam => "scam",
            Self::Other => "other",
        }
    }
}

impl<'de> Deserialize<'de> for ReportReason {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "name" => Self::Name,
            "profile" => Self::Profile,
            "harassment" => Self::Harassment,
            "inappropriate" => Self::Inappropriate,
            "spam" => Self::Spam,
            "scam" => Self::Scam,
            _ => Self::Other,
        })
    }
}

/// A one-time note from Projects' moderators (`me.notices`, `{"t":"notice"}`): shown once, on the
/// device that sees it first, then marked seen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModNotice {
    pub id: String,
    pub kind: NoticeKind,
    /// Unix seconds.
    #[serde(default)]
    pub at: i64,
    /// What the moderators acted on (a report reason).
    #[serde(default)]
    pub reason: Option<ReportReason>,
}

// --- chat (CHAT1) -------------------------------------------------------------------------------

/// A chat message as the service seals it: crew chat `c:<team id>`, direct `d:<id>:<id>`. `id` is
/// the hub's clock in milliseconds when it took the message, the order key within a conversation
/// (an edit keeps it); `mac` proves the version is genuine and goes back unchanged in an edit, a
/// delete or a report. `nonce` (the sender's own key) reaches only the sender's devices.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    /// Missing inside a sync answer (the conversation is said once there).
    #[serde(default)]
    pub conv: String,
    pub id: i64,
    pub from: String,
    pub text: String,
    pub mac: String,
    /// Milliseconds, when the message was edited ("düzenlendi").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
}

impl ChatMessage {
    /// The message as the service wants it back (an edit, a delete, a report): without the
    /// sender's own nonce.
    pub fn sealed(&self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "conv": self.conv,
            "id": self.id,
            "from": self.from,
            "text": self.text,
            "mac": self.mac,
        });
        if let Some(at) = self.edited_at {
            value["editedAt"] = at.into();
        }
        value
    }
}

/// A deletion in a sync answer.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatDeletion {
    pub id: i64,
    /// Who deleted it: a user id, `staff`, or empty once that account is gone.
    #[serde(default)]
    pub by: String,
    #[serde(default)]
    pub at: i64,
}

/// `hello.chat` / `chat_state`: the service's switch, the mailbox, the player's daily limit and a
/// moderator's mute.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatStatus {
    #[serde(default)]
    pub on: bool,
    #[serde(default = "mailbox_on")]
    pub mailbox: bool,
    /// Unix seconds (the hub's clock) when the mailbox opens again; None while unknown.
    #[serde(default)]
    pub mailbox_back: Option<i64>,
    #[serde(default = "chat_per_day")]
    pub per_day: u32,
    /// Unix seconds (the hub's clock): the player may not send until then.
    #[serde(default)]
    pub muted_until: Option<i64>,
}

fn mailbox_on() -> bool {
    true
}

fn chat_per_day() -> u32 {
    500
}

/// Why the service turned a chat text away (`chat_rejected`). One this version does not know
/// reads as `Words` (the text can't be sent as it is, nothing more to say).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ChatRejection {
    Link,
    Phone,
    Words,
}

impl ChatRejection {
    pub fn parse(value: &str) -> Self {
        match value {
            "link" => Self::Link,
            "phone" => Self::Phone,
            _ => Self::Words,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_me_and_friends() {
        let me: Me = serde_json::from_str(
            r#"{"id":"123456789012345678","username":"ayse","displayName":"Ayşe","avatar":"preset:tilki",
                "avatarHash":null,"discordAvatarHash":"0123456789abcdef0123456789abcdef",
                "friendCode":"PRJ-ABCD-EFGH","status":"dnd",
                "privacy":{"shareServer":false,"shareLastSeen":true},"createdOn":"2026-10-16",
                "settings":{"version":2,"updatedAt":1760000000,"device":"Ev PC","size":512}}"#,
        )
        .unwrap();
        assert_eq!(me.profile.name(), "Ayşe");
        assert_eq!(me.status, StatusMode::Dnd);
        assert_eq!(me.profile.avatar_choice(), Avatar::Preset("tilki".into()));
        assert_eq!(me.settings.unwrap().device, "Ev PC");

        let overview: FriendsOverview = serde_json::from_str(
            r#"{"friends":[{"id":"1","username":"b","displayName":null,"avatar":"discord","avatarHash":null,
                "since":"2026-10-01","lastSeen":"yesterday"}],
                "incoming":[],"outgoing":[{"id":"2","username":"c","displayName":null,"avatar":"discord",
                "avatarHash":null,"sentAt":1760000000}],"blocked":[],
                "limits":{"friends":200,"outgoingRequests":50}}"#,
        )
        .unwrap();
        assert_eq!(overview.friends[0].last_seen, Some(LastSeen::Yesterday));
        assert_eq!(overview.outgoing[0].sent_at, 1_760_000_000);

        let outcome: RequestOutcome = serde_json::from_str(
            r#"{"status":"requested","user":{"id":"2","username":"c","displayName":null,
                "avatar":"discord","avatarHash":null},"sentAt":5}"#,
        )
        .unwrap();
        assert!(matches!(
            outcome,
            RequestOutcome::Requested { sent_at: 5, .. }
        ));
        let lookup: CodeLookup = serde_json::from_str(
            r#"{"user":{"id":"2","username":"c","displayName":null,"avatar":"discord","avatarHash":null},
                "relation":"self"}"#,
        )
        .unwrap();
        assert_eq!(lookup.relation, Relation::Myself);
    }

    #[test]
    fn avatars() {
        assert_eq!(Avatar::parse("discord"), Some(Avatar::Discord));
        assert_eq!(
            Avatar::parse("preset:kirmizi-tilki"),
            Some(Avatar::Preset("kirmizi-tilki".into()))
        );
        assert_eq!(Avatar::parse("preset:Büyük"), None);
        assert_eq!(Avatar::parse("preset:"), None);
        let hash = "ab12".repeat(16);
        assert_eq!(
            Avatar::parse(&format!("minecraft:{hash}")).map(|a| a.to_wire()),
            Some(format!("minecraft:{hash}"))
        );
        assert_eq!(Avatar::parse("https://example.com/a.png"), None);
        assert_eq!(
            discord_avatar_url("80351110224678912", Some("a_abc"), 100),
            "https://cdn.discordapp.com/avatars/80351110224678912/a_abc.gif?size=128"
        );
        assert!(discord_avatar_url("80351110224678912", None, 64).ends_with(".png"));
    }

    #[test]
    fn parses_categories_and_teams_and_older_services() {
        let overview: FriendsOverview = serde_json::from_str(
            r#"{"friends":[],"incoming":[],"outgoing":[],"blocked":[],
                "limits":{"friends":200,"outgoingRequests":50,"teamMembers":30,"groups":20},
                "groups":{"version":3,"groups":[{"id":"okul","name":"Okul"}],
                  "members":{"223456789012345678":"okul"}},
                "team":{"id":"0123456789abcdef","name":"Gece Kartalları","tag":"GKR",
                  "emblem":"sword","code":"TKM-ABCD-EFGH","createdOn":"2026-10-19",
                  "members":[{"id":"1","username":"a","displayName":null,"avatar":"discord",
                    "avatarHash":null,"role":"founder","joinedAt":5}]},
                "teamInvited":["2"],
                "teamInvites":[{"team":{"id":"fedcba9876543210","name":"PvP","tag":"PVP",
                  "emblem":"bolt","members":4},"from":{"id":"3","username":"c",
                  "displayName":null,"avatar":"discord","avatarHash":null},"sentAt":9}]}"#,
        )
        .unwrap();
        let groups = overview.groups.unwrap();
        assert_eq!(groups.version, 3);
        assert_eq!(groups.members["223456789012345678"], "okul");
        let team = overview.team.unwrap();
        assert_eq!(team.members[0].role, TeamRole::Founder);
        assert_eq!(team.members[0].profile.name(), "a");
        assert_eq!(overview.team_invited, vec!["2".to_owned()]);
        assert_eq!(overview.team_invites[0].team.members, 4);
        assert_eq!(overview.limits.team_members, Some(30));

        // An older service: no categories, no team, nothing breaks.
        let older: FriendsOverview = serde_json::from_str(
            r#"{"friends":[],"incoming":[],"outgoing":[],"blocked":[],
                "limits":{"friends":200,"outgoingRequests":50}}"#,
        )
        .unwrap();
        assert!(older.groups.is_none() && older.team.is_none());
        assert_eq!(older.limits.team_members, None);

        let exit: TeamExit = serde_json::from_str(r#""someday""#).unwrap();
        assert_eq!(exit, TeamExit::Other);
        assert_eq!(
            serde_json::to_string(&TeamEdit {
                emblem: Some("crown".into()),
                ..TeamEdit::default()
            })
            .unwrap(),
            r#"{"emblem":"crown"}"#
        );
    }

    #[test]
    fn crew_looks_and_features_with_and_without_them() {
        let overview: FriendsOverview = serde_json::from_str(
            r#"{"friends":[],"incoming":[],"outgoing":[],"blocked":[],
                "limits":{"friends":200,"outgoingRequests":50,"teamMembers":30,"groups":20},
                "team":{"id":"0a1b2c3d4e5f6a7b","name":"Gece Kartalları","tag":"GKR",
                  "emblem":"controller","color":"teal","frame":"banner","motto":"Birlikte güçlüyüz!",
                  "code":"TKM-ABCD-EFGH","createdOn":"2026-10-20",
                  "members":[{"id":"1","username":"a","displayName":null,"avatar":"discord",
                    "avatarHash":null,"role":"founder","joinedAt":5},
                    {"id":"2","username":"b","displayName":null,"avatar":"discord",
                    "avatarHash":null,"role":"officer","joinedAt":6}]},
                "teamInvites":[{"team":{"id":"fedcba9876543210","name":"PvP","tag":"PVP",
                  "emblem":"robot","color":"crimson","frame":"hex","motto":"","members":4},
                  "from":{"id":"3","username":"c","displayName":null,"avatar":"discord",
                  "avatarHash":null},"sentAt":9}],
                "teamFeatures":["look","calls","plans"]}"#,
        )
        .unwrap();
        let team = overview.team.as_ref().unwrap();
        assert_eq!(
            (
                team.color.as_str(),
                team.frame.as_str(),
                team.motto.as_str()
            ),
            ("teal", "banner", "Birlikte güçlüyüz!")
        );
        // A role this version does not know is a member, never a broken list.
        assert_eq!(team.members[1].role, TeamRole::Member);
        assert_eq!(overview.team_invites[0].team.frame, "hex");
        assert!(
            overview
                .team_features
                .iter()
                .any(|f| f == TEAM_FEATURE_CALLS)
        );

        // The Worker before looks: today's look, no features.
        let older: TeamsOverview = serde_json::from_str(
            r#"{"team":{"id":"0a1b2c3d4e5f6a7b","name":"Gece","tag":"GKR","emblem":"sword",
                "code":"TKM-ABCD-EFGH","createdOn":"2026-10-20","members":[]},"invited":[],
                "teamInvites":[]}"#,
        )
        .unwrap();
        let team = older.team.unwrap();
        assert_eq!(
            (
                team.color.as_str(),
                team.frame.as_str(),
                team.motto.as_str()
            ),
            (DEFAULT_TEAM_COLOR, DEFAULT_TEAM_FRAME, "")
        );
        assert!(older.team_features.is_empty());
        let summary: TeamLookup = serde_json::from_str(
            r#"{"team":{"id":"x","name":"PvP","tag":"PVP","emblem":"bolt","members":3},
                "relation":"none"}"#,
        )
        .unwrap();
        assert_eq!(summary.team.color, "gold");

        let edit = TeamEdit {
            color: Some("teal".into()),
            motto: Some(String::new()),
            ..TeamEdit::default()
        };
        assert_eq!(
            serde_json::to_string(&edit).unwrap(),
            r#"{"color":"teal","motto":""}"#
        );
        assert!(edit.changes_text());
        assert!(edit.clone().limited_to(false, true).is_empty());
        assert_eq!(edit.clone().limited_to(true, false), edit);
        assert!(
            !TeamEdit {
                color: Some("teal".into()),
                ..TeamEdit::default()
            }
            .changes_text()
        );
    }

    #[test]
    fn moderators_and_permissions_with_and_without_roles() {
        let team: Team = serde_json::from_str(
            r#"{"id":"0a1b2c3d4e5f6a7b","name":"Gece","tag":"GKR","emblem":"sword",
                "code":"TKM-ABCD-EFGH","createdOn":"2026-10-20",
                "members":[{"id":"1","username":"a","displayName":null,"avatar":"discord",
                  "avatarHash":null,"role":"founder","moderator":false,"joinedAt":5},
                  {"id":"2","username":"b","displayName":null,"avatar":"discord",
                  "avatarHash":null,"role":"member","moderator":true,"joinedAt":6}],
                "permissions":{"invite":"moderators","call":"members","kick":"moderators",
                  "edit":"everyone"}}"#,
        )
        .unwrap();
        assert!(team.members[1].moderator && !team.members[0].moderator);
        assert_eq!(team.permissions.invite, PermissionLevel::Moderators);
        assert_eq!(team.permissions.call, PermissionLevel::Members);
        // A level this version does not know keeps the right with the leader.
        assert_eq!(team.permissions.edit, PermissionLevel::Leader);
        assert!(PermissionLevel::Members < PermissionLevel::Moderators);

        // A service without roles: nobody is a moderator, everyone invites, the leader removes.
        let older: Team = serde_json::from_str(
            r#"{"id":"x","name":"Gece","tag":"GKR","emblem":"sword","code":"TKM-ABCD-EFGH",
                "createdOn":"2026-10-20","members":[{"id":"1","username":"a","displayName":null,
                "avatar":"discord","avatarHash":null,"role":"founder","joinedAt":5}]}"#,
        )
        .unwrap();
        assert!(!older.members[0].moderator);
        assert_eq!(older.permissions, TeamPermissions::default());
        assert_eq!(older.permissions.kick, PermissionLevel::Leader);

        let edit = TeamEdit {
            permissions: Some(PermissionsEdit {
                edit: Some(PermissionLevel::Moderators),
                ..PermissionsEdit::default()
            }),
            ..TeamEdit::default()
        };
        assert_eq!(
            serde_json::to_string(&edit).unwrap(),
            r#"{"permissions":{"edit":"moderators"}}"#
        );
        assert!(!edit.changes_text());
        assert!(edit.clone().limited_to(true, false).is_empty());
        let nothing = TeamEdit {
            permissions: Some(PermissionsEdit::default()),
            ..TeamEdit::default()
        };
        assert!(nothing.limited_to(true, true).is_empty());
    }

    #[test]
    fn crew_mutes_end_by_themselves_and_go_out_as_little_as_changes() {
        let overview: TeamsOverview = serde_json::from_str(
            r#"{"team":null,"invited":[],"teamInvites":[],"teamFeatures":["look","mute"],
                "teamNotifications":{"calls":false,"muted":true,"muteUntil":1000}}"#,
        )
        .unwrap();
        let settings = overview.team_notifications.unwrap();
        assert!(!settings.calls && settings.muted_at(999) && !settings.muted_at(1000));
        assert!(!settings.calls_at(999));
        // A timed mute that ran out reads as no mute; calls keep their own switch.
        assert_eq!(
            settings.at(1000),
            TeamNotifications {
                calls: false,
                muted: false,
                mute_until: None
            }
        );
        let forever = TeamNotifications {
            muted: true,
            ..TeamNotifications::default()
        };
        assert!(forever.muted_at(i64::MAX - 1) && !forever.calls_at(0));
        assert!(TeamNotifications::default().calls_at(0));
        // An older service sends none; one with half of it gets the defaults for the rest.
        let older: TeamsOverview =
            serde_json::from_str(r#"{"team":null,"invited":[],"teamInvites":[]}"#).unwrap();
        assert_eq!(older.team_notifications, None);
        let partial: TeamNotifications = serde_json::from_str(r#"{"muted":false}"#).unwrap();
        assert_eq!(partial, TeamNotifications::default());

        let mute = TeamNotificationsEdit {
            muted: Some(true),
            mute_until: Some(4_600),
            ..TeamNotificationsEdit::default()
        };
        assert_eq!(
            serde_json::to_string(&mute).unwrap(),
            r#"{"muted":true,"muteUntil":4600}"#
        );
        assert!(mute.valid_at(1_000) && !mute.valid_at(4_600));
        assert!(!mute.valid_at(4_600 - MAX_MUTE_SECS - 1));
        let until_unmuted = TeamNotificationsEdit {
            muted: Some(true),
            ..TeamNotificationsEdit::default()
        };
        assert_eq!(
            serde_json::to_string(&until_unmuted).unwrap(),
            r#"{"muted":true}"#
        );
        let stray = TeamNotificationsEdit {
            muted: Some(false),
            mute_until: Some(4_600),
            ..TeamNotificationsEdit::default()
        };
        assert!(!stray.valid_at(1_000));
        let calls = TeamNotificationsEdit {
            calls: Some(false),
            ..TeamNotificationsEdit::default()
        };
        assert_eq!(serde_json::to_string(&calls).unwrap(), r#"{"calls":false}"#);
        assert!(TeamNotificationsEdit::default().is_empty() && !calls.is_empty());
    }

    #[test]
    fn several_crews_come_in_the_players_order_with_their_mutes() {
        let crew = |id: &str, name: &str| {
            format!(
                r#"{{"id":"{id}","name":"{name}","tag":"GKR","emblem":"sword","code":"TKM-ABCD-EFGH",
                "createdOn":"2026-10-20","members":[]}}"#
            )
        };
        let overview: FriendsOverview = serde_json::from_str(&format!(
            r#"{{"friends":[],"incoming":[],"outgoing":[],"blocked":[],
                "limits":{{"friends":200,"outgoingRequests":50,"teamMembers":30}},
                "team":{a},"teamInvited":["2"],
                "crews":[{{"team":{a},"invited":["2"],"notifications":{{"muted":false,"muteUntil":null}}}},
                  {{"team":{b},"notifications":{{"muted":true,"muteUntil":1000}}}}],
                "crewLimit":5,"teamFeatures":["look","calls","roles","mute","multi","callServer"]}}"#,
            a = crew("0a1b2c3d4e5f6a7b", "Gece Kartalları"),
            b = crew("9f8e7d6c5b4a3f2e", "PvP"),
        ))
        .unwrap();
        assert_eq!(overview.crew_limit, Some(5));
        let crews = crews_of(overview.crews, overview.team, overview.team_invited);
        assert_eq!(
            crews
                .iter()
                .map(|c| c.team.name.as_str())
                .collect::<Vec<_>>(),
            vec!["Gece Kartalları", "PvP"]
        );
        assert_eq!(crews[0].invited, vec!["2".to_owned()]);
        assert!(crews[1].invited.is_empty());
        let mute = crews[1].notifications;
        assert!(mute.muted_at(999) && !mute.muted_at(1000));
        assert_eq!(mute.at(1000), CrewMute::default());
        assert_eq!(mute.at(999), mute);

        // A service with one crew per player: its `team` is the list.
        let older: TeamsOverview = serde_json::from_str(&format!(
            r#"{{"team":{a},"invited":["3"],"teamInvites":[]}}"#,
            a = crew("0a1b2c3d4e5f6a7b", "Gece Kartalları")
        ))
        .unwrap();
        assert_eq!(older.crew_limit, None);
        let crews = crews_of(older.crews, older.team, older.invited);
        assert_eq!(crews.len(), 1);
        assert_eq!(crews[0].invited, vec!["3".to_owned()]);
        assert!(crews_of(None, None, vec![]).is_empty());

        let limit: TeamLookup = serde_json::from_str(
            r#"{"team":{"id":"x","name":"PvP","tag":"PVP","emblem":"bolt","members":3},
                "relation":"limit"}"#,
        )
        .unwrap();
        assert_eq!(limit.relation, TeamRelation::Limit);
    }

    #[test]
    fn one_crews_mute_goes_out_as_the_service_takes_it() {
        let timed = CrewMuteEdit {
            muted: true,
            mute_until: Some(4_600),
        };
        assert_eq!(
            serde_json::to_string(&timed).unwrap(),
            r#"{"muted":true,"muteUntil":4600}"#
        );
        assert!(timed.valid_at(1_000) && !timed.valid_at(4_600));
        assert!(!timed.valid_at(4_600 - MAX_MUTE_SECS - 1));
        let forever = CrewMuteEdit {
            muted: true,
            mute_until: None,
        };
        assert_eq!(
            serde_json::to_string(&forever).unwrap(),
            r#"{"muted":true}"#
        );
        let off = CrewMuteEdit {
            muted: false,
            mute_until: None,
        };
        assert_eq!(serde_json::to_string(&off).unwrap(), r#"{"muted":false}"#);
        assert!(off.valid_at(0));
        let stray = CrewMuteEdit {
            muted: false,
            mute_until: Some(4_600),
        };
        assert!(!stray.valid_at(1_000));
    }

    #[test]
    fn profile_update_skips_unset_fields() {
        let update = ProfileUpdate::default()
            .status(StatusMode::Invisible)
            .share_last_seen(false);
        assert_eq!(
            serde_json::to_string(&update).unwrap(),
            r#"{"status":"invisible","shareLastSeen":false}"#
        );
    }
}
