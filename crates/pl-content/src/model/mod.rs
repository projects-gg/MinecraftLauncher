//! Wire formats. Every type is `serde` (camelCase) + `specta`; optional fields have defaults
//! and unknown fields are ignored, so older clients keep working when Studio adds fields.
//! Enums read from remote documents end with an `unknown` catch-all for the same reason.
//! The normative description lives in `docs/content-format.md`.

mod chatemoji;
mod common;
mod config;
mod feed;
mod index;
mod itemart;
mod links;
mod modset;
mod news;
mod overlay;
mod pack;
mod release;
mod subserver;

pub use chatemoji::{
    CHAT_EMOJIS_SCHEMA, ChatEmoji, ChatEmojiFault, ChatEmojiKind, ChatEmojiMeaning,
    ChatEmojiProblem, ChatEmojis, DEFAULT_CHAT_EMOJI_HOSTS, MAX_CHAT_EMOJI_ALIASES,
    MAX_CHAT_EMOJI_CODE_CHARS, MAX_CHAT_EMOJI_HOSTS, MAX_CHAT_EMOJI_LOOK_CHARS,
    MAX_CHAT_EMOJI_MEANING_CHARS, MAX_CHAT_EMOJI_RANK_CHARS, MAX_CHAT_EMOJI_SERVER_CHARS,
    MAX_CHAT_EMOJI_SERVERS, MAX_CHAT_EMOJIS, emoji_fault, is_chat_emoji_code, look_problem,
};
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
    FEED_MAX_MINECRAFT, FEED_MAX_MIRRORS, FEED_MAX_SUMMARY_CHARS, FEED_MAX_TITLE_CHARS,
    FeedConfigProblem, FeedEvent, FeedIssue, FeedItem, FeedKey, FeedOverride, FeedProblem,
    FeedServiceState, FeedSource, FeedSourceKind, FeedStatus, FeedStatusLevel, FeedStatusService,
    FeedTarget, MINECRAFT_ARTICLE_HOSTS, MINECRAFT_MAX_LIFETIME, MINECRAFT_MAX_VERSION_CHARS,
    MinecraftNews, MinecraftNewsKind, MinecraftText, MinecraftTranslations, NewsFeedConfig,
    OPEN_EVENT_DURATION, STATUS_MAX_LIFETIME, STATUS_MAX_MESSAGE_CHARS,
    STATUS_MAX_SERVICE_NAME_CHARS, STATUS_MAX_SERVICES, STATUS_MAX_TITLE_CHARS, is_safe_link,
    is_valid_feed_cover, markdown_images, markdown_links, parse_feed_mirror,
};
pub use index::{
    AFK_AUTO_VERSION, AfkMinecraftVersion, AfkRelease, AfkVersionProblem, FamilyProblem,
    MAX_AFK_MINECRAFT_VERSIONS, PLATFORM_KEYS, PackRef, ROOT_INDEX_SCHEMA, RootIndex,
    compare_minecraft_versions, current_platform_key, family_problems,
};
pub use itemart::{
    DEFAULT_ITEM_ART_HOSTS, GlyphImage, ITEM_ART_SCHEMA, ItemArt, ItemArtAtlas, ItemArtEntry,
    ItemArtGlyph, ItemArtLookup, ItemArtProblem, MAX_ITEM_ART_ATLAS_COLUMNS,
    MAX_ITEM_ART_GLYPH_EDGE, MAX_ITEM_ART_GLYPH_OFFSET, MAX_ITEM_ART_GLYPHS, MAX_ITEM_ART_HOSTS,
    MAX_ITEM_ART_ID_LEN, MAX_ITEM_ART_ITEMS, MAX_ITEM_ART_NAME_CHARS, is_private_use,
    is_resource_location, is_valid_item_art_host, item_art_host_matches, normalize_item_id,
};
pub use jiff::Timestamp;
pub use links::{CommunityLink, CommunityPlatform};
pub use modset::{
    MAX_MOD_SET_ITEMS, MAX_MOD_SET_NAME_CHARS, MAX_MOD_SET_SUMMARY_CHARS, MAX_MOD_SET_VARIANTS,
    MAX_MOD_SETS, MOD_SET_CATALOG_SCHEMA, ModSet, ModSetCatalog, ModSetItemKind, ModSetProblem,
    ModSetPurpose, ModSetText, ModSetVariant, is_projects_hosted, loader_name,
};
pub use news::{NEWS_FEED_SCHEMA, NewsCategory, NewsFeed, NewsItem, compare_news};
pub use overlay::{
    MAX_OVERLAY_JAR_BYTES, MAX_OVERLAY_JARS, MAX_OVERLAY_RANGE_LEN, MAX_OVERLAY_VERSION_LEN,
    OVERLAY_CI_SCHEMA, OVERLAY_JAR_PREFIX, OverlayCiCount, OverlayCiJar, OverlayCiRelease,
    OverlayCiTests, OverlayJar, OverlayJarFault, OverlayProblem, OverlayRelease,
    is_overlay_version, overlay_jar_fault,
};
pub use pack::{
    ConfigPolicy, EnvSupport, FilePolicy, ItemCategory, ItemProblem, MRPACK_FORMAT_VERSION,
    ManifestProblem, ModrinthRef, OptionalGroup, PackDependencies, PackEnv, PackFile, PackHashes,
    PackItem, PackLoader, PackManifest, PackServer, PolicyRule, ProjectsExtension,
};
pub use release::{
    ReleaseChannel, ReleasePlatform, UpdateDecision, release_bundle_allowed, release_channel_urls,
    rollout_bucket,
};
pub use subserver::{
    MAX_SUB_SERVER_COMMAND_CHARS, MAX_SUB_SERVER_ID_CHARS, MAX_SUB_SERVER_NAME_CHARS,
    MAX_SUB_SERVERS, SUB_SERVER_HOST, SubServer, SubServerFault, SubServerProblem,
    is_sub_server_command, is_sub_server_host, is_sub_server_id, sub_server_fault,
};

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
