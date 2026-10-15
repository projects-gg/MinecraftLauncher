//! Wire formats. Every type is `serde` (camelCase) + `specta`; optional fields have defaults
//! and unknown fields are ignored, so older clients keep working when Studio adds fields.
//! Enums read from remote documents end with an `unknown` catch-all for the same reason.
//! The normative description lives in `docs/content-format.md`.

mod common;
mod config;
mod index;
mod news;
mod pack;
mod release;

pub use common::{Cta, DocRef, FileRef, MediaKind, MediaRef};
pub use config::{
    Banner, BannerLevel, CustomLink, DiscordConfig, EventCountdown, EventPhase, Features, Hero,
    LAUNCHER_CONFIG_SCHEMA, LauncherConfig, Links, Maintenance, Particles, PresenceButton,
    PresenceTemplate, PresenceTemplates, Season, ServerEntry, Theme, fill_placeholders,
};
pub use index::{
    AfkRelease, PLATFORM_KEYS, PackRef, ROOT_INDEX_SCHEMA, RootIndex, current_platform_key,
};
pub use jiff::Timestamp;
pub use news::{NEWS_FEED_SCHEMA, NewsCategory, NewsFeed, NewsItem, compare_news};
pub use pack::{
    ConfigPolicy, EnvSupport, FilePolicy, MRPACK_FORMAT_VERSION, OptionalGroup, PackDependencies,
    PackEnv, PackFile, PackHashes, PackLoader, PackManifest, PackServer, PolicyRule,
    ProjectsExtension,
};
pub use release::{ReleaseChannel, ReleasePlatform, UpdateDecision, rollout_bucket};

pub(crate) fn default_schema() -> u32 {
    1
}

pub(crate) fn default_true() -> bool {
    true
}

/// True for absolute `http://` or `https://` URLs.
pub(crate) fn is_http_url(text: &str) -> bool {
    url::Url::parse(text).is_ok_and(|url| matches!(url.scheme(), "http" | "https"))
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
