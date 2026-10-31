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
    /// The card of the crew tag they wear before their name (TEAM6, a service with
    /// [`TEAM_FEATURE_BADGES`]). It comes only with profiles that reach the player as a friend or
    /// a crewmate (the friend list, the crews' member lists, a friend just added, `me`); every
    /// other profile has none, and neither has someone who wears no tag.
    #[serde(
        default,
        deserialize_with = "readable_badge",
        skip_serializing_if = "Option::is_none"
    )]
    pub badge: Option<CrewBadge>,
    /// The mark Projects' staff gave the account in Studio (SOC-VERIFY): None without one, from a
    /// service without marks, and for a mark this version does not know.
    #[serde(
        default,
        deserialize_with = "readable_mark",
        skip_serializing_if = "Option::is_none"
    )]
    pub mark: Option<ProfileMark>,
}

/// A mark Projects' staff give an account in Studio (SOC-VERIFY), shown right of the name wherever
/// the account shows: the verified mark (blue, "Doğrulanmış hesap") or the admin mark (red,
/// "Projects yöneticisi").
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProfileMark {
    Verified,
    Admin,
}

/// A mark this version cannot read is no mark: the profile around it still reads.
pub(crate) fn readable_mark<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<ProfileMark>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw.and_then(|value| serde_json::from_value(value).ok()))
}

/// The card of a crew tag ("[GK] Ayşe", TEAM6): what a wearer's profile carries. The service makes
/// it from the crew's own row at every read, so it is as new as the list it came with; it never
/// names the crew's members, leader, code or invite link.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrewBadge {
    /// The crew's id.
    pub crew: String,
    /// 2-4 letters or digits, upper case.
    pub tag: String,
    #[serde(default = "default_team_color")]
    pub color: String,
    #[serde(default = "default_team_frame")]
    pub frame: String,
    pub emblem: String,
    pub name: String,
    /// Empty when there is none.
    #[serde(default)]
    pub motto: String,
    /// `YYYY-MM-DD`
    #[serde(default)]
    pub created_on: String,
    /// How many are in the crew.
    #[serde(default)]
    pub members: u32,
    /// The crew takes join requests from its tag's card now (its setting and Studio's are on, it
    /// is not full and its day's 10 are not used up; a service with
    /// [`TEAM_FEATURE_BADGE_REQUESTS`]). It may be stale: pressing then gets a refusal.
    #[serde(default)]
    pub requests: bool,
}

/// A tag's card this version cannot read is no card: the profile around it still reads.
pub(crate) fn readable_badge<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<CrewBadge>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw.and_then(|value| serde_json::from_value(value).ok()))
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
    /// "Görüldü bilgisi" (CHAT2): read marks in direct messages, both ways (off: the player sees
    /// none either). On by default; None from a service without chat extras.
    #[serde(default)]
    pub share_read: Option<bool>,
    /// "Yazıyor bilgisi" (CHAT2): "yazıyor…", both ways. On by default; None from a service
    /// without chat extras.
    #[serde(default)]
    pub share_typing: Option<bool>,
}

impl Privacy {
    /// Read marks are shared (and seen): on unless the player turned them off.
    pub fn shares_read(&self) -> bool {
        self.share_read != Some(false)
    }

    /// "Yazıyor…" is shared (and seen): on unless the player turned it off.
    pub fn shares_typing(&self) -> bool {
        self.share_typing != Some(false)
    }
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
    /// The player's own join requests that wait, newest first (a service with
    /// [`TEAM_FEATURE_REQUESTS`]; at most 3).
    #[serde(default)]
    pub team_requests: Vec<TeamRequest>,
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
    /// SCALE-SOC: the player's profile as `GET /v1/me` gives it, read with the lists (`?rel=1`;
    /// none from an older service).
    #[serde(default)]
    pub me: Option<Me>,
    /// SCALE-SOC: the relations version of this answer (`?rel=1`; none from an older service):
    /// the presence `hello` names the current one after a reconnect, and the launcher reads the
    /// lists again only when the two differ.
    #[serde(default)]
    pub rel: Option<String>,
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
/// `teamFeatures`: the join setting (a crew that approves newcomers), join requests and
/// `permissions.admit` (TEAM5).
pub const TEAM_FEATURE_REQUESTS: &str = "requests";
/// `teamFeatures`: crew invite links and their page (TEAM5).
pub const TEAM_FEATURE_LINK: &str = "link";
/// `teamFeatures`: crew plans on the presence socket (TEAM5).
pub const TEAM_FEATURE_PLANS: &str = "plans";
/// `teamFeatures`: a plan's short note and `plan_note` (FIX2).
pub const TEAM_FEATURE_PLAN_NOTES: &str = "planNotes";
/// `teamFeatures`: a friend request to a crewmate without their code (FIX2).
pub const TEAM_FEATURE_CREW_FRIEND_REQUESTS: &str = "crewFriendRequests";
/// `teamFeatures`: crew tags before players' names (`badge` on profiles, `PATCH /v1/me {badge}`),
/// hidden crews and reports from a tag (TEAM6). Listed only while Studio's "Ekip etiketleri" is on.
pub const TEAM_FEATURE_BADGES: &str = "badges";
/// `teamFeatures`: join requests from a tag's card (`POST /v1/teams/:id/requests {via}`, the
/// crew's "Etiketten istek", `via` on requests; TEAM6). Listed only with [`TEAM_FEATURE_BADGES`]
/// and while Studio's "Etiketten katılma istekleri" is on.
pub const TEAM_FEATURE_BADGE_REQUESTS: &str = "badgeRequests";
/// A plan's note: one line of at most this many characters (code points).
pub const PLAN_NOTE_MAX: usize = 80;

/// A join request waits this long; then it goes without a message (the service's rule).
pub const JOIN_REQUEST_LIFETIME_SECS: i64 = 7 * 86_400;

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
    /// The crew's waiting join requests, oldest first, for a player whose role may answer them
    /// (`permissions.admit`; a service with [`TEAM_FEATURE_REQUESTS`]); empty for everyone else.
    #[serde(default)]
    pub requests: Vec<JoinRequest>,
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
            requests: Vec::new(),
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
    /// Answer join requests and make, renew or switch off the invite link (`moderators` or
    /// `leader`; a service with [`TEAM_FEATURE_REQUESTS`]).
    #[serde(default = "PermissionLevel::moderators")]
    pub admit: PermissionLevel,
}

impl PermissionLevel {
    fn members() -> Self {
        Self::Members
    }

    fn moderators() -> Self {
        Self::Moderators
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
            admit: PermissionLevel::Moderators,
        }
    }
}

/// A change of permissions (`PATCH /v1/teams/:id {"permissions": {…}}`, leader); unset rights
/// stay as they are. `admit` only goes to a service with [`TEAM_FEATURE_REQUESTS`].
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
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub admit: Option<PermissionLevel>,
}

/// How newcomers who come with the crew's code or invite link get in (a service with
/// [`TEAM_FEATURE_REQUESTS`]): at once (`open`: every crew from before), or once the crew's leaders
/// take their request (`approval`). A member's invite lets a friend in either way. A value this
/// version does not know reads as `Approval`: the button asks, the service decides.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum JoinMode {
    #[default]
    Open,
    Approval,
}

impl<'de> Deserialize<'de> for JoinMode {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "open" => Self::Open,
            _ => Self::Approval,
        })
    }
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
    /// How newcomers with the code or the invite link get in (`open` from a service without
    /// [`TEAM_FEATURE_REQUESTS`]).
    #[serde(default)]
    pub join: JoinMode,
    /// The crew's invite link, `XXXX-XXXX-XXXX` (a service with [`TEAM_FEATURE_LINK`]), or none.
    #[serde(default)]
    pub link: Option<String>,
    /// The leader hid the crew (TEAM6, a service with [`TEAM_FEATURE_BADGES`]): its tag shows
    /// nowhere and cannot be worn. Everything else about the crew stays.
    #[serde(default)]
    pub hidden: bool,
    /// "Etiketten istek" (TEAM6, a service with [`TEAM_FEATURE_BADGE_REQUESTS`]): members'
    /// friends may ask to join from the tag's card. On by default, as the service keeps it.
    #[serde(default = "badge_requests_on")]
    pub badge_requests: bool,
}

fn badge_requests_on() -> bool {
    true
}

/// What an invite, a code or a link shows of a team before joining it.
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
    /// How newcomers get in (`open` from an older service).
    #[serde(default)]
    pub join: JoinMode,
}

/// One of the player's own join requests that waits (`teamRequests`; the request of a lookup
/// carries only `sentAt`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamRequest {
    pub team: TeamSummary,
    /// Unix seconds.
    pub sent_at: i64,
}

/// Someone who asked to join one of the player's crews (`crews[].requests`, the
/// `team_requests` socket message).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct JoinRequest {
    pub from: PublicProfile,
    /// Unix seconds.
    pub sent_at: i64,
    /// The member whose tag's card the request came from (TEAM6), while they are still in the
    /// crew: "Ayşe aracılığıyla". None for requests by code or link.
    #[serde(default)]
    pub via: Option<String>,
}

/// The player's join request to a crew they looked up, while it waits.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct WaitingRequest {
    /// Unix seconds.
    pub sent_at: i64,
}

/// What asking to join a crew led to: in at once (an open crew, or a crew that invited the
/// player), or a request that waits for the crew's leaders (a crew that approves newcomers).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum JoinOutcome {
    Joined(TeamAnswer),
    Requested(TeamRequest),
}

/// A crew's invite link as the service made it: `XXXX-XXXX-XXXX` and its page.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct CrewLink {
    pub link: String,
    pub url: String,
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
    /// The player's join request to this crew, while it waits (a service with
    /// [`TEAM_FEATURE_REQUESTS`]).
    #[serde(default)]
    pub request: Option<WaitingRequest>,
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
    /// The player's own join requests that wait (a service with [`TEAM_FEATURE_REQUESTS`]).
    #[serde(default)]
    pub team_requests: Vec<TeamRequest>,
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
/// service that offers looks, permissions to one that offers roles, the join setting and
/// `permissions.admit` to one that offers join requests; `motto: Some("")` takes the motto away.
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
    /// The join setting (the leader; [`TEAM_FEATURE_REQUESTS`]).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub join: Option<JoinMode>,
    /// "Görünürlük": the crew hidden or shown (the leader; [`TEAM_FEATURE_BADGES`], TEAM6).
    /// Counts with the name, tag and motto (10 a day per crew).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hidden: Option<bool>,
    /// "Etiketten istek" (the leader; [`TEAM_FEATURE_BADGE_REQUESTS`], TEAM6). Counts like
    /// `hidden`.
    #[serde(rename = "badgeRequests", skip_serializing_if = "Option::is_none")]
    pub badge_requests: Option<bool>,
}

/// What a service offers that a crew change may use (`teamFeatures`).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct EditFeatures {
    /// [`TEAM_FEATURE_LOOK`]: colour, frame, motto.
    pub look: bool,
    /// [`TEAM_FEATURE_ROLES`]: permissions.
    pub roles: bool,
    /// [`TEAM_FEATURE_REQUESTS`]: the join setting and `permissions.admit`.
    pub requests: bool,
    /// [`TEAM_FEATURE_BADGES`]: the crew's visibility (TEAM6).
    pub badges: bool,
    /// [`TEAM_FEATURE_BADGE_REQUESTS`]: "Etiketten istek" (TEAM6).
    pub badge_requests: bool,
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

    /// Without what a service would refuse: the look without looks, permissions without roles,
    /// the join setting and `admit` without join requests, the visibility without crew tags and
    /// "Etiketten istek" without requests from tags (and an empty permissions change).
    pub fn limited_to(self, offers: EditFeatures) -> Self {
        let mut edit = self;
        if !offers.look {
            edit.color = None;
            edit.frame = None;
            edit.motto = None;
        }
        if !offers.badges {
            edit.hidden = None;
        }
        if !offers.badge_requests {
            edit.badge_requests = None;
        }
        if !offers.requests {
            edit.join = None;
            if let Some(permissions) = &mut edit.permissions {
                permissions.admit = None;
            }
        }
        if !offers.roles || edit.permissions.is_some_and(|p| p.is_empty()) {
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
    /// "Görüldü bilgisi" (CHAT2; an older service turns the field away).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_read: Option<bool>,
    /// "Yazıyor bilgisi" (CHAT2; an older service turns the field away).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub share_typing: Option<bool>,
    /// The crew tag worn (TEAM6): `Some(Some(team id))` puts that crew's on in place of any
    /// other, `Some(None)` takes it off (`null`). Only to a service with [`TEAM_FEATURE_BADGES`]:
    /// an older one turns the field away.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub badge: Option<Option<String>>,
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

    pub fn share_read(mut self, share: bool) -> Self {
        self.share_read = Some(share);
        self
    }

    pub fn share_typing(mut self, share: bool) -> Self {
        self.share_typing = Some(share);
        self
    }

    /// Puts on the tag of crew `team`, or takes the tag off (None; TEAM6).
    pub fn badge(mut self, team: Option<&str>) -> Self {
        self.badge = Some(team.map(str::to_owned));
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
    /// Making or renewing a crew's invite link (TEAM5).
    CrewLink,
    /// Making crew plans (answering them is fine; TEAM5).
    CrewPlans,
    /// Links in chat messages (CHAT2).
    ChatLinks,
    /// Stickers a day (CHAT2).
    ChatStickers,
    /// Reaction changes a day (CHAT2).
    ChatReactions,
    /// Changing the Projects skin (SKIN1; linking is fine).
    SkinChange,
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
            "crew_link" => Self::CrewLink,
            "crew_plans" => Self::CrewPlans,
            "chat_links" => Self::ChatLinks,
            "chat_stickers" => Self::ChatStickers,
            "chat_reactions" => Self::ChatReactions,
            "skin_change" => Self::SkinChange,
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
    /// Staff in game reset the player's Projects skin (SKIN1), perhaps keeping them from changing
    /// it for a while ([`ModNotice::until`]).
    SkinReset,
    Other,
}

impl<'de> Deserialize<'de> for NoticeKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(match String::deserialize(deserializer)?.as_str() {
            "warning" => Self::Warning,
            "profile_reset" => Self::ProfileReset,
            "skin_reset" => Self::SkinReset,
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
    /// `skin_reset` (SKIN1): skin changes stay off until then (Unix seconds; `Some(None)`: for
    /// good). `None`: the note has no `until`, nothing is blocked.
    #[serde(
        default,
        deserialize_with = "present",
        skip_serializing_if = "Option::is_none"
    )]
    pub until: Option<Option<i64>>,
}

/// A time that may be missing (`None`) or `null` (`Some(None)`). Anything else that is not a
/// whole number reads as missing, so one odd field never hides the note.
fn present<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<i64>>, D::Error> {
    Ok(match serde_json::Value::deserialize(deserializer)? {
        serde_json::Value::Null => Some(None),
        serde_json::Value::Number(number) => number.as_i64().map(Some),
        _ => None,
    })
}

// --- chat (CHAT1, CHAT2) -------------------------------------------------------------------------

/// The message a reply answers (CHAT2): its id and sender. The service keeps only this link (with a
/// seal of its own), never the quoted text: a device shows the message from its own history.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ChatReplyRef {
    pub id: i64,
    pub from: String,
}

/// One kind of reaction on a message (CHAT2): a fully qualified emoji and who gave it, in the
/// order they reacted. Only people the player sees are listed (counts may differ between players).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Reaction {
    pub e: String,
    #[serde(default)]
    pub by: Vec<String>,
}

/// A chat message as the service seals it: crew chat `c:<team id>`, direct `d:<id>:<id>`. `id` is
/// the hub's clock in milliseconds when it took the message, the order key within a conversation
/// (an edit keeps it); `mac` proves the version is genuine and goes back unchanged in an edit, a
/// delete or a report. `nonce` (the sender's own key) reaches only the sender's devices. CHAT2 adds
/// a sticker in place of the text, the link to the message a reply answers with that link's seal,
/// and the reactions (each only to sockets that take them).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ChatMessage {
    /// Missing inside a sync answer (the conversation is said once there).
    #[serde(default)]
    pub conv: String,
    pub id: i64,
    pub from: String,
    /// The text; empty for a sticker.
    #[serde(default)]
    pub text: String,
    pub mac: String,
    /// Milliseconds, when the message was edited ("düzenlendi").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub edited_at: Option<i64>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub nonce: Option<String>,
    /// A Projects sticker (`pl1/gg`) in place of the text (CHAT2). Never edited.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub sticker: Option<String>,
    /// The message this one answers (CHAT2), and the seal of that link (22 characters).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reply: Option<ChatReplyRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rmac: Option<String>,
    /// Its reactions as the player sees them (CHAT2); none when absent.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub reactions: Vec<Reaction>,
}

impl ChatMessage {
    /// The message as the service wants it back (an edit, a delete, a report, the message a reply
    /// answers): what its seals cover, a sticker in place of the text and a reply's link with its
    /// own seal; without the sender's own nonce and the reactions.
    pub fn sealed(&self) -> serde_json::Value {
        let mut value = serde_json::json!({
            "conv": self.conv,
            "id": self.id,
            "from": self.from,
            "mac": self.mac,
        });
        match &self.sticker {
            Some(sticker) => value["sticker"] = sticker.as_str().into(),
            None => value["text"] = self.text.as_str().into(),
        }
        if let Some(at) = self.edited_at {
            value["editedAt"] = at.into();
        }
        if let (Some(reply), Some(rmac)) = (&self.reply, &self.rmac) {
            value["reply"] = serde_json::json!({ "id": reply.id, "from": reply.from });
            value["rmac"] = rmac.as_str().into();
        }
        value
    }
}

/// What a chat message carries: a text, or a Projects sticker (CHAT2, `pl1/gg`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ChatBody {
    Text(String),
    Sticker(String),
}

/// A message of a sync whose reactions changed while the message itself did not (CHAT2; an empty
/// list once the last went).
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatReacted {
    pub id: i64,
    #[serde(default)]
    pub reactions: Vec<Reaction>,
}

/// A read mark (CHAT2): `by` has read a conversation up to message `id`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ReadMark {
    pub by: String,
    pub id: i64,
}

/// A link's preview card as the hub made it (`ack.card`, CHAT2): fetched by the hub, never by this
/// computer. `icon` names one of the launcher's own icons, `kind` the card's shape.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LinkCard {
    pub url: String,
    #[serde(default)]
    pub site: String,
    #[serde(default)]
    pub icon: String,
    #[serde(default)]
    pub kind: String,
    #[serde(default)]
    pub title: Option<String>,
    #[serde(default)]
    pub author: Option<String>,
    #[serde(default)]
    pub desc: Option<String>,
    /// `data:image/jpeg|png|webp;base64,…` (trusted sites only), `w` × `h` pixels.
    #[serde(default)]
    pub image: Option<String>,
    #[serde(default)]
    pub w: Option<u32>,
    #[serde(default)]
    pub h: Option<u32>,
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
/// moderator's mute; CHAT2 adds which extras Studio has on, the sticker sets the service takes and
/// the link rules. `caps` is no part of the message: the extras this connection's socket took.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
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
    /// The extras Studio has on now (CHAT2; None from an older service: none of them).
    #[serde(default)]
    pub extras: Option<ChatExtras>,
    /// The sticker sets the service takes (CHAT2, `pl1`).
    #[serde(default)]
    pub stickers: Vec<String>,
    /// The link rules and Studio's lists, while links are on (CHAT2).
    #[serde(default)]
    pub links: Option<ChatLinkRules>,
    /// The extras this connection's socket took (`hello.caps`; the presence task fills it in).
    #[serde(skip)]
    pub caps: ChatCaps,
}

impl ChatStatus {
    /// What the player may use beyond CHAT1 now: the socket took it and Studio has it on (links:
    /// Studio's switch alone, a link is text). None from an older service.
    pub fn offered(&self) -> Option<ChatExtras> {
        let on = self.extras?;
        Some(ChatExtras {
            replies: on.replies && self.caps.replies,
            stickers: on.stickers && self.caps.stickers,
            reactions: on.reactions && self.caps.reactions,
            typing: on.typing && self.caps.typing,
            reads: on.reads && self.caps.reads,
            links: on.links,
        })
    }
}

fn mailbox_on() -> bool {
    true
}

fn chat_per_day() -> u32 {
    500
}

/// CHAT2's extras, one switch each: replies, stickers, reactions, "yazıyor…", read marks, links
/// (one a newer service adds is left out; one it leaves out is off).
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Deserialize)]
pub struct ChatExtras {
    #[serde(default)]
    pub replies: bool,
    #[serde(default)]
    pub stickers: bool,
    #[serde(default)]
    pub reactions: bool,
    #[serde(default)]
    pub typing: bool,
    #[serde(default)]
    pub reads: bool,
    #[serde(default)]
    pub links: bool,
}

/// The chat capabilities a socket took (`hello.caps`, CHAT2): what the service sends it and takes
/// from it. A socket without them gets exactly what CHAT1 sent.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct ChatCaps {
    pub replies: bool,
    pub stickers: bool,
    pub reactions: bool,
    pub typing: bool,
    pub reads: bool,
}

impl ChatCaps {
    /// Any of CHAT2's capabilities.
    pub fn any(&self) -> bool {
        self.replies || self.stickers || self.reactions || self.typing || self.reads
    }
}

/// The link rules the service applies (`hello.chat.links`, CHAT2): the rule table's version
/// (`chat-links.json`), Studio's trusted hosts, blocked hosts and pages, and whether Discord
/// invites may be linked.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ChatLinkRules {
    #[serde(default)]
    pub rules: u32,
    #[serde(default)]
    pub trusted: Vec<String>,
    #[serde(default)]
    pub blocked: Vec<String>,
    #[serde(default = "invites_on")]
    pub invites: bool,
}

fn invites_on() -> bool {
    true
}

/// Why the service turned a link away (`chat_rejected` with `link` and `rule`, CHAT2; the rule
/// table `chat-links.json`). One this version does not know reads as `Other`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LinkRule {
    /// Links are off (Studio's switch, or an older service).
    Off,
    /// Not a plain `https://` link with a host name, or an address hidden in the text.
    Form,
    /// More than 3 links.
    Count,
    Http,
    Credentials,
    Ip,
    Port,
    /// A brand's name on someone else's host, or a host that is not plain ASCII.
    Lookalike,
    Long,
    Shortener,
    Grabber,
    /// Adult and gambling sites.
    Adult,
    Scam,
    File,
    /// A Discord invite while Studio keeps them off.
    Invite,
    /// Studio blocked the site or the page.
    Blocked,
    Other,
}

impl<'de> Deserialize<'de> for LinkRule {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Self::parse(&String::deserialize(deserializer)?))
    }
}

impl LinkRule {
    pub fn parse(value: &str) -> Self {
        match value {
            "off" => Self::Off,
            "form" => Self::Form,
            "count" => Self::Count,
            "http" => Self::Http,
            "credentials" => Self::Credentials,
            "ip" => Self::Ip,
            "port" => Self::Port,
            "lookalike" => Self::Lookalike,
            "long" => Self::Long,
            "shortener" => Self::Shortener,
            "grabber" => Self::Grabber,
            "adult" => Self::Adult,
            "scam" => Self::Scam,
            "file" => Self::File,
            "invite" => Self::Invite,
            "blocked" => Self::Blocked,
            _ => Self::Other,
        }
    }
}

/// A chat quota of its own that ran out for the day (`rate_limited` with `item`, CHAT2).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ChatQuota {
    /// Stickers (100 a day; 20 for a limited account).
    Stickers,
    /// Reaction changes (300 a day; 100 for a limited account).
    Reactions,
    /// Messages with links (30 a day).
    Links,
    /// Link cards the hub fetched for the player (60 a day).
    Unfurls,
    Other,
}

impl ChatQuota {
    pub fn parse(value: &str) -> Self {
        match value {
            "stickers" => Self::Stickers,
            "reactions" => Self::Reactions,
            "links" => Self::Links,
            "unfurls" => Self::Unfurls,
            _ => Self::Other,
        }
    }
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
        let roles_only = EditFeatures {
            roles: true,
            requests: true,
            ..EditFeatures::default()
        };
        assert!(edit.clone().limited_to(roles_only).is_empty());
        let look_only = EditFeatures {
            look: true,
            ..EditFeatures::default()
        };
        assert_eq!(edit.clone().limited_to(look_only), edit);
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
        let without_roles = EditFeatures {
            look: true,
            requests: true,
            ..EditFeatures::default()
        };
        assert!(edit.clone().limited_to(without_roles).is_empty());
        let nothing = TeamEdit {
            permissions: Some(PermissionsEdit::default()),
            ..TeamEdit::default()
        };
        let everything = EditFeatures {
            look: true,
            roles: true,
            requests: true,
            ..EditFeatures::default()
        };
        assert!(nothing.limited_to(everything).is_empty());
    }

    #[test]
    fn joining_settings_requests_and_links_with_and_without_them() {
        let entry: CrewEntry = serde_json::from_str(
            r#"{"team":{"id":"0a1b2c3d4e5f6a7b","name":"Gece","tag":"GKR","emblem":"sword",
                "code":"TKM-ABCD-EFGH","createdOn":"2026-10-20","members":[],
                "permissions":{"invite":"members","call":"members","kick":"moderators",
                  "edit":"leader","admit":"leader"},
                "join":"approval","link":"ABCD-EFGH-JKMN"},
              "requests":[{"from":{"id":"2","username":"b","displayName":"Bora","avatar":"discord",
                "avatarHash":null},"sentAt":1792520000}]}"#,
        )
        .unwrap();
        assert_eq!(entry.team.join, JoinMode::Approval);
        assert_eq!(entry.team.link.as_deref(), Some("ABCD-EFGH-JKMN"));
        assert_eq!(entry.team.permissions.admit, PermissionLevel::Leader);
        assert_eq!(entry.requests[0].from.name(), "Bora");
        assert_eq!(entry.requests[0].sent_at, 1_792_520_000);

        // From before join requests: open, no link, moderators would answer, nobody asks.
        let older: CrewEntry = serde_json::from_str(
            r#"{"team":{"id":"0a1b2c3d4e5f6a7b","name":"Gece","tag":"GKR","emblem":"sword",
                "code":"TKM-ABCD-EFGH","createdOn":"2026-10-20","members":[]}}"#,
        )
        .unwrap();
        assert_eq!(older.team.join, JoinMode::Open);
        assert_eq!(older.team.link, None);
        assert_eq!(older.team.permissions.admit, PermissionLevel::Moderators);
        assert!(older.requests.is_empty());
        // A join setting this version does not know asks rather than lets in.
        let unknown: JoinMode = serde_json::from_str(r#""invite_only""#).unwrap();
        assert_eq!(unknown, JoinMode::Approval);

        let overview: TeamsOverview = serde_json::from_str(
            r#"{"team":null,"invited":[],"teamInvites":[],
                "teamRequests":[{"team":{"id":"fedcba9876543210","name":"PvP","tag":"PVP",
                  "emblem":"bolt","members":12,"join":"approval"},"sentAt":1792519000}],
                "teamFeatures":["requests","link","plans"]}"#,
        )
        .unwrap();
        assert_eq!(overview.team_requests[0].team.join, JoinMode::Approval);
        assert_eq!(overview.team_requests[0].sent_at, 1_792_519_000);
        let lookup: TeamLookup = serde_json::from_str(
            r#"{"team":{"id":"x","name":"PvP","tag":"PVP","emblem":"bolt","members":3,
                "join":"approval"},"relation":"none","request":{"sentAt":1792519000}}"#,
        )
        .unwrap();
        assert_eq!(
            lookup.request,
            Some(WaitingRequest {
                sent_at: 1_792_519_000
            })
        );
        let plain: TeamLookup = serde_json::from_str(
            r#"{"team":{"id":"x","name":"PvP","tag":"PVP","emblem":"bolt","members":3},
                "relation":"none","request":null}"#,
        )
        .unwrap();
        assert_eq!((plain.team.join, plain.request), (JoinMode::Open, None));

        // The join setting and `admit` go only where the service takes them.
        let edit = TeamEdit {
            join: Some(JoinMode::Approval),
            permissions: Some(PermissionsEdit {
                admit: Some(PermissionLevel::Leader),
                ..PermissionsEdit::default()
            }),
            ..TeamEdit::default()
        };
        assert_eq!(
            serde_json::to_string(&edit).unwrap(),
            r#"{"permissions":{"admit":"leader"},"join":"approval"}"#
        );
        let everything = EditFeatures {
            look: true,
            roles: true,
            requests: true,
            ..EditFeatures::default()
        };
        assert_eq!(edit.clone().limited_to(everything), edit);
        let older = EditFeatures {
            requests: false,
            ..everything
        };
        assert!(edit.limited_to(older).is_empty());
        assert_eq!(LimitedKey::parse("crew_link"), LimitedKey::CrewLink);
        assert_eq!(LimitedKey::parse("crew_plans"), LimitedKey::CrewPlans);
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
        // CHAT2's two switches go only when set.
        let chat = ProfileUpdate::default()
            .share_read(false)
            .share_typing(true);
        assert_eq!(
            serde_json::to_string(&chat).unwrap(),
            r#"{"shareRead":false,"shareTyping":true}"#
        );
        // TEAM6: a tag put on names its crew; taking it off is `null`; untouched, nothing goes.
        assert_eq!(
            serde_json::to_string(&ProfileUpdate::default().badge(Some("0a1b2c3d4e5f6a7b")))
                .unwrap(),
            r#"{"badge":"0a1b2c3d4e5f6a7b"}"#
        );
        assert_eq!(
            serde_json::to_string(&ProfileUpdate::default().badge(None)).unwrap(),
            r#"{"badge":null}"#
        );
        assert_eq!(
            serde_json::to_string(&ProfileUpdate::default()).unwrap(),
            "{}"
        );
    }

    const BADGE: &str = r#"{"crew":"0a1b2c3d4e5f6a7b","tag":"GK","color":"navy","frame":"shield",
        "emblem":"bolt","name":"Gece Kartalları","motto":"Gece bizimdir","createdOn":"2026-09-20",
        "members":12,"requests":true}"#;

    #[test]
    fn crew_tags_come_with_friends_crewmates_and_me_and_nowhere_else() {
        let overview: FriendsOverview = serde_json::from_str(&format!(
            r#"{{"friends":[{{"id":"1","username":"ayse","displayName":"Ayşe","avatar":"discord",
                "avatarHash":null,"since":"2026-10-01","lastSeen":"today","badge":{BADGE}}},
                {{"id":"2","username":"bora","displayName":null,"avatar":"discord","avatarHash":null,
                "since":"2026-10-01","lastSeen":null,"badge":null}}],
                "incoming":[{{"id":"3","username":"c","displayName":null,"avatar":"discord",
                "avatarHash":null,"sentAt":5}}],"outgoing":[],"blocked":[],
                "limits":{{"friends":200,"outgoingRequests":50,"teamMembers":30}},
                "teamFeatures":["badges","badgeRequests"],
                "crews":[{{"team":{{"id":"0a1b2c3d4e5f6a7b","name":"Gece Kartalları","tag":"GK",
                  "emblem":"bolt","color":"navy","frame":"shield","motto":"Gece bizimdir",
                  "code":"TKM-ABCD-EFGH","createdOn":"2026-09-20","hidden":false,
                  "badgeRequests":false,"members":[{{"id":"1","username":"ayse","displayName":"Ayşe",
                  "avatar":"discord","avatarHash":null,"role":"founder","joinedAt":5,
                  "badge":{BADGE}}}]}},
                  "requests":[{{"from":{{"id":"4","username":"d","displayName":null,
                  "avatar":"discord","avatarHash":null}},"sentAt":9,"via":"1"}}]}}]}}"#
        ))
        .unwrap();
        let card = overview.friends[0].profile.badge.as_ref().unwrap();
        assert_eq!(
            (
                card.crew.as_str(),
                card.tag.as_str(),
                card.members,
                card.requests
            ),
            ("0a1b2c3d4e5f6a7b", "GK", 12, true)
        );
        assert_eq!(card.created_on, "2026-09-20");
        assert!(overview.friends[1].profile.badge.is_none());
        assert!(overview.incoming[0].profile.badge.is_none());
        let crew = &overview.crews.as_ref().unwrap()[0];
        assert!(!crew.team.hidden && !crew.team.badge_requests);
        assert_eq!(
            crew.team.members[0]
                .profile
                .badge
                .as_ref()
                .map(|b| b.name.as_str()),
            Some("Gece Kartalları")
        );
        assert_eq!(crew.requests[0].via.as_deref(), Some("1"));

        let me: Me = serde_json::from_str(&format!(
            r#"{{"id":"1","username":"ayse","displayName":"Ayşe","avatar":"discord","avatarHash":null,
                "discordAvatarHash":null,"friendCode":"PRJ-ABCD-EFGH","status":"online",
                "privacy":{{"shareServer":false,"shareLastSeen":true}},"createdOn":"2026-10-16",
                "settings":null,"badge":{BADGE}}}"#
        ))
        .unwrap();
        assert_eq!(me.profile.badge.map(|b| b.tag), Some("GK".to_owned()));
        // A card this version cannot read is no card; the profile still reads.
        let odd: Friend = serde_json::from_str(
            r#"{"id":"1","username":"ayse","displayName":null,"avatar":"discord","avatarHash":null,
                "since":"2026-10-01","lastSeen":null,"badge":{"crew":7}}"#,
        )
        .unwrap();
        assert!(odd.profile.badge.is_none());
        // A sparse card takes the defaults of a crew from before looks.
        let sparse: CrewBadge = serde_json::from_str(
            r#"{"crew":"0a1b2c3d4e5f6a7b","tag":"GK","emblem":"bolt","name":"Gece"}"#,
        )
        .unwrap();
        assert_eq!(
            (
                sparse.color.as_str(),
                sparse.frame.as_str(),
                sparse.requests
            ),
            ("gold", "cut", false)
        );
        // A profile without a tag writes none (nothing new in what is kept or sent).
        let plain = serde_json::to_value(&overview.friends[1].profile).unwrap();
        assert!(plain.get("badge").is_none());
    }

    #[test]
    fn crews_from_before_tags_read_visible_and_taking_requests() {
        let team: Team = serde_json::from_str(
            r#"{"id":"0a1b2c3d4e5f6a7b","name":"Gece","tag":"GK","emblem":"bolt",
                "code":"TKM-ABCD-EFGH","createdOn":"2026-09-20","members":[]}"#,
        )
        .unwrap();
        assert!(!team.hidden && team.badge_requests);
        let request: JoinRequest = serde_json::from_str(
            r#"{"from":{"id":"4","username":"d","displayName":null,"avatar":"discord",
                "avatarHash":null},"sentAt":9}"#,
        )
        .unwrap();
        assert!(request.via.is_none());
    }

    #[test]
    fn the_crews_tag_settings_go_only_to_a_service_that_has_them() {
        let edit = TeamEdit {
            hidden: Some(true),
            badge_requests: Some(false),
            ..TeamEdit::default()
        };
        assert_eq!(
            serde_json::to_string(&edit).unwrap(),
            r#"{"hidden":true,"badgeRequests":false}"#
        );
        let tags = EditFeatures {
            badges: true,
            badge_requests: true,
            ..EditFeatures::default()
        };
        assert_eq!(edit.clone().limited_to(tags), edit);
        // Studio's request switch off: the visibility still goes, "Etiketten istek" does not.
        let tags_only = EditFeatures {
            badges: true,
            ..EditFeatures::default()
        };
        assert_eq!(
            edit.clone().limited_to(tags_only),
            TeamEdit {
                hidden: Some(true),
                ..TeamEdit::default()
            }
        );
        // An older service: nothing.
        assert!(edit.limited_to(EditFeatures::default()).is_empty());
    }

    #[test]
    fn chat_privacy_and_link_rules_read_as_older_services_send_them() {
        // An older service has neither switch: both count as on.
        let older: Privacy =
            serde_json::from_str(r#"{"shareServer":false,"shareLastSeen":true}"#).unwrap();
        assert!(older.shares_read() && older.shares_typing());
        let off: Privacy = serde_json::from_str(
            r#"{"shareServer":false,"shareLastSeen":true,"shareRead":false,"shareTyping":true}"#,
        )
        .unwrap();
        assert!(!off.shares_read() && off.shares_typing());
        // A rule this version does not know is `other`, and every rule comes back as it went.
        for (wire, rule) in [
            ("\"shortener\"", LinkRule::Shortener),
            ("\"invite\"", LinkRule::Invite),
            ("\"hologram\"", LinkRule::Other),
        ] {
            assert_eq!(serde_json::from_str::<LinkRule>(wire).unwrap(), rule);
        }
        let shown = serde_json::to_string(&LinkRule::Lookalike).unwrap();
        assert_eq!(shown, "\"lookalike\"");
        assert_eq!(
            serde_json::from_str::<LinkRule>(&shown).unwrap(),
            LinkRule::Lookalike
        );
        assert_eq!(ChatQuota::parse("stickers"), ChatQuota::Stickers);
        assert_eq!(LimitedKey::parse("chat_links"), LimitedKey::ChatLinks);
    }
}
