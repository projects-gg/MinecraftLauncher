use std::cmp::Ordering;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{Cta, FeedOverride, MediaRef, check_schema};
use crate::error::Result;

/// Highest `NewsFeed.schema` this build understands.
pub const NEWS_FEED_SCHEMA: u32 = 1;

/// `news/news.<sha256>.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase", default)]
pub struct NewsFeed {
    pub schema: u32,
    pub items: Vec<NewsItem>,
}

impl Default for NewsFeed {
    fn default() -> Self {
        Self {
            schema: NEWS_FEED_SCHEMA,
            items: Vec::new(),
        }
    }
}

#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum NewsCategory {
    Update,
    Event,
    #[default]
    Announcement,
    PatchNotes,
    Community,
    /// A category added by a newer Studio; show it like an announcement.
    #[serde(other)]
    Unknown,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NewsItem {
    /// Stable id (deep links, CTA targets, "read" state).
    pub id: String,
    pub title: String,
    #[serde(default)]
    pub summary: String,
    /// Markdown; rendered with a sanitizing renderer, links open in the system browser.
    #[serde(default)]
    pub body: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cover: Option<MediaRef>,
    #[serde(default)]
    pub category: NewsCategory,
    #[serde(default)]
    pub tags: Vec<String>,
    /// Hidden before this moment (scheduled posts).
    pub published_at: Timestamp,
    /// Hidden from this moment on.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub expires_at: Option<Timestamp>,
    #[serde(default)]
    pub pinned: bool,
    /// Position among the pinned items (1 first); ignored on items that are not pinned. Pinned
    /// items without one follow those with one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub pin_order: Option<u32>,
    #[serde(default)]
    pub featured: bool,
    /// Never shown. Studio publishes a hidden item to take the feed item with its id off the
    /// home screen (Studio wins on a shared id).
    #[serde(default, skip_serializing_if = "std::ops::Not::not")]
    pub hidden: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cta: Option<Cta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
    /// Studio's record of the feed item this one replaces or hides; launchers ignore it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub feed_override: Option<FeedOverride>,
}

impl NewsItem {
    /// Not hidden, published by `now` and not expired yet.
    pub fn is_visible(&self, now: Timestamp) -> bool {
        !self.hidden
            && self.published_at <= now
            && self.expires_at.is_none_or(|expires| now < expires)
    }

    /// Sort key among pinned items: those with a `pinOrder` first, smallest first.
    fn pin_rank(&self) -> (bool, u32) {
        (self.pin_order.is_none(), self.pin_order.unwrap_or(0))
    }
}

/// Display order: pinned first (by `pinOrder`, then as below), then newest first, then by id
/// (stable).
pub fn compare_news(a: &NewsItem, b: &NewsItem) -> Ordering {
    b.pinned
        .cmp(&a.pinned)
        .then_with(|| {
            if a.pinned && b.pinned {
                a.pin_rank().cmp(&b.pin_rank())
            } else {
                Ordering::Equal
            }
        })
        .then_with(|| b.published_at.cmp(&a.published_at))
        .then_with(|| a.id.cmp(&b.id))
}

impl NewsFeed {
    pub fn check_schema(&self) -> Result<()> {
        check_schema("news feed", self.schema, NEWS_FEED_SCHEMA)
    }

    /// Items visible at `now` in display order.
    pub fn visible(&self, now: Timestamp) -> Vec<&NewsItem> {
        let mut items: Vec<_> = self.items.iter().filter(|i| i.is_visible(now)).collect();
        items.sort_by(|a, b| compare_news(a, b));
        items
    }

    /// Visible featured items in display order (the home screen carousel).
    pub fn featured(&self, now: Timestamp) -> Vec<&NewsItem> {
        let mut items = self.visible(now);
        items.retain(|item| item.featured);
        items
    }

    pub fn get(&self, id: &str) -> Option<&NewsItem> {
        self.items.iter().find(|item| item.id == id)
    }

    /// Every media file the feed references (covers and posters).
    pub fn media_refs(&self) -> Vec<&MediaRef> {
        self.items
            .iter()
            .filter_map(|item| item.cover.as_ref())
            .flat_map(MediaRef::with_posters)
            .collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn item(id: &str, published: &str, pinned: bool, featured: bool) -> NewsItem {
        NewsItem {
            id: id.into(),
            title: id.into(),
            summary: String::new(),
            body: String::new(),
            cover: None,
            category: NewsCategory::Announcement,
            tags: Vec::new(),
            published_at: published.parse().unwrap(),
            expires_at: None,
            pinned,
            pin_order: None,
            featured,
            hidden: false,
            cta: None,
            author: None,
            feed_override: None,
        }
    }

    #[test]
    fn pinned_items_follow_their_order() {
        let now: Timestamp = "2026-10-15T12:00:00Z".parse().unwrap();
        let ordered = |id: &str, published: &str, order: u32| NewsItem {
            pin_order: Some(order),
            ..item(id, published, true, false)
        };
        // An order on an item that is not pinned means nothing.
        let unpinned_with_order = NewsItem {
            pin_order: Some(1),
            ..item("unpinned", "2026-10-15T11:00:00Z", false, false)
        };
        let feed = NewsFeed {
            schema: 1,
            items: vec![
                unpinned_with_order,
                ordered("second", "2026-10-14T00:00:00Z", 2),
                item("unordered-new", "2026-10-13T00:00:00Z", true, false),
                ordered("first", "2026-09-01T00:00:00Z", 1),
                item("unordered-old", "2026-10-01T00:00:00Z", true, false),
                ordered("third", "2026-10-15T00:00:00Z", 3),
                item("newest", "2026-10-15T11:30:00Z", false, false),
            ],
        };
        let ids: Vec<_> = feed.visible(now).iter().map(|i| i.id.as_str()).collect();
        assert_eq!(
            ids,
            [
                "first",
                "second",
                "third",
                "unordered-new",
                "unordered-old",
                "newest",
                "unpinned"
            ]
        );
        // Equal orders fall back to newest, then id.
        let tie_a = ordered("tie-a", "2026-10-01T00:00:00Z", 1);
        let tie_b = ordered("tie-b", "2026-10-01T00:00:00Z", 1);
        let tie_new = ordered("tie-new", "2026-10-02T00:00:00Z", 1);
        assert_eq!(compare_news(&tie_new, &tie_a), Ordering::Less);
        assert_eq!(compare_news(&tie_a, &tie_b), Ordering::Less);
    }

    #[test]
    fn hidden_items_are_never_shown() {
        let now: Timestamp = "2026-10-15T12:00:00Z".parse().unwrap();
        let hidden = NewsItem {
            hidden: true,
            ..item("hidden", "2026-10-01T00:00:00Z", true, true)
        };
        assert!(!hidden.is_visible(now));
        let feed = NewsFeed {
            schema: 1,
            items: vec![hidden, item("shown", "2026-10-02T00:00:00Z", false, true)],
        };
        let ids: Vec<_> = feed.featured(now).iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["shown"]);
        assert!(feed.get("hidden").is_some(), "still in the document");

        // Readers that predate `hidden` see what Studio writes next to it: `expiresAt` equal to
        // `publishedAt`, so they never show it either.
        let json = serde_json::json!({
            "id": "discord-1",
            "title": "Gizlendi",
            "publishedAt": "2026-10-01T00:00:00Z",
            "expiresAt": "2026-10-01T00:00:00Z",
            "hidden": true
        });
        let parsed: NewsItem = serde_json::from_value(json).unwrap();
        assert!(parsed.hidden);
        let unaware = NewsItem {
            hidden: false,
            ..parsed
        };
        assert!(!unaware.is_visible(now));
    }

    #[test]
    fn visibility_and_order() {
        let now: Timestamp = "2026-10-15T12:00:00Z".parse().unwrap();
        let mut expired = item("expired", "2026-10-01T00:00:00Z", true, true);
        expired.expires_at = Some("2026-10-10T00:00:00Z".parse().unwrap());
        let feed = NewsFeed {
            schema: 1,
            items: vec![
                item("old", "2026-09-01T00:00:00Z", false, true),
                item("new", "2026-10-14T00:00:00Z", false, false),
                item("pinned", "2026-08-01T00:00:00Z", true, true),
                item("scheduled", "2026-10-20T00:00:00Z", true, true),
                item("same-b", "2026-10-02T00:00:00Z", false, false),
                item("same-a", "2026-10-02T00:00:00Z", false, false),
                expired,
            ],
        };
        let ids: Vec<_> = feed.visible(now).iter().map(|i| i.id.as_str()).collect();
        assert_eq!(ids, ["pinned", "new", "same-a", "same-b", "old"]);
        let featured: Vec<_> = feed.featured(now).iter().map(|i| i.id.as_str()).collect();
        assert_eq!(featured, ["pinned", "old"]);
        assert!(feed.get("scheduled").is_some());
    }

    #[test]
    fn unknown_category_and_defaults() {
        let item: NewsItem = serde_json::from_str(
            r#"{"id":"x","title":"T","publishedAt":"2026-10-15T12:00:00Z","category":"poll"}"#,
        )
        .unwrap();
        assert_eq!(item.category, NewsCategory::Unknown);
        assert!(!item.pinned && item.tags.is_empty() && item.body.is_empty());
        assert!(!item.hidden && item.pin_order.is_none() && item.feed_override.is_none());
        // The new fields stay out of documents that do not use them.
        let text = serde_json::to_string(&item).unwrap();
        assert!(
            !text.contains("hidden")
                && !text.contains("pinOrder")
                && !text.contains("feedOverride"),
            "{text}"
        );
        let feed: NewsFeed = serde_json::from_str("{}").unwrap();
        assert_eq!(feed, NewsFeed::default());
    }
}
