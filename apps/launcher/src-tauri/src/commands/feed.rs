//! The Discord bot's news feed on the home screen (`docs/content-format.md` → "News feed
//! (Discord bot)"): settings from the signed launcher config, the verified cache, and a refresh
//! that follows every content refresh (no polling of its own). The merge with Studio's content
//! is `pl_content::merge_news` / `merge_events` in `commands::content`.

use pl_content::{FeedSettings, FeedSnapshot, LauncherConfig, MediaRef};
use tauri::{AppHandle, Emitter, Manager};

use crate::state::AppState;

/// Feed settings while the signed config enables the feed. Debug builds also read
/// `PROJECTS_FEED_MIRRORS` (comma-separated bases, `http://` allowed for local tests; replaces the
/// config's mirrors) and `PROJECTS_FEED_TEST_KEY` (one more trusted base64 public key). Release
/// builds ignore both, and neither turns the feed on.
pub(crate) fn settings(config: &LauncherConfig) -> Option<FeedSettings> {
    #[allow(unused_mut)]
    let mut settings = FeedSettings::from_config(config.active_news_feed()?)?;
    #[cfg(debug_assertions)]
    debug_overrides(&mut settings);
    settings.is_usable().then_some(settings)
}

#[cfg(debug_assertions)]
fn debug_overrides(settings: &mut FeedSettings) {
    if let Ok(list) = std::env::var("PROJECTS_FEED_MIRRORS") {
        let mirrors: Vec<tauri::Url> = list
            .split(',')
            .map(str::trim)
            .filter(|m| !m.is_empty())
            .filter_map(|m| {
                let base = if m.ends_with('/') {
                    m.to_owned()
                } else {
                    format!("{m}/")
                };
                tauri::Url::parse(&base).ok()
            })
            .filter(|url| matches!(url.scheme(), "http" | "https"))
            .collect();
        if !mirrors.is_empty() {
            settings.mirrors = mirrors;
        }
    }
    if let Ok(key) = std::env::var("PROJECTS_FEED_TEST_KEY")
        && let Ok(key) = pl_content::decode_verifying_key(key.trim())
    {
        settings.keys.push(key);
    }
}

/// The verified cached feed for the home screen; `None` while the config disables it or nothing
/// valid is cached.
pub(crate) async fn cached(state: &AppState, config: &LauncherConfig) -> Option<FeedSnapshot> {
    let settings = settings(config)?;
    state.feed.load_cached(&settings).await
}

/// Local path of a feed cover that is already downloaded and verified. The home screen never
/// waits for the network here: covers arrive with the refresh, which then updates the screen.
pub(crate) async fn cover_path(state: &AppState, media: Option<&MediaRef>) -> Option<String> {
    let path = state.feed.cached_media(media?).await?;
    Some(path.to_string_lossy().into_owned())
}

/// Refreshes the feed with `config` (right after a content refresh) and downloads the covers of
/// the visible items. True when the home screen should update. A disabled feed is never
/// requested, and its cache is deleted.
pub(crate) async fn refresh(state: &AppState, config: &LauncherConfig) -> bool {
    let Some(settings) = settings(config) else {
        if state.feed.has_cache()
            && let Err(error) = state.feed.clear().await
        {
            tracing::warn!(%error, "could not delete the news feed cache");
        }
        return false;
    };
    let before = state
        .feed
        .load_cached(&settings)
        .await
        .map(|snapshot| snapshot.feed.sequence);
    match state.feed.refresh(&settings).await {
        Ok(snapshot) => {
            let media = state
                .feed
                .prefetch_media(&settings, &snapshot, jiff::Timestamp::now())
                .await;
            if media.failed > 0 {
                tracing::info!(
                    failed = media.failed,
                    "some news feed covers are unavailable"
                );
            }
            Some(snapshot.feed.sequence) != before || media.downloaded > 0
        }
        Err(error) => {
            tracing::info!(%error, "news feed refresh failed");
            false
        }
    }
}

/// [`refresh`] in the background, telling the UI (`content-changed`) when something changed.
pub(crate) fn spawn_refresh(app: AppHandle, config: LauncherConfig) {
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        if refresh(&state, &config).await {
            let _ = app.emit("content-changed", ());
        }
    });
}
