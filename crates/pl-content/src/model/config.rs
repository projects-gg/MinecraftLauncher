use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{
    Cta, FeedOverride, MediaRef, NewsFeedConfig, SubServer, check_schema, default_true,
    is_valid_minecraft_version,
};
use crate::error::Result;

/// Highest `LauncherConfig.schema` this build understands.
pub const LAUNCHER_CONFIG_SCHEMA: u32 = 1;

/// The Minecraft version this launcher recommends while the signed config names none (or an
/// invalid one): see [`LauncherConfig::recommended_minecraft`]. Studio shows it as the default of
/// its setting. Change it only together with the release notes of a launcher version.
pub const DEFAULT_RECOMMENDED_MINECRAFT_VERSION: &str = "26.1.2";

/// `config/launcher.<sha256>.json`: home screen, banners, servers, links, Discord templates,
/// feature flags and the minimum launcher version. Every field is optional.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct LauncherConfig {
    pub schema: u32,
    /// Launchers older than this must update before playing (lock screen + auto install).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub min_launcher_version: Option<semver::Version>,
    pub maintenance: Maintenance,
    pub banners: Vec<Banner>,
    pub hero: Hero,
    pub theme: Theme,
    pub events: Vec<EventCountdown>,
    pub servers: Vec<ServerEntry>,
    pub links: Links,
    pub discord: DiscordConfig,
    pub features: Features,
    /// Pack id suggested for new installs.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_pack: Option<String>,
    /// The Minecraft version launchers recommend ("Önerilen" in their version lists, preselected
    /// for new custom instances); read it through [`Self::recommended_minecraft`]. Written only
    /// when set, so configs without it keep their exact shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub recommended_minecraft_version: Option<String>,
    /// The Discord bot's news feed. Missing or disabled: launchers never fetch it. Published only
    /// while enabled, so configs without it keep their exact shape.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub news_feed: Option<NewsFeedConfig>,
}

impl Default for LauncherConfig {
    fn default() -> Self {
        Self {
            schema: LAUNCHER_CONFIG_SCHEMA,
            min_launcher_version: None,
            maintenance: Maintenance::default(),
            banners: Vec::new(),
            hero: Hero::default(),
            theme: Theme::default(),
            events: Vec::new(),
            servers: Vec::new(),
            links: Links::default(),
            discord: DiscordConfig::default(),
            features: Features::default(),
            recommended_pack: None,
            recommended_minecraft_version: None,
            news_feed: None,
        }
    }
}

impl LauncherConfig {
    pub fn check_schema(&self) -> Result<()> {
        check_schema("launcher config", self.schema, LAUNCHER_CONFIG_SCHEMA)
    }

    /// The news feed settings when the feed is enabled.
    pub fn active_news_feed(&self) -> Option<&NewsFeedConfig> {
        self.news_feed.as_ref().filter(|feed| feed.enabled)
    }

    /// The Minecraft version the launcher recommends: `recommendedMinecraftVersion` when it is a
    /// valid version id ([`is_valid_minecraft_version`]), else
    /// [`DEFAULT_RECOMMENDED_MINECRAFT_VERSION`]. Whether Mojang lists it is up to the caller (a
    /// version list marks it only where it appears).
    pub fn recommended_minecraft(&self) -> &str {
        self.recommended_minecraft_version
            .as_deref()
            .filter(|version| is_valid_minecraft_version(version))
            .unwrap_or(DEFAULT_RECOMMENDED_MINECRAFT_VERSION)
    }

    /// Drops what writers leave out of a published config: a `newsFeed` that is not enabled
    /// (Studio keeps it in its workspace so turning the feed back on loses nothing), an empty
    /// `recommendedMinecraftVersion` (launchers then use their built-in default) and community
    /// platforms emptied in Studio while another one has an address
    /// ([`Links::drop_empty_platforms`]).
    pub fn normalize_for_publish(&mut self) {
        self.links.drop_empty_platforms();
        if self.news_feed.as_ref().is_some_and(|feed| !feed.enabled) {
            self.news_feed = None;
        }
        if self
            .recommended_minecraft_version
            .as_deref()
            .is_some_and(|version| version.trim().is_empty())
        {
            self.recommended_minecraft_version = None;
        }
    }

    /// True when `current` is below `minLauncherVersion`.
    pub fn requires_launcher_update(&self, current: &semver::Version) -> bool {
        self.min_launcher_version
            .as_ref()
            .is_some_and(|min| current < min)
    }

    /// Banners whose time window contains `now`, in authored order.
    pub fn active_banners(&self, now: Timestamp) -> Vec<&Banner> {
        self.banners.iter().filter(|b| b.is_active(now)).collect()
    }

    /// Events that are not hidden and have not ended yet, soonest first.
    pub fn visible_events(&self, now: Timestamp) -> Vec<&EventCountdown> {
        let mut events: Vec<_> = self
            .events
            .iter()
            .filter(|e| !e.hidden && e.phase(now) != EventPhase::Ended)
            .collect();
        events.sort_by(|a, b| a.starts_at.cmp(&b.starts_at).then_with(|| a.id.cmp(&b.id)));
        events
    }

    /// Every media file this config references (including posters), for prefetch and cache
    /// garbage collection.
    pub fn media_refs(&self) -> Vec<&MediaRef> {
        let hero = self.hero.media.iter();
        let events = self.events.iter().filter_map(|e| e.image.as_ref());
        let servers = self.servers.iter().filter_map(|s| s.icon.as_ref());
        hero.chain(events)
            .chain(servers)
            .flat_map(MediaRef::with_posters)
            .collect()
    }
}

/// Maintenance mode: the launcher shows `title`/`message` and blocks playing until `until`.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Maintenance {
    pub enabled: bool,
    pub title: String,
    pub message: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<Timestamp>,
}

impl Maintenance {
    pub fn is_active(&self, now: Timestamp) -> bool {
        self.enabled && self.until.is_none_or(|until| now < until)
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum BannerLevel {
    #[default]
    Info,
    Warning,
    Event,
    Critical,
    /// A level added by a newer Studio; render like `info`.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Banner {
    /// Stable id, used to remember dismissals.
    pub id: String,
    #[serde(default)]
    pub level: BannerLevel,
    pub text: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub starts_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<Timestamp>,
    #[serde(default = "default_true")]
    pub dismissible: bool,
}

impl Banner {
    pub fn is_active(&self, now: Timestamp) -> bool {
        self.starts_at.is_none_or(|start| start <= now) && self.ends_at.is_none_or(|end| now < end)
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum Particles {
    #[default]
    Embers,
    Sparks,
    Snow,
    None,
    /// An effect added by a newer Studio; use the default.
    #[serde(other)]
    Unknown,
}

/// Full-screen home art.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Hero {
    /// Image or video; the launcher's bundled art is used when absent.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub media: Option<MediaRef>,
    pub title: String,
    pub subtitle: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub cta: Option<Cta>,
    pub particles: Particles,
    /// Accent colour as `#RRGGBB`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum Season {
    #[default]
    None,
    Halloween,
    Winter,
    Holiday,
    Event,
    /// A season added by a newer Studio; use the default look.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Theme {
    pub season: Season,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub accent: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum EventPhase {
    Upcoming,
    Live,
    Ended,
}

/// An event with a countdown to `startsAt` that is live until `endsAt`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EventCountdown {
    pub id: String,
    pub title: String,
    pub starts_at: Timestamp,
    pub ends_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub image: Option<MediaRef>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The news item the event card opens (while that item is on the home screen).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub news_id: Option<String>,
    /// Among events live at the same time, the card shows the highest priority first (then the
    /// one that started last). Default 0.
    #[serde(default, skip_serializing_if = "super::is_default")]
    pub priority: i32,
    /// The event takes place on the server: while it is live, the event card offers "Katıl",
    /// which starts the official Projects instance straight into the server. Default `false`,
    /// left out while `false` (launchers that do not know it ignore it).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub join: bool,
    /// Never shown. Studio publishes a hidden event to take the feed event with its id off the
    /// event card (Studio wins on a shared id).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    /// Studio's record of the feed event this one replaces or hides; launchers ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_override: Option<FeedOverride>,
}

impl EventCountdown {
    pub fn phase(&self, now: Timestamp) -> EventPhase {
        if now < self.starts_at {
            EventPhase::Upcoming
        } else if now < self.ends_at {
            EventPhase::Live
        } else {
            EventPhase::Ended
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ServerEntry {
    pub id: String,
    pub name: String,
    pub address: String,
    /// Defaults to 25565 (or the SRV record) when absent.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<u16>,
    #[serde(default)]
    pub featured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<MediaRef>,
    /// The games behind a Projects lobby and the command that switches there (AUTOJOIN,
    /// [`SubServer`]): offered only while `features.subServers` is on and only for a Projects
    /// address ([`ServerEntry::usable_sub_servers`]). Left out while empty.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub sub_servers: Vec<SubServer>,
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Links {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub website: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub discord: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub store: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub support: Option<String>,
    /// Additional links. Entries whose `id` or `icon` names a community platform (wiki, forum,
    /// Instagram, …) are the launcher's community row ([`Links::community`]); the others are
    /// listed by label ([`Links::other_custom`]).
    pub custom: Vec<CustomLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomLink {
    pub id: String,
    pub label: String,
    pub url: String,
    /// A community platform's key (`instagram`, …; [`crate::CommunityPlatform::key`]).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon: Option<String>,
}

/// Discord Rich Presence settings managed from Studio.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct DiscordConfig {
    pub enabled: bool,
    /// Discord application id; empty means "use the launcher's built-in id".
    pub client_id: String,
    pub templates: PresenceTemplates,
}

impl Default for DiscordConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            client_id: String::new(),
            templates: PresenceTemplates::default(),
        }
    }
}

/// One template per launcher state. Missing templates fall back to the built-in text.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PresenceTemplates {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub idle: Option<PresenceTemplate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub browsing: Option<PresenceTemplate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub downloading: Option<PresenceTemplate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub in_game: Option<PresenceTemplate>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub afk: Option<PresenceTemplate>,
}

/// Text with `{placeholder}`s (`{instance}`, `{version}`, `{loader}`, `{server}`,
/// `{players}`, `{maxPlayers}`, `{percent}`, `{afkCount}`, `{player}`), filled by the launcher.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct PresenceTemplate {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub state: Option<String>,
    /// Asset key or `https://` URL.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub large_text: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_image: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub small_text: Option<String>,
    /// At most two are shown by Discord.
    pub buttons: Vec<PresenceButton>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PresenceButton {
    pub label: String,
    pub url: String,
}

impl PresenceTemplate {
    /// Fills every placeholder through `lookup`; fields that end up empty are dropped.
    pub fn render(&self, lookup: &dyn Fn(&str) -> Option<String>) -> PresenceTemplate {
        let fill = |text: &Option<String>| {
            text.as_deref()
                .map(|t| fill_placeholders(t, lookup))
                .filter(|t| !t.is_empty())
        };
        PresenceTemplate {
            details: fill(&self.details),
            state: fill(&self.state),
            large_image: fill(&self.large_image),
            large_text: fill(&self.large_text),
            small_image: fill(&self.small_image),
            small_text: fill(&self.small_text),
            buttons: self
                .buttons
                .iter()
                .map(|b| PresenceButton {
                    label: fill_placeholders(&b.label, lookup),
                    url: fill_placeholders(&b.url, lookup),
                })
                .filter(|b| !b.label.is_empty() && !b.url.is_empty())
                .take(2)
                .collect(),
        }
    }
}

/// Replaces `{name}` placeholders (`name` = ASCII letters, digits, `_`) with `lookup(name)`;
/// unknown names become empty. Other braces are kept. The result is trimmed.
pub fn fill_placeholders(text: &str, lookup: &dyn Fn(&str) -> Option<String>) -> String {
    let mut out = String::with_capacity(text.len());
    let mut rest = text;
    while let Some(open) = rest.find('{') {
        out.push_str(&rest[..open]);
        let after = &rest[open + 1..];
        match after.find('}') {
            Some(close)
                if close > 0
                    && after[..close]
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_') =>
            {
                out.push_str(&lookup(&after[..close]).unwrap_or_default());
                rest = &after[close + 1..];
            }
            _ => {
                out.push('{');
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out.trim().to_owned()
}

/// Feature flags; everything is on unless Studio turns it off, except `friends`, `teams`, `chat`,
/// `skins`, `worlds`, `subServers` and `overlayJars`, which stay off until Studio turns them on.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Features {
    pub afk: bool,
    pub premium_login: bool,
    pub offline_accounts: bool,
    pub modrinth_browser: bool,
    /// The optional friends feature (Discord sign-in, friend list, presence). Off by default:
    /// launchers show nothing of it until the service is live. Written only when on, so
    /// configs without it keep their exact shape.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub friends: bool,
    /// Teams inside the friends feature (needs `friends` too). Off by default until the friends
    /// service with teams is live; written only when on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub teams: bool,
    /// Crew chat and direct messages inside the friends feature (needs `friends` too; CHAT1).
    /// Off by default until the friends service with chat (and its reports and spam protection)
    /// is live; written only when on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub chat: bool,
    /// Projects skins for username accounts and linking a game account from the launcher, inside
    /// the friends feature (needs `friends` too; SKIN1). Off by default until the friends service
    /// with SKIN1 and ProBot's side are live; written only when on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub skins: bool,
    /// "Dünyanı arkadaşlarına aç" (HOST1): a player opens their singleplayer world to friends
    /// (needs `friends` too, and the friends service's `worlds`). Off by default until the
    /// service's world sessions and the overlay mod that hosts are live; written only when on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub worlds: bool,
    /// "play.projects.gg → SkyBlock" (AUTOJOIN): the player picks one of a Projects server's
    /// [`ServerEntry::sub_servers`] beside "Doğrudan sunucuya bağlan" and the in-game overlay
    /// switches there once after the lobby's login. Off by default until the lobby's script is
    /// live; written only when on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub sub_servers: bool,
    /// The in-game overlay's jars from the signed content (OVL-AUTO, `RootIndex.overlay`) for the
    /// Minecraft versions the launcher's own jars leave out. Off by default: launchers do not read
    /// `overlay` until it is on; written only when on.
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    pub overlay_jars: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            afk: true,
            premium_login: true,
            offline_accounts: true,
            modrinth_browser: true,
            friends: false,
            teams: false,
            chat: false,
            skins: false,
            worlds: false,
            sub_servers: false,
            overlay_jars: false,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    fn event(id: &str, start: &str, end: &str) -> EventCountdown {
        EventCountdown {
            id: id.into(),
            title: id.into(),
            starts_at: ts(start),
            ends_at: ts(end),
            image: None,
            url: None,
            news_id: None,
            priority: 0,
            join: false,
            hidden: false,
            feed_override: None,
        }
    }

    #[test]
    fn event_additions_keep_the_old_shape() {
        let plain: EventCountdown = serde_json::from_str(
            r#"{"id":"cup","title":"Kupa","startsAt":"2026-10-20T18:00:00Z","endsAt":"2026-10-20T21:00:00Z"}"#,
        )
        .unwrap();
        assert!(!plain.hidden && plain.news_id.is_none() && plain.feed_override.is_none());
        assert_eq!(plain.priority, 0);
        assert!(!plain.join);
        let written = serde_json::to_value(&plain).unwrap();
        for key in ["hidden", "newsId", "feedOverride", "priority", "join"] {
            assert!(written.get(key).is_none(), "{key} is left out when unused");
        }
        let raised = EventCountdown {
            priority: -2,
            ..plain.clone()
        };
        assert_eq!(serde_json::to_value(&raised).unwrap()["priority"], -2);
        let on_server = EventCountdown {
            join: true,
            ..plain
        };
        assert_eq!(serde_json::to_value(&on_server).unwrap()["join"], true);
    }

    #[test]
    fn join_is_read_and_older_readers_skip_it() {
        let text = r#"{"id":"cup","title":"Kupa","startsAt":"2026-10-20T18:00:00Z","endsAt":"2026-10-20T21:00:00Z","join":true}"#;
        let event: EventCountdown = serde_json::from_str(text).unwrap();
        assert!(event.join);
        // A launcher from before the field reads the same event (unknown fields are skipped).
        #[derive(Deserialize)]
        #[serde(rename_all = "camelCase")]
        struct Before {
            id: String,
            starts_at: Timestamp,
        }
        let before: Before = serde_json::from_str(text).unwrap();
        assert_eq!(before.id, "cup");
        assert_eq!(before.starts_at, event.starts_at);
        let config: LauncherConfig =
            serde_json::from_str(&format!(r#"{{"events":[{text}]}}"#)).unwrap();
        assert!(config.events[0].join);
    }

    #[test]
    fn empty_document_uses_defaults() {
        let config: LauncherConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, LauncherConfig::default());
        assert_eq!(config.schema, 1);
        assert!(config.features.afk && config.features.premium_login);
        assert!(config.discord.enabled);
        assert_eq!(config.hero.particles, Particles::Embers);
        config.check_schema().unwrap();
    }

    #[test]
    fn unknown_fields_and_values_are_tolerated() {
        let config: LauncherConfig = serde_json::from_str(
            r#"{
                "futureField": {"nested": true},
                "banners": [{"id": "b", "text": "Hi", "level": "rainbow", "extra": 1}],
                "hero": {"particles": "confetti"},
                "theme": {"season": "spring"},
                "features": {"afk": false, "newThing": true}
            }"#,
        )
        .unwrap();
        assert_eq!(config.banners[0].level, BannerLevel::Unknown);
        assert!(config.banners[0].dismissible);
        assert_eq!(config.hero.particles, Particles::Unknown);
        assert_eq!(config.theme.season, Season::Unknown);
        assert!(!config.features.afk);
        assert!(config.features.modrinth_browser);
    }

    #[test]
    fn friends_flag_is_off_unless_turned_on() {
        let config: LauncherConfig = serde_json::from_str("{}").unwrap();
        assert!(!config.features.friends);
        // Off is not written, so older configs keep their exact shape.
        let written = serde_json::to_value(&config).unwrap();
        assert!(written["features"].get("friends").is_none());
        let on: LauncherConfig =
            serde_json::from_str(r#"{"features": {"friends": true}}"#).unwrap();
        assert!(on.features.friends);
        assert!(on.features.afk, "other flags keep their defaults");
        assert_eq!(
            serde_json::to_value(&on).unwrap()["features"]["friends"],
            serde_json::Value::Bool(true)
        );
        // Teams the same way.
        assert!(!on.features.teams);
        assert!(written["features"].get("teams").is_none());
        let teams: LauncherConfig =
            serde_json::from_str(r#"{"features": {"friends": true, "teams": true}}"#).unwrap();
        assert!(teams.features.teams);
        assert_eq!(
            serde_json::to_value(&teams).unwrap()["features"]["teams"],
            serde_json::Value::Bool(true)
        );
        // Chat the same way.
        assert!(!teams.features.chat);
        assert!(written["features"].get("chat").is_none());
        let chat: LauncherConfig =
            serde_json::from_str(r#"{"features": {"friends": true, "chat": true}}"#).unwrap();
        assert!(chat.features.chat && !chat.features.teams);
        assert_eq!(
            serde_json::to_value(&chat).unwrap()["features"]["chat"],
            serde_json::Value::Bool(true)
        );
        // Projects skins the same way (SKIN1).
        assert!(!chat.features.skins);
        assert!(written["features"].get("skins").is_none());
        let skins: LauncherConfig =
            serde_json::from_str(r#"{"features": {"friends": true, "skins": true}}"#).unwrap();
        assert!(skins.features.skins && !skins.features.chat);
        assert_eq!(
            serde_json::to_value(&skins).unwrap()["features"]["skins"],
            serde_json::Value::Bool(true)
        );
    }

    #[test]
    fn news_feed_is_published_only_while_enabled() {
        let config: LauncherConfig = serde_json::from_str("{}").unwrap();
        assert!(config.news_feed.is_none() && config.active_news_feed().is_none());
        let written = serde_json::to_value(&config).unwrap();
        assert!(
            written.get("newsFeed").is_none(),
            "older configs keep their shape"
        );

        let mut on: LauncherConfig = serde_json::from_str(
            r#"{"newsFeed": {"enabled": true, "mirrors": ["https://projects.gg/launcher-feed/v1/"]}}"#,
        )
        .unwrap();
        let feed = on.active_news_feed().unwrap();
        assert_eq!(feed.max_age_days, 60, "default");
        assert!(feed.keys.is_empty());
        on.normalize_for_publish();
        assert!(on.news_feed.is_some(), "an enabled feed is published");

        let mut off = on.clone();
        off.news_feed.as_mut().unwrap().enabled = false;
        assert!(off.active_news_feed().is_none());
        off.normalize_for_publish();
        assert_eq!(off, LauncherConfig::default());
    }

    #[test]
    fn recommended_minecraft_version_falls_back_to_the_built_in_default() {
        let config: LauncherConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(
            config.recommended_minecraft(),
            DEFAULT_RECOMMENDED_MINECRAFT_VERSION
        );
        assert_eq!(DEFAULT_RECOMMENDED_MINECRAFT_VERSION, "26.1.2");
        // Not written while unset, so existing configs keep their exact shape.
        let written = serde_json::to_value(&config).unwrap();
        assert!(written.get("recommendedMinecraftVersion").is_none());

        let set: LauncherConfig =
            serde_json::from_str(r#"{"recommendedMinecraftVersion": "1.21.11"}"#).unwrap();
        assert_eq!(set.recommended_minecraft(), "1.21.11");
        assert_eq!(
            serde_json::to_value(&set).unwrap()["recommendedMinecraftVersion"],
            "1.21.11"
        );

        let with = |version: &str| LauncherConfig {
            recommended_minecraft_version: Some(version.to_owned()),
            ..LauncherConfig::default()
        };
        // Snapshots and pre-releases are version ids too.
        for good in ["26w41a", "1.21.4-pre1", "26.2-snapshot-3", &"9".repeat(32)] {
            assert_eq!(with(good).recommended_minecraft(), good);
        }
        // A broken value never reaches a version list: the built-in default stands in.
        for bad in ["", " ", "1.21 4", "1.21.4\n", "1.21\u{7}", &"9".repeat(33)] {
            assert_eq!(
                with(bad).recommended_minecraft(),
                DEFAULT_RECOMMENDED_MINECRAFT_VERSION,
                "{bad:?}"
            );
        }

        // Writers leave an empty value out.
        let mut empty = with("  ");
        empty.normalize_for_publish();
        assert_eq!(empty, LauncherConfig::default());
        let mut kept = with("26.1.2");
        kept.normalize_for_publish();
        assert_eq!(
            kept.recommended_minecraft_version.as_deref(),
            Some("26.1.2")
        );
    }

    #[test]
    fn minimum_version() {
        let mut config = LauncherConfig {
            min_launcher_version: Some(semver::Version::new(2, 1, 0)),
            ..LauncherConfig::default()
        };
        assert!(config.requires_launcher_update(&semver::Version::new(2, 0, 9)));
        assert!(config.requires_launcher_update(&"2.1.0-beta.1".parse().unwrap()));
        assert!(!config.requires_launcher_update(&semver::Version::new(2, 1, 0)));
        config.min_launcher_version = None;
        assert!(!config.requires_launcher_update(&semver::Version::new(0, 0, 1)));
    }

    #[test]
    fn time_windows() {
        let now = ts("2026-10-15T12:00:00Z");
        let maintenance = Maintenance {
            enabled: true,
            until: Some(ts("2026-10-15T13:00:00Z")),
            ..Maintenance::default()
        };
        assert!(maintenance.is_active(now));
        assert!(!maintenance.is_active(ts("2026-10-15T13:00:00Z")));

        let banner = |id: &str, start: Option<&str>, end: Option<&str>| Banner {
            id: id.into(),
            level: BannerLevel::Info,
            text: id.into(),
            url: None,
            starts_at: start.map(ts),
            ends_at: end.map(ts),
            dismissible: true,
        };
        let config = LauncherConfig {
            banners: vec![
                banner("always", None, None),
                banner("future", Some("2026-10-16T00:00:00Z"), None),
                banner("past", None, Some("2026-10-15T11:59:59Z")),
                banner(
                    "now",
                    Some("2026-10-15T12:00:00Z"),
                    Some("2026-10-15T12:00:01Z"),
                ),
            ],
            events: vec![
                event("later", "2026-11-01T18:00:00Z", "2026-11-01T20:00:00Z"),
                event("live", "2026-10-15T11:00:00Z", "2026-10-15T14:00:00Z"),
                event("over", "2026-10-01T11:00:00Z", "2026-10-01T14:00:00Z"),
                EventCountdown {
                    hidden: true,
                    ..event("hidden", "2026-10-15T11:00:00Z", "2026-10-15T14:00:00Z")
                },
            ],
            ..LauncherConfig::default()
        };
        let active: Vec<_> = config
            .active_banners(now)
            .iter()
            .map(|b| b.id.as_str())
            .collect();
        assert_eq!(active, ["always", "now"]);
        let events: Vec<_> = config
            .visible_events(now)
            .iter()
            .map(|e| e.id.as_str())
            .collect();
        assert_eq!(events, ["live", "later"]);
        assert_eq!(config.events[1].phase(now), EventPhase::Live);
        assert_eq!(config.events[0].phase(now), EventPhase::Upcoming);
    }

    #[test]
    fn templates_render_placeholders() {
        let template = PresenceTemplate {
            details: Some("{instance} · {version}".into()),
            state: Some("{server}".into()),
            large_text: Some("{unknown}".into()),
            buttons: vec![
                PresenceButton {
                    label: "Katıl".into(),
                    url: "https://projects.gg/{instance}".into(),
                },
                PresenceButton {
                    label: "{missing}".into(),
                    url: "https://x".into(),
                },
            ],
            ..PresenceTemplate::default()
        };
        let lookup = |name: &str| match name {
            "instance" => Some("Projects".to_owned()),
            "version" => Some("1.21.1".to_owned()),
            _ => None,
        };
        let rendered = template.render(&lookup);
        assert_eq!(rendered.details.as_deref(), Some("Projects · 1.21.1"));
        assert_eq!(rendered.state, None, "empty after filling");
        assert_eq!(rendered.large_text, None);
        assert_eq!(rendered.buttons.len(), 1);
        assert_eq!(rendered.buttons[0].url, "https://projects.gg/Projects");
        assert_eq!(fill_placeholders("{a b} {} {", &lookup), "{a b} {} {");
    }
}
