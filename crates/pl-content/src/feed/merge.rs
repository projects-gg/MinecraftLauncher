//! Pure home screen rules: Studio's news and events merged with the bot's feed.
//!
//! Studio always wins: a feed item or event whose id Studio also publishes is ignored, even when
//! Studio's own entry is not visible (hidden, scheduled or expired). That is how Studio edits a
//! feed entry (its copy with the same id is shown instead) and hides one (a `hidden` entry with
//! the same id shows nothing). Feed items follow the [`NewsItem`] visibility rules plus the
//! config's age limit; pinned Studio items lead in their `pinOrder` ([`compare_news`]).

use std::cmp::Ordering;
use std::collections::HashSet;

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};

use crate::model::{
    BotFeed, EventCountdown, EventPhase, FeedSource, FeedStatus, MediaRef, MinecraftNews,
    NewsCategory, NewsFeed, NewsItem, compare_news,
};

/// Where a home screen entry comes from.
#[derive(
    Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum NewsOrigin {
    /// Studio's signed content (listed first on ties).
    Studio,
    /// The bot's feed.
    Feed,
}

/// A news item of the home screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeNewsItem {
    pub item: NewsItem,
    pub origin: NewsOrigin,
    /// The message a feed item summarizes.
    pub source: Option<FeedSource>,
}

/// Oldest `publishedAt` a feed item may have at `now` to be shown.
pub fn feed_age_floor(now: Timestamp, max_age_days: u32) -> Timestamp {
    now.checked_sub(SignedDuration::from_hours(24 * i64::from(max_age_days)))
        .unwrap_or(Timestamp::MIN)
}

/// The news column: Studio's visible news plus the feed's visible items (published at most
/// `max_age_days` ago) that Studio does not override, in display order (pinned first in their
/// `pinOrder`, then newest, then id; feed items are never pinned). A Studio item with a feed
/// item's id, shown or not (`hidden`, scheduled, expired), keeps that feed item out.
pub fn merge_news(
    studio: &NewsFeed,
    feed: Option<&BotFeed>,
    max_age_days: u32,
    now: Timestamp,
) -> Vec<HomeNewsItem> {
    let mut merged: Vec<HomeNewsItem> = studio
        .items
        .iter()
        .filter(|item| item.is_visible(now))
        .map(|item| HomeNewsItem {
            item: item.clone(),
            origin: NewsOrigin::Studio,
            source: None,
        })
        .collect();
    if let Some(feed) = feed {
        let studio_ids: HashSet<&str> = studio.items.iter().map(|i| i.id.as_str()).collect();
        let floor = feed_age_floor(now, max_age_days);
        merged.extend(
            feed.items
                .iter()
                .filter(|item| {
                    !studio_ids.contains(item.id.as_str())
                        && item.is_visible(now)
                        && item.published_at >= floor
                })
                .map(|item| HomeNewsItem {
                    item: item.to_news_item(),
                    origin: NewsOrigin::Feed,
                    source: item.source.clone(),
                }),
        );
    }
    merged.sort_by(|a, b| compare_news(&a.item, &b.item));
    merged
}

/// The feed's Minecraft entries the home screen may show at `now`, newest first (then the smaller
/// id): visible (`publishedAt <= now < expiresAt`), published at most `max_age_days` ago, and not
/// overridden by Studio (a Studio news item with the same id hides the entry even while that item
/// is not shown itself, as for feed items). Where they go is the screen's business: only into the
/// places Projects news leaves empty, never the hero, the patch notes box or the event card.
pub fn merge_minecraft<'a>(
    studio: &NewsFeed,
    feed: Option<&'a BotFeed>,
    max_age_days: u32,
    now: Timestamp,
) -> Vec<&'a MinecraftNews> {
    let Some(feed) = feed else {
        return Vec::new();
    };
    let studio_ids: HashSet<&str> = studio.items.iter().map(|i| i.id.as_str()).collect();
    let floor = feed_age_floor(now, max_age_days);
    let mut entries: Vec<&MinecraftNews> = feed
        .minecraft
        .iter()
        .filter(|entry| {
            !studio_ids.contains(entry.id.as_str())
                && entry.is_visible(now)
                && entry.published_at >= floor
        })
        .collect();
    entries.sort_by(|a, b| {
        b.published_at
            .cmp(&a.published_at)
            .then_with(|| a.id.cmp(&b.id))
    });
    entries
}

/// The feed's status notice for the home screen: shown while it has not expired
/// ([`FeedStatus::is_active`]) and never while Studio's maintenance notice is up (Studio wins;
/// both sit in the same place).
pub fn home_status(
    feed: Option<&BotFeed>,
    maintenance_active: bool,
    now: Timestamp,
) -> Option<&FeedStatus> {
    if maintenance_active {
        return None;
    }
    feed?.status.as_ref().filter(|status| status.is_active(now))
}

/// The patch notes box: the newest item of a [`merge_news`] result with category
/// `patchNotes` (by `publishedAt`, then id), from either source.
pub fn newest_patch_notes(news: &[HomeNewsItem]) -> Option<&HomeNewsItem> {
    news.iter()
        .filter(|entry| entry.item.category == NewsCategory::PatchNotes)
        .min_by(|a, b| {
            b.item
                .published_at
                .cmp(&a.item.published_at)
                .then_with(|| a.item.id.cmp(&b.item.id))
        })
}

/// An event of the home screen.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HomeEvent {
    pub id: String,
    pub title: String,
    pub starts_at: Timestamp,
    /// `endsAt`, or `startsAt` + [`OPEN_EVENT_DURATION`](crate::model::OPEN_EVENT_DURATION) for a
    /// feed event that did not say.
    pub ends_at: Timestamp,
    /// The feed did not say when it ends: show it as started, without time left.
    pub open_ended: bool,
    pub url: Option<String>,
    /// The news item about the event, when it is on the home screen.
    pub news_id: Option<String>,
    /// Studio's event art (config events only).
    pub image: Option<MediaRef>,
    /// Higher leads among live events (default 0).
    pub priority: i32,
    /// On the server: the card offers "Katıl" while the event is live.
    pub join: bool,
    pub origin: NewsOrigin,
}

impl HomeEvent {
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

/// Config and feed events that have not ended, in event card order (the card shows the first;
/// see [`card_order`]): live events first, the highest `priority` leading, then the one that
/// started last (a short event overlays a long one while it runs, and the long one comes back
/// when it ends), then upcoming events, soonest first. Hidden config events are left out, and a
/// feed event whose id a config event uses (hidden or not) is ignored. An event's `newsId` is
/// kept only when that item is in `news`.
pub fn merge_events(
    config: &[EventCountdown],
    feed: Option<&BotFeed>,
    news: &[HomeNewsItem],
    now: Timestamp,
) -> Vec<HomeEvent> {
    let news_ids: HashSet<&str> = news.iter().map(|n| n.item.id.as_str()).collect();
    let mut events: Vec<HomeEvent> = config
        .iter()
        .filter(|event| !event.hidden)
        .map(|event| HomeEvent {
            id: event.id.clone(),
            title: event.title.clone(),
            starts_at: event.starts_at,
            ends_at: event.ends_at,
            open_ended: false,
            url: event.url.clone(),
            news_id: event
                .news_id
                .clone()
                .filter(|id| news_ids.contains(id.as_str())),
            image: event.image.clone(),
            priority: event.priority,
            join: event.join,
            origin: NewsOrigin::Studio,
        })
        .collect();
    if let Some(feed) = feed {
        let config_ids: HashSet<&str> = config.iter().map(|e| e.id.as_str()).collect();
        events.extend(
            feed.events
                .iter()
                .filter(|event| !config_ids.contains(event.id.as_str()))
                .map(|event| HomeEvent {
                    id: event.id.clone(),
                    title: event.title.clone(),
                    starts_at: event.starts_at,
                    ends_at: event.end(),
                    open_ended: event.ends_at.is_none(),
                    url: event.url.clone(),
                    news_id: event
                        .news_id
                        .clone()
                        .filter(|id| news_ids.contains(id.as_str())),
                    image: None,
                    priority: event.priority,
                    join: event.join,
                    origin: NewsOrigin::Feed,
                }),
        );
    }
    events.retain(|event| event.phase(now) != EventPhase::Ended);
    events.sort_by(|a, b| card_order(a, b, now));
    events
}

/// Event card order at `now` (the card shows the first event that has not ended):
///
/// 1. Live events before upcoming ones.
/// 2. Live: higher `priority` first, then the later `startsAt` (the newest event overlays the
///    ones already running; when it ends they come back in the same order).
/// 3. Upcoming: the sooner `startsAt` first, then the higher `priority`.
/// 4. Config before feed, then the smaller id.
pub fn card_order(a: &HomeEvent, b: &HomeEvent, now: Timestamp) -> Ordering {
    let a_live = a.phase(now) == EventPhase::Live;
    let b_live = b.phase(now) == EventPhase::Live;
    b_live
        .cmp(&a_live)
        .then_with(|| {
            if a_live {
                b.priority
                    .cmp(&a.priority)
                    .then_with(|| b.starts_at.cmp(&a.starts_at))
            } else {
                a.starts_at
                    .cmp(&b.starts_at)
                    .then_with(|| b.priority.cmp(&a.priority))
            }
        })
        .then_with(|| a.origin.cmp(&b.origin))
        .then_with(|| a.id.cmp(&b.id))
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;

    fn ts(text: &str) -> Timestamp {
        text.parse().unwrap()
    }

    const NOW: &str = "2026-10-16T12:00:00Z";

    fn studio_item(id: &str, published: &str) -> NewsItem {
        NewsItem {
            id: id.into(),
            title: id.into(),
            summary: String::new(),
            body: String::new(),
            cover: None,
            category: NewsCategory::Announcement,
            tags: Vec::new(),
            published_at: ts(published),
            expires_at: None,
            pinned: false,
            pin_order: None,
            featured: false,
            hidden: false,
            cta: None,
            author: None,
            feed_override: None,
        }
    }

    fn feed_item(id: &str, published: &str) -> Value {
        json!({
            "id": id,
            "title": id,
            "publishedAt": published,
            "source": { "kind": "discord", "url": format!("https://discord.com/channels/1/2/{id}") }
        })
    }

    fn bot_feed(items: Vec<Value>, events: Vec<Value>) -> BotFeed {
        let bytes = serde_json::to_vec(&json!({
            "schema": 1,
            "sequence": 1,
            "generatedAt": NOW,
            "items": items,
            "events": events
        }))
        .unwrap();
        let (feed, problems) = BotFeed::parse(&bytes).unwrap();
        assert!(
            problems.iter().all(|p| !p.issue.drops()),
            "fixture items must be valid: {problems:?}"
        );
        feed
    }

    fn ids(news: &[HomeNewsItem]) -> Vec<&str> {
        news.iter().map(|n| n.item.id.as_str()).collect()
    }

    #[test]
    fn the_status_notice_shows_until_it_expires_and_maintenance_wins() {
        let bytes = serde_json::to_vec(&json!({
            "schema": 1,
            "sequence": 1,
            "generatedAt": NOW,
            "status": {
                "level": "outage",
                "title": "Bazı hizmetlerde sorun var",
                "message": "Şu an erişilemiyor: Survival Zephyr",
                "updatedAt": "2026-10-16T11:55:00Z",
                "expiresAt": "2026-10-16T12:10:00Z"
            }
        }))
        .unwrap();
        let (feed, problems) = BotFeed::parse(&bytes).unwrap();
        assert!(problems.is_empty(), "{problems:?}");

        let shown = home_status(Some(&feed), false, ts(NOW)).expect("active");
        assert_eq!(shown.title, "Bazı hizmetlerde sorun var");
        assert!(home_status(Some(&feed), false, ts("2026-10-16T12:09:59Z")).is_some());
        assert!(
            home_status(Some(&feed), false, ts("2026-10-16T12:10:00Z")).is_none(),
            "an expired notice is never shown, so a stopped bot leaves nothing behind"
        );
        assert!(
            home_status(Some(&feed), true, ts(NOW)).is_none(),
            "Studio's maintenance notice wins"
        );
        assert!(home_status(None, false, ts(NOW)).is_none());
        let quiet = bot_feed(vec![feed_item("discord-1", NOW)], Vec::new());
        assert!(home_status(Some(&quiet), false, ts(NOW)).is_none());
    }

    #[test]
    fn studio_wins_and_order_holds() {
        let mut pinned = studio_item("studio-pinned", "2026-09-01T00:00:00Z");
        pinned.pinned = true;
        let mut hidden = studio_item("discord-2", "2026-10-30T00:00:00Z");
        hidden.title = "Studio'nun planladığı".into();
        let studio = NewsFeed {
            schema: 1,
            items: vec![
                pinned,
                studio_item("studio-new", "2026-10-16T10:00:00Z"),
                studio_item("discord-1", "2026-10-15T00:00:00Z"),
                hidden,
            ],
        };
        let mut pinned_feed = feed_item("discord-3", "2026-10-16T11:00:00Z");
        pinned_feed["pinned"] = json!(true);
        let feed = bot_feed(
            vec![
                feed_item("discord-1", "2026-10-16T11:30:00Z"),
                feed_item("discord-2", "2026-10-16T11:30:00Z"),
                pinned_feed,
                feed_item("discord-4", "2026-10-16T10:00:00Z"),
            ],
            Vec::new(),
        );
        let merged = merge_news(&studio, Some(&feed), 60, ts(NOW));
        assert_eq!(
            ids(&merged),
            [
                "studio-pinned",
                "discord-3",
                "discord-4",
                "studio-new",
                "discord-1"
            ]
        );
        // discord-1 is Studio's copy; discord-2 is hidden by Studio's scheduled item.
        let copy = merged.iter().find(|n| n.item.id == "discord-1").unwrap();
        assert_eq!(copy.origin, NewsOrigin::Studio);
        assert!(copy.source.is_none());
        let from_feed = merged.iter().find(|n| n.item.id == "discord-3").unwrap();
        assert_eq!(from_feed.origin, NewsOrigin::Feed);
        assert!(!from_feed.item.pinned, "the feed cannot pin");
        assert!(from_feed.source.as_ref().unwrap().url.is_some());

        assert_eq!(merge_news(&studio, None, 60, ts(NOW)).len(), 3);
    }

    #[test]
    fn feed_visibility_and_age_limit() {
        let mut expired = feed_item("expired", "2026-10-01T00:00:00Z");
        expired["expiresAt"] = json!("2026-10-16T12:00:00Z");
        let mut expiring = feed_item("expiring", "2026-10-01T00:00:00Z");
        expiring["expiresAt"] = json!("2026-10-16T12:00:01Z");
        let feed = bot_feed(
            vec![
                feed_item("scheduled", "2026-10-16T12:00:01Z"),
                feed_item("now", NOW),
                expired,
                expiring,
                // Exactly 60 days old is still shown; one second more is not.
                feed_item("edge", "2026-08-17T12:00:00Z"),
                feed_item("old", "2026-08-17T11:59:59Z"),
            ],
            Vec::new(),
        );
        let empty = NewsFeed::default();
        let merged = merge_news(&empty, Some(&feed), 60, ts(NOW));
        assert_eq!(ids(&merged), ["now", "expiring", "edge"]);
        let one_day = merge_news(&empty, Some(&feed), 1, ts(NOW));
        assert_eq!(ids(&one_day), ["now"]);
        assert_eq!(feed_age_floor(Timestamp::MIN, 365), Timestamp::MIN);
    }

    fn minecraft(version: &str, published: &str, expires: &str) -> Value {
        json!({
            "id": format!("minecraft-{version}"),
            "version": version,
            "kind": "release",
            "title": format!("Minecraft {version}"),
            "publishedAt": published,
            "expiresAt": expires,
            "source": {
                "kind": "minecraft",
                "url": "https://www.minecraft.net/article/minecraft-java-edition"
            }
        })
    }

    fn feed_with_minecraft(items: Vec<Value>, minecraft: Vec<Value>) -> BotFeed {
        let bytes = serde_json::to_vec(&json!({
            "schema": 1,
            "sequence": 1,
            "generatedAt": NOW,
            "items": items,
            "minecraft": minecraft
        }))
        .unwrap();
        let (feed, problems) = BotFeed::parse(&bytes).unwrap();
        assert!(
            problems.is_empty(),
            "fixture entries must be valid: {problems:?}"
        );
        feed
    }

    fn minecraft_ids(entries: Vec<&MinecraftNews>) -> Vec<&str> {
        entries.into_iter().map(|m| m.id.as_str()).collect()
    }

    #[test]
    fn minecraft_entries_show_while_visible_and_young_enough() {
        let feed = feed_with_minecraft(
            vec![feed_item("discord-1", "2026-10-16T10:00:00Z")],
            vec![
                minecraft("26.4", "2026-10-16T12:00:01Z", "2026-10-30T00:00:00Z"),
                minecraft("26.3", "2026-09-15T11:23:02Z", "2026-10-30T11:23:02Z"),
                minecraft("26.2", "2026-08-20T00:00:00Z", "2026-10-16T12:00:00Z"),
                minecraft("26.1.2", "2026-10-01T00:00:00Z", "2026-10-16T12:00:01Z"),
                // Exactly 60 days old is still shown; one second more is not.
                minecraft("26.1", "2026-08-17T12:00:00Z", "2026-11-01T00:00:00Z"),
                minecraft("26.0", "2026-08-17T11:59:59Z", "2026-11-01T00:00:00Z"),
            ],
        );
        let empty = NewsFeed::default();
        assert_eq!(
            minecraft_ids(merge_minecraft(&empty, Some(&feed), 60, ts(NOW))),
            ["minecraft-26.1.2", "minecraft-26.3", "minecraft-26.1"],
            "scheduled and expired entries wait or go; newest first"
        );
        assert!(merge_minecraft(&empty, Some(&feed), 1, ts(NOW)).is_empty());
        assert!(merge_minecraft(&empty, None, 60, ts(NOW)).is_empty());

        // They never become Projects news, patch notes or events.
        let news = merge_news(&empty, Some(&feed), 60, ts(NOW));
        assert_eq!(ids(&news), ["discord-1"]);
        assert!(newest_patch_notes(&news).is_none());
        assert!(merge_events(&[], Some(&feed), &news, ts(NOW)).is_empty());
    }

    #[test]
    fn studio_hides_or_takes_over_minecraft_entries() {
        let feed = feed_with_minecraft(
            Vec::new(),
            vec![
                minecraft("26.1", "2026-09-01T00:00:00Z", "2026-11-01T00:00:00Z"),
                minecraft("26.3", "2026-10-01T00:00:00Z", "2026-11-01T00:00:00Z"),
                minecraft("26.2", "2026-10-01T00:00:00Z", "2026-11-01T00:00:00Z"),
                minecraft(
                    "26.4-snapshot-4",
                    "2026-10-13T12:05:46Z",
                    "2026-10-27T12:05:46Z",
                ),
            ],
        );
        let empty = NewsFeed::default();
        assert_eq!(
            minecraft_ids(merge_minecraft(&empty, Some(&feed), 60, ts(NOW))),
            [
                "minecraft-26.4-snapshot-4",
                "minecraft-26.2",
                "minecraft-26.3",
                "minecraft-26.1"
            ],
            "newest first, then the smaller id"
        );

        // Studio hides 26.3, takes 26.1 over with its own copy and schedules one of the snapshot.
        let mut hidden = studio_item("minecraft-26.3", "2026-10-01T00:00:00Z");
        hidden.hidden = true;
        let studio = NewsFeed {
            schema: 1,
            items: vec![
                hidden,
                studio_item("minecraft-26.1", "2026-09-01T00:00:00Z"),
                studio_item("minecraft-26.4-snapshot-4", "2026-10-30T00:00:00Z"),
            ],
        };
        assert_eq!(
            minecraft_ids(merge_minecraft(&studio, Some(&feed), 60, ts(NOW))),
            ["minecraft-26.2"],
            "Studio wins on the id, shown or not"
        );
        let news = merge_news(&studio, Some(&feed), 60, ts(NOW));
        assert_eq!(ids(&news), ["minecraft-26.1"], "the copy is Projects news");
        assert_eq!(news[0].origin, NewsOrigin::Studio);
    }

    #[test]
    fn patch_notes_pick_the_newest_from_either_source() {
        let mut studio_notes = studio_item("studio-notes", "2026-10-10T00:00:00Z");
        studio_notes.category = NewsCategory::PatchNotes;
        studio_notes.pinned = true;
        let studio = NewsFeed {
            schema: 1,
            items: vec![studio_notes, studio_item("other", "2026-10-16T11:00:00Z")],
        };
        let mut b = feed_item("discord-b", "2026-10-15T00:00:00Z");
        b["category"] = json!("patchNotes");
        let mut a = feed_item("discord-a", "2026-10-15T00:00:00Z");
        a["category"] = json!("patchNotes");
        let mut future = feed_item("discord-future", "2026-10-17T00:00:00Z");
        future["category"] = json!("patchNotes");
        let feed = bot_feed(vec![b, a, future], Vec::new());

        let merged = merge_news(&studio, Some(&feed), 60, ts(NOW));
        let notes = newest_patch_notes(&merged).unwrap();
        assert_eq!(notes.item.id, "discord-a", "newest, then smallest id");
        let studio_only = merge_news(&studio, None, 60, ts(NOW));
        assert_eq!(
            newest_patch_notes(&studio_only).unwrap().item.id,
            "studio-notes"
        );
        assert!(newest_patch_notes(&[]).is_none());
    }

    fn config_event(id: &str, start: &str, end: &str) -> EventCountdown {
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
    fn join_follows_each_source_and_studio_wins() {
        let feed = bot_feed(
            Vec::new(),
            vec![
                json!({ "id": "cup", "title": "Kupa", "startsAt": "2026-10-16T11:00:00Z", "join": true }),
                json!({ "id": "talk", "title": "Soru-cevap", "startsAt": "2026-10-16T11:30:00Z" }),
                json!({ "id": "raid", "title": "Baskın", "startsAt": "2026-10-16T11:45:00Z", "join": true }),
            ],
        );
        // Studio marks the feed's "talk" as on the server and takes "raid" over without it.
        let mut talk = config_event("talk", "2026-10-16T11:30:00Z", "2026-10-16T14:00:00Z");
        talk.join = true;
        let raid = config_event("raid", "2026-10-16T11:45:00Z", "2026-10-16T14:00:00Z");
        let season = config_event("season", "2026-10-01T00:00:00Z", "2026-11-01T00:00:00Z");
        let events = merge_events(&[talk, raid, season], Some(&feed), &[], ts(NOW));
        let joins: Vec<(&str, bool)> = events.iter().map(|e| (e.id.as_str(), e.join)).collect();
        assert_eq!(
            joins,
            [
                ("raid", false),
                ("talk", true),
                ("cup", true),
                ("season", false)
            ]
        );
    }

    fn event_ids(events: &[HomeEvent]) -> Vec<&str> {
        events.iter().map(|e| e.id.as_str()).collect()
    }

    /// What the card shows at `at`.
    fn on_card(config: &[EventCountdown], feed: Option<&BotFeed>, at: &str) -> Option<String> {
        merge_events(config, feed, &[], ts(at))
            .first()
            .map(|e| e.id.clone())
    }

    #[test]
    fn live_events_lead_then_the_soonest_upcoming() {
        let config = [
            config_event("season", "2026-10-01T00:00:00Z", "2026-11-01T00:00:00Z"),
            config_event("cup", "2026-10-20T18:00:00Z", "2026-10-20T21:00:00Z"),
            config_event("over", "2026-10-15T18:00:00Z", "2026-10-16T12:00:00Z"),
        ];
        let feed = bot_feed(
            vec![feed_item("discord-1", "2026-10-16T09:00:00Z")],
            vec![
                json!({ "id": "tonight", "title": "Bu akşam", "startsAt": "2026-10-16T11:00:00Z", "endsAt": "2026-10-16T14:00:00Z", "newsId": "discord-1" }),
                json!({ "id": "open", "title": "Başladı", "startsAt": "2026-10-16T10:00:00Z" }),
                json!({ "id": "next", "title": "Yarın", "startsAt": "2026-10-17T18:00:00Z" }),
                json!({ "id": "cup", "title": "Kopya", "startsAt": "2026-10-16T11:00:00Z" }),
            ],
        );
        let news = merge_news(&NewsFeed::default(), Some(&feed), 60, ts(NOW));
        let events = merge_events(&config, Some(&feed), &news, ts(NOW));
        // Live, the last started first: "tonight" (11:00), "open" (10:00), "season" (October 1).
        // Upcoming: "next" (tomorrow), "cup" (the config's, the feed copy is ignored).
        assert_eq!(
            event_ids(&events),
            ["tonight", "open", "season", "next", "cup"]
        );
        let open = &events[1];
        assert!(open.open_ended);
        assert_eq!(open.ends_at, ts("2026-10-16T13:00:00Z"));
        assert_eq!(open.phase(ts(NOW)), EventPhase::Live);
        assert_eq!(events[0].news_id.as_deref(), Some("discord-1"));
        assert_eq!(events[4].origin, NewsOrigin::Studio);
        assert_eq!(events[4].title, "cup");

        // The news item is not on the home screen (too old): the event keeps no link.
        let news = merge_news(
            &NewsFeed::default(),
            Some(&feed),
            60,
            ts("2026-12-30T00:00:00Z"),
        );
        assert!(news.is_empty());
        let later = merge_events(&config, Some(&feed), &news, ts("2026-10-16T13:30:00Z"));
        assert_eq!(event_ids(&later), ["tonight", "season", "next", "cup"]);
        assert_eq!(later[0].news_id, None);
    }

    #[test]
    fn clock_edges_and_ties() {
        let config = [
            config_event("b-config", "2026-10-16T12:00:00Z", "2026-10-16T13:00:00Z"),
            config_event("z-config", "2026-10-16T14:00:00Z", "2026-10-16T15:00:00Z"),
        ];
        let feed = bot_feed(
            Vec::new(),
            vec![
                json!({ "id": "a-feed", "title": "A", "startsAt": "2026-10-16T12:00:00Z", "endsAt": "2026-10-16T13:00:00Z" }),
                json!({ "id": "a-upcoming", "title": "A", "startsAt": "2026-10-16T14:00:00Z", "endsAt": "2026-10-16T15:00:00Z" }),
            ],
        );
        // Starting now is live; same end: config first even with a larger id.
        let at_start = merge_events(&config, Some(&feed), &[], ts(NOW));
        assert_eq!(
            event_ids(&at_start),
            ["b-config", "a-feed", "z-config", "a-upcoming"]
        );
        // Ending now is over.
        let at_end = merge_events(&config, Some(&feed), &[], ts("2026-10-16T13:00:00Z"));
        assert_eq!(event_ids(&at_end), ["z-config", "a-upcoming"]);
        assert_eq!(
            at_end[0].phase(ts("2026-10-16T13:00:00Z")),
            EventPhase::Upcoming
        );
        assert!(merge_events(&[], None, &[], ts(NOW)).is_empty());
    }

    #[test]
    fn studio_edits_hides_and_orders_feed_news() {
        // Studio's copy of discord-1 (edited title, now patch notes), a hidden discord-2, a hidden
        // item without a feed twin, and two pinned items in a chosen order.
        let mut copy = studio_item("discord-1", "2026-10-16T11:30:00Z");
        copy.title = "Studio'nun düzelttiği".into();
        copy.category = NewsCategory::PatchNotes;
        let mut hidden = studio_item("discord-2", "2026-10-16T11:00:00Z");
        hidden.hidden = true;
        hidden.expires_at = Some(hidden.published_at);
        let mut lonely_hidden = studio_item("lonely", "2026-10-16T09:00:00Z");
        lonely_hidden.hidden = true;
        let mut pinned_second = studio_item("pinned-second", "2026-10-16T10:00:00Z");
        pinned_second.pinned = true;
        pinned_second.pin_order = Some(2);
        let mut pinned_first = studio_item("pinned-first", "2026-09-01T00:00:00Z");
        pinned_first.pinned = true;
        pinned_first.pin_order = Some(1);
        let studio = NewsFeed {
            schema: 1,
            items: vec![copy, hidden, lonely_hidden, pinned_second, pinned_first],
        };
        let mut newer_notes = feed_item("discord-2", "2026-10-16T11:00:00Z");
        newer_notes["category"] = json!("patchNotes");
        let mut older_notes = feed_item("discord-3", "2026-10-16T08:00:00Z");
        older_notes["category"] = json!("patchNotes");
        let feed = bot_feed(
            vec![
                feed_item("discord-1", "2026-10-16T11:30:00Z"),
                newer_notes,
                older_notes,
            ],
            Vec::new(),
        );

        let merged = merge_news(&studio, Some(&feed), 60, ts(NOW));
        assert_eq!(
            ids(&merged),
            ["pinned-first", "pinned-second", "discord-1", "discord-3"]
        );
        let edited = &merged[2];
        assert_eq!(edited.origin, NewsOrigin::Studio);
        assert_eq!(edited.item.title, "Studio'nun düzelttiği");
        // The box takes Studio's copy (now patch notes, newest); the hidden discord-2 never counts.
        assert_eq!(
            newest_patch_notes(&merged).map(|n| n.item.id.as_str()),
            Some("discord-1")
        );
        let without_copy = NewsFeed {
            schema: 1,
            items: studio.items[1..].to_vec(),
        };
        let merged = merge_news(&without_copy, Some(&feed), 60, ts(NOW));
        assert_eq!(
            newest_patch_notes(&merged).map(|n| n.item.id.as_str()),
            Some("discord-3"),
            "hidden discord-2 is skipped, not just shadowed"
        );
        // Without the feed, hidden items still show nothing.
        assert_eq!(
            ids(&merge_news(&studio, None, 60, ts(NOW))),
            ["pinned-first", "pinned-second", "discord-1"]
        );
    }

    #[test]
    fn studio_edits_and_hides_feed_events() {
        let feed = bot_feed(
            vec![feed_item("discord-1", "2026-10-16T09:00:00Z")],
            vec![
                json!({ "id": "discord-1", "title": "Discord'daki", "startsAt": "2026-10-16T13:00:00Z", "newsId": "discord-1" }),
                json!({ "id": "discord-2", "title": "Gizlenecek", "startsAt": "2026-10-16T11:00:00Z", "endsAt": "2026-10-16T15:00:00Z" }),
                json!({ "id": "discord-3", "title": "Dokunulmayan", "startsAt": "2026-10-17T18:00:00Z" }),
            ],
        );
        let news = merge_news(&NewsFeed::default(), Some(&feed), 60, ts(NOW));
        // Studio's copy of discord-1 (new title and end, still opening the article), a hidden
        // discord-2, and a config event linking to an article that is not on the home screen.
        let mut copy = config_event("discord-1", "2026-10-16T13:00:00Z", "2026-10-16T18:00:00Z");
        copy.title = "Studio'daki".into();
        copy.news_id = Some("discord-1".into());
        let mut hidden = config_event("discord-2", "2026-10-16T11:00:00Z", "2026-10-16T15:00:00Z");
        hidden.hidden = true;
        let mut stale_link = config_event("cup", "2026-10-18T18:00:00Z", "2026-10-18T21:00:00Z");
        stale_link.news_id = Some("gone".into());
        let config = [copy, hidden, stale_link];

        let events = merge_events(&config, Some(&feed), &news, ts(NOW));
        assert_eq!(event_ids(&events), ["discord-1", "discord-3", "cup"]);
        assert_eq!(events[0].origin, NewsOrigin::Studio);
        assert_eq!(events[0].title, "Studio'daki");
        assert_eq!(events[0].ends_at, ts("2026-10-16T18:00:00Z"));
        assert!(!events[0].open_ended);
        assert_eq!(events[0].news_id.as_deref(), Some("discord-1"));
        assert_eq!(
            events[2].news_id, None,
            "the article is not on the home screen"
        );
        // A hidden config event never reaches the card, with or without the feed.
        assert!(
            merge_events(&config, None, &news, ts(NOW))
                .iter()
                .all(|e| e.id != "discord-2")
        );
    }

    #[test]
    fn a_short_event_overlays_a_long_one_while_it_runs() {
        // The owner's case: a week-long beginner event; a two-hour tournament starts during it.
        let beginner = config_event("beginner", "2026-10-16T00:00:00Z", "2026-10-23T00:00:00Z");
        let feed = bot_feed(
            Vec::new(),
            vec![
                json!({ "id": "cup", "title": "Kupa", "startsAt": "2026-10-16T14:00:00Z", "endsAt": "2026-10-16T16:00:00Z" }),
            ],
        );
        let config = [beginner.clone()];
        let card = |at: &str| on_card(&config, Some(&feed), at);
        assert_eq!(card("2026-10-16T13:59:59Z").as_deref(), Some("beginner"));
        assert_eq!(card("2026-10-16T14:00:00Z").as_deref(), Some("cup"));
        assert_eq!(card("2026-10-16T15:59:59Z").as_deref(), Some("cup"));
        assert_eq!(card("2026-10-16T16:00:00Z").as_deref(), Some("beginner"));
        // While the cup runs, the beginner event waits right behind it.
        let both = merge_events(&config, Some(&feed), &[], ts("2026-10-16T15:00:00Z"));
        assert_eq!(event_ids(&both), ["cup", "beginner"]);

        // Priority wins over "started last": a raised beginner event stays in front, a lowered
        // tournament never comes forward while something else runs.
        let raised = [EventCountdown {
            priority: 1,
            ..beginner.clone()
        }];
        assert_eq!(
            on_card(&raised, Some(&feed), "2026-10-16T15:00:00Z").as_deref(),
            Some("beginner")
        );
        let lowered = bot_feed(
            Vec::new(),
            vec![
                json!({ "id": "cup", "title": "Kupa", "startsAt": "2026-10-16T14:00:00Z", "endsAt": "2026-10-16T16:00:00Z", "priority": -1 }),
            ],
        );
        assert_eq!(
            on_card(&config, Some(&lowered), "2026-10-16T15:00:00Z").as_deref(),
            Some("beginner")
        );
        // Nothing live: the soonest upcoming event, whatever its priority.
        let later = [config_event(
            "night",
            "2026-10-25T20:00:00Z",
            "2026-10-25T22:00:00Z",
        )];
        let upcoming = [
            EventCountdown {
                priority: 5,
                ..config_event("far", "2026-10-26T20:00:00Z", "2026-10-26T22:00:00Z")
            },
            later[0].clone(),
        ];
        assert_eq!(
            on_card(&upcoming, None, "2026-10-24T00:00:00Z").as_deref(),
            Some("night")
        );
        // Upcoming at the same moment: the higher priority first.
        let same_start = [
            config_event("a-plain", "2026-10-25T20:00:00Z", "2026-10-25T22:00:00Z"),
            EventCountdown {
                priority: 2,
                ..config_event("z-raised", "2026-10-25T20:00:00Z", "2026-10-25T21:00:00Z")
            },
        ];
        assert_eq!(
            event_ids(&merge_events(
                &same_start,
                None,
                &[],
                ts("2026-10-24T00:00:00Z")
            )),
            ["z-raised", "a-plain"]
        );
        // An open-ended feed event is live for three hours and overlays like any other.
        let open = bot_feed(
            Vec::new(),
            vec![json!({ "id": "raid", "title": "Baskın", "startsAt": "2026-10-17T20:00:00Z" })],
        );
        assert_eq!(
            on_card(&config, Some(&open), "2026-10-17T22:59:59Z").as_deref(),
            Some("raid")
        );
        assert_eq!(
            on_card(&config, Some(&open), "2026-10-17T23:00:00Z").as_deref(),
            Some("beginner")
        );
    }
}
