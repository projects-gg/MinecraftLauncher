use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::time::Duration;

use pl_content::{ContentSnapshot, HomeNewsItem, MediaRef, NewsOrigin};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Manager, State};

use crate::error::CmdResult;
use crate::state::AppState;

/// Whether the content mirrors could be reached, for the home screen's offline strip. Changes
/// arrive as the `content-status` event.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentStatusView {
    /// The last refresh reached no mirror: the cached (or bundled) content is shown.
    pub offline: bool,
}

/// Last known reachability of the mirrors (false until a refresh says otherwise).
static OFFLINE: AtomicBool = AtomicBool::new(false);
/// Refreshes in a row that found no mirror (spaces out the retries).
static OFFLINE_STREAK: AtomicU32 = AtomicU32::new(0);
/// A retry after an offline refresh is waiting.
static RETRY_PENDING: AtomicBool = AtomicBool::new(false);
/// Sequence of the index last applied (0: none), so only a newer one counts as new content.
static APPLIED_SEQUENCE: AtomicU64 = AtomicU64::new(0);

/// Waits before checking the mirrors again while offline: soon at first, then every minute.
fn offline_retry_delay(streak: u32) -> Duration {
    Duration::from_secs(match streak {
        0 => 15,
        1 => 30,
        _ => 60,
    })
}

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
    /// From the Discord bot's news feed (without a cover the UI shows the season art).
    pub from_feed: bool,
    /// The original Discord message of a feed item (`https://` only), for "Discord'da aç".
    pub source_url: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BannerView {
    pub id: String,
    pub level: String,
    pub text: String,
    pub url: Option<String>,
    /// The player may close it (and it stays closed).
    pub dismissible: bool,
    /// When it stops being shown (the UI drops it then without a refresh).
    pub ends_at: Option<String>,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct EventView {
    pub id: String,
    pub title: String,
    pub starts_at: String,
    /// When it ends; for an `open_ended` feed event, three hours after the start.
    pub ends_at: String,
    /// The feed did not say when it ends: shown as started, without time left.
    pub open_ended: bool,
    pub url: Option<String>,
    /// The news item about the event (on the home screen).
    pub news_id: Option<String>,
    /// Among live events the highest leads the card (`pl_content::card_order`).
    pub priority: i32,
    /// From the Discord bot's feed (config events win ties).
    pub from_feed: bool,
}

/// A link Studio publishes (`LauncherConfig.links.custom`).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LinkView {
    pub label: String,
    pub url: String,
}

/// Studio's links (`LauncherConfig.links`), only `https://` and `projects://` ones.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct LinksView {
    pub website: Option<String>,
    pub discord: Option<String>,
    pub store: Option<String>,
    pub support: Option<String>,
    pub custom: Vec<LinkView>,
}

impl LinksView {
    fn from_config(links: &pl_content::Links) -> Self {
        let safe = |url: &Option<String>| url.clone().filter(|u| pl_content::is_safe_link(u));
        Self {
            website: safe(&links.website),
            discord: safe(&links.discord),
            store: safe(&links.store),
            support: safe(&links.support),
            custom: links
                .custom
                .iter()
                .filter(|l| !l.label.trim().is_empty() && pl_content::is_safe_link(&l.url))
                .map(|l| LinkView {
                    label: l.label.clone(),
                    url: l.url.clone(),
                })
                .collect(),
        }
    }
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MaintenanceView {
    pub title: String,
    pub message: String,
    pub until: Option<String>,
}

/// The news feed's status notice (`pl_content::home_status`): an active problem on the status
/// page, shown where the maintenance notice goes while there is no maintenance notice.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct StatusView {
    /// `outage`, `degraded` or `notice`.
    pub level: String,
    pub title: String,
    /// May be empty.
    pub message: String,
    /// The status page (`https://` only).
    pub url: Option<String>,
    /// When it stops being shown (the UI drops it then without a refresh).
    pub expires_at: String,
}

/// Latest release of the official pack, for the patch notes card.
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct PackNotesView {
    pub name: String,
    pub version: String,
    pub published_at: String,
    /// Markdown.
    pub changelog: String,
}

#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct HomeView {
    pub hero: HeroView,
    pub banners: Vec<BannerView>,
    /// Studio's and the news feed's events that have not ended, in event card order (the card
    /// shows the first): live ones first, ending soonest leading, then the soonest upcoming.
    pub events: Vec<EventView>,
    /// Studio's news and the news feed's items, merged (Studio wins on the same id).
    pub news: Vec<NewsView>,
    pub maintenance: Option<MaintenanceView>,
    /// The news feed's status notice; `None` while `maintenance` is shown.
    pub status: Option<StatusView>,
    pub pack_notes: Option<PackNotesView>,
    /// The newest patch notes in `news` (the patch notes box; `pack_notes` when there is none).
    pub patch_notes_id: Option<String>,
    /// Studio's links (Settings → Hakkında).
    pub links: LinksView,
    pub offline: bool,
    pub from_cache: bool,
}

/// Effects of a content snapshot beyond the home screen: the AFK client build to run, the AFK
/// accounts' game versions and whether the friends feature is on.
pub(crate) fn apply_snapshot(state: &AppState, snapshot: &ContentSnapshot) {
    APPLIED_SEQUENCE.store(snapshot.index.sequence, Ordering::SeqCst);
    crate::commands::social::apply_flag(&state.social, snapshot.config.features.friends);
    let release = snapshot.index.afk.as_ref().and_then(|afk| {
        let build = afk.current_build()?;
        Some(pl_afk::AfkClientRelease {
            version: afk.version.clone(),
            build: pl_afk::AfkClientBuild {
                urls: state.content.file_urls(build),
                sha256: build.sha256.clone(),
                size: build.size,
            },
        })
    });
    state.afk.set_client_release(release);
    state
        .afk
        .set_versions(snapshot.index.afk.as_ref().and_then(afk_versions));
}

/// The AFK version picker the index publishes with the client (`afk.minecraftVersions` and the
/// recommendation chosen in Studio). `None` keeps the built-in list: nothing published, or a list
/// breaking the content format's rules (Studio refuses those; clients ignore them).
fn afk_versions(afk: &pl_content::AfkRelease) -> Option<pl_afk::VersionCatalog> {
    let problems = afk.minecraft_version_problems();
    if !problems.is_empty() {
        tracing::warn!(
            ?problems,
            "ignoring the AFK version list of the content index"
        );
        return None;
    }
    if afk.minecraft_versions.is_empty() && afk.recommended_minecraft_version.is_none() {
        return None;
    }
    pl_afk::VersionCatalog::from_release(
        afk.minecraft_versions
            .iter()
            .map(|entry| (entry.value.clone(), entry.label.clone())),
        afk.recommended_minecraft_version.as_deref(),
    )
}

/// The Minecraft version the launcher recommends ("Önerilen" in the version lists, preselected for
/// new custom instances): the cached signed config's `recommendedMinecraftVersion`, else the
/// built-in `pl_content::DEFAULT_RECOMMENDED_MINECRAFT_VERSION`. Read from the content cache on
/// every call, so a refresh that brings a new config applies at once (`content-changed` makes the
/// UI ask again).
pub(crate) async fn recommended_minecraft(state: &AppState) -> String {
    match state.content.load_cached().await {
        Ok(snapshot) => snapshot.config.recommended_minecraft().to_owned(),
        Err(_) => pl_content::DEFAULT_RECOMMENDED_MINECRAFT_VERSION.to_owned(),
    }
}

async fn pack_notes(state: &AppState, snapshot: &ContentSnapshot) -> Option<PackNotesView> {
    let pack = crate::commands::official::official_pack_ref(snapshot)?;
    let manifest = state.content.pack_manifest(pack).await.ok()?;
    let changelog = manifest.projects.changelog.trim();
    (!changelog.is_empty()).then(|| PackNotesView {
        name: manifest.name.clone(),
        version: manifest.version_id.clone(),
        published_at: snapshot.index.generated_at.to_string(),
        changelog: changelog.to_owned(),
    })
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

async fn news_view(state: &AppState, entry: &HomeNewsItem) -> NewsView {
    let item = &entry.item;
    let from_feed = entry.origin == NewsOrigin::Feed;
    let cover_path = if from_feed {
        crate::commands::feed::cover_path(state, item.cover.as_ref()).await
    } else {
        media_path(state, item.cover.as_ref()).await
    };
    NewsView {
        id: item.id.clone(),
        title: item.title.clone(),
        summary: item.summary.clone(),
        body: item.body.clone(),
        cover_path,
        category: enum_str(&item.category),
        published_at: item.published_at.to_string(),
        pinned: item.pinned,
        featured: item.featured,
        cta_label: item.cta.as_ref().map(|c| c.label.clone()),
        cta_url: item.cta.as_ref().and_then(|c| c.url.clone()),
        from_feed,
        source_url: entry.source.as_ref().and_then(|s| s.url.clone()),
    }
}

async fn home_view(state: &AppState, snapshot: &ContentSnapshot) -> HomeView {
    let now = jiff::Timestamp::now();
    let hero = &snapshot.config.hero;
    // The bot's news feed joins Studio's news and events while the signed config enables it.
    let feed = crate::commands::feed::cached(state, &snapshot.config).await;
    let feed = feed.as_ref().map(|snapshot| &snapshot.feed);
    let max_age_days = snapshot
        .config
        .active_news_feed()
        .map_or(pl_content::FEED_DEFAULT_MAX_AGE_DAYS, |f| {
            f.effective_max_age_days()
        });
    let merged = pl_content::merge_news(&snapshot.news, feed, max_age_days, now);
    let mut news = Vec::new();
    for entry in &merged {
        news.push(news_view(state, entry).await);
    }
    let events = pl_content::merge_events(&snapshot.config.events, feed, &merged, now);
    let maintenance = Some(&snapshot.config.maintenance)
        .filter(|m| m.is_active(now))
        .map(|m| MaintenanceView {
            title: m.title.clone(),
            message: m.message.clone(),
            until: m.until.map(|t| t.to_string()),
        });
    let status =
        pl_content::home_status(feed, maintenance.is_some(), now).map(|status| StatusView {
            level: enum_str(&status.level),
            title: status.title.clone(),
            message: status.message.clone(),
            url: status.url.clone(),
            expires_at: status.expires_at.to_string(),
        });
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
                dismissible: b.dismissible,
                ends_at: b.ends_at.map(|t| t.to_string()),
            })
            .collect(),
        events: events
            .into_iter()
            .map(|e| EventView {
                id: e.id,
                title: e.title,
                starts_at: e.starts_at.to_string(),
                ends_at: e.ends_at.to_string(),
                open_ended: e.open_ended,
                url: e.url,
                news_id: e.news_id,
                priority: e.priority,
                from_feed: e.origin == NewsOrigin::Feed,
            })
            .collect(),
        patch_notes_id: pl_content::newest_patch_notes(&merged).map(|n| n.item.id.clone()),
        news,
        maintenance,
        status,
        pack_notes: pack_notes(state, snapshot).await,
        links: LinksView::from_config(&snapshot.config.links),
        offline: snapshot.offline,
        from_cache: snapshot.from_cache,
    }
}

/// Home content. Returns the cached snapshot immediately (refreshing in the background and
/// emitting `content-changed` when something new arrives); without a cache it waits for the
/// network. `None` means "nothing yet": the UI shows its bundled welcome content. Whether the
/// mirrors could be reached is [`content_status`] (and the `content-status` event).
#[tauri::command]
#[specta::specta]
pub async fn content_home(
    app: AppHandle,
    state: State<'_, AppState>,
) -> CmdResult<Option<HomeView>> {
    if let Ok(cached) = state.content.load_cached().await {
        apply_snapshot(&state, &cached);
        spawn_refresh(app, false);
        let mut view = home_view(&state, &cached).await;
        view.offline = OFFLINE.load(Ordering::SeqCst);
        return Ok(Some(view));
    }
    match state.content.refresh().await {
        Ok(snapshot) => {
            apply_snapshot(&state, &snapshot);
            report_reachability(&app, snapshot.from_cache && snapshot.offline);
            if !snapshot.offline {
                crate::commands::feed::spawn_refresh(app.clone(), snapshot.config.clone());
            }
            Ok(Some(home_view(&state, &snapshot).await))
        }
        Err(error) => {
            tracing::info!(%error, "no content yet");
            report_reachability(&app, error.is_offline());
            Ok(None)
        }
    }
}

/// Whether the last refresh reached a mirror.
#[tauri::command]
#[specta::specta]
pub fn content_status() -> ContentStatusView {
    ContentStatusView {
        offline: OFFLINE.load(Ordering::SeqCst),
    }
}

/// Checks the mirrors again right away (the UI calls it when the system reports the network gone
/// or back, so the offline strip does not wait for the next scheduled refresh). The outcome
/// arrives as `content-status` and, when there is something new, `content-changed`.
#[tauri::command]
#[specta::specta]
pub fn content_recheck(app: AppHandle) {
    spawn_refresh(app, true);
}

/// Background refreshes are at most this frequent (the home screen asks on every visit).
const MIN_REFRESH_INTERVAL: std::time::Duration = std::time::Duration::from_secs(5 * 60);

/// Refreshes in the background, at most every [`MIN_REFRESH_INTERVAL`] unless `force` (retries
/// while offline, the network going or coming back).
fn spawn_refresh(app: AppHandle, force: bool) {
    static LAST_REFRESH: std::sync::Mutex<Option<std::time::Instant>> = std::sync::Mutex::new(None);
    {
        let mut last = LAST_REFRESH.lock().unwrap_or_else(|e| e.into_inner());
        if !force && last.is_some_and(|at| at.elapsed() < MIN_REFRESH_INTERVAL) {
            return;
        }
        *last = Some(std::time::Instant::now());
    }
    let state = app.state::<AppState>();
    if state.content_refreshing.swap(true, Ordering::SeqCst) {
        return;
    }
    tauri::async_runtime::spawn(async move {
        let state = app.state::<AppState>();
        let offline = match state.content.refresh().await {
            Ok(snapshot) => {
                let fresh = !snapshot.from_cache
                    && snapshot.index.sequence != APPLIED_SEQUENCE.load(Ordering::SeqCst);
                if fresh {
                    apply_snapshot(&state, &snapshot);
                    let _ = state.content.prefetch_media(&snapshot).await;
                    let _ = app.emit("content-changed", ());
                }
                // The bot's news feed follows the content refresh: same cadence, never while
                // offline, only when the (possibly new) signed config enables it.
                if !snapshot.offline {
                    crate::commands::feed::spawn_refresh(app.clone(), snapshot.config.clone());
                }
                snapshot.from_cache && snapshot.offline
            }
            Err(error) => {
                tracing::info!(%error, "content refresh failed");
                error.is_offline()
            }
        };
        state.content_refreshing.store(false, Ordering::SeqCst);
        report_reachability(&app, offline);
    });
}

/// Records whether the mirrors were reachable, tells the UI when that changed and, while they are
/// not, checks again later (soon at first, then every minute).
fn report_reachability(app: &AppHandle, offline: bool) {
    if OFFLINE.swap(offline, Ordering::SeqCst) != offline {
        tracing::info!(offline, "content mirrors reachability changed");
        let _ = app.emit("content-status", ContentStatusView { offline });
    }
    if !offline {
        OFFLINE_STREAK.store(0, Ordering::SeqCst);
        return;
    }
    let streak = OFFLINE_STREAK.fetch_add(1, Ordering::SeqCst);
    if RETRY_PENDING.swap(true, Ordering::SeqCst) {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        tokio::time::sleep(offline_retry_delay(streak)).await;
        RETRY_PENDING.store(false, Ordering::SeqCst);
        if OFFLINE.load(Ordering::SeqCst) {
            spawn_refresh(app, true);
        }
    });
}
