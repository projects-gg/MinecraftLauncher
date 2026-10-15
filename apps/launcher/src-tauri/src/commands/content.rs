use std::sync::atomic::Ordering;

use pl_content::{ContentSnapshot, MediaRef, NewsItem};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::CmdResult;
use crate::state::AppState;

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HeroView {
    /// Local file path of the cached art (convert with `convertFileSrc`); `None` = bundled art.
    pub image_path: Option<String>,
    pub video_path: Option<String>,
    pub title: String,
    pub subtitle: String,
    pub news_id: Option<String>,
    pub cta_label: Option<String>,
    pub cta_url: Option<String>,
    pub particles: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct NewsView {
    pub id: String,
    pub title: String,
    pub summary: String,
    pub body: String,
    pub cover_path: Option<String>,
    pub category: String,
    pub published_at: String,
    pub pinned: bool,
    pub featured: bool,
    pub cta_label: Option<String>,
    pub cta_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BannerView {
    pub id: String,
    pub level: String,
    pub text: String,
    pub url: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EventView {
    pub id: String,
    pub title: String,
    pub starts_at: String,
    pub ends_at: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HomeView {
    pub hero: HeroView,
    pub banners: Vec<BannerView>,
    pub events: Vec<EventView>,
    pub news: Vec<NewsView>,
    pub offline: bool,
    pub from_cache: bool,
}

/// Serialize a serde enum to its JSON string form (camelCase variant names).
fn enum_str<T: Serialize>(value: &T) -> String {
    serde_json::to_value(value)
        .ok()
        .and_then(|v| v.as_str().map(str::to_owned))
        .unwrap_or_default()
}

async fn media_path(state: &AppState, media: Option<&MediaRef>) -> Option<String> {
    let media = media?;
    match state.content.media(media).await {
        Ok(path) => Some(path.to_string_lossy().into_owned()),
        Err(error) => {
            tracing::debug!(%error, path = %media.path, "media unavailable");
            None
        }
    }
}

async fn news_view(state: &AppState, item: &NewsItem) -> NewsView {
    NewsView {
        id: item.id.clone(),
        title: item.title.clone(),
        summary: item.summary.clone(),
        body: item.body.clone(),
        cover_path: media_path(state, item.cover.as_ref()).await,
        category: enum_str(&item.category),
        published_at: item.published_at.to_string(),
        pinned: item.pinned,
        featured: item.featured,
        cta_label: item.cta.as_ref().map(|c| c.label.clone()),
        cta_url: item.cta.as_ref().and_then(|c| c.url.clone()),
    }
}

async fn home_view(state: &AppState, snapshot: &ContentSnapshot) -> HomeView {
    let now = jiff::Timestamp::now();
    let hero = &snapshot.config.hero;
    let mut news = Vec::new();
    for item in snapshot.news.visible(now) {
        news.push(news_view(state, item).await);
    }
    let video = hero
        .media
        .as_ref()
        .filter(|m| matches!(m.kind, pl_content::MediaKind::Video));
    let image = match video {
        Some(v) => v.poster.as_deref(),
        None => hero.media.as_ref(),
    };
    HomeView {
        hero: HeroView {
            image_path: media_path(state, image).await,
            video_path: media_path(state, video).await,
            title: hero.title.clone(),
            subtitle: hero.subtitle.clone(),
            news_id: hero.cta.as_ref().and_then(|c| c.news_id.clone()),
            cta_label: hero.cta.as_ref().map(|c| c.label.clone()),
            cta_url: hero.cta.as_ref().and_then(|c| c.url.clone()),
            particles: enum_str(&hero.particles),
        },
        banners: snapshot
            .config
            .banners
            .iter()
            .filter(|b| b.is_active(now))
            .map(|b| BannerView {
                id: b.id.clone(),
                level: enum_str(&b.level),
                text: b.text.clone(),
                url: b.url.clone(),
            })
            .collect(),
        events: snapshot
            .config
            .visible_events(now)
            .into_iter()
            .map(|e| EventView {
                id: e.id.clone(),
                title: e.title.clone(),
                starts_at: e.starts_at.to_string(),
                ends_at: e.ends_at.to_string(),
            })
            .collect(),
        news,
        offline: snapshot.offline,
        from_cache: snapshot.from_cache,
    }
}

/// Home content. Returns the cached snapshot immediately (refreshing in the background and
/// emitting `content-changed` when something new arrives); without a cache it waits for the
/// network. `None` means "nothing yet": the UI shows its bundled welcome content.
#[tauri::command]
#[specta::specta]
pub async fn content_home(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<HomeView>> {
    if let Ok(cached) = state.content.load_cached().await {
        spawn_refresh(app, cached.index.sequence);
        return Ok(Some(home_view(&state, &cached).await));
    }
    match state.content.refresh().await {
        Ok(snapshot) => Ok(Some(home_view(&state, &snapshot).await)),
        Err(error) => {
            tracing::info!(%error, "no content yet");
            Ok(None)
        }
    }
}

fn spawn_refresh(app: AppHandle, known_sequence: u64) {
    let state = app.state::<AppState>();
    if state.content_refreshing.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        match state.content.refresh().await {
            Ok(snapshot) if !snapshot.from_cache && snapshot.index.sequence != known_sequence => {
                let _ = state.content.prefetch_media(&snapshot).await;
                let _ = app.emit("content-changed", ());
            }
            Ok(_) => {}
            Err(error) => tracing::info!(%error, "content refresh failed"),
        }
        state.content_refreshing.store(false, Ordering::SeqCst);
    });
}
