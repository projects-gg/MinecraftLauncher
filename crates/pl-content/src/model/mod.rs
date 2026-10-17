//! Wire formats. Every type is `serde` (camelCase) + `specta`; optional fields have defaults
//! and unknown fields are ignored, so older clients keep working when Studio adds fields.
//! Enums read from remote documents end with an `unknown` catch-all for the same reason.
//! The normative description lives in `docs/content-format.md`.

mod common;
mod config;
mod feed;
mod index;
mod news;
mod pack;
mod release;

pub use common::{Cta, DocRef, FileRef, MediaKind, MediaRef};
pub use config::{
    Banner, BannerLevel, CustomLink, DEFAULT_RECOMMENDED_MINECRAFT_VERSION, DiscordConfig,
    EventCountdown, EventPhase, Features, Hero, LAUNCHER_CONFIG_SCHEMA, LauncherConfig, Links,
    Maintenance, Particles, PresenceButton, PresenceTemplate, PresenceTemplates, Season,
    ServerEntry, Theme, fill_placeholders,
};
pub use feed::{
    BOT_FEED_SCHEMA, BotFeed, FEED_DEFAULT_MAX_AGE_DAYS, FEED_MAX_AGE_DAYS, FEED_MAX_BODY_CHARS,
    FEED_MAX_BYTES, FEED_MAX_EVENTS, FEED_MAX_ITEMS, FEED_MAX_KEYS, FEED_MAX_MEDIA_BYTES,
    FEED_MAX_MIRRORS, FEED_MAX_SUMMARY_CHARS, FEED_MAX_TITLE_CHARS, FeedConfigProblem, FeedEvent,
    FeedIssue, FeedItem, FeedKey, FeedOverride, FeedProblem, FeedServiceState, FeedSource,
    FeedSourceKind, FeedStatus, FeedStatusLevel, FeedStatusService, FeedTarget, NewsFeedConfig,
    OPEN_EVENT_DURATION, STATUS_MAX_LIFETIME, STATUS_MAX_MESSAGE_CHARS,
    STATUS_MAX_SERVICE_NAME_CHARS, STATUS_MAX_SERVICES, STATUS_MAX_TITLE_CHARS, is_safe_link,
    is_valid_feed_cover, markdown_images, markdown_links, parse_feed_mirror,
};
pub use index::{
    AFK_AUTO_VERSION, AfkMinecraftVersion, AfkRelease, AfkVersionProblem, FamilyProblem,
    MAX_AFK_MINECRAFT_VERSIONS, PLATFORM_KEYS, PackRef, ROOT_INDEX_SCHEMA, RootIndex,
    compare_minecraft_versions, current_platform_key, family_problems,
};
pub use jiff::Timestamp;
pub use news::{NEWS_FEED_SCHEMA, NewsCategory, NewsFeed, NewsItem, compare_news};
pub use pack::{
    ConfigPolicy, EnvSupport, FilePolicy, ItemCategory, ItemProblem, MRPACK_FORMAT_VERSION,
    ManifestProblem, ModrinthRef, OptionalGroup, PackDependencies, PackEnv, PackFile, PackHashes,
    PackItem, PackLoader, PackManifest, PackServer, PolicyRule, ProjectsExtension,
};
pub use release::{ReleaseChannel, ReleasePlatform, UpdateDecision, rollout_bucket};

/// Longest family or item id, in bytes.
pub const MAX_ID_LEN: usize = 64;

/// Longest Minecraft version id the content format carries, in bytes.
pub const MAX_MINECRAFT_VERSION_LEN: usize = 32;

/// Minecraft version ids as documents carry them (the AFK client's version values,
/// `recommendedMinecraftVersion`): 1–[`MAX_MINECRAFT_VERSION_LEN`] bytes without whitespace or
/// control characters, so snapshots and pre-releases fit (`26w41a`, `1.21.4-pre1`). Whether
/// Mojang lists the id is not checked here.
pub fn is_valid_minecraft_version(version: &str) -> bool {
    (1..=MAX_MINECRAFT_VERSION_LEN).contains(&version.len())
        && !version.chars().any(|c| c.is_whitespace() || c.is_control())
}

pub(crate) fn default_schema() -> u32 {
    1
}

pub(crate) fn default_true() -> bool {
    true
}

/// `skip_serializing_if` for fields left out while they hold their default (`priority: 0`).
pub(crate) fn is_default<T: Default + PartialEq>(value: &T) -> bool {
    *value == T::default()
}

/// True for absolute `http://` or `https://` URLs.
pub(crate) fn is_http_url(text: &str) -> bool {
    url::Url::parse(text).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
}

/// True for absolute `https://` URLs.
pub(crate) fn is_https_url(text: &str) -> bool {
    url::Url::parse(text).is_ok_and(|url| url.scheme() == "https")
}

/// Family and item ids: 1–[`MAX_ID_LEN`] characters from `[A-Za-z0-9._-]`, not `.` or `..`.
/// Modrinth project ids and slugs fit.
pub fn is_valid_id(id: &str) -> bool {
    (1..=MAX_ID_LEN).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
        && id != "."
        && id != ".."
}

pub(crate) fn check_schema(what: &str, found: u32, supported: u32) -> crate::Result<()> {
    if found > supported || found == 0 {
        return Err(crate::Error::UnsupportedSchema {
            what: what.to_owned(),
            found,
            supported,
        });
    }
    Ok(())
}
