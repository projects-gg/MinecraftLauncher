//! The Discord bot's news feed (`feed.json`) and its launcher config (`LauncherConfig.newsFeed`).
//!
//! The bot publishes a full snapshot ([`BotFeed`]) on its own mirrors and signs it with a feed
//! key. Launchers trust that key only while Studio lists it in the signed launcher config
//! ([`NewsFeedConfig`]), so delegation, revocation and rotation need no launcher release. A feed
//! key can only add news: items are never pinned, links open only `https://` and `projects://`,
//! covers are small images verified by size and SHA-256, and one broken item never hides the
//! others ([`BotFeed::parse`]). The feed may also carry a short-lived notice about the servers
//! ([`FeedStatus`]), shown only until it expires, and Minecraft's own updates ([`MinecraftNews`]),
//! which only fill places Projects news leaves empty.
//!
//! The normative description lives in `docs/content-format.md` ("News feed (Discord bot)").

use std::collections::{HashMap, HashSet};
use std::ops::RangeInclusive;

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use url::Url;

use super::{
    Cta, EventPhase, MediaKind, MediaRef, NewsCategory, NewsItem, check_schema, default_schema,
    is_valid_id,
};
use crate::error::{Error, Result};
use crate::fsutil::sha256_hex;
use crate::signing::{VerifyingKey, decode_verifying_key, key_id};

/// Highest `BotFeed.schema` this build understands.
pub const BOT_FEED_SCHEMA: u32 = 1;
/// Most items one feed may carry.
pub const FEED_MAX_ITEMS: usize = 50;
/// Most events one feed may carry.
pub const FEED_MAX_EVENTS: usize = 20;
/// Largest `feed.json`, in bytes.
pub const FEED_MAX_BYTES: u64 = 512 * 1024;
/// Largest media file a feed item may reference, in bytes.
pub const FEED_MAX_MEDIA_BYTES: u64 = 2 * 1024 * 1024;
/// Longest item or event title, in characters.
pub const FEED_MAX_TITLE_CHARS: usize = 120;
/// Longest item summary, in characters.
pub const FEED_MAX_SUMMARY_CHARS: usize = 300;
/// Longest item body (Markdown), in characters.
pub const FEED_MAX_BODY_CHARS: usize = 8_000;
/// Most mirrors and keys `newsFeed` may list.
pub const FEED_MAX_MIRRORS: usize = 4;
pub const FEED_MAX_KEYS: usize = 4;
/// `newsFeed.maxAgeDays` when it is missing (or out of range on a client).
pub const FEED_DEFAULT_MAX_AGE_DAYS: u32 = 60;
/// Allowed `newsFeed.maxAgeDays`.
pub const FEED_MAX_AGE_DAYS: RangeInclusive<u32> = 1..=365;
/// How long a feed event without `endsAt` counts as live.
pub const OPEN_EVENT_DURATION: SignedDuration = SignedDuration::from_hours(3);
/// Longest `status.title`, in characters.
pub const STATUS_MAX_TITLE_CHARS: usize = 80;
/// Longest `status.message`, in characters.
pub const STATUS_MAX_MESSAGE_CHARS: usize = 200;
/// Most entries `status.services` may list.
pub const STATUS_MAX_SERVICES: usize = 20;
/// Longest `status.services[].name`, in characters.
pub const STATUS_MAX_SERVICE_NAME_CHARS: usize = 64;
/// Latest `status.expiresAt` after its `updatedAt`.
pub const STATUS_MAX_LIFETIME: SignedDuration = SignedDuration::from_mins(15);
/// Most `minecraft[]` entries one feed may carry; a longer array is ignored as a whole.
pub const FEED_MAX_MINECRAFT: usize = 10;
/// Longest `minecraft[].version`, in characters.
pub const MINECRAFT_MAX_VERSION_CHARS: usize = 40;
/// Latest `minecraft[].expiresAt` after its `publishedAt`.
pub const MINECRAFT_MAX_LIFETIME: SignedDuration = SignedDuration::from_hours(24 * 90);
/// Hosts a `minecraft[].source.url` may point at: the official article.
pub const MINECRAFT_ARTICLE_HOSTS: [&str; 2] = ["minecraft.net", "www.minecraft.net"];

/// `LauncherConfig.newsFeed`: whether launchers read the bot's feed, from where, and which keys
/// may sign it. Writers omit the object while `enabled` is false (Studio keeps it locally).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsFeedConfig {
    pub enabled: bool,
    /// Feed mirror bases (`https://`, ending in `/`), tried in order.
    pub mirrors: Vec<String>,
    /// Ed25519 keys the bot may sign `feed.json` with.
    pub keys: Vec<FeedKey>,
    /// Feed items published longer ago than this are hidden.
    pub max_age_days: u32,
}

impl Default for NewsFeedConfig {
    fn default() -> Self {
        Self {
            enabled: false,
            mirrors: Vec::new(),
            keys: Vec::new(),
            max_age_days: FEED_DEFAULT_MAX_AGE_DAYS,
        }
    }
}

/// A key the bot may sign the feed with.
#[derive(Debug, Clone, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct FeedKey {
    /// [`key_id`] of `publicKey`: lowercase hex of the first 8 bytes of its SHA-256.
    pub key_id: String,
    /// Base64 of the 32-byte Ed25519 public key.
    pub public_key: String,
}

impl FeedKey {
    /// A key entry for `public_key` (base64) with its computed id.
    pub fn from_public_key(public_key: &str) -> Result<Self> {
        let key = decode_verifying_key(public_key)?;
        Ok(Self {
            key_id: key_id(&key),
            public_key: crate::signing::encode_verifying_key(&key),
        })
    }

    /// The key when `publicKey` decodes and `keyId` is exactly its id.
    pub fn verifying_key(&self) -> Option<VerifyingKey> {
        let key = decode_verifying_key(&self.public_key).ok()?;
        (key_id(&key) == self.key_id).then_some(key)
    }
}

/// Why Studio refuses to publish a `newsFeed` (clients skip the broken parts instead).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FeedConfigProblem {
    NoMirrors,
    TooManyMirrors,
    /// Not an `https://` base with a host and a path ending in `/` (no credentials, query or
    /// fragment).
    BadMirror {
        mirror: String,
    },
    DuplicateMirror {
        mirror: String,
    },
    NoKeys,
    TooManyKeys,
    /// The public key is not base64 of a valid 32-byte Ed25519 point.
    BadKey {
        key_id: String,
    },
    /// `keyId` is not the id of the public key.
    KeyIdMismatch {
        key_id: String,
        expected: String,
    },
    DuplicateKey {
        key_id: String,
    },
    BadMaxAge {
        days: u32,
    },
}

impl NewsFeedConfig {
    /// Everything that keeps the config from being published as it is. Empty when valid.
    pub fn problems(&self) -> Vec<FeedConfigProblem> {
        let mut problems = Vec::new();
        if self.mirrors.is_empty() {
            problems.push(FeedConfigProblem::NoMirrors);
        } else if self.mirrors.len() > FEED_MAX_MIRRORS {
            problems.push(FeedConfigProblem::TooManyMirrors);
        }
        let mut seen = HashSet::new();
        for mirror in &self.mirrors {
            match parse_feed_mirror(mirror) {
                None => problems.push(FeedConfigProblem::BadMirror {
                    mirror: mirror.clone(),
                }),
                Some(url) => {
                    if !seen.insert(url) {
                        problems.push(FeedConfigProblem::DuplicateMirror {
                            mirror: mirror.clone(),
                        });
                    }
                }
            }
        }

        if self.keys.is_empty() {
            problems.push(FeedConfigProblem::NoKeys);
        } else if self.keys.len() > FEED_MAX_KEYS {
            problems.push(FeedConfigProblem::TooManyKeys);
        }
        let mut seen = HashSet::new();
        for key in &self.keys {
            let Ok(decoded) = decode_verifying_key(&key.public_key) else {
                problems.push(FeedConfigProblem::BadKey {
                    key_id: key.key_id.clone(),
                });
                continue;
            };
            let expected = key_id(&decoded);
            if key.key_id != expected {
                problems.push(FeedConfigProblem::KeyIdMismatch {
                    key_id: key.key_id.clone(),
                    expected,
                });
            } else if !seen.insert(expected) {
                problems.push(FeedConfigProblem::DuplicateKey {
                    key_id: key.key_id.clone(),
                });
            }
        }

        if !FEED_MAX_AGE_DAYS.contains(&self.max_age_days) {
            problems.push(FeedConfigProblem::BadMaxAge {
                days: self.max_age_days,
            });
        }
        problems
    }

    /// The valid mirrors in order, without duplicates, at most [`FEED_MAX_MIRRORS`].
    pub fn mirror_bases(&self) -> Vec<Url> {
        let mut bases: Vec<Url> = Vec::new();
        for url in self.mirrors.iter().filter_map(|m| parse_feed_mirror(m)) {
            if bases.len() < FEED_MAX_MIRRORS && !bases.contains(&url) {
                bases.push(url);
            }
        }
        bases
    }

    /// The keys that decode and match their `keyId`, without duplicates, at most
    /// [`FEED_MAX_KEYS`].
    pub fn trusted_keys(&self) -> Vec<VerifyingKey> {
        let mut keys: Vec<VerifyingKey> = Vec::new();
        for key in self.keys.iter().filter_map(FeedKey::verifying_key) {
            if keys.len() < FEED_MAX_KEYS && !keys.contains(&key) {
                keys.push(key);
            }
        }
        keys
    }

    /// `maxAgeDays`, or the default when it is out of range.
    pub fn effective_max_age_days(&self) -> u32 {
        if FEED_MAX_AGE_DAYS.contains(&self.max_age_days) {
            self.max_age_days
        } else {
            FEED_DEFAULT_MAX_AGE_DAYS
        }
    }
}

/// A feed mirror base: `https://` with a host, no credentials, query or fragment, and a path
/// ending in `/`. Surrounding whitespace is refused, not trimmed.
pub fn parse_feed_mirror(text: &str) -> Option<Url> {
    if text != text.trim() {
        return None;
    }
    let url = Url::parse(text).ok()?;
    let valid = url.scheme() == "https"
        && url.host_str().is_some_and(|host| !host.is_empty())
        && url.username().is_empty()
        && url.password().is_none()
        && url.query().is_none()
        && url.fragment().is_none()
        && url.path().ends_with('/');
    valid.then_some(url)
}

/// Links feed content may open: `https://` URLs with a host (no credentials) and `projects://`
/// deep links. Everything else (`http://`, `mailto:`, `javascript:`, …) is shown as plain text.
pub fn is_safe_link(text: &str) -> bool {
    if text.is_empty() || text != text.trim() || text.chars().any(char::is_control) {
        return false;
    }
    if let Some(rest) = text.strip_prefix("projects://") {
        return !rest.is_empty() && !rest.contains(char::is_whitespace);
    }
    Url::parse(text).is_ok_and(|url| {
        url.scheme() == "https"
            && url.host_str().is_some_and(|host| !host.is_empty())
            && url.username().is_empty()
            && url.password().is_none()
    })
}

/// `feed.json`: the bot's full snapshot of what it publishes. Read it with [`BotFeed::parse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BotFeed {
    #[serde(default = "default_schema")]
    pub schema: u32,
    /// Strictly increasing (`max(previous + 1, unix seconds)`); clients refuse a lower one.
    pub sequence: u64,
    pub generated_at: Timestamp,
    #[serde(default)]
    pub items: Vec<FeedItem>,
    #[serde(default)]
    pub events: Vec<FeedEvent>,
    /// What players should know about the servers right now; missing while all is well.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub status: Option<FeedStatus>,
    /// Minecraft's own updates, shown only where Projects news leaves room; missing when there
    /// are none.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub minecraft: Vec<MinecraftNews>,
}

/// A news item of the feed: a [`NewsItem`] plus where it came from. `cover.path` is relative to
/// the feed mirror.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FeedItem {
    /// `discord-<messageId>`; edits keep it.
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    /// Markdown.
    #[serde(default)]
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<MediaRef>,
    #[serde(default)]
    pub category: NewsCategory,
    #[serde(default)]
    pub tags: Vec<String>,
    pub published_at: Timestamp,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    /// Never set by the bot; clients ignore it.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub pinned: bool,
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub featured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cta: Option<Cta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<FeedSource>,
}

impl FeedItem {
    /// The item as a home screen news item (never pinned).
    pub fn to_news_item(&self) -> NewsItem {
        NewsItem {
            id: self.id.clone(),
            title: self.title.clone(),
            summary: self.summary.clone(),
            body: self.body.clone(),
            cover: self.cover.clone(),
            category: self.category,
            tags: self.tags.clone(),
            published_at: self.published_at,
            expires_at: self.expires_at,
            pinned: false,
            pin_order: None,
            featured: self.featured,
            hidden: false,
            cta: self.cta.clone(),
            author: self.author.clone(),
            feed_override: None,
        }
    }

    /// Visible at `now` by the [`NewsItem`] rules (the age limit is applied when merging).
    pub fn is_visible(&self, now: Timestamp) -> bool {
        self.published_at <= now && self.expires_at.is_none_or(|expires| now < expires)
    }

    /// What Studio keeps in [`FeedOverride::fingerprint`] to tell whether the item changed after
    /// an admin took it over: the SHA-256 (lowercase hex) of what launchers show of it (title,
    /// summary, body, category, tags, dates, the cover's SHA-256 and the button). `id`, `source`
    /// and the flags the feed may not use are left out, so a renamed channel changes nothing.
    pub fn fingerprint(&self) -> String {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Print<'a> {
            title: &'a str,
            summary: &'a str,
            body: &'a str,
            category: NewsCategory,
            tags: &'a [String],
            published_at: Timestamp,
            expires_at: Option<Timestamp>,
            cover: Option<&'a str>,
            cta: Option<&'a Cta>,
        }
        fingerprint_of(&Print {
            title: &self.title,
            summary: &self.summary,
            body: &self.body,
            category: self.category,
            tags: &self.tags,
            published_at: self.published_at,
            expires_at: self.expires_at,
            cover: self.cover.as_ref().map(|cover| cover.sha256.as_str()),
            cta: self.cta.as_ref(),
        })
    }
}

/// SHA-256 (lowercase hex) of the JSON of a fixed-shape value.
fn fingerprint_of<T: Serialize>(value: &T) -> String {
    // Plain strings, numbers and timestamps: serializing cannot fail.
    let bytes = serde_json::to_vec(value).unwrap_or_default();
    sha256_hex(&bytes)
}

/// Studio's record of the feed item, event or Minecraft entry a Studio entry with the same id
/// replaces or hides (`NewsItem.feedOverride`, `EventCountdown.feedOverride`). Launchers ignore
/// it: Studio wins on a shared id with or without it. Studio compares `fingerprint` with the
/// current feed entry's ([`FeedItem::fingerprint`], [`FeedEvent::fingerprint`],
/// [`MinecraftNews::fingerprint`]) to tell the admin that the original changed since.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct FeedOverride {
    /// Fingerprint of the feed entry when the admin took it over (or last accepted a change).
    pub fingerprint: String,
    /// The original Discord message, or a Minecraft entry's article (`https://`), kept for
    /// Studio while the entry is gone from the feed.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub source_url: Option<String>,
}

/// An event of the feed: the shape of `LauncherConfig.events[]` with an optional end.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FeedEvent {
    pub id: String,
    pub title: String,
    pub starts_at: Timestamp,
    /// Missing: live for [`OPEN_EVENT_DURATION`] after `startsAt`, shown without time left.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub ends_at: Option<Timestamp>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    /// The feed item the event came from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub news_id: Option<String>,
    /// Like `LauncherConfig.events[].priority`: among live events the highest leads. Default 0.
    #[serde(default, skip_serializing_if = "super::is_default")]
    pub priority: i32,
    /// Like `LauncherConfig.events[].join`: the event takes place on the server ("Katıl" on the
    /// card while it is live). Default `false`.
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub join: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub source: Option<FeedSource>,
}

impl FeedEvent {
    /// When the event ends: `endsAt`, or `startsAt` + [`OPEN_EVENT_DURATION`].
    pub fn end(&self) -> Timestamp {
        self.ends_at
            .unwrap_or_else(|| saturating_add(self.starts_at, OPEN_EVENT_DURATION))
    }

    pub fn phase(&self, now: Timestamp) -> EventPhase {
        if now < self.starts_at {
            EventPhase::Upcoming
        } else if now < self.end() {
            EventPhase::Live
        } else {
            EventPhase::Ended
        }
    }

    /// Like [`FeedItem::fingerprint`]: the SHA-256 (lowercase hex) of the event's title, times,
    /// link, news item, priority and `join`. `join` is left out while `false`, so the events
    /// Studio took over before it existed keep their fingerprints.
    pub fn fingerprint(&self) -> String {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Print<'a> {
            title: &'a str,
            starts_at: Timestamp,
            ends_at: Option<Timestamp>,
            url: Option<&'a str>,
            news_id: Option<&'a str>,
            priority: i32,
            #[serde(skip_serializing_if = "std::ops::Not::not")]
            join: bool,
        }
        fingerprint_of(&Print {
            title: &self.title,
            starts_at: self.starts_at,
            ends_at: self.ends_at,
            url: self.url.as_deref(),
            news_id: self.news_id.as_deref(),
            priority: self.priority,
            join: self.join,
        })
    }
}

/// `t + d`, clamped to the representable range.
pub(crate) fn saturating_add(t: Timestamp, d: SignedDuration) -> Timestamp {
    t.checked_add(d).unwrap_or(if d.is_negative() {
        Timestamp::MIN
    } else {
        Timestamp::MAX
    })
}

/// Where a feed item or event came from.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FeedSource {
    #[serde(default)]
    pub kind: FeedSourceKind,
    /// Display name of the channel (`duyurular`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub channel: Option<String>,
    /// Link to the original message (`https://` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum FeedSourceKind {
    Discord,
    /// Mojang's public launcher content (`minecraft[]` entries); `url` is the official article.
    Minecraft,
    /// A source added by a newer bot.
    #[default]
    #[serde(other)]
    Unknown,
}

/// `feed.json` → `minecraft[]`: one of Minecraft's own updates (a Java Edition release or
/// snapshot), summarized by the bot in its own words, with the official article in `source.url`.
/// Launchers show it only in places Projects news leaves empty: never as the hero, in the patch
/// notes box or on the event card, never pinned, featured or "new". Read with
/// [`BotFeed::parse`]; visible while [`MinecraftNews::is_visible`] and within `maxAgeDays`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftNews {
    /// `minecraft-<version>`.
    pub id: String,
    /// Mojang's version id (`26.3`, `26.1.2`, `26.4-snapshot-4`).
    pub version: String,
    #[serde(default)]
    pub kind: MinecraftNewsKind,
    /// In the feed's language (Turkish), like an item's texts.
    pub title: String,
    #[serde(default)]
    pub summary: String,
    /// Markdown.
    #[serde(default)]
    pub body: String,
    /// Relative to the feed mirror, with the rules of item covers.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<MediaRef>,
    /// When Mojang released the version.
    pub published_at: Timestamp,
    /// After `publishedAt`, at most [`MINECRAFT_MAX_LIFETIME`] later.
    pub expires_at: Timestamp,
    /// `kind: minecraft` and the official article (`https://` on [`MINECRAFT_ARTICLE_HOSTS`]).
    pub source: FeedSource,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub translations: Option<MinecraftTranslations>,
}

impl MinecraftNews {
    /// Shown at `now`: `publishedAt <= now < expiresAt` (the age limit is applied when merging).
    pub fn is_visible(&self, now: Timestamp) -> bool {
        self.published_at <= now && now < self.expires_at
    }

    /// The official article: `source.url` (always set on an entry [`BotFeed::parse`] kept).
    pub fn article_url(&self) -> &str {
        self.source.url.as_deref().unwrap_or_default()
    }

    /// Like [`FeedItem::fingerprint`], for Studio's record of a hidden entry
    /// ([`FeedOverride::fingerprint`]): the SHA-256 (lowercase hex) of what launchers show of it
    /// (title, summary, body, the cover's SHA-256, `publishedAt` and the English texts), so Studio
    /// can tell the admin that the bot rewrote an entry they hid. `id`, `version`, `kind`,
    /// `expiresAt` and `source` are left out: they are not shown, and a new expiry is no rewrite.
    pub fn fingerprint(&self) -> String {
        #[derive(Serialize)]
        #[serde(rename_all = "camelCase")]
        struct Print<'a> {
            title: &'a str,
            summary: &'a str,
            body: &'a str,
            cover: Option<&'a str>,
            published_at: Timestamp,
            #[serde(skip_serializing_if = "Option::is_none")]
            en: Option<&'a MinecraftText>,
        }
        fingerprint_of(&Print {
            title: &self.title,
            summary: &self.summary,
            body: &self.body,
            cover: self.cover.as_ref().map(|cover| cover.sha256.as_str()),
            published_at: self.published_at,
            en: self.translations.as_ref().and_then(|t| t.en.as_ref()),
        })
    }
}

/// Which kind of Minecraft version a [`MinecraftNews`] is about.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum MinecraftNewsKind {
    Release,
    /// Snapshots, pre-releases and release candidates; also any kind a newer bot adds.
    #[default]
    #[serde(other)]
    Snapshot,
}

/// A [`MinecraftNews`]'s texts in other languages (today only English).
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct MinecraftTranslations {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub en: Option<MinecraftText>,
}

/// One language's texts of a [`MinecraftNews`]; a missing `summary` or `body` falls back to the
/// base text. Each field has the limits of the field it replaces.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftText {
    pub title: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub summary: Option<String>,
    /// Markdown.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub body: Option<String>,
}

/// `feed.json` → `status`: what players should know about the servers right now, from the
/// server's status page (services down or degraded, or a warning posted there). Short-lived:
/// launchers show it only while `now < expiresAt` ([`FeedStatus::is_active`]) and the bot
/// republishes it before then while the problem lasts, so a bot that stops leaves no stale
/// notice. Purely informational; it never blocks playing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FeedStatus {
    #[serde(default)]
    pub level: FeedStatusLevel,
    /// 1–[`STATUS_MAX_TITLE_CHARS`] characters.
    pub title: String,
    /// At most [`STATUS_MAX_MESSAGE_CHARS`] characters; may be empty.
    #[serde(default)]
    pub message: String,
    /// The affected services, at most [`STATUS_MAX_SERVICES`].
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub services: Vec<FeedStatusService>,
    /// The status page (`https://` only).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub url: Option<String>,
    pub updated_at: Timestamp,
    /// After `updatedAt`, at most [`STATUS_MAX_LIFETIME`] later.
    pub expires_at: Timestamp,
}

impl FeedStatus {
    /// Shown at `now`: it has not expired.
    pub fn is_active(&self, now: Timestamp) -> bool {
        now < self.expires_at
    }
}

/// How serious a [`FeedStatus`] is.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum FeedStatusLevel {
    /// Services are down.
    Outage,
    /// Services are reachable only in part.
    Degraded,
    /// A notice posted on the status page; also any level added by a newer bot.
    #[default]
    #[serde(other)]
    Notice,
}

/// A service a [`FeedStatus`] is about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FeedStatusService {
    /// 1–[`STATUS_MAX_SERVICE_NAME_CHARS`] characters, as players know it.
    pub name: String,
    #[serde(default)]
    pub state: FeedServiceState,
}

/// What a [`FeedStatusService`] is going through.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum FeedServiceState {
    Down,
    Degraded,
    /// A state added by a newer bot.
    #[default]
    #[serde(other)]
    Unknown,
}

/// Item, event or status notice a [`FeedProblem`] is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FeedTarget {
    Item,
    Event,
    /// `status` (the problem's id is empty).
    Status,
    /// A `minecraft[]` entry, or the array itself when the problem's id is empty.
    Minecraft,
}

/// What was wrong with one item or event, the status notice or a Minecraft entry of an otherwise
/// valid feed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FeedIssue {
    /// Dropped: not an item, event or status notice of this schema (missing or mistyped fields).
    Unreadable,
    /// Dropped: the id is not 1–64 characters from `[A-Za-z0-9._-]`.
    BadId,
    /// Dropped: an earlier entry has the same id.
    DuplicateId,
    /// Dropped: the title is empty.
    EmptyTitle,
    /// Dropped: longer than [`FEED_MAX_TITLE_CHARS`] ([`STATUS_MAX_TITLE_CHARS`] for the status).
    TitleTooLong,
    /// Dropped: longer than [`FEED_MAX_SUMMARY_CHARS`].
    SummaryTooLong,
    /// Dropped: longer than [`FEED_MAX_BODY_CHARS`].
    BodyTooLong,
    /// Dropped: `endsAt` is not after `startsAt`.
    EndsBeforeStart,
    /// Dropped (status): the message is longer than [`STATUS_MAX_MESSAGE_CHARS`].
    MessageTooLong,
    /// Dropped (status): more than [`STATUS_MAX_SERVICES`] services, or a service name that is
    /// empty or longer than [`STATUS_MAX_SERVICE_NAME_CHARS`].
    BadServices,
    /// Dropped: `expiresAt` is not after `updatedAt` (status) or `publishedAt` (Minecraft), or more
    /// than [`STATUS_MAX_LIFETIME`] or [`MINECRAFT_MAX_LIFETIME`] after it.
    BadExpiry,
    /// Dropped (Minecraft): `version` is missing, empty, longer than
    /// [`MINECRAFT_MAX_VERSION_CHARS`] or has characters outside `[A-Za-z0-9._-]`.
    BadVersion,
    /// Dropped (Minecraft): `source.url` is missing or not an `https://` link to the official
    /// article ([`MINECRAFT_ARTICLE_HOSTS`]); the link is part of the attribution.
    MissingSource,
    /// Ignored (Minecraft): `minecraft` is not an array or lists more than
    /// [`FEED_MAX_MINECRAFT`] entries; the rest of the feed is used.
    BadList,
    /// Kept without its cover (not an image of at most 2 MB with a safe path and a SHA-256).
    BadCover,
    /// Kept without its `cta` or `url` (not a safe link, or a button without a label or target;
    /// the status page link must be `https://`).
    BadLink,
    /// Kept without `source.url` (not `https://`).
    BadSourceLink,
    /// Kept without `newsId` (no such item in the feed).
    UnknownNews,
    /// Kept: the body links somewhere other than `https://` or `projects://`; launchers show
    /// such links as plain text.
    UnsafeBodyLink,
    /// Kept without that translation (Minecraft): not an object, or a text breaking the limits
    /// of the field it replaces; the base texts are shown.
    BadTranslation,
}

impl FeedIssue {
    /// True when the item, event, notice or entry (or the Minecraft array) is not shown at all.
    pub fn drops(self) -> bool {
        matches!(
            self,
            Self::Unreadable
                | Self::BadId
                | Self::DuplicateId
                | Self::EmptyTitle
                | Self::TitleTooLong
                | Self::SummaryTooLong
                | Self::BodyTooLong
                | Self::EndsBeforeStart
                | Self::MessageTooLong
                | Self::BadServices
                | Self::BadExpiry
                | Self::BadVersion
                | Self::MissingSource
                | Self::BadList
        )
    }
}

/// A problem of one item or event, found by [`BotFeed::parse`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct FeedProblem {
    pub target: FeedTarget,
    /// Id of the item or event (empty when it had none).
    pub id: String,
    pub issue: FeedIssue,
}

impl FeedProblem {
    fn item(id: &str, issue: FeedIssue) -> Self {
        Self {
            target: FeedTarget::Item,
            id: id.to_owned(),
            issue,
        }
    }

    fn event(id: &str, issue: FeedIssue) -> Self {
        Self {
            target: FeedTarget::Event,
            id: id.to_owned(),
            issue,
        }
    }

    fn status(issue: FeedIssue) -> Self {
        Self {
            target: FeedTarget::Status,
            id: String::new(),
            issue,
        }
    }

    /// A Minecraft entry's problem; an empty `id` means the whole array.
    fn minecraft(id: &str, issue: FeedIssue) -> Self {
        Self {
            target: FeedTarget::Minecraft,
            id: id.to_owned(),
            issue,
        }
    }
}

/// The feed before its items are read one by one. Unknown fields are ignored, so a field a
/// newer bot adds never breaks this reader.
#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawFeed {
    #[serde(default = "default_schema")]
    schema: u32,
    sequence: u64,
    generated_at: Timestamp,
    #[serde(default)]
    items: Vec<Value>,
    #[serde(default)]
    events: Vec<Value>,
    #[serde(default)]
    status: Option<Value>,
    #[serde(default)]
    minecraft: Option<Value>,
}

impl BotFeed {
    /// Reads `feed.json` bytes (after the signature verified). The size, `schema` and the item
    /// and event counts are whole-feed rules: breaking one fails the feed. A broken item or
    /// event is dropped or repaired on its own and reported (see [`FeedIssue`]); `pinned` is
    /// always cleared and cover posters are ignored. A broken status notice is dropped the same
    /// way (an unsafe status page link only loses the link); whether it has expired is
    /// [`FeedStatus::is_active`], checked when it is shown. Minecraft entries follow their own
    /// rules ([`MinecraftNews`]): a broken entry is dropped alone, a `minecraft` that is not an
    /// array or is too long is ignored as a whole, never the feed.
    pub fn parse(bytes: &[u8]) -> Result<(BotFeed, Vec<FeedProblem>)> {
        if bytes.len() as u64 > FEED_MAX_BYTES {
            return Err(Error::Invalid(format!(
                "feed.json is larger than {FEED_MAX_BYTES} bytes"
            )));
        }
        let raw: RawFeed =
            serde_json::from_slice(bytes).map_err(|e| Error::json("feed.json", e))?;
        check_schema("news feed", raw.schema, BOT_FEED_SCHEMA)?;
        if raw.items.len() > FEED_MAX_ITEMS {
            return Err(Error::Invalid(format!(
                "feed.json lists {} items; at most {FEED_MAX_ITEMS} are allowed",
                raw.items.len()
            )));
        }
        if raw.events.len() > FEED_MAX_EVENTS {
            return Err(Error::Invalid(format!(
                "feed.json lists {} events; at most {FEED_MAX_EVENTS} are allowed",
                raw.events.len()
            )));
        }
        let mut problems = Vec::new();
        let items = read_items(raw.items, &mut problems);
        let events = read_events(raw.events, &items, &mut problems);
        let status = read_status(raw.status, &mut problems);
        let minecraft = read_minecraft(raw.minecraft, &mut problems);
        Ok((
            BotFeed {
                schema: raw.schema,
                sequence: raw.sequence,
                generated_at: raw.generated_at,
                items,
                events,
                status,
                minecraft,
            },
            problems,
        ))
    }

    pub fn item(&self, id: &str) -> Option<&FeedItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Every cover the feed references: its items' and its Minecraft entries'.
    pub fn media_refs(&self) -> Vec<&MediaRef> {
        self.items
            .iter()
            .filter_map(|item| item.cover.as_ref())
            .chain(
                self.minecraft
                    .iter()
                    .filter_map(|entry| entry.cover.as_ref()),
            )
            .collect()
    }
}

fn raw_id(value: &Value) -> String {
    value
        .get("id")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .chars()
        .take(80)
        .collect()
}

fn too_long(text: &str, max: usize) -> bool {
    text.chars().nth(max).is_some()
}

/// Why an item's own text makes it unusable.
fn item_text_issue(item: &FeedItem) -> Option<FeedIssue> {
    if !is_valid_id(&item.id) {
        Some(FeedIssue::BadId)
    } else if item.title.trim().is_empty() {
        Some(FeedIssue::EmptyTitle)
    } else if too_long(&item.title, FEED_MAX_TITLE_CHARS) {
        Some(FeedIssue::TitleTooLong)
    } else if too_long(&item.summary, FEED_MAX_SUMMARY_CHARS) {
        Some(FeedIssue::SummaryTooLong)
    } else if too_long(&item.body, FEED_MAX_BODY_CHARS) {
        Some(FeedIssue::BodyTooLong)
    } else {
        None
    }
}

/// A cover the launcher may download: a safe path, a SHA-256, an image of 1 byte to 2 MB.
pub fn is_valid_feed_cover(media: &MediaRef) -> bool {
    media.validate().is_ok()
        && media.kind == MediaKind::Image
        && (1..=FEED_MAX_MEDIA_BYTES).contains(&media.size)
}

/// A usable call to action: a label and exactly one target, a safe link or a news id.
fn is_valid_cta(cta: &Cta) -> bool {
    let label = !cta.label.trim().is_empty();
    match (&cta.url, &cta.news_id) {
        (Some(url), None) => label && is_safe_link(url),
        (None, Some(id)) => label && is_valid_id(id),
        _ => false,
    }
}

fn clean_source(source: &mut Option<FeedSource>) -> bool {
    match source {
        Some(source) if source.url.as_deref().is_some_and(|url| !is_https(url)) => {
            source.url = None;
            false
        }
        _ => true,
    }
}

fn is_https(text: &str) -> bool {
    is_safe_link(text) && text.starts_with("https://")
}

fn read_items(raw: Vec<Value>, problems: &mut Vec<FeedProblem>) -> Vec<FeedItem> {
    let mut seen = HashSet::new();
    let mut items = Vec::new();
    for value in raw {
        let id = raw_id(&value);
        let mut item: FeedItem = match serde_json::from_value(value) {
            Ok(item) => item,
            Err(_) => {
                problems.push(FeedProblem::item(&id, FeedIssue::Unreadable));
                continue;
            }
        };
        if let Some(issue) = item_text_issue(&item) {
            problems.push(FeedProblem::item(&item.id, issue));
            continue;
        }
        if !seen.insert(item.id.clone()) {
            problems.push(FeedProblem::item(&item.id, FeedIssue::DuplicateId));
            continue;
        }
        item.pinned = false;
        if let Some(mut cover) = item.cover.take() {
            cover.poster = None;
            if is_valid_feed_cover(&cover) {
                item.cover = Some(cover);
            } else {
                problems.push(FeedProblem::item(&item.id, FeedIssue::BadCover));
            }
        }
        if let Some(cta) = item.cta.take() {
            if is_valid_cta(&cta) {
                item.cta = Some(cta);
            } else {
                problems.push(FeedProblem::item(&item.id, FeedIssue::BadLink));
            }
        }
        if !clean_source(&mut item.source) {
            problems.push(FeedProblem::item(&item.id, FeedIssue::BadSourceLink));
        }
        if markdown_links(&item.body)
            .iter()
            .any(|link| !is_safe_link(link))
        {
            problems.push(FeedProblem::item(&item.id, FeedIssue::UnsafeBodyLink));
        }
        items.push(item);
    }
    items
}

fn read_events(
    raw: Vec<Value>,
    items: &[FeedItem],
    problems: &mut Vec<FeedProblem>,
) -> Vec<FeedEvent> {
    let item_ids: HashSet<&str> = items.iter().map(|item| item.id.as_str()).collect();
    let mut seen = HashSet::new();
    let mut events = Vec::new();
    for value in raw {
        let id = raw_id(&value);
        let mut event: FeedEvent = match serde_json::from_value(value) {
            Ok(event) => event,
            Err(_) => {
                problems.push(FeedProblem::event(&id, FeedIssue::Unreadable));
                continue;
            }
        };
        let issue = if !is_valid_id(&event.id) {
            Some(FeedIssue::BadId)
        } else if event.title.trim().is_empty() {
            Some(FeedIssue::EmptyTitle)
        } else if too_long(&event.title, FEED_MAX_TITLE_CHARS) {
            Some(FeedIssue::TitleTooLong)
        } else if event.ends_at.is_some_and(|end| end <= event.starts_at) {
            Some(FeedIssue::EndsBeforeStart)
        } else {
            None
        };
        if let Some(issue) = issue {
            problems.push(FeedProblem::event(&event.id, issue));
            continue;
        }
        if !seen.insert(event.id.clone()) {
            problems.push(FeedProblem::event(&event.id, FeedIssue::DuplicateId));
            continue;
        }
        if event.url.as_deref().is_some_and(|url| !is_safe_link(url)) {
            event.url = None;
            problems.push(FeedProblem::event(&event.id, FeedIssue::BadLink));
        }
        if event
            .news_id
            .as_deref()
            .is_some_and(|id| !item_ids.contains(id))
        {
            event.news_id = None;
            problems.push(FeedProblem::event(&event.id, FeedIssue::UnknownNews));
        }
        if !clean_source(&mut event.source) {
            problems.push(FeedProblem::event(&event.id, FeedIssue::BadSourceLink));
        }
        events.push(event);
    }
    events
}

/// The status notice when it follows the rules (unknown fields, levels and states are tolerated;
/// a status page link that is not `https://` is removed). Expiry against the clock is left to
/// [`FeedStatus::is_active`]: a cached feed is read again later.
fn read_status(raw: Option<Value>, problems: &mut Vec<FeedProblem>) -> Option<FeedStatus> {
    let mut status: FeedStatus = match serde_json::from_value(raw?) {
        Ok(status) => status,
        Err(_) => {
            problems.push(FeedProblem::status(FeedIssue::Unreadable));
            return None;
        }
    };
    let bad_service = |service: &FeedStatusService| {
        service.name.trim().is_empty() || too_long(&service.name, STATUS_MAX_SERVICE_NAME_CHARS)
    };
    let latest_end = saturating_add(status.updated_at, STATUS_MAX_LIFETIME);
    let issue = if status.title.trim().is_empty() {
        Some(FeedIssue::EmptyTitle)
    } else if too_long(&status.title, STATUS_MAX_TITLE_CHARS) {
        Some(FeedIssue::TitleTooLong)
    } else if too_long(&status.message, STATUS_MAX_MESSAGE_CHARS) {
        Some(FeedIssue::MessageTooLong)
    } else if status.services.len() > STATUS_MAX_SERVICES || status.services.iter().any(bad_service)
    {
        Some(FeedIssue::BadServices)
    } else if status.expires_at <= status.updated_at || status.expires_at > latest_end {
        Some(FeedIssue::BadExpiry)
    } else {
        None
    };
    if let Some(issue) = issue {
        problems.push(FeedProblem::status(issue));
        return None;
    }
    if status.url.as_deref().is_some_and(|url| !is_https(url)) {
        status.url = None;
        problems.push(FeedProblem::status(FeedIssue::BadLink));
    }
    Some(status)
}

/// A `minecraft[].version`: 1–[`MINECRAFT_MAX_VERSION_CHARS`] characters from `[A-Za-z0-9._-]`.
fn is_valid_minecraft_news_version(version: &str) -> bool {
    (1..=MINECRAFT_MAX_VERSION_CHARS).contains(&version.len())
        && version
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'_' | b'-'))
}

/// The official article of a Minecraft entry: an `https://` link on [`MINECRAFT_ARTICLE_HOSTS`].
fn is_minecraft_article(url: &str) -> bool {
    is_https(url)
        && Url::parse(url).is_ok_and(|url| {
            url.host_str()
                .is_some_and(|host| MINECRAFT_ARTICLE_HOSTS.contains(&host))
        })
}

/// The Minecraft entries that follow the rules, in feed order. The array itself is ignored when
/// it is not an array or lists more than [`FEED_MAX_MINECRAFT`] entries (one problem with an
/// empty id); a broken entry is dropped alone; a bad cover or translation only loses that part.
fn read_minecraft(raw: Option<Value>, problems: &mut Vec<FeedProblem>) -> Vec<MinecraftNews> {
    let entries = match raw {
        None | Some(Value::Null) => return Vec::new(),
        Some(Value::Array(entries)) if entries.len() <= FEED_MAX_MINECRAFT => entries,
        Some(_) => {
            problems.push(FeedProblem::minecraft("", FeedIssue::BadList));
            return Vec::new();
        }
    };
    let mut seen = HashSet::new();
    let mut out = Vec::new();
    for mut value in entries {
        let id = raw_id(&value);
        // An entry is an object (serde would also read a struct from an array).
        if !value.is_object() {
            problems.push(FeedProblem::minecraft(&id, FeedIssue::Unreadable));
            continue;
        }
        // Read apart: a broken translation loses only its language, never the entry.
        let translations = value
            .as_object_mut()
            .and_then(|entry| entry.remove("translations"));
        let mut entry: MinecraftNews = match serde_json::from_value(value) {
            Ok(entry) => entry,
            Err(_) => {
                problems.push(FeedProblem::minecraft(&id, FeedIssue::Unreadable));
                continue;
            }
        };
        let latest_end = saturating_add(entry.published_at, MINECRAFT_MAX_LIFETIME);
        let issue = if !is_valid_id(&entry.id) {
            Some(FeedIssue::BadId)
        } else if !is_valid_minecraft_news_version(&entry.version) {
            Some(FeedIssue::BadVersion)
        } else if entry.id.strip_prefix("minecraft-") != Some(entry.version.as_str()) {
            Some(FeedIssue::BadId)
        } else if entry.title.trim().is_empty() {
            Some(FeedIssue::EmptyTitle)
        } else if too_long(&entry.title, FEED_MAX_TITLE_CHARS) {
            Some(FeedIssue::TitleTooLong)
        } else if too_long(&entry.summary, FEED_MAX_SUMMARY_CHARS) {
            Some(FeedIssue::SummaryTooLong)
        } else if too_long(&entry.body, FEED_MAX_BODY_CHARS) {
            Some(FeedIssue::BodyTooLong)
        } else if entry.expires_at <= entry.published_at || entry.expires_at > latest_end {
            Some(FeedIssue::BadExpiry)
        } else if !entry
            .source
            .url
            .as_deref()
            .is_some_and(is_minecraft_article)
        {
            Some(FeedIssue::MissingSource)
        } else {
            None
        };
        if let Some(issue) = issue {
            problems.push(FeedProblem::minecraft(&entry.id, issue));
            continue;
        }
        if !seen.insert(entry.id.clone()) {
            problems.push(FeedProblem::minecraft(&entry.id, FeedIssue::DuplicateId));
            continue;
        }
        if let Some(mut cover) = entry.cover.take() {
            cover.poster = None;
            if is_valid_feed_cover(&cover) {
                entry.cover = Some(cover);
            } else {
                problems.push(FeedProblem::minecraft(&entry.id, FeedIssue::BadCover));
            }
        }
        entry.translations = read_translations(translations, &entry.id, problems);
        let english_body = entry
            .translations
            .as_ref()
            .and_then(|t| t.en.as_ref())
            .and_then(|en| en.body.as_deref());
        if std::iter::once(entry.body.as_str())
            .chain(english_body)
            .flat_map(markdown_links)
            .any(|link| !is_safe_link(&link))
        {
            problems.push(FeedProblem::minecraft(&entry.id, FeedIssue::UnsafeBodyLink));
        }
        out.push(entry);
    }
    out
}

/// A Minecraft entry's `translations`, each language on its own (today only `en`): a value that
/// is not an object, or a text that breaks the limits of the field it replaces, is left out.
fn read_translations(
    raw: Option<Value>,
    id: &str,
    problems: &mut Vec<FeedProblem>,
) -> Option<MinecraftTranslations> {
    let languages = match raw? {
        Value::Null => return None,
        Value::Object(languages) => languages,
        _ => {
            problems.push(FeedProblem::minecraft(id, FeedIssue::BadTranslation));
            return None;
        }
    };
    let en = match languages.get("en") {
        None | Some(Value::Null) => None,
        Some(value) => {
            // Only an object: serde would also read a struct from an array.
            let parsed = value
                .is_object()
                .then(|| MinecraftText::deserialize(value).ok())
                .flatten();
            let text = parsed.filter(|text| {
                !text.title.trim().is_empty()
                    && !too_long(&text.title, FEED_MAX_TITLE_CHARS)
                    && text
                        .summary
                        .as_deref()
                        .is_none_or(|summary| !too_long(summary, FEED_MAX_SUMMARY_CHARS))
                    && text
                        .body
                        .as_deref()
                        .is_none_or(|body| !too_long(body, FEED_MAX_BODY_CHARS))
            });
            if text.is_none() {
                problems.push(FeedProblem::minecraft(id, FeedIssue::BadTranslation));
            }
            text
        }
    };
    en.map(|en| MinecraftTranslations { en: Some(en) })
}

/// Link targets in Markdown, for checking: inline links and images (`[t](dest)`), autolinks
/// (`<scheme:…>`, `<name@host>` as `mailto:`), reference definitions (`[t]: dest`) and the bare
/// URLs GitHub-flavoured Markdown turns into links (`http://…`, `https://…`, `www.…` as
/// `http://www.…`). Fenced code blocks and code spans are skipped. An approximation for
/// reports; renderers decide on their own parse.
pub fn markdown_links(body: &str) -> Vec<String> {
    let mut links = Vec::new();
    let mut fence: Option<&str> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(marker) = fence {
            if trimmed.starts_with(marker) {
                fence = None;
            }
            continue;
        }
        if let Some(marker) = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m)) {
            fence = Some(marker);
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent <= 3
            && trimmed.starts_with('[')
            && let Some(close) = trimmed.find("]:")
            && close > 1
        {
            let rest = trimmed[close + 2..].trim_start();
            let target = match rest.strip_prefix('<') {
                Some(inner) => inner.split('>').next().unwrap_or_default(),
                None => rest.split_whitespace().next().unwrap_or_default(),
            };
            if !target.is_empty() {
                links.push(target.to_owned());
            }
            continue;
        }
        scan_inline(&strip_code_spans(line), &mut links);
    }
    let mut seen = HashSet::new();
    links.retain(|link| seen.insert(link.clone()));
    links
}

/// Picture sources in Markdown: inline images (`![alt](src)`) and reference images
/// (`![alt][label]`, `![label][]`, `![label]`) with their definitions (`[label]: src`). Fenced
/// code blocks and code spans are skipped. An approximation for reports, like [`markdown_links`].
pub fn markdown_images(body: &str) -> Vec<String> {
    let mut images = Vec::new();
    let mut labels: Vec<String> = Vec::new();
    let mut definitions: HashMap<String, String> = HashMap::new();
    let mut fence: Option<&str> = None;
    for line in body.lines() {
        let trimmed = line.trim_start();
        if let Some(marker) = fence {
            if trimmed.starts_with(marker) {
                fence = None;
            }
            continue;
        }
        if let Some(marker) = ["```", "~~~"].into_iter().find(|m| trimmed.starts_with(m)) {
            fence = Some(marker);
            continue;
        }
        let indent = line.len() - trimmed.len();
        if indent <= 3
            && trimmed.starts_with('[')
            && let Some(close) = trimmed.find("]:")
            && close > 1
        {
            let label = trimmed[1..close].trim().to_lowercase();
            let rest = trimmed[close + 2..].trim_start();
            let target = match rest.strip_prefix('<') {
                Some(inner) => inner.split('>').next().unwrap_or_default(),
                None => rest.split_whitespace().next().unwrap_or_default(),
            };
            if !target.is_empty() {
                definitions
                    .entry(label)
                    .or_insert_with(|| target.to_owned());
            }
            continue;
        }
        let line = strip_code_spans(line);
        let mut from = 0;
        while let Some(pos) = line[from..].find("![") {
            let start = from + pos + 2;
            // The alt text ends at its matching `]`.
            let mut depth = 0usize;
            let mut close = None;
            for (i, c) in line[start..].char_indices() {
                match c {
                    '[' => depth += 1,
                    ']' if depth == 0 => {
                        close = Some(start + i);
                        break;
                    }
                    ']' => depth -= 1,
                    _ => {}
                }
            }
            let Some(close) = close else { break };
            let alt = line[start..close].trim().to_lowercase();
            let after = &line[close + 1..];
            if let Some(dest) = after.strip_prefix('(') {
                let dest = dest.trim_start();
                let target = match dest.strip_prefix('<') {
                    Some(inner) => inner.split('>').next().unwrap_or_default(),
                    None => {
                        let mut depth = 0usize;
                        let mut len = dest.len();
                        for (i, c) in dest.char_indices() {
                            match c {
                                '(' => depth += 1,
                                ')' if depth == 0 => {
                                    len = i;
                                    break;
                                }
                                ')' => depth -= 1,
                                c if c.is_whitespace() => {
                                    len = i;
                                    break;
                                }
                                _ => {}
                            }
                        }
                        &dest[..len]
                    }
                };
                if !target.is_empty() {
                    images.push(target.to_owned());
                }
            } else if let Some(rest) = after.strip_prefix('[') {
                let label = rest.split(']').next().unwrap_or_default().trim();
                labels.push(if label.is_empty() {
                    alt
                } else {
                    label.to_lowercase()
                });
            } else {
                labels.push(alt);
            }
            from = close + 1;
        }
    }
    images.extend(
        labels
            .iter()
            .filter_map(|label| definitions.get(label).cloned()),
    );
    let mut seen = HashSet::new();
    images.retain(|image| seen.insert(image.clone()));
    images
}

/// The line with the content of its code spans removed.
fn strip_code_spans(line: &str) -> String {
    let mut out = String::with_capacity(line.len());
    let mut rest = line;
    while let Some(start) = rest.find('`') {
        out.push_str(&rest[..start]);
        let run = rest[start..].chars().take_while(|c| *c == '`').count();
        let after = &rest[start + run..];
        let fence = "`".repeat(run);
        match find_exact_run(after, &fence) {
            Some(end) => rest = &after[end + run..],
            None => {
                out.push_str(&rest[start..start + run]);
                rest = after;
            }
        }
    }
    out.push_str(rest);
    out
}

/// Position of a backtick run of exactly `fence.len()` in `text`.
fn find_exact_run(text: &str, fence: &str) -> Option<usize> {
    let bytes = text.as_bytes();
    let mut from = 0;
    while let Some(pos) = text[from..].find(fence) {
        let at = from + pos;
        let end = at + fence.len();
        let before = at > 0 && bytes[at - 1] == b'`';
        let after = bytes.get(end) == Some(&b'`');
        if !before && !after {
            return Some(at);
        }
        from = end;
    }
    None
}

/// Blanks `range` of `masked` so later passes do not see it again (ASCII spaces keep it UTF-8).
fn mask(masked: &mut String, range: std::ops::Range<usize>) {
    let spaces = " ".repeat(range.len());
    masked.replace_range(range, &spaces);
}

fn scan_inline(line: &str, links: &mut Vec<String>) {
    let mut masked = line.to_owned();

    // Inline links and images: `](destination "title")`.
    let mut from = 0;
    while let Some(pos) = line[from..].find("](") {
        let open = from + pos + 2;
        let after = &line[open..];
        let skipped = after.len() - after.trim_start().len();
        let start = open + skipped;
        let after = &line[start..];
        let (target, end) = if let Some(inner) = after.strip_prefix('<') {
            let len = inner.find('>').unwrap_or(inner.len());
            (&inner[..len], start + 1 + len)
        } else {
            let mut depth = 0usize;
            let mut len = after.len();
            for (i, c) in after.char_indices() {
                match c {
                    '(' => depth += 1,
                    ')' if depth == 0 => {
                        len = i;
                        break;
                    }
                    ')' => depth -= 1,
                    c if c.is_whitespace() => {
                        len = i;
                        break;
                    }
                    _ => {}
                }
            }
            (&after[..len], start + len)
        };
        if !target.is_empty() {
            links.push(target.to_owned());
        }
        mask(&mut masked, start..end);
        from = open;
    }

    // Autolinks: `<scheme:…>` and `<name@host>`.
    let line = masked.clone();
    let mut from = 0;
    while let Some(pos) = line[from..].find('<') {
        let start = from + pos + 1;
        if let Some(len) = line[start..].find('>') {
            let inner = &line[start..start + len];
            if !inner.is_empty() && !inner.contains(char::is_whitespace) && !inner.contains('<') {
                if has_scheme(inner) {
                    links.push(inner.to_owned());
                    mask(&mut masked, start..start + len);
                } else if inner.contains('@') && !inner.contains(':') {
                    links.push(format!("mailto:{inner}"));
                    mask(&mut masked, start..start + len);
                }
            }
        }
        from = start;
    }

    // Bare URLs that GFM links on its own.
    let line = masked;
    for (start, _) in line.match_indices(['h', 'w']) {
        let boundary = line[..start]
            .chars()
            .next_back()
            .is_none_or(|c| c.is_whitespace() || "(*_~\"'".contains(c));
        if !boundary {
            continue;
        }
        let tail = &line[start..];
        let lower = tail.get(..8).unwrap_or(tail).to_ascii_lowercase();
        let prefix = if lower.starts_with("https://") || lower.starts_with("http://") {
            ""
        } else if lower.starts_with("www.") {
            "http://"
        } else {
            continue;
        };
        let end = tail
            .find(|c: char| c.is_whitespace() || c == '<')
            .unwrap_or(tail.len());
        let url = trim_autolink(&tail[..end]);
        if url.len() > 4 {
            links.push(format!("{prefix}{url}"));
        }
    }
}

/// GFM's end of an autolinked URL: no trailing punctuation, no unbalanced closing paren.
fn trim_autolink(mut url: &str) -> &str {
    loop {
        let trimmed =
            url.trim_end_matches(['.', ',', ':', ';', '!', '?', '"', '\'', '*', '_', '~']);
        let unbalanced =
            trimmed.ends_with(')') && trimmed.matches(')').count() > trimmed.matches('(').count();
        let next = if unbalanced {
            &trimmed[..trimmed.len() - 1]
        } else {
            trimmed
        };
        if next.len() == url.len() {
            return next;
        }
        url = next;
    }
}

/// `scheme:` with a scheme of 2–32 characters from `[A-Za-z0-9+.-]`, starting with a letter.
fn has_scheme(text: &str) -> bool {
    let Some((scheme, _)) = text.split_once(':') else {
        return false;
    };
    (2..=32).contains(&scheme.len())
        && scheme.starts_with(|c: char| c.is_ascii_alphabetic())
        && scheme
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '+' | '.' | '-'))
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;
    use crate::signing::{SigningKey, encode_verifying_key};

    fn ts(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    fn feed_key(seed: u8) -> FeedKey {
        let key = SigningKey::from_bytes(&[seed; 32]).verifying_key();
        FeedKey {
            key_id: key_id(&key),
            public_key: encode_verifying_key(&key),
        }
    }

    fn cover() -> Value {
        let sha = "3fa405a8301ace34d11cf44a816080b8f0e49a48fbd048b8aef1543a8c58bdb6";
        json!({ "path": format!("media/{sha}.webp"), "sha256": sha, "size": 98304, "kind": "image" })
    }

    fn item(id: &str) -> Value {
        json!({
            "id": id,
            "title": "Kış etkinliği başlıyor",
            "summary": "Cumartesi 20.00'de başlıyor.",
            "body": "## Neler var\n\n- Yeni harita",
            "category": "event",
            "publishedAt": "2026-10-16T08:58:00Z",
            "source": { "kind": "discord", "channel": "duyurular", "url": "https://discord.com/channels/1/2/3" }
        })
    }

    fn feed(items: Vec<Value>, events: Vec<Value>) -> Vec<u8> {
        serde_json::to_vec(&json!({
            "schema": 1,
            "sequence": 7,
            "generatedAt": "2026-10-16T09:00:00Z",
            "items": items,
            "events": events
        }))
        .unwrap()
    }

    fn parse(items: Vec<Value>, events: Vec<Value>) -> (BotFeed, Vec<FeedProblem>) {
        BotFeed::parse(&feed(items, events)).unwrap()
    }

    fn issues(problems: &[FeedProblem]) -> Vec<(&str, FeedIssue)> {
        problems.iter().map(|p| (p.id.as_str(), p.issue)).collect()
    }

    #[test]
    fn a_clean_feed_has_no_problems() {
        let mut first = item("discord-1");
        first["cover"] = cover();
        let (feed, problems) = parse(
            vec![first, item("discord-2")],
            vec![json!({
                "id": "discord-1",
                "title": "Kış etkinliği",
                "startsAt": "2026-10-18T17:00:00Z",
                "endsAt": "2026-10-18T20:00:00Z",
                "newsId": "discord-1"
            })],
        );
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(feed.sequence, 7);
        assert_eq!(feed.items.len(), 2);
        assert_eq!(feed.events[0].news_id.as_deref(), Some("discord-1"));
        assert_eq!(feed.media_refs().len(), 1);
        assert_eq!(
            feed.items[0].source.as_ref().unwrap().kind,
            FeedSourceKind::Discord
        );
    }

    #[test]
    fn whole_feed_rules_fail_the_feed() {
        let too_many: Vec<Value> = (0..=FEED_MAX_ITEMS)
            .map(|i| item(&format!("discord-{i}")))
            .collect();
        assert!(BotFeed::parse(&feed(too_many, Vec::new())).is_err());
        let events: Vec<Value> = (0..=FEED_MAX_EVENTS)
            .map(|i| json!({ "id": format!("e{i}"), "title": "E", "startsAt": "2026-10-18T17:00:00Z" }))
            .collect();
        assert!(BotFeed::parse(&feed(Vec::new(), events)).is_err());

        let mut future =
            json!({ "schema": 2, "sequence": 1, "generatedAt": "2026-10-16T09:00:00Z" });
        assert!(matches!(
            BotFeed::parse(&serde_json::to_vec(&future).unwrap()),
            Err(Error::UnsupportedSchema { .. })
        ));
        future["schema"] = json!(0);
        assert!(BotFeed::parse(&serde_json::to_vec(&future).unwrap()).is_err());
        assert!(BotFeed::parse(br#"{"schema":1,"generatedAt":"2026-10-16T09:00:00Z"}"#).is_err());
        assert!(BotFeed::parse(b"not json").is_err());

        // Exactly the byte limit passes the size rule; one byte more does not.
        let mut padded = feed(vec![item("discord-1")], Vec::new());
        padded.truncate(padded.len() - 1);
        padded.resize(FEED_MAX_BYTES as usize - 1, b' ');
        padded.push(b'}');
        assert!(BotFeed::parse(&padded).is_ok());
        padded.insert(padded.len() - 1, b' ');
        assert!(matches!(BotFeed::parse(&padded), Err(Error::Invalid(_))));
    }

    #[test]
    fn broken_items_drop_only_themselves() {
        let mut no_title = item("discord-3");
        no_title["title"] = json!("   ");
        let mut long_title = item("discord-4");
        long_title["title"] = json!("ş".repeat(FEED_MAX_TITLE_CHARS + 1));
        let mut exact_title = item("discord-5");
        exact_title["title"] = json!("ş".repeat(FEED_MAX_TITLE_CHARS));
        let mut long_summary = item("discord-6");
        long_summary["summary"] = json!("a".repeat(FEED_MAX_SUMMARY_CHARS + 1));
        let mut long_body = item("discord-7");
        long_body["body"] = json!("b".repeat(FEED_MAX_BODY_CHARS + 1));
        let mut exact_body = item("discord-8");
        exact_body["body"] = json!("ü".repeat(FEED_MAX_BODY_CHARS));
        let mut unreadable = item("discord-9");
        unreadable["publishedAt"] = json!("dün");
        let (feed, problems) = parse(
            vec![
                item("discord-1"),
                item("discord-1"),
                item("../etc"),
                no_title,
                long_title,
                exact_title,
                long_summary,
                long_body,
                exact_body,
                unreadable,
                json!(42),
            ],
            Vec::new(),
        );
        let ids: Vec<&str> = feed.items.iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["discord-1", "discord-5", "discord-8"]);
        assert_eq!(
            issues(&problems),
            [
                ("discord-1", FeedIssue::DuplicateId),
                ("../etc", FeedIssue::BadId),
                ("discord-3", FeedIssue::EmptyTitle),
                ("discord-4", FeedIssue::TitleTooLong),
                ("discord-6", FeedIssue::SummaryTooLong),
                ("discord-7", FeedIssue::BodyTooLong),
                ("discord-9", FeedIssue::Unreadable),
                ("", FeedIssue::Unreadable),
            ]
        );
        assert!(problems.iter().all(|p| p.issue.drops()));
    }

    #[test]
    fn unsafe_parts_are_removed_and_pins_ignored() {
        let mut pinned = item("discord-1");
        pinned["pinned"] = json!(true);
        pinned["featured"] = json!(true);
        pinned["cta"] = json!({ "label": "Aç", "url": "javascript:alert(1)" });
        let mut video = item("discord-2");
        video["cover"] = cover();
        video["cover"]["kind"] = json!("video");
        let mut huge = item("discord-3");
        huge["cover"] = cover();
        huge["cover"]["size"] = json!(FEED_MAX_MEDIA_BYTES + 1);
        let mut escape = item("discord-4");
        escape["cover"] = cover();
        escape["cover"]["path"] = json!("../../secret.webp");
        let mut poster = item("discord-5");
        poster["cover"] = cover();
        poster["cover"]["poster"] = cover();
        poster["cta"] = json!({ "label": "Paketi güncelle", "url": "projects://pack/projects" });
        poster["source"]["url"] = json!("http://discord.com/channels/1/2/3");
        let mut links = item("discord-6");
        links["body"] = json!("Ayrıntılar [burada](http://example.com) ve `[kod](javascript:x)`.");
        let (feed, problems) = parse(vec![pinned, video, huge, escape, poster, links], Vec::new());
        assert_eq!(feed.items.len(), 6, "nothing is dropped");
        assert!(!feed.items[0].pinned && feed.items[0].featured);
        assert!(feed.items[0].cta.is_none());
        assert!(feed.items[1..4].iter().all(|i| i.cover.is_none()));
        let kept = &feed.items[4];
        assert!(kept.cover.as_ref().unwrap().poster.is_none());
        assert_eq!(
            kept.cta.as_ref().and_then(|c| c.url.as_deref()),
            Some("projects://pack/projects")
        );
        assert_eq!(kept.source.as_ref().unwrap().url, None);
        assert_eq!(
            issues(&problems),
            [
                ("discord-1", FeedIssue::BadLink),
                ("discord-2", FeedIssue::BadCover),
                ("discord-3", FeedIssue::BadCover),
                ("discord-4", FeedIssue::BadCover),
                ("discord-5", FeedIssue::BadSourceLink),
                ("discord-6", FeedIssue::UnsafeBodyLink),
            ]
        );
        assert!(problems.iter().all(|p| !p.issue.drops()));
        assert!(!feed.items[0].to_news_item().pinned);
    }

    #[test]
    fn events_are_checked_one_by_one() {
        let event = |id: &str, start: &str, end: Option<&str>| {
            let mut e = json!({ "id": id, "title": "Turnuva", "startsAt": start });
            if let Some(end) = end {
                e["endsAt"] = json!(end);
            }
            e
        };
        let mut linked = event("e1", "2026-10-18T17:00:00Z", None);
        linked["newsId"] = json!("discord-404");
        linked["url"] = json!("ftp://x");
        let mut untitled = event("e5", "2026-10-18T17:00:00Z", None);
        untitled["title"] = json!("");
        let (feed, problems) = parse(
            vec![item("discord-1")],
            vec![
                linked,
                event("e2", "2026-10-18T17:00:00Z", Some("2026-10-18T17:00:00Z")),
                event("e1", "2026-10-19T17:00:00Z", None),
                event("bad id", "2026-10-18T17:00:00Z", None),
                untitled,
                json!({ "id": "e6", "title": "Saatsiz" }),
            ],
        );
        assert_eq!(feed.events.len(), 1);
        let open = &feed.events[0];
        assert_eq!(open.url, None);
        assert_eq!(open.news_id, None);
        assert_eq!(open.end(), ts("2026-10-18T20:00:00Z"));
        assert_eq!(
            issues(&problems),
            [
                ("e1", FeedIssue::BadLink),
                ("e1", FeedIssue::UnknownNews),
                ("e2", FeedIssue::EndsBeforeStart),
                ("e1", FeedIssue::DuplicateId),
                ("bad id", FeedIssue::BadId),
                ("e5", FeedIssue::EmptyTitle),
                ("e6", FeedIssue::Unreadable),
            ]
        );
    }

    #[test]
    fn open_ended_event_phases() {
        let event = FeedEvent {
            id: "e".into(),
            title: "E".into(),
            starts_at: ts("2026-10-18T17:00:00Z"),
            ends_at: None,
            url: None,
            news_id: None,
            priority: 0,
            join: false,
            source: None,
        };
        assert_eq!(
            event.phase(ts("2026-10-18T16:59:59Z")),
            EventPhase::Upcoming
        );
        assert_eq!(event.phase(ts("2026-10-18T17:00:00Z")), EventPhase::Live);
        assert_eq!(event.phase(ts("2026-10-18T19:59:59Z")), EventPhase::Live);
        assert_eq!(event.phase(ts("2026-10-18T20:00:00Z")), EventPhase::Ended);
        let far = FeedEvent {
            starts_at: Timestamp::MAX,
            ..event
        };
        assert_eq!(far.end(), Timestamp::MAX);
    }

    #[test]
    fn fingerprints_follow_what_players_see() {
        let (feed, _) = parse(
            vec![json!({
                "id": "discord-1",
                "title": "T",
                "summary": "S",
                "body": "B",
                "category": "event",
                "publishedAt": "2026-10-16T08:58:00Z",
                "source": { "kind": "discord", "channel": "duyurular", "url": "https://discord.com/channels/1/2/3" }
            })],
            vec![json!({ "id": "e", "title": "E", "startsAt": "2026-10-18T17:00:00Z" })],
        );
        let item = &feed.items[0];
        // The exact shape is pinned: Studio publishes fingerprints, so a change would mark every
        // override as changed.
        assert_eq!(
            item.fingerprint(),
            sha256_hex(
                br#"{"title":"T","summary":"S","body":"B","category":"event","tags":[],"publishedAt":"2026-10-16T08:58:00Z","expiresAt":null,"cover":null,"cta":null}"#
            )
        );
        let event = &feed.events[0];
        assert_eq!(
            event.fingerprint(),
            sha256_hex(
                br#"{"title":"E","startsAt":"2026-10-18T17:00:00Z","endsAt":null,"url":null,"newsId":null,"priority":0}"#
            )
        );

        // Where it came from does not count; what players see does.
        let mut moved = item.clone();
        moved.source = None;
        moved.id = "discord-2".into();
        assert_eq!(moved.fingerprint(), item.fingerprint());
        let mut edited = item.clone();
        edited.body.push_str("\n- Yeni madde");
        assert_ne!(edited.fingerprint(), item.fingerprint());
        let mut with_cover = item.clone();
        with_cover.cover = serde_json::from_value(cover()).unwrap();
        assert_ne!(with_cover.fingerprint(), item.fingerprint());
        let mut same_picture = with_cover.clone();
        if let Some(cover) = same_picture.cover.as_mut() {
            cover.blurhash = Some("L6PZfSi_.AyE_3t7t7R**0o#DgR4".into());
        }
        assert_eq!(same_picture.fingerprint(), with_cover.fingerprint());
        let mut later = event.clone();
        later.ends_at = Some(ts("2026-10-18T21:00:00Z"));
        assert_ne!(later.fingerprint(), event.fingerprint());
        let mut raised = event.clone();
        raised.priority = 1;
        assert_ne!(raised.fingerprint(), event.fingerprint());
        // "Katıl" changes what players see; an event without it keeps its old fingerprint.
        let mut on_server = event.clone();
        on_server.join = true;
        assert_eq!(
            on_server.fingerprint(),
            sha256_hex(
                br#"{"title":"E","startsAt":"2026-10-18T17:00:00Z","endsAt":null,"url":null,"newsId":null,"priority":0,"join":true}"#
            )
        );
    }

    #[test]
    fn event_join_is_read_and_checked() {
        let (feed, problems) = parse(
            Vec::new(),
            vec![
                json!({ "id": "cup", "title": "Kupa", "startsAt": "2026-10-18T17:00:00Z", "join": true }),
                json!({ "id": "talk", "title": "Soru-cevap", "startsAt": "2026-10-18T17:00:00Z" }),
                json!({ "id": "odd", "title": "Bozuk", "startsAt": "2026-10-18T17:00:00Z", "join": "evet" }),
            ],
        );
        assert_eq!(feed.events.len(), 2);
        assert!(feed.events[0].join);
        assert!(!feed.events[1].join);
        assert_eq!(issues(&problems), [("odd", FeedIssue::Unreadable)]);
        // Written back only when set, like `priority`.
        let written = serde_json::to_value(&feed.events[1]).unwrap();
        assert!(written.get("join").is_none());
    }

    #[test]
    fn event_priority_is_read_and_checked() {
        let (feed, problems) = parse(
            Vec::new(),
            vec![
                json!({ "id": "high", "title": "Yüksek", "startsAt": "2026-10-18T17:00:00Z", "priority": 3 }),
                json!({ "id": "plain", "title": "Sıradan", "startsAt": "2026-10-18T17:00:00Z" }),
                json!({ "id": "odd", "title": "Bozuk", "startsAt": "2026-10-18T17:00:00Z", "priority": "çok" }),
            ],
        );
        assert_eq!(feed.events.len(), 2);
        assert_eq!(feed.events[0].priority, 3);
        assert_eq!(feed.events[1].priority, 0);
        assert_eq!(issues(&problems), [("odd", FeedIssue::Unreadable)]);
    }

    #[test]
    fn unknown_fields_and_values_are_tolerated() {
        let mut future = item("discord-1");
        future["category"] = json!("poll");
        future["reactions"] = json!(12);
        future["source"]["kind"] = json!("telegram");
        let (feed, problems) = BotFeed::parse(
            &serde_json::to_vec(&json!({
                "sequence": 1,
                "generatedAt": "2026-10-16T09:00:00Z",
                "items": [future],
                "polls": []
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(problems.is_empty());
        assert_eq!(feed.schema, 1);
        assert_eq!(feed.items[0].category, NewsCategory::Unknown);
        assert_eq!(
            feed.items[0].source.as_ref().unwrap().kind,
            FeedSourceKind::Unknown
        );
        assert!(feed.events.is_empty());
    }

    #[test]
    fn later_additions_never_break_the_reader() {
        // Fields a newer bot may add anywhere: at the top, in items, events, the status notice
        // and its services. This reader skips them and keeps everything it knows.
        let mut news = item("discord-1");
        news["reactions"] = json!({ "👍": 4 });
        let mut status = status_value("outage");
        status["incidentId"] = json!("abc");
        status["services"][0]["region"] = json!("eu");
        let (feed, problems) = BotFeed::parse(
            &serde_json::to_vec(&json!({
                "schema": 1,
                "sequence": 3,
                "generatedAt": "2026-10-16T09:00:00Z",
                "items": [news],
                "events": [{ "id": "e", "title": "E", "startsAt": "2026-10-18T17:00:00Z", "stream": "https://x.example" }],
                "status": status,
                "polls": [{ "id": "p" }],
                "servers": { "count": 3 },
                "motd": 7
            }))
            .unwrap(),
        )
        .unwrap();
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!((feed.items.len(), feed.events.len()), (1, 1));
        let status = feed.status.expect("status kept");
        assert_eq!(status.services[0].name, "Survival Zephyr");
    }

    fn status_value(level: &str) -> Value {
        json!({
            "level": level,
            "title": "Bazı hizmetlerde sorun var",
            "message": "Şu an erişilemiyor: Survival Zephyr",
            "services": [{ "name": "Survival Zephyr", "state": "down" }],
            "url": "https://status.projects.gg",
            "updatedAt": "2026-10-16T09:00:00Z",
            "expiresAt": "2026-10-16T09:15:00Z"
        })
    }

    fn with_status(status: Value) -> (BotFeed, Vec<FeedProblem>) {
        let mut doc: Value =
            serde_json::from_slice(&feed(vec![item("discord-1")], Vec::new())).expect("feed json");
        doc["status"] = status;
        BotFeed::parse(&serde_json::to_vec(&doc).unwrap()).unwrap()
    }

    #[test]
    fn a_valid_status_is_read_and_shown_until_it_expires() {
        let (feed, problems) = with_status(status_value("outage"));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(feed.items.len(), 1, "the news is untouched");
        let status = feed.status.as_ref().expect("status");
        assert_eq!(status.level, FeedStatusLevel::Outage);
        assert_eq!(status.title, "Bazı hizmetlerde sorun var");
        assert_eq!(status.message, "Şu an erişilemiyor: Survival Zephyr");
        assert_eq!(
            status.services,
            [FeedStatusService {
                name: "Survival Zephyr".into(),
                state: FeedServiceState::Down
            }]
        );
        assert_eq!(status.url.as_deref(), Some("https://status.projects.gg"));
        assert!(status.is_active(ts("2026-10-16T09:00:00Z")));
        assert!(status.is_active(ts("2026-10-16T09:14:59Z")));
        assert!(!status.is_active(ts("2026-10-16T09:15:00Z")), "expired");
        assert!(!status.is_active(ts("2026-10-17T09:00:00Z")));

        // The least a notice needs: a title and its two times.
        let (feed, problems) = with_status(json!({
            "title": "Bu gece bakım var",
            "updatedAt": "2026-10-16T09:00:00Z",
            "expiresAt": "2026-10-16T09:01:00Z"
        }));
        assert!(problems.is_empty(), "{problems:?}");
        let status = feed.status.expect("status");
        assert_eq!(status.level, FeedStatusLevel::Notice);
        assert!(status.message.is_empty() && status.services.is_empty() && status.url.is_none());
    }

    #[test]
    fn a_missing_status_means_all_is_well() {
        let (feed, problems) = parse(vec![item("discord-1")], Vec::new());
        assert!(feed.status.is_none() && problems.is_empty());
        let (feed, problems) = with_status(Value::Null);
        assert!(feed.status.is_none() && problems.is_empty());
    }

    #[test]
    fn status_limits_are_inclusive() {
        let mut value = status_value("degraded");
        value["title"] = json!("ş".repeat(STATUS_MAX_TITLE_CHARS));
        value["message"] = json!("ü".repeat(STATUS_MAX_MESSAGE_CHARS));
        value["services"] = json!(
            (0..STATUS_MAX_SERVICES)
                .map(|i| json!({
                    "name": format!("{i:02}{}", "ğ".repeat(STATUS_MAX_SERVICE_NAME_CHARS - 2)),
                    "state": "degraded"
                }))
                .collect::<Vec<_>>()
        );
        value["expiresAt"] = json!("2026-10-16T09:15:00Z");
        let (feed, problems) = with_status(value);
        assert!(problems.is_empty(), "{problems:?}");
        let status = feed.status.expect("status");
        assert_eq!(status.level, FeedStatusLevel::Degraded);
        assert_eq!(status.services.len(), STATUS_MAX_SERVICES);
    }

    #[test]
    fn a_broken_status_is_dropped_but_the_feed_stays() {
        let broken = |edit: &dyn Fn(&mut Value)| {
            let mut value = status_value("outage");
            edit(&mut value);
            let (feed, problems) = with_status(value);
            assert!(feed.status.is_none(), "dropped");
            assert_eq!(feed.items.len(), 1, "the rest of the feed stays");
            assert_eq!(problems.len(), 1, "{problems:?}");
            assert_eq!(problems[0].target, FeedTarget::Status);
            assert!(problems[0].id.is_empty());
            assert!(problems[0].issue.drops());
            problems[0].issue
        };
        let services = |count: usize| {
            json!(
                (0..count)
                    .map(|i| json!({ "name": format!("Sunucu {i}"), "state": "down" }))
                    .collect::<Vec<_>>()
            )
        };
        let remove = |key: &'static str| {
            move |s: &mut Value| {
                s.as_object_mut().expect("object").remove(key);
            }
        };

        assert_eq!(broken(&|s| s["title"] = json!(" ")), FeedIssue::EmptyTitle);
        assert_eq!(
            broken(&|s| s["title"] = json!("a".repeat(STATUS_MAX_TITLE_CHARS + 1))),
            FeedIssue::TitleTooLong
        );
        assert_eq!(
            broken(&|s| s["message"] = json!("ı".repeat(STATUS_MAX_MESSAGE_CHARS + 1))),
            FeedIssue::MessageTooLong
        );
        assert_eq!(
            broken(&|s| s["services"] = services(STATUS_MAX_SERVICES + 1)),
            FeedIssue::BadServices
        );
        assert_eq!(
            broken(&|s| s["services"][0]["name"] = json!("  ")),
            FeedIssue::BadServices
        );
        assert_eq!(
            broken(&|s| s["services"][0]["name"] =
                json!("a".repeat(STATUS_MAX_SERVICE_NAME_CHARS + 1))),
            FeedIssue::BadServices
        );
        // A life longer than 15 minutes, none at all, or a backwards one.
        assert_eq!(
            broken(&|s| s["expiresAt"] = json!("2026-10-16T09:15:01Z")),
            FeedIssue::BadExpiry
        );
        assert_eq!(
            broken(&|s| s["expiresAt"] = json!("2026-10-16T09:00:00Z")),
            FeedIssue::BadExpiry
        );
        assert_eq!(
            broken(&|s| s["expiresAt"] = json!("2026-10-15T09:00:00Z")),
            FeedIssue::BadExpiry
        );
        // Mistyped or missing fields.
        assert_eq!(broken(&|s| s["level"] = json!(2)), FeedIssue::Unreadable);
        assert_eq!(broken(&|s| s["level"] = Value::Null), FeedIssue::Unreadable);
        assert_eq!(broken(&|s| s["url"] = json!(42)), FeedIssue::Unreadable);
        assert_eq!(
            broken(&|s| s["expiresAt"] = json!("yarın")),
            FeedIssue::Unreadable
        );
        assert_eq!(
            broken(&|s| s["services"] = json!(["Survival Zephyr"])),
            FeedIssue::Unreadable
        );
        assert_eq!(broken(&remove("title")), FeedIssue::Unreadable);
        assert_eq!(broken(&remove("updatedAt")), FeedIssue::Unreadable);
        assert_eq!(broken(&remove("expiresAt")), FeedIssue::Unreadable);
        assert_eq!(
            broken(&|s| *s = json!("Bazı hizmetlerde sorun var")),
            FeedIssue::Unreadable
        );
    }

    #[test]
    fn unknown_status_values_fall_back_and_unsafe_links_go() {
        let mut value = status_value("maintenance");
        value["services"][0]["state"] = json!("slow");
        let (feed, problems) = with_status(value);
        assert!(problems.is_empty(), "{problems:?}");
        let status = feed.status.expect("status");
        assert_eq!(status.level, FeedStatusLevel::Notice, "the calmest level");
        assert_eq!(status.services[0].state, FeedServiceState::Unknown);

        for url in [
            "http://status.projects.gg",
            "javascript:alert(1)",
            "projects://news/sezon-5",
            "https://user:pw@status.projects.gg",
            " https://status.projects.gg",
            "",
        ] {
            let mut value = status_value("outage");
            value["url"] = json!(url);
            let (feed, problems) = with_status(value);
            let status = feed.status.expect("kept without the link");
            assert_eq!(status.url, None, "{url:?}");
            assert_eq!(issues(&problems), [("", FeedIssue::BadLink)], "{url:?}");
            assert!(!problems[0].issue.drops());
        }
    }

    fn minecraft_entry(version: &str) -> Value {
        json!({
            "id": format!("minecraft-{version}"),
            "version": version,
            "kind": "release",
            "title": format!("Minecraft {version} çıktı"),
            "summary": "Java sürümüne yeni bir güncelleme geldi.",
            "body": "Keşif temalı yeni güncelleme.\n\n## Öne çıkanlar\n\n- **Dappled Forest** biyomu",
            "cover": cover(),
            "publishedAt": "2026-09-15T11:23:02Z",
            "expiresAt": "2026-10-30T11:23:02Z",
            "source": {
                "kind": "minecraft",
                "url": format!(
                    "https://www.minecraft.net/article/minecraft-java-edition-{}",
                    version.replace('.', "-")
                )
            },
            "translations": {
                "en": {
                    "title": format!("Minecraft {version} is out"),
                    "summary": "A new update for Java Edition.",
                    "body": "A new exploration update.\n\n## Highlights\n\n- The **Dappled Forest** biome"
                }
            }
        })
    }

    fn with_minecraft(minecraft: Value) -> (BotFeed, Vec<FeedProblem>) {
        let mut doc: Value =
            serde_json::from_slice(&feed(vec![item("discord-1")], Vec::new())).expect("feed json");
        doc["minecraft"] = minecraft;
        BotFeed::parse(&serde_json::to_vec(&doc).unwrap()).unwrap()
    }

    /// Many entries read in feeds of at most [`FEED_MAX_MINECRAFT`], in order: what was kept and
    /// every problem (repeats are only found within one feed).
    fn minecraft_in_feeds(entries: Vec<Value>) -> (Vec<MinecraftNews>, Vec<FeedProblem>) {
        let mut kept = Vec::new();
        let mut problems = Vec::new();
        for chunk in entries.chunks(FEED_MAX_MINECRAFT) {
            let (feed, found) = with_minecraft(Value::Array(chunk.to_vec()));
            assert_eq!(feed.items.len(), 1, "the news is untouched");
            kept.extend(feed.minecraft);
            problems.extend(found);
        }
        (kept, problems)
    }

    #[test]
    fn minecraft_fingerprints_follow_what_players_see() {
        let (feed, problems) = with_minecraft(json!([{
            "id": "minecraft-26.3",
            "version": "26.3",
            "kind": "release",
            "title": "T",
            "summary": "S",
            "body": "B",
            "publishedAt": "2026-09-15T11:23:02Z",
            "expiresAt": "2026-10-30T11:23:02Z",
            "source": {
                "kind": "minecraft",
                "url": "https://www.minecraft.net/article/minecraft-java-edition-26-3"
            }
        }]));
        assert!(problems.is_empty(), "{problems:?}");
        let entry = &feed.minecraft[0];
        // The exact shape is pinned: Studio publishes fingerprints, so a change would mark every
        // hidden entry as rewritten.
        assert_eq!(
            entry.fingerprint(),
            sha256_hex(
                br#"{"title":"T","summary":"S","body":"B","cover":null,"publishedAt":"2026-09-15T11:23:02Z"}"#
            )
        );

        // Nothing players see: the id, version, kind, expiry and article link.
        let mut moved = entry.clone();
        moved.id = "minecraft-26.3.1".into();
        moved.version = "26.3.1".into();
        moved.kind = MinecraftNewsKind::Snapshot;
        moved.expires_at = ts("2026-10-20T00:00:00Z");
        moved.source.url = Some("https://minecraft.net/article/minecraft-java-edition".into());
        assert_eq!(moved.fingerprint(), entry.fingerprint());

        // The texts, the picture and the English texts are.
        let mut rewritten = entry.clone();
        rewritten.body.push_str("\n- Yeni madde");
        assert_ne!(rewritten.fingerprint(), entry.fingerprint());
        let mut with_cover = entry.clone();
        with_cover.cover = serde_json::from_value(cover()).unwrap();
        assert_ne!(with_cover.fingerprint(), entry.fingerprint());
        let mut english = entry.clone();
        english.translations = Some(MinecraftTranslations {
            en: Some(MinecraftText {
                title: "T".into(),
                summary: None,
                body: None,
            }),
        });
        assert_ne!(english.fingerprint(), entry.fingerprint());
        let mut retold = english.clone();
        if let Some(en) = retold.translations.as_mut().and_then(|t| t.en.as_mut()) {
            en.body = Some("B".into());
        }
        assert_ne!(retold.fingerprint(), english.fingerprint());
    }

    #[test]
    fn minecraft_entries_are_read_with_their_english_texts() {
        let (feed, problems) =
            with_minecraft(json!([minecraft_entry("26.3"), minecraft_entry("26.1.2")]));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(feed.items.len(), 1, "the news is untouched");
        let ids: Vec<&str> = feed.minecraft.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(
            ids,
            ["minecraft-26.3", "minecraft-26.1.2"],
            "feed order kept"
        );
        let entry = &feed.minecraft[0];
        assert_eq!(entry.version, "26.3");
        assert_eq!(entry.kind, MinecraftNewsKind::Release);
        assert_eq!(entry.source.kind, FeedSourceKind::Minecraft);
        assert_eq!(
            entry.article_url(),
            "https://www.minecraft.net/article/minecraft-java-edition-26-3"
        );
        let en = entry.translations.as_ref().and_then(|t| t.en.as_ref());
        assert_eq!(
            en.map(|en| en.title.as_str()),
            Some("Minecraft 26.3 is out")
        );
        assert_eq!(
            feed.media_refs().len(),
            2,
            "Minecraft covers are feed covers"
        );

        assert!(
            !entry.is_visible(ts("2026-09-15T11:23:01Z")),
            "not released yet"
        );
        assert!(entry.is_visible(ts("2026-09-15T11:23:02Z")));
        assert!(entry.is_visible(ts("2026-10-30T11:23:01Z")));
        assert!(!entry.is_visible(ts("2026-10-30T11:23:02Z")), "expired");

        // The least an entry needs; a missing kind reads as a snapshot.
        let (feed, problems) = with_minecraft(json!([{
            "id": "minecraft-26.4-snapshot-4",
            "version": "26.4-snapshot-4",
            "title": "Minecraft 26.4 Snapshot 4",
            "publishedAt": "2026-10-13T12:05:46Z",
            "expiresAt": "2026-10-27T12:05:46Z",
            "source": { "kind": "minecraft", "url": "https://minecraft.net/article/minecraft-26-4-snapshot-4" }
        }]));
        assert!(problems.is_empty(), "{problems:?}");
        let entry = &feed.minecraft[0];
        assert_eq!(entry.kind, MinecraftNewsKind::Snapshot);
        assert!(entry.summary.is_empty() && entry.body.is_empty());
        assert!(entry.cover.is_none() && entry.translations.is_none());

        // A feed without Minecraft entries is written as before: no `minecraft` key.
        let (plain, _) = parse(vec![item("discord-1")], Vec::new());
        assert!(plain.minecraft.is_empty());
        let written = serde_json::to_value(&plain).unwrap();
        assert!(written.get("minecraft").is_none());
    }

    #[test]
    fn a_broken_minecraft_array_is_ignored_but_the_feed_stays() {
        let (feed, problems) = with_minecraft(Value::Null);
        assert!(feed.minecraft.is_empty() && problems.is_empty());

        for broken in [
            json!({ "entries": [] }),
            json!("minecraft-26.3"),
            json!(3),
            json!(
                (0..=FEED_MAX_MINECRAFT)
                    .map(|i| minecraft_entry(&format!("26.{i}")))
                    .collect::<Vec<_>>()
            ),
        ] {
            let (feed, problems) = with_minecraft(broken);
            assert!(feed.minecraft.is_empty(), "the whole array goes");
            assert_eq!(feed.items.len(), 1, "the rest of the feed stays");
            assert_eq!(issues(&problems), [("", FeedIssue::BadList)]);
            assert_eq!(problems[0].target, FeedTarget::Minecraft);
            assert!(problems[0].issue.drops());
        }

        let (feed, problems) = with_minecraft(json!(
            (0..FEED_MAX_MINECRAFT)
                .map(|i| minecraft_entry(&format!("26.{i}")))
                .collect::<Vec<_>>()
        ));
        assert!(problems.is_empty(), "{problems:?}");
        assert_eq!(
            feed.minecraft.len(),
            FEED_MAX_MINECRAFT,
            "the limit is inclusive"
        );
    }

    #[test]
    fn broken_minecraft_entries_drop_only_themselves() {
        let entry = |version: &str, edit: &dyn Fn(&mut Value)| {
            let mut value = minecraft_entry(version);
            edit(&mut value);
            value
        };
        let remove = |key: &'static str| {
            move |e: &mut Value| {
                e.as_object_mut().expect("object").remove(key);
            }
        };
        let (minecraft, problems) = minecraft_in_feeds(vec![
            minecraft_entry("26.3"),
            minecraft_entry("26.3"),
            entry("26.2", &|e| e["id"] = json!("../26.2")),
            entry("26.1", &|e| e["id"] = json!("minecraft-26.9")),
            entry("26.0", &|e| e["id"] = json!("26.0")),
            entry("26 4", &|e| e["id"] = json!("minecraft-26-4")),
            entry("x", &|e| {
                e["version"] = json!("");
                e["id"] = json!("minecraft-");
            }),
            entry(&"9".repeat(MINECRAFT_MAX_VERSION_CHARS + 1), &|_| {}),
            entry("25.1", &|e| e["title"] = json!("  ")),
            entry("25.2", &|e| {
                e["title"] = json!("ş".repeat(FEED_MAX_TITLE_CHARS + 1))
            }),
            entry("25.3", &|e| {
                e["summary"] = json!("a".repeat(FEED_MAX_SUMMARY_CHARS + 1))
            }),
            entry("25.4", &|e| {
                e["body"] = json!("b".repeat(FEED_MAX_BODY_CHARS + 1))
            }),
            entry("25.5", &|e| e["expiresAt"] = json!("2026-09-15T11:23:02Z")),
            entry("25.6", &|e| e["expiresAt"] = json!("2026-09-14T11:23:02Z")),
            // 90 days and a second.
            entry("25.7", &|e| e["expiresAt"] = json!("2026-12-14T11:23:03Z")),
            entry("25.8", &|e| {
                e["source"]["url"] = json!("http://www.minecraft.net/article/x")
            }),
            entry("25.9", &|e| {
                e["source"]["url"] = json!("https://minecraft.net.example/article")
            }),
            entry("24.1", &|e| {
                e["source"]["url"] = json!("https://user@www.minecraft.net/a")
            }),
            entry("24.2", &|e| {
                e["source"].as_object_mut().expect("object").remove("url");
            }),
            entry("24.3", &remove("source")),
            entry("24.4", &remove("publishedAt")),
            entry("24.5", &remove("expiresAt")),
            entry("24.6", &remove("version")),
            entry("24.7", &|e| e["kind"] = json!(3)),
            json!(42),
            json!(["minecraft-23.1", "23.1", "release", "Minecraft 23.1"]),
            // Exactly 90 days is still fine.
            entry("24.8", &|e| e["expiresAt"] = json!("2026-12-14T11:23:02Z")),
        ]);
        let ids: Vec<&str> = minecraft.iter().map(|m| m.id.as_str()).collect();
        assert_eq!(ids, ["minecraft-26.3", "minecraft-24.8"]);
        let long_version = format!("minecraft-{}", "9".repeat(MINECRAFT_MAX_VERSION_CHARS + 1));
        assert_eq!(
            issues(&problems),
            [
                ("minecraft-26.3", FeedIssue::DuplicateId),
                ("../26.2", FeedIssue::BadId),
                ("minecraft-26.9", FeedIssue::BadId),
                ("26.0", FeedIssue::BadId),
                ("minecraft-26-4", FeedIssue::BadVersion),
                ("minecraft-", FeedIssue::BadVersion),
                (long_version.as_str(), FeedIssue::BadVersion),
                ("minecraft-25.1", FeedIssue::EmptyTitle),
                ("minecraft-25.2", FeedIssue::TitleTooLong),
                ("minecraft-25.3", FeedIssue::SummaryTooLong),
                ("minecraft-25.4", FeedIssue::BodyTooLong),
                ("minecraft-25.5", FeedIssue::BadExpiry),
                ("minecraft-25.6", FeedIssue::BadExpiry),
                ("minecraft-25.7", FeedIssue::BadExpiry),
                ("minecraft-25.8", FeedIssue::MissingSource),
                ("minecraft-25.9", FeedIssue::MissingSource),
                ("minecraft-24.1", FeedIssue::MissingSource),
                ("minecraft-24.2", FeedIssue::MissingSource),
                ("minecraft-24.3", FeedIssue::Unreadable),
                ("minecraft-24.4", FeedIssue::Unreadable),
                ("minecraft-24.5", FeedIssue::Unreadable),
                ("minecraft-24.6", FeedIssue::Unreadable),
                ("minecraft-24.7", FeedIssue::Unreadable),
                ("", FeedIssue::Unreadable),
                ("", FeedIssue::Unreadable),
            ]
        );
        assert!(problems.iter().all(|p| p.issue.drops()));
        assert!(problems.iter().all(|p| p.target == FeedTarget::Minecraft));
    }

    #[test]
    fn minecraft_parts_are_repaired_alone() {
        let entry = |version: &str, edit: &dyn Fn(&mut Value)| {
            let mut value = minecraft_entry(version);
            edit(&mut value);
            value
        };
        let (minecraft, problems) = minecraft_in_feeds(vec![
            entry("26.3", &|e| e["cover"]["kind"] = json!("video")),
            entry("26.2", &|e| {
                e["translations"] = json!("Minecraft 26.2 is out")
            }),
            entry("26.1", &|e| {
                e["translations"]["en"] = json!(["Minecraft 26.1"])
            }),
            entry("26.0", &|e| {
                e["translations"]["en"]["title"] = json!("a".repeat(FEED_MAX_TITLE_CHARS + 1))
            }),
            entry("25.9", &|e| {
                e["translations"]["en"]["summary"] = json!("a".repeat(FEED_MAX_SUMMARY_CHARS + 1))
            }),
            entry("25.8", &|e| {
                e["translations"]["en"]["body"] = json!("b".repeat(FEED_MAX_BODY_CHARS + 1))
            }),
            entry("25.7", &|e| e["translations"]["en"]["title"] = json!(" ")),
            entry("25.6", &|e| {
                e["translations"]["en"]
                    .as_object_mut()
                    .expect("object")
                    .remove("title");
            }),
            entry("25.5", &|e| {
                e["body"] = json!("Bkz. [notlar](http://example.com/notes)")
            }),
            entry("25.4", &|e| {
                e["translations"]["en"]["body"] =
                    json!("Write to [the team](mailto:team@example.com)")
            }),
            // Not problems: no translation, an English title alone, unknown values and fields.
            entry("25.3", &|e| e["translations"] = json!({})),
            entry("25.2", &|e| e["translations"]["en"] = Value::Null),
            entry("25.1", &|e| {
                e["translations"]["en"] = json!({ "title": "Minecraft 25.1 is out" });
                e["translations"]["de"] = json!({ "title": "Minecraft 25.1 ist da" });
            }),
            entry("24.9", &|e| {
                e["kind"] = json!("preRelease");
                e["category"] = json!("patchNotes");
                e["pinned"] = json!(true);
                e["featured"] = json!(true);
                e["cta"] = json!({ "label": "Oyna", "url": "projects://play" });
                e["source"]["kind"] = json!("mojang");
            }),
        ]);
        assert_eq!(minecraft.len(), 14, "every entry stays: {problems:?}");
        assert_eq!(
            issues(&problems),
            [
                ("minecraft-26.3", FeedIssue::BadCover),
                ("minecraft-26.2", FeedIssue::BadTranslation),
                ("minecraft-26.1", FeedIssue::BadTranslation),
                ("minecraft-26.0", FeedIssue::BadTranslation),
                ("minecraft-25.9", FeedIssue::BadTranslation),
                ("minecraft-25.8", FeedIssue::BadTranslation),
                ("minecraft-25.7", FeedIssue::BadTranslation),
                ("minecraft-25.6", FeedIssue::BadTranslation),
                ("minecraft-25.5", FeedIssue::UnsafeBodyLink),
                ("minecraft-25.4", FeedIssue::UnsafeBodyLink),
            ]
        );
        assert!(problems.iter().all(|p| !p.issue.drops()));
        let by_id = |id: &str| minecraft.iter().find(|m| m.id == id).expect(id);
        assert!(by_id("minecraft-26.3").cover.is_none(), "the cover goes");
        for id in [
            "minecraft-26.2",
            "minecraft-26.1",
            "minecraft-26.0",
            "minecraft-25.9",
            "minecraft-25.8",
            "minecraft-25.7",
            "minecraft-25.6",
            "minecraft-25.3",
            "minecraft-25.2",
        ] {
            assert!(by_id(id).translations.is_none(), "{id}: base texts only");
        }
        let english = by_id("minecraft-25.1")
            .translations
            .as_ref()
            .and_then(|t| t.en.as_ref())
            .expect("English kept");
        assert_eq!(english.title, "Minecraft 25.1 is out");
        assert!(
            english.summary.is_none() && english.body.is_none(),
            "base texts fill in"
        );
        let future = by_id("minecraft-24.9");
        assert_eq!(
            future.kind,
            MinecraftNewsKind::Snapshot,
            "unknown kinds are snapshots"
        );
        assert_eq!(future.source.kind, FeedSourceKind::Unknown);
        assert!(by_id("minecraft-25.5").translations.is_some());
    }

    #[test]
    fn config_problems() {
        let good = NewsFeedConfig {
            enabled: true,
            mirrors: vec!["https://projects.gg/launcher-feed/v1/".into()],
            keys: vec![feed_key(1)],
            max_age_days: 60,
        };
        assert!(good.problems().is_empty());
        assert_eq!(good.mirror_bases().len(), 1);
        assert_eq!(good.trusted_keys().len(), 1);

        let empty = NewsFeedConfig {
            enabled: true,
            ..NewsFeedConfig::default()
        };
        assert_eq!(
            empty.problems(),
            [FeedConfigProblem::NoMirrors, FeedConfigProblem::NoKeys]
        );

        let mut wrong_id = feed_key(2);
        wrong_id.key_id = feed_key(3).key_id;
        let expected = feed_key(2).key_id;
        let broken = NewsFeedConfig {
            enabled: true,
            mirrors: vec![
                "http://projects.gg/feed/".into(),
                "https://projects.gg/feed".into(),
                "https://projects.gg/feed/?x=1".into(),
                "https://user:pw@projects.gg/feed/".into(),
                " https://projects.gg/feed/".into(),
                "https://a.example/v1/".into(),
                "https://a.example/v1/".into(),
            ],
            keys: vec![
                feed_key(1),
                feed_key(1),
                wrong_id.clone(),
                FeedKey {
                    key_id: "x".into(),
                    public_key: "bm90IGEga2V5".into(),
                },
                feed_key(4),
            ],
            max_age_days: 0,
        };
        let problems = broken.problems();
        assert!(problems.contains(&FeedConfigProblem::TooManyMirrors));
        assert!(problems.contains(&FeedConfigProblem::TooManyKeys));
        assert_eq!(
            problems
                .iter()
                .filter(|p| matches!(p, FeedConfigProblem::BadMirror { .. }))
                .count(),
            5
        );
        assert!(problems.contains(&FeedConfigProblem::DuplicateMirror {
            mirror: "https://a.example/v1/".into()
        }));
        assert!(problems.contains(&FeedConfigProblem::DuplicateKey {
            key_id: feed_key(1).key_id
        }));
        assert!(problems.contains(&FeedConfigProblem::KeyIdMismatch {
            key_id: wrong_id.key_id.clone(),
            expected
        }));
        assert!(problems.contains(&FeedConfigProblem::BadKey { key_id: "x".into() }));
        assert!(problems.contains(&FeedConfigProblem::BadMaxAge { days: 0 }));

        // Clients use what is valid.
        assert_eq!(broken.mirror_bases().len(), 1);
        assert_eq!(broken.trusted_keys().len(), 2, "duplicates count once");
        assert_eq!(broken.effective_max_age_days(), FEED_DEFAULT_MAX_AGE_DAYS);
        let year = NewsFeedConfig {
            max_age_days: 365,
            ..good.clone()
        };
        assert_eq!(year.effective_max_age_days(), 365);
        assert!(
            NewsFeedConfig {
                max_age_days: 366,
                ..good
            }
            .problems()
            .len()
                == 1
        );
    }

    #[test]
    fn feed_key_from_public_key() {
        let key = feed_key(5);
        let parsed = FeedKey::from_public_key(&format!(" {} ", key.public_key)).unwrap();
        assert_eq!(parsed, key);
        assert!(parsed.verifying_key().is_some());
        assert!(FeedKey::from_public_key("AAAA").is_err());
        let upper = FeedKey {
            key_id: key.key_id.to_ascii_uppercase(),
            ..key.clone()
        };
        if upper.key_id != key.key_id {
            assert!(upper.verifying_key().is_none(), "key ids are lowercase");
        }
    }

    #[test]
    fn config_defaults_and_shape() {
        let config: NewsFeedConfig = serde_json::from_str("{}").unwrap();
        assert_eq!(config, NewsFeedConfig::default());
        assert!(!config.enabled);
        assert_eq!(config.max_age_days, FEED_DEFAULT_MAX_AGE_DAYS);
        let key: FeedKey = serde_json::from_str(r#"{"keyId":"ab"}"#).unwrap();
        assert!(key.public_key.is_empty() && key.verifying_key().is_none());
    }

    #[test]
    fn safe_links() {
        for safe in [
            "https://projects.gg",
            "https://discord.com/channels/1/2/3",
            "HTTPS://projects.gg/x",
            "projects://news/sezon-5",
        ] {
            assert!(is_safe_link(safe), "{safe}");
        }
        for unsafe_link in [
            "",
            "http://projects.gg",
            "javascript:alert(1)",
            "mailto:a@b.c",
            "data:text/html,x",
            "file:///C:/x",
            "projects://",
            "projects://a b",
            " https://projects.gg",
            "https://user:pw@projects.gg",
            "https://projects.gg/\n",
            "//projects.gg",
            "/relative",
        ] {
            assert!(!is_safe_link(unsafe_link), "{unsafe_link:?}");
        }
    }

    #[test]
    fn markdown_links_are_found() {
        let body = "Bak [site](https://projects.gg/a_(b)) ve ![görsel](http://img.example/x.png \"t\").\n\
            <https://discord.gg/projects> <destek@projects.gg> <span>\n\
            Bare http://plain.example/yol. and www.example.com, not mywww.x or `http://kod`.\n\
            ```\n[gizli](javascript:x)\n```\n\
            [ref]: <ftp://files.example>\n\
            [diğer]:   mailto:x@y.z  \"başlık\"\n\
            Satır `` [x](javascript:y) `` sonu [boş]()";
        assert_eq!(
            markdown_links(body),
            [
                "https://projects.gg/a_(b)",
                "http://img.example/x.png",
                "https://discord.gg/projects",
                "mailto:destek@projects.gg",
                "http://plain.example/yol",
                "http://www.example.com",
                "ftp://files.example",
                "mailto:x@y.z",
            ]
        );
        assert!(markdown_links("Saat 20.00: başlıyor (yarın)").is_empty());
    }

    #[test]
    fn markdown_images_are_found() {
        let body = "Önce ![kapak](https://cdn.modrinth.com/a.png \"başlık\") sonra [bağlantı](https://x.example).\n\
            ![iç [köşeli] metin](<https://i.example/b c.png>) ve ![Logo][logo], ![logo2][] ile ![yalnız].\n\
            `![kod](https://kod.example/x.png)`\n\
            ```\n![gizli](https://gizli.example/x.png)\n```\n\
            [LOGO]: https://raw.githubusercontent.com/x/logo.png\n\
            [logo2]: <https://cdn.discordapp.com/l2.png>\n\
            [yalnız]: https://textures.minecraft.net/texture/abc \"skin\"\n\
            [kullanılmayan]: https://unused.example/u.png";
        assert_eq!(
            markdown_images(body),
            [
                "https://cdn.modrinth.com/a.png",
                "https://i.example/b c.png",
                "https://raw.githubusercontent.com/x/logo.png",
                "https://cdn.discordapp.com/l2.png",
                "https://textures.minecraft.net/texture/abc",
            ]
        );
        assert!(markdown_images("Resim yok, yalnız [bağlantı](https://x.example).").is_empty());
        assert!(markdown_images("Yarım ![alt metin").is_empty());
    }
}
