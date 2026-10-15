use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{Cta, MediaRef, check_schema, default_true};
use crate::error::Result;

/// Highest `LauncherConfig.schema` this build understands.
pub const LAUNCHER_CONFIG_SCHEMA: u32 = 1;

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
        }
    }
}

impl LauncherConfig {
    pub fn check_schema(&self) -> Result<()> {
        check_schema("launcher config", self.schema, LAUNCHER_CONFIG_SCHEMA)
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

    /// Events that have not ended yet, soonest first.
    pub fn visible_events(&self, now: Timestamp) -> Vec<&EventCountdown> {
        let mut events: Vec<_> = self
            .events
            .iter()
            .filter(|e| e.phase(now) != EventPhase::Ended)
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
    /// Additional links (social media, wiki, …).
    pub custom: Vec<CustomLink>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CustomLink {
    pub id: String,
    pub label: String,
    pub url: String,
    /// Icon name from the launcher's icon set.
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

/// Feature flags; everything is on unless Studio turns it off.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct Features {
    pub afk: bool,
    pub premium_login: bool,
    pub offline_accounts: bool,
    pub modrinth_browser: bool,
}

impl Default for Features {
    fn default() -> Self {
        Self {
            afk: true,
            premium_login: true,
            offline_accounts: true,
            modrinth_browser: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ts(text: &str) -> Timestamp {
        text.parse().unwrap()
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
                EventCountdown {
                    id: "later".into(),
                    title: "Later".into(),
                    starts_at: ts("2026-11-01T18:00:00Z"),
                    ends_at: ts("2026-11-01T20:00:00Z"),
                    image: None,
                    url: None,
                },
                EventCountdown {
                    id: "live".into(),
                    title: "Live".into(),
                    starts_at: ts("2026-10-15T11:00:00Z"),
                    ends_at: ts("2026-10-15T14:00:00Z"),
                    image: None,
                    url: None,
                },
                EventCountdown {
                    id: "over".into(),
                    title: "Over".into(),
                    starts_at: ts("2026-10-01T11:00:00Z"),
                    ends_at: ts("2026-10-01T14:00:00Z"),
                    image: None,
                    url: None,
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
