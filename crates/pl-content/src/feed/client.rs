//! Fetching, verifying and caching the bot's news feed.
//!
//! The flow mirrors [`ContentClient`](crate::ContentClient): [`FeedClient::load_cached`] gives the
//! last verified feed without the network, [`FeedClient::refresh`] asks each feed mirror in order
//! for `feed.json` (conditional on the cached ETag), verifies `feed.json.sig` against the keys
//! Studio delegated in the signed launcher config, refuses older sequences and commits the new
//! feed. Nothing here runs while the config disables the feed: callers build [`FeedSettings`]
//! only from an enabled [`NewsFeedConfig`].
//!
//! Cache layout (`cache_dir` is `<data>/cache/feed/`):
//! ```text
//! current.json            exact feed.json text + signature, ETags per mirror, fetch time, GC hints
//! media/<sha256>.<ext>    covers (verified by size and SHA-256 on download, then trusted by size)
//! ```
//! The cached feed is verified again on every read against the *current* keys, so a key Studio
//! removes also takes away what it signed, including its sequence floor.

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures::StreamExt;
use jiff::Timestamp;
use pl_net::{Http, NetError};
use reqwest::StatusCode;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use serde::{Deserialize, Serialize};
use url::Url;

use super::merge::feed_age_floor;
use crate::client::{ensure_success, parse_json, read_limited, resource_url, sweep};
use crate::error::{Error, Result};
use crate::fsutil::{normalize_hex, read_optional, sha256_hex, write_atomic};
use crate::model::{
    BotFeed, FEED_MAX_BYTES, FeedProblem, MediaRef, NewsFeedConfig, is_valid_feed_cover,
};
use crate::signing::{SignatureFile, VerifyingKey, key_id, verify_index};

const FEED_NAME: &str = "feed.json";
const SIGNATURE_NAME: &str = "feed.json.sig";
const STATE_NAME: &str = "current.json";
const MEDIA_DIR: &str = "media";
const MAX_SIGNATURE_BYTES: u64 = 16 << 10;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_MEDIA_TIMEOUT: Duration = Duration::from_secs(30);

/// Where to read the feed and whom to trust, from an enabled [`NewsFeedConfig`]. The fields are
/// public so a debug build can point them at a local mirror and a throwaway key.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedSettings {
    /// Mirror bases (ending in `/`), tried in order.
    pub mirrors: Vec<Url>,
    /// Keys allowed to sign `feed.json`.
    pub keys: Vec<VerifyingKey>,
    /// Feed items published longer ago than this are hidden.
    pub max_age_days: u32,
}

impl FeedSettings {
    /// The usable part of `config` (valid mirrors and keys, `maxAgeDays` in range); `None` while
    /// the feed is disabled.
    pub fn from_config(config: &NewsFeedConfig) -> Option<Self> {
        config.enabled.then(|| Self {
            mirrors: config.mirror_bases(),
            keys: config.trusted_keys(),
            max_age_days: config.effective_max_age_days(),
        })
    }

    /// At least one mirror to ask and one key to trust.
    pub fn is_usable(&self) -> bool {
        !self.mirrors.is_empty() && !self.keys.is_empty()
    }
}

/// A verified feed, ready for [`merge_news`](super::merge_news) and
/// [`merge_events`](super::merge_events).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FeedSnapshot {
    /// The feed with broken items dropped or repaired.
    pub feed: BotFeed,
    /// What was dropped or repaired (logs, Studio's preview).
    pub problems: Vec<FeedProblem>,
    /// Id of the key that signed it.
    pub key_id: String,
    /// Mirror base the feed was (last) fetched from; covers are tried there first.
    pub mirror_used: String,
    pub fetched_at: Timestamp,
    /// The refresh failed and this is the cached feed.
    pub from_cache: bool,
    /// No mirror could be reached.
    pub offline: bool,
    /// Technical reason the refresh failed (logs and "copy details" only).
    pub problem: Option<String>,
}

/// Result of [`FeedClient::prefetch_media`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct FeedMediaReport {
    /// Covers of the items visible now.
    pub requested: usize,
    /// Covers downloaded by this call (the others were already cached).
    pub downloaded: usize,
    pub failed: usize,
}

/// One mirror's answer in [`FeedClient::probe`].
#[derive(Debug)]
pub struct MirrorProbe {
    pub mirror: String,
    pub result: Result<FeedSnapshot>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheState {
    /// Exact text of the verified `feed.json`.
    feed: String,
    signature: SignatureFile,
    mirror_used: String,
    fetched_at: Timestamp,
    /// Mirror base → ETag under which that mirror served exactly `feed`.
    #[serde(default)]
    etags: BTreeMap<String, String>,
    /// Covers of the feed this one replaced; garbage collection keeps them.
    #[serde(default)]
    previous_media: BTreeSet<String>,
}

/// The verified local cache.
struct Cached {
    state: CacheState,
    feed: BotFeed,
    problems: Vec<FeedProblem>,
    key_id: String,
}

impl Cached {
    fn snapshot(&self, from_cache: bool, offline: bool, problem: Option<String>) -> FeedSnapshot {
        FeedSnapshot {
            feed: self.feed.clone(),
            problems: self.problems.clone(),
            key_id: self.key_id.clone(),
            mirror_used: self.state.mirror_used.clone(),
            fetched_at: self.state.fetched_at,
            from_cache,
            offline,
            problem,
        }
    }
}

enum Fetched {
    NotModified,
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
}

enum Attempt {
    /// The mirror serves exactly the cached feed.
    Unchanged { etag: Option<String> },
    /// A verified feed with a higher sequence.
    Newer(Box<Verified>),
}

struct Verified {
    text: String,
    signature: SignatureFile,
    feed: BotFeed,
    problems: Vec<FeedProblem>,
    key_id: String,
    etag: Option<String>,
}

/// Client for the bot's feed mirrors. Cheap to share behind an `Arc`.
#[derive(Debug)]
pub struct FeedClient {
    http: Http,
    cache_dir: PathBuf,
    request_timeout: Duration,
    refresh_lock: tokio::sync::Mutex<()>,
    media_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl FeedClient {
    pub fn new(http: Http, cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            http,
            cache_dir: cache_dir.into(),
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            refresh_lock: tokio::sync::Mutex::new(()),
            media_locks: Mutex::new(HashMap::new()),
        }
    }

    /// Per-request timeout for `feed.json` and its signature (covers get at least 30 s).
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// The cached feed when it still verifies against `settings.keys`, without the network.
    pub async fn load_cached(&self, settings: &FeedSettings) -> Option<FeedSnapshot> {
        self.load_verified(settings)
            .await
            .map(|cached| cached.snapshot(true, false, None))
    }

    /// Fetches and verifies the newest feed. Falls back to the cache on any failure; errors only
    /// when nothing usable is cached.
    pub async fn refresh(&self, settings: &FeedSettings) -> Result<FeedSnapshot> {
        let _guard = self.refresh_lock.lock().await;
        let cached = self.load_verified(settings).await;

        let mut problems = Vec::new();
        let mut all_offline = true;
        let mut content_error = None;
        for base in &settings.mirrors {
            match self.refresh_from(base, settings, cached.as_ref()).await {
                Ok(snapshot) => return Ok(snapshot),
                Err(error) => {
                    tracing::warn!(mirror = %base, %error, "news feed mirror failed");
                    problems.push(format!("{base}: {error}"));
                    if !error.is_offline() {
                        all_offline = false;
                        content_error.get_or_insert(error);
                    }
                }
            }
        }

        let problem = problems.join("; ");
        if let Some(cached) = cached {
            return Ok(cached.snapshot(true, all_offline, Some(problem)));
        }
        Err(match content_error {
            Some(error) => error,
            None => Error::Offline(problem),
        })
    }

    /// Asks every mirror once, without the cache or its rollback floor (Studio's preview).
    pub async fn probe(&self, settings: &FeedSettings) -> Vec<MirrorProbe> {
        let mut probes = Vec::new();
        for base in &settings.mirrors {
            let result = self
                .fetch_verified(base, settings)
                .await
                .map(|verified| FeedSnapshot {
                    feed: verified.feed,
                    problems: verified.problems,
                    key_id: verified.key_id,
                    mirror_used: base.to_string(),
                    fetched_at: Timestamp::now(),
                    from_cache: false,
                    offline: false,
                    problem: None,
                });
            probes.push(MirrorProbe {
                mirror: base.to_string(),
                result,
            });
        }
        probes
    }

    /// The cached copy of a cover (verified when it was downloaded, trusted by size now).
    pub async fn cached_media(&self, media: &MediaRef) -> Option<PathBuf> {
        if !is_valid_feed_cover(media) {
            return None;
        }
        let dest = self.media_path(media);
        (file_size(&dest).await == Some(media.size)).then_some(dest)
    }

    /// Local path of a cover, downloading it on first use from the feed mirrors (`preferred`
    /// first when it is one of them). The bytes are read up to the declared size and written
    /// only when the size and the SHA-256 match.
    pub async fn media(
        &self,
        settings: &FeedSettings,
        preferred: Option<&str>,
        media: &MediaRef,
    ) -> Result<PathBuf> {
        media.validate()?;
        if !is_valid_feed_cover(media) {
            return Err(Error::Invalid(format!(
                "{} is not a feed cover (an image of at most 2 MB)",
                media.path
            )));
        }
        let sha256 = normalize_hex(&media.sha256, 64)
            .ok_or_else(|| Error::Invalid(format!("bad sha256 for {}", media.path)))?;
        let dest = self.media_path(media);
        let lock = self.media_lock(&sha256);
        let _guard = lock.lock().await;
        if file_size(&dest).await == Some(media.size) {
            return Ok(dest);
        }

        let timeout = self.request_timeout.max(MIN_MEDIA_TIMEOUT);
        let preferred = preferred.and_then(|p| settings.mirrors.iter().find(|m| m.as_str() == p));
        let ordered = preferred
            .into_iter()
            .chain(settings.mirrors.iter().filter(|m| Some(*m) != preferred));
        let mut content_error = None;
        let mut network_error = None;
        for base in ordered {
            let url = base
                .join(&media.path)
                .map_err(|e| Error::Invalid(format!("bad media URL: {e}")))?;
            match self.get_limited(&url, media.size, timeout).await {
                Ok(bytes) => {
                    let actual = sha256_hex(&bytes);
                    if bytes.len() as u64 == media.size && actual == sha256 {
                        write_atomic(&dest, &bytes).await?;
                        return Ok(dest);
                    }
                    content_error.get_or_insert(Error::Integrity {
                        what: url.to_string(),
                        expected: sha256.clone(),
                        actual,
                    });
                }
                Err(error) if error.is_offline() => network_error = Some(error),
                Err(error) => {
                    content_error.get_or_insert(error);
                }
            }
        }
        Err(content_error
            .or(network_error)
            .unwrap_or_else(|| Error::Offline(media.path.clone())))
    }

    /// Downloads the covers of the items and Minecraft entries visible at `now` that are not
    /// cached yet, so the home screen never waits for them. Failures are counted, not returned.
    pub async fn prefetch_media(
        &self,
        settings: &FeedSettings,
        snapshot: &FeedSnapshot,
        now: Timestamp,
    ) -> FeedMediaReport {
        let floor = feed_age_floor(now, settings.max_age_days);
        let mut seen = HashSet::new();
        let items = snapshot
            .feed
            .items
            .iter()
            .filter(|item| item.is_visible(now) && item.published_at >= floor)
            .filter_map(|item| item.cover.as_ref());
        let minecraft = snapshot
            .feed
            .minecraft
            .iter()
            .filter(|entry| entry.is_visible(now) && entry.published_at >= floor)
            .filter_map(|entry| entry.cover.as_ref());
        let wanted: Vec<&MediaRef> = items
            .chain(minecraft)
            .filter(|media| seen.insert(media.sha256.trim().to_ascii_lowercase()))
            .collect();
        let mut missing = Vec::new();
        for media in &wanted {
            if self.cached_media(media).await.is_none() {
                missing.push(*media);
            }
        }
        // Build the futures eagerly so the returned future stays `Send` (see ContentClient).
        let downloads: Vec<_> = missing
            .iter()
            .map(|media| self.media(settings, Some(&snapshot.mirror_used), media))
            .collect();
        let results: Vec<Result<PathBuf>> = futures::stream::iter(downloads)
            .buffer_unordered(4)
            .collect()
            .await;
        let failed = results.iter().filter(|result| result.is_err()).count();
        FeedMediaReport {
            requested: wanted.len(),
            downloaded: results.len() - failed,
            failed,
        }
    }

    /// True when a feed was ever cached here.
    pub fn has_cache(&self) -> bool {
        self.state_path().is_file()
    }

    /// Deletes the cache (the config turned the feed off).
    pub async fn clear(&self) -> Result<()> {
        let _guard = self.refresh_lock.lock().await;
        match tokio::fs::remove_dir_all(&self.cache_dir).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(Error::io(&self.cache_dir, e)),
        }
    }

    async fn refresh_from(
        &self,
        base: &Url,
        settings: &FeedSettings,
        cached: Option<&Cached>,
    ) -> Result<FeedSnapshot> {
        let etag = cached
            .and_then(|c| c.state.etags.get(base.as_str()))
            .map(String::as_str);
        let attempt = match self.fetch_feed(base, etag, None).await? {
            Fetched::NotModified => match cached {
                Some(_) => Attempt::Unchanged {
                    etag: etag.map(str::to_owned),
                },
                None => return Err(Error::Invalid("unexpected 304 for feed.json".into())),
            },
            Fetched::Body { bytes, etag } => {
                match self
                    .classify(base, bytes, etag, settings, cached, None)
                    .await
                {
                    Err(Error::BadSignature) => {
                        // Right after a publish an edge cache can pair a new feed with the old
                        // signature (or the reverse); retry once past the cache.
                        let token = Timestamp::now().as_millisecond().to_string();
                        match self.fetch_feed(base, None, Some(&token)).await? {
                            Fetched::Body { bytes, etag } => {
                                self.classify(base, bytes, etag, settings, cached, Some(&token))
                                    .await?
                            }
                            Fetched::NotModified => return Err(Error::BadSignature),
                        }
                    }
                    other => other?,
                }
            }
        };

        match attempt {
            Attempt::Unchanged { etag } => {
                let cached = cached.ok_or(Error::NoCache)?;
                let mut state = cached.state.clone();
                state.fetched_at = Timestamp::now();
                state.mirror_used = base.to_string();
                if let Some(etag) = etag {
                    state.etags.insert(base.to_string(), etag);
                }
                self.save_state(&state).await;
                Ok(FeedSnapshot {
                    mirror_used: state.mirror_used,
                    fetched_at: state.fetched_at,
                    ..cached.snapshot(false, false, None)
                })
            }
            Attempt::Newer(verified) => {
                let Verified {
                    text,
                    signature,
                    feed,
                    problems,
                    key_id,
                    etag,
                } = *verified;
                let state = CacheState {
                    feed: text,
                    signature,
                    mirror_used: base.to_string(),
                    fetched_at: Timestamp::now(),
                    etags: etag.map(|e| (base.to_string(), e)).into_iter().collect(),
                    previous_media: cached.map(|c| media_names(&c.feed)).unwrap_or_default(),
                };
                self.save_state(&state).await;
                let mut keep = media_names(&feed);
                keep.extend(state.previous_media.iter().cloned());
                if let Err(error) = sweep(&self.cache_dir.join(MEDIA_DIR), &keep).await {
                    tracing::warn!(%error, "news feed cache cleanup failed");
                }
                if !problems.is_empty() {
                    tracing::warn!(?problems, "news feed entries dropped or repaired");
                }
                tracing::info!(sequence = feed.sequence, mirror = %base, "news feed updated");
                Ok(FeedSnapshot {
                    feed,
                    problems,
                    key_id,
                    mirror_used: state.mirror_used,
                    fetched_at: state.fetched_at,
                    from_cache: false,
                    offline: false,
                    problem: None,
                })
            }
        }
    }

    /// Verifies a fetched feed body and decides whether it is new.
    async fn classify(
        &self,
        base: &Url,
        bytes: Vec<u8>,
        etag: Option<String>,
        settings: &FeedSettings,
        cached: Option<&Cached>,
        cache_buster: Option<&str>,
    ) -> Result<Attempt> {
        if let Some(cached) = cached
            && cached.state.feed.as_bytes() == bytes.as_slice()
        {
            return Ok(Attempt::Unchanged { etag });
        }
        let verified = self
            .verify(base, bytes, etag, settings, cache_buster)
            .await?;
        if let Some(cached) = cached {
            let floor = cached.feed.sequence;
            if verified.feed.sequence < floor {
                return Err(Error::Rollback {
                    received: verified.feed.sequence,
                    cached: floor,
                });
            }
            if verified.feed.sequence == floor {
                return Err(Error::Invalid(format!(
                    "feed sequence {floor} was reused for different content"
                )));
            }
        }
        Ok(Attempt::Newer(Box::new(verified)))
    }

    /// One verified fetch from `base` (with the cache-busting retry), ignoring the cache.
    async fn fetch_verified(&self, base: &Url, settings: &FeedSettings) -> Result<Verified> {
        let (bytes, etag) = match self.fetch_feed(base, None, None).await? {
            Fetched::Body { bytes, etag } => (bytes, etag),
            Fetched::NotModified => {
                return Err(Error::Invalid("unexpected 304 for feed.json".into()));
            }
        };
        match self.verify(base, bytes, etag, settings, None).await {
            Err(Error::BadSignature) => {
                let token = Timestamp::now().as_millisecond().to_string();
                match self.fetch_feed(base, None, Some(&token)).await? {
                    Fetched::Body { bytes, etag } => {
                        self.verify(base, bytes, etag, settings, Some(&token)).await
                    }
                    Fetched::NotModified => Err(Error::BadSignature),
                }
            }
            other => other,
        }
    }

    /// Signature (strict Ed25519, a delegated key), then the feed rules.
    async fn verify(
        &self,
        base: &Url,
        bytes: Vec<u8>,
        etag: Option<String>,
        settings: &FeedSettings,
        cache_buster: Option<&str>,
    ) -> Result<Verified> {
        let signature = self.fetch_signature(base, cache_buster).await?;
        let key = verify_index(&bytes, &signature, &settings.keys)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| Error::Invalid("feed.json is not valid UTF-8".into()))?;
        let (feed, problems) = BotFeed::parse(text.as_bytes())?;
        Ok(Verified {
            text,
            signature,
            feed,
            problems,
            key_id: key_id(&key),
            etag,
        })
    }

    async fn fetch_feed(
        &self,
        base: &Url,
        etag: Option<&str>,
        cache_buster: Option<&str>,
    ) -> Result<Fetched> {
        let url = resource_url(base, FEED_NAME, cache_buster)?;
        let mut request = self
            .http
            .client()
            .get(url.as_str())
            .timeout(self.request_timeout);
        if let Some(etag) = etag {
            request = request.header(IF_NONE_MATCH, etag);
        }
        let response = request.send().await.map_err(|source| NetError::Request {
            url: url.to_string(),
            source,
        })?;
        if response.status() == StatusCode::NOT_MODIFIED {
            return Ok(Fetched::NotModified);
        }
        let response = ensure_success(response, &url)?;
        let etag = response
            .headers()
            .get(ETAG)
            .and_then(|value| value.to_str().ok())
            .map(str::to_owned);
        let bytes = read_limited(response, FEED_MAX_BYTES, &url).await?;
        Ok(Fetched::Body { bytes, etag })
    }

    async fn fetch_signature(
        &self,
        base: &Url,
        cache_buster: Option<&str>,
    ) -> Result<SignatureFile> {
        let url = resource_url(base, SIGNATURE_NAME, cache_buster)?;
        let bytes = self
            .get_limited(&url, MAX_SIGNATURE_BYTES, self.request_timeout)
            .await?;
        parse_json(&bytes, SIGNATURE_NAME)
    }

    async fn get_limited(&self, url: &Url, limit: u64, timeout: Duration) -> Result<Vec<u8>> {
        let response = self
            .http
            .client()
            .get(url.as_str())
            .timeout(timeout)
            .send()
            .await
            .map_err(|source| NetError::Request {
                url: url.to_string(),
                source,
            })?;
        let response = ensure_success(response, url)?;
        read_limited(response, limit, url).await
    }

    async fn load_verified(&self, settings: &FeedSettings) -> Option<Cached> {
        match self.try_load(settings).await {
            Ok(cached) => cached,
            Err(error) => {
                tracing::debug!(%error, "ignoring the news feed cache");
                None
            }
        }
    }

    async fn try_load(&self, settings: &FeedSettings) -> Result<Option<Cached>> {
        let Some(bytes) = read_optional(&self.state_path()).await? else {
            return Ok(None);
        };
        let state: CacheState = parse_json(&bytes, STATE_NAME)?;
        let key = verify_index(state.feed.as_bytes(), &state.signature, &settings.keys)?;
        let (feed, problems) = BotFeed::parse(state.feed.as_bytes())?;
        Ok(Some(Cached {
            state,
            feed,
            problems,
            key_id: key_id(&key),
        }))
    }

    async fn save_state(&self, state: &CacheState) {
        let result = match serde_json::to_vec_pretty(state) {
            Ok(bytes) => write_atomic(&self.state_path(), &bytes).await,
            Err(e) => Err(Error::json(STATE_NAME, e)),
        };
        if let Err(error) = result {
            tracing::warn!(%error, "could not persist the news feed cache");
        }
    }

    fn media_lock(&self, sha256: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self
            .media_locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry(sha256.to_owned()).or_default())
    }

    fn media_path(&self, media: &MediaRef) -> PathBuf {
        self.cache_dir.join(MEDIA_DIR).join(media.cache_file_name())
    }

    fn state_path(&self) -> PathBuf {
        self.cache_dir.join(STATE_NAME)
    }
}

/// Cache names (`<sha256>`) of a feed's covers.
fn media_names(feed: &BotFeed) -> BTreeSet<String> {
    feed.media_refs()
        .into_iter()
        .map(|media| media.sha256.trim().to_ascii_lowercase())
        .collect()
}

async fn file_size(path: &Path) -> Option<u64> {
    tokio::fs::metadata(path)
        .await
        .ok()
        .filter(std::fs::Metadata::is_file)
        .map(|meta| meta.len())
}

#[cfg(test)]
mod tests {
    use serde_json::{Value, json};

    use super::*;
    use crate::json::to_canonical_json;
    use crate::signing::{SigningKey, sign_index};
    use crate::testutil::{TestServer, dead_base, test_http};

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn settings(bases: &[String], keys: &[&SigningKey]) -> FeedSettings {
        FeedSettings {
            mirrors: bases.iter().map(|b| Url::parse(b).unwrap()).collect(),
            keys: keys.iter().map(|k| k.verifying_key()).collect(),
            max_age_days: 60,
        }
    }

    fn client(dir: &Path) -> FeedClient {
        FeedClient::new(test_http(), dir).with_request_timeout(Duration::from_secs(5))
    }

    fn item(id: &str, cover: Option<&MediaRef>) -> Value {
        let mut item = json!({
            "id": id,
            "title": format!("Haber {id}"),
            "summary": "Özet",
            "body": "- Madde",
            "category": "announcement",
            "publishedAt": Timestamp::now().to_string(),
            "source": { "kind": "discord", "channel": "duyurular", "url": "https://discord.com/channels/1/2/3" }
        });
        if let Some(cover) = cover {
            item["cover"] = serde_json::to_value(cover).unwrap();
        }
        item
    }

    fn feed_bytes(sequence: u64, items: Vec<Value>) -> Vec<u8> {
        to_canonical_json(&json!({
            "schema": 1,
            "sequence": sequence,
            "generatedAt": "2026-10-16T09:00:00Z",
            "items": items,
            "events": []
        }))
        .unwrap()
    }

    /// Serves `bytes` as `feed.json` with a signature by `signer`.
    fn publish(server: &TestServer, signer: &SigningKey, bytes: &[u8]) {
        let signature = to_canonical_json(&sign_index(bytes, signer)).unwrap();
        server.put("/v1/feed.json.sig", signature);
        server.put("/v1/feed.json", bytes.to_vec());
    }

    fn cover(bytes: &[u8]) -> MediaRef {
        crate::studio::media_ref(bytes, "webp", crate::model::MediaKind::Image)
    }

    #[tokio::test]
    async fn refresh_verifies_caches_and_uses_etags() {
        let server = TestServer::start().await;
        let signer = key(1);
        publish(
            &server,
            &signer,
            &feed_bytes(10, vec![item("discord-1", None)]),
        );
        let dir = tempfile::tempdir().unwrap();
        let feeds = client(dir.path());
        let settings = settings(&[server.base()], &[&signer]);

        assert!(feeds.load_cached(&settings).await.is_none());
        let snapshot = feeds.refresh(&settings).await.unwrap();
        assert!(!snapshot.from_cache && !snapshot.offline && snapshot.problem.is_none());
        assert_eq!(snapshot.feed.sequence, 10);
        assert_eq!(snapshot.key_id, key_id(&signer.verifying_key()));
        assert_eq!(snapshot.mirror_used, server.base());
        assert!(feeds.has_cache());

        // Unchanged: a conditional request, no signature download.
        let again = feeds.refresh(&settings).await.unwrap();
        assert!(!again.from_cache);
        assert_eq!(server.hits("/v1/feed.json"), 2);
        assert_eq!(server.hits("/v1/feed.json.sig"), 1);

        // Next launch: the cache verifies without the network.
        let cached = client(dir.path()).load_cached(&settings).await.unwrap();
        assert!(cached.from_cache);
        assert_eq!(cached.feed, snapshot.feed);
    }

    #[tokio::test]
    async fn a_newer_feed_replaces_the_cache_and_removed_items_disappear() {
        let server = TestServer::start().await;
        let signer = key(1);
        publish(
            &server,
            &signer,
            &feed_bytes(10, vec![item("discord-1", None), item("discord-2", None)]),
        );
        let dir = tempfile::tempdir().unwrap();
        let feeds = client(dir.path());
        let settings = settings(&[server.base()], &[&signer]);
        assert_eq!(feeds.refresh(&settings).await.unwrap().feed.items.len(), 2);

        publish(
            &server,
            &signer,
            &feed_bytes(11, vec![item("discord-2", None)]),
        );
        let snapshot = feeds.refresh(&settings).await.unwrap();
        assert_eq!(snapshot.feed.sequence, 11);
        assert!(snapshot.feed.item("discord-1").is_none());
        assert_eq!(
            feeds.load_cached(&settings).await.unwrap().feed.items.len(),
            1
        );
    }

    #[tokio::test]
    async fn tampering_rollback_and_wrong_keys_are_refused() {
        let server = TestServer::start().await;
        let signer = key(1);
        let dir = tempfile::tempdir().unwrap();
        let feeds = client(dir.path());
        let settings = settings(&[server.base()], &[&signer]);
        publish(
            &server,
            &signer,
            &feed_bytes(10, vec![item("discord-1", None)]),
        );
        feeds.refresh(&settings).await.unwrap();

        // Tampered: the bytes changed after signing. Retried once past the edge cache, then
        // the cached feed stays.
        let signed = feed_bytes(12, vec![item("discord-9", None)]);
        publish(&server, &signer, &signed);
        let tampered = String::from_utf8(signed)
            .unwrap()
            .replace("Haber discord-9", "Sahte haber")
            .into_bytes();
        server.put("/v1/feed.json", tampered);
        let hits = server.hits("/v1/feed.json");
        let snapshot = feeds.refresh(&settings).await.unwrap();
        assert!(snapshot.from_cache && !snapshot.offline);
        assert_eq!(snapshot.feed.sequence, 10);
        assert_eq!(server.hits("/v1/feed.json"), hits + 2);
        assert!(snapshot.problem.unwrap().contains("signature"));

        // Rollback: a lower sequence, or the same one with other bytes.
        publish(
            &server,
            &signer,
            &feed_bytes(9, vec![item("discord-0", None)]),
        );
        let snapshot = feeds.refresh(&settings).await.unwrap();
        assert!(snapshot.from_cache);
        assert!(snapshot.problem.unwrap().contains("older"));
        publish(
            &server,
            &signer,
            &feed_bytes(10, vec![item("discord-x", None)]),
        );
        assert!(feeds.refresh(&settings).await.unwrap().from_cache);

        // A key Studio did not delegate.
        publish(
            &server,
            &key(2),
            &feed_bytes(20, vec![item("discord-2", None)]),
        );
        let snapshot = feeds.refresh(&settings).await.unwrap();
        assert!(snapshot.from_cache);
        assert!(snapshot.problem.unwrap().contains("untrusted"));
        let empty = tempfile::tempdir().unwrap();
        let error = client(empty.path()).refresh(&settings).await.unwrap_err();
        assert!(matches!(error, Error::UntrustedKey { .. }), "{error}");
        assert!(!error.is_offline());
    }

    #[tokio::test]
    async fn revoking_a_key_drops_its_cache_and_floor() {
        let server = TestServer::start().await;
        let (old, new) = (key(1), key(2));
        let dir = tempfile::tempdir().unwrap();
        let feeds = client(dir.path());
        publish(
            &server,
            &old,
            &feed_bytes(1_000, vec![item("discord-1", None)]),
        );
        feeds
            .refresh(&settings(&[server.base()], &[&old]))
            .await
            .unwrap();

        // Studio replaced the key: the old cache no longer counts, a lower sequence is fine.
        let rotated = settings(&[server.base()], &[&new]);
        assert!(feeds.load_cached(&rotated).await.is_none());
        publish(&server, &new, &feed_bytes(5, vec![item("discord-2", None)]));
        let snapshot = feeds.refresh(&rotated).await.unwrap();
        assert_eq!(snapshot.feed.sequence, 5);
        assert!(!snapshot.from_cache);
    }

    #[tokio::test]
    async fn oversize_and_broken_feeds_are_refused() {
        let server = TestServer::start().await;
        let signer = key(1);
        let settings = settings(&[server.base()], &[&signer]);
        let dir = tempfile::tempdir().unwrap();
        let feeds = client(dir.path());

        let mut huge = feed_bytes(1, vec![item("discord-1", None)]);
        huge.truncate(huge.len() - 2);
        huge.resize(FEED_MAX_BYTES as usize + 10, b' ');
        huge.extend_from_slice(b"}\n");
        publish(&server, &signer, &huge);
        let error = feeds.refresh(&settings).await.unwrap_err();
        assert!(error.to_string().contains("larger"), "{error}");
        assert_eq!(server.hits("/v1/feed.json.sig"), 0, "not even verified");

        let many: Vec<Value> = (0..51)
            .map(|i| item(&format!("discord-{i}"), None))
            .collect();
        publish(&server, &signer, &feed_bytes(2, many));
        assert!(feeds.refresh(&settings).await.is_err());

        publish(
            &server,
            &signer,
            b"{\"schema\": 2, \"sequence\": 3, \"generatedAt\": \"2026-10-16T09:00:00Z\"}",
        );
        assert!(matches!(
            feeds.refresh(&settings).await,
            Err(Error::UnsupportedSchema { .. })
        ));
        assert!(!feeds.has_cache());
    }

    #[tokio::test]
    async fn mirrors_fall_back_and_offline_keeps_the_cache() {
        let good = TestServer::start().await;
        let signer = key(1);
        publish(
            &good,
            &signer,
            &feed_bytes(3, vec![item("discord-1", None)]),
        );
        let dir = tempfile::tempdir().unwrap();
        let feeds = client(dir.path());
        let missing = TestServer::start().await;

        let both = settings(
            &[dead_base().await, missing.base(), good.base()],
            &[&signer],
        );
        let snapshot = feeds.refresh(&both).await.unwrap();
        assert_eq!(snapshot.mirror_used, good.base());

        let offline = settings(&[dead_base().await], &[&signer]);
        let snapshot = feeds.refresh(&offline).await.unwrap();
        assert!(snapshot.from_cache && snapshot.offline);
        assert_eq!(snapshot.feed.sequence, 3);

        let empty = tempfile::tempdir().unwrap();
        let error = client(empty.path()).refresh(&offline).await.unwrap_err();
        assert!(error.is_offline(), "{error}");

        // The preview asks every mirror and reports each.
        let probes = feeds.probe(&both).await;
        assert_eq!(probes.len(), 3);
        assert!(probes[0].result.as_ref().unwrap_err().is_offline());
        assert!(matches!(
            probes[1].result.as_ref().unwrap_err(),
            Error::Net(NetError::Status { status: 404, .. })
        ));
        assert_eq!(probes[2].result.as_ref().unwrap().feed.sequence, 3);
    }

    #[tokio::test]
    async fn covers_are_verified_cached_and_collected() {
        let server = TestServer::start().await;
        let signer = key(1);
        let first_bytes = b"RIFF....WEBPVP8 first cover".to_vec();
        let first = cover(&first_bytes);
        server.put(&format!("/v1/{}", first.path), first_bytes.clone());
        publish(
            &server,
            &signer,
            &feed_bytes(1, vec![item("discord-1", Some(&first))]),
        );
        let dir = tempfile::tempdir().unwrap();
        let feeds = client(dir.path());
        let settings = settings(&[server.base()], &[&signer]);
        let snapshot = feeds.refresh(&settings).await.unwrap();

        assert!(feeds.cached_media(&first).await.is_none());
        let report = feeds
            .prefetch_media(&settings, &snapshot, Timestamp::now())
            .await;
        assert_eq!(
            report,
            FeedMediaReport {
                requested: 1,
                downloaded: 1,
                failed: 0
            }
        );
        let path = feeds.cached_media(&first).await.unwrap();
        assert_eq!(std::fs::read(&path).unwrap(), first_bytes);
        let again = feeds
            .prefetch_media(&settings, &snapshot, Timestamp::now())
            .await;
        assert_eq!(again.downloaded, 0);
        assert_eq!(server.hits(&format!("/v1/{}", first.path)), 1);

        // Wrong bytes, a bigger file than declared, a video or an unsafe path: never cached.
        let bogus = cover(b"expected bytes");
        server.put(&format!("/v1/{}", bogus.path), b"tampered bytes".to_vec());
        assert!(matches!(
            feeds.media(&settings, None, &bogus).await,
            Err(Error::Integrity { .. })
        ));
        let short = cover(b"short");
        server.put(&format!("/v1/{}", short.path), b"short but longer".to_vec());
        assert!(feeds.media(&settings, None, &short).await.is_err());
        assert!(feeds.cached_media(&bogus).await.is_none());
        let mut video = first.clone();
        video.kind = crate::model::MediaKind::Video;
        assert!(feeds.media(&settings, None, &video).await.is_err());
        let mut escape = first.clone();
        escape.path = "../../escape.webp".into();
        assert!(matches!(
            feeds.media(&settings, None, &escape).await,
            Err(Error::UnsafePath { .. })
        ));

        // Two newer feeds: the first cover is kept one generation, then collected.
        let second_bytes = b"second cover".to_vec();
        let second = cover(&second_bytes);
        server.put(&format!("/v1/{}", second.path), second_bytes);
        publish(
            &server,
            &signer,
            &feed_bytes(2, vec![item("discord-2", Some(&second))]),
        );
        let snapshot = feeds.refresh(&settings).await.unwrap();
        feeds
            .prefetch_media(&settings, &snapshot, Timestamp::now())
            .await;
        assert!(feeds.cached_media(&first).await.is_some());
        publish(&server, &signer, &feed_bytes(3, Vec::new()));
        feeds.refresh(&settings).await.unwrap();
        assert!(feeds.cached_media(&first).await.is_none());
        assert!(feeds.cached_media(&second).await.is_some());

        feeds.clear().await.unwrap();
        assert!(!feeds.has_cache());
        assert!(feeds.load_cached(&settings).await.is_none());
        feeds.clear().await.unwrap();
    }

    #[tokio::test]
    async fn minecraft_covers_are_prefetched_and_collected_like_item_covers() {
        let server = TestServer::start().await;
        let signer = key(1);
        let now = Timestamp::now();
        let days = |n: i64| {
            now.checked_add(jiff::SignedDuration::from_hours(24 * n))
                .unwrap()
        };
        let shown_bytes = b"RIFF....WEBPVP8 wilderness".to_vec();
        let shown = cover(&shown_bytes);
        server.put(&format!("/v1/{}", shown.path), shown_bytes);
        let gone_bytes = b"RIFF....WEBPVP8 old drop".to_vec();
        let gone = cover(&gone_bytes);
        server.put(&format!("/v1/{}", gone.path), gone_bytes);
        let entry = |version: &str, published: Timestamp, expires: Timestamp, cover: &MediaRef| {
            json!({
                "id": format!("minecraft-{version}"),
                "version": version,
                "kind": "release",
                "title": format!("Minecraft {version} çıktı"),
                "cover": cover,
                "publishedAt": published.to_string(),
                "expiresAt": expires.to_string(),
                "source": { "kind": "minecraft", "url": "https://www.minecraft.net/article/x" }
            })
        };
        let with_minecraft = |sequence: u64, minecraft: Vec<Value>| {
            to_canonical_json(&json!({
                "schema": 1,
                "sequence": sequence,
                "generatedAt": "2026-10-16T09:00:00Z",
                "items": [item("discord-1", None)],
                "minecraft": minecraft
            }))
            .unwrap()
        };
        let entries = vec![
            entry("26.3", days(-2), days(40), &shown),
            // Expired yesterday: its cover is never fetched.
            entry("26.2", days(-50), days(-1), &gone),
        ];
        publish(&server, &signer, &with_minecraft(1, entries.clone()));
        let dir = tempfile::tempdir().unwrap();
        let feeds = client(dir.path());
        let settings = settings(&[server.base()], &[&signer]);
        let snapshot = feeds.refresh(&settings).await.unwrap();
        assert_eq!(snapshot.feed.minecraft.len(), 2);

        let report = feeds.prefetch_media(&settings, &snapshot, now).await;
        assert_eq!(
            report,
            FeedMediaReport {
                requested: 1,
                downloaded: 1,
                failed: 0
            }
        );
        assert!(feeds.cached_media(&shown).await.is_some());
        assert!(feeds.cached_media(&gone).await.is_none());
        assert_eq!(server.hits(&format!("/v1/{}", gone.path)), 0);

        // Kept while a current or the previous feed lists it, then collected.
        publish(&server, &signer, &with_minecraft(2, entries));
        feeds.refresh(&settings).await.unwrap();
        assert!(feeds.cached_media(&shown).await.is_some());
        publish(&server, &signer, &with_minecraft(3, Vec::new()));
        feeds.refresh(&settings).await.unwrap();
        assert!(feeds.cached_media(&shown).await.is_some());
        publish(&server, &signer, &with_minecraft(4, Vec::new()));
        feeds.refresh(&settings).await.unwrap();
        assert!(feeds.cached_media(&shown).await.is_none());
    }

    #[test]
    fn settings_come_from_an_enabled_config_only() {
        let signer = key(1);
        let config = NewsFeedConfig {
            enabled: true,
            mirrors: vec![
                "https://projects.gg/launcher-feed/v1/".into(),
                "http://x/".into(),
            ],
            keys: vec![
                crate::model::FeedKey::from_public_key(&crate::signing::encode_verifying_key(
                    &signer.verifying_key(),
                ))
                .unwrap(),
            ],
            max_age_days: 999,
        };
        let settings = FeedSettings::from_config(&config).unwrap();
        assert!(settings.is_usable());
        assert_eq!(settings.mirrors.len(), 1);
        assert_eq!(settings.max_age_days, 60);
        let off = NewsFeedConfig {
            enabled: false,
            ..config.clone()
        };
        assert!(FeedSettings::from_config(&off).is_none());
        let keyless = NewsFeedConfig {
            keys: Vec::new(),
            ..config
        };
        assert!(!FeedSettings::from_config(&keyless).unwrap().is_usable());
    }
}
