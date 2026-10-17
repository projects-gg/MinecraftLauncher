//! The Discord bot's news feed on the client side: fetching, verifying and caching it
//! ([`FeedClient`]) and the pure home screen rules that merge it with Studio's content
//! ([`merge_news`], [`newest_patch_notes`], [`merge_events`]). Wire format and item rules:
//! [`BotFeed`](crate::model::BotFeed) and [`NewsFeedConfig`](crate::model::NewsFeedConfig).

mod client;
mod merge;

pub use client::{FeedClient, FeedMediaReport, FeedSettings, FeedSnapshot, MirrorProbe};
pub use merge::{
    HomeEvent, HomeNewsItem, NewsOrigin, card_order, feed_age_floor, merge_events, merge_news,
    newest_patch_notes,
};
