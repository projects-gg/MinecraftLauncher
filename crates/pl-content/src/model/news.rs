use std::cmp::Ordering;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};

use super::{Cta, MediaRef, check_schema};
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
    #[serde(default)]
    pub featured: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub cta: Option<Cta>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub author: Option<String>,
}

impl NewsItem {
    pub fn is_visible(&self, now: Timestamp) -> bool {
        self.published_at <= now && self.expires_at.is_none_or(|expires| now < expires)
    }
}

/// Display order: pinned first, then newest first, then by id (stable).
pub fn compare_news(a: &NewsItem, b: &NewsItem) -> Ordering {
    b.pinned
        .cmp(&a.pinned)
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
            featured,
            cta: None,
            author: None,
        }
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
        let feed: NewsFeed = serde_json::from_str("{}").unwrap();
        assert_eq!(feed, NewsFeed::default());
    }
}
