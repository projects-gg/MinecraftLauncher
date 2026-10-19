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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum TeamRole {
    /// Edits the team, renews its code, removes members, hands the role on, disbands it.
    Founder,
    /// Invites friends, may leave.
    Member,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TeamMember {
    #[serde(flatten)]
    pub profile: PublicProfile,
    pub role: TeamRole,
    /// Unix seconds.
    pub joined_at: i64,
}

/// A team as its members see it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Team {
    pub id: String,
    pub name: String,
    /// 2-4 letters or digits, upper case.
    pub tag: String,
    /// One of the service's emblem names (`sword`, `shield`, …), drawn with a bundled icon.
    pub emblem: String,
    /// `TKM-XXXX-XXXX`
    pub code: String,
    /// `YYYY-MM-DD`
    pub created_on: String,
    /// Founder first, then by joining time.
    pub members: Vec<TeamMember>,
}

/// What an invite or a code shows of a team before joining it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TeamSummary {
    pub id: String,
    pub name: String,
    pub tag: String,
    pub emblem: String,
    /// How many are in it.
    pub members: u32,
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

/// `PATCH /v1/teams/:id`; unset fields stay as they are.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct TeamEdit {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub tag: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub emblem: Option<String>,
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
