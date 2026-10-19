//! Fetching, verifying and caching published content.
//!
//! Flow: [`ContentClient::load_cached`] gives an instant (offline-capable) start, then
//! [`ContentClient::refresh`] asks each mirror in order for `index.json` (conditional on the
//! cached ETag), verifies `index.json.sig` against the trusted keys, refuses older sequences,
//! downloads the referenced documents by SHA-256 and commits the new snapshot. When every
//! mirror fails, a valid cache still yields a snapshot (`from_cache = true`, `offline` tells
//! whether connectivity was the cause), never an error.
//!
//! Cache layout (`cache_dir` is `DataDirs`' `cache/content/`):
//! ```text
//! current.json            verified index text + signature, ETags, fetch time, GC hints
//! docs/<sha256>.json      config, news and pack manifests (content addressed, reused)
//! media/<sha256>.<ext>    images and videos (served through Tauri's asset protocol)
//! ```

use std::collections::{BTreeMap, BTreeSet, HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex, PoisonError};
use std::time::Duration;

use futures::StreamExt;
use jiff::Timestamp;
use pl_net::{Checksum, DownloadItem, DownloadOptions, Downloader, Http, NetError, VerifyMode};
use reqwest::StatusCode;
use reqwest::header::{ETAG, IF_NONE_MATCH};
use serde::de::DeserializeOwned;
use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{Error, Result};
use crate::fsutil::{normalize_hex, read_optional, sha256_hex, write_atomic};
use crate::model::{
    DocRef, FileRef, LauncherConfig, MediaRef, ModSetCatalog, NewsFeed, PackManifest, PackRef,
    RootIndex,
};
use crate::signing::{SignatureFile, VerifyingKey, verify_index};

/// Primary mirror (Cloudflare R2 behind the CDN).
pub const PRIMARY_MIRROR: &str = "https://cdn.projects.gg/v1/";
/// GitHub fallback mirror (small files committed to `projects-gg/launcher-content`).
pub const GITHUB_MIRROR: &str =
    "https://raw.githubusercontent.com/projects-gg/launcher-content/main/v1/";

const INDEX_NAME: &str = "index.json";
const SIGNATURE_NAME: &str = "index.json.sig";
const STATE_NAME: &str = "current.json";
const DOCS_DIR: &str = "docs";
const MEDIA_DIR: &str = "media";
const MAX_INDEX_BYTES: u64 = 4 << 20;
const MAX_SIGNATURE_BYTES: u64 = 16 << 10;
const MAX_DOC_BYTES: u64 = 64 << 20;
const DEFAULT_REQUEST_TIMEOUT: Duration = Duration::from_secs(10);
const MIN_DOC_TIMEOUT: Duration = Duration::from_secs(30);
const STALE_TEMP_AGE: Duration = Duration::from_secs(24 * 60 * 60);

/// Everything the home screen needs, from one verified index.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ContentSnapshot {
    pub index: RootIndex,
    pub config: LauncherConfig,
    pub news: NewsFeed,
    /// Mirror base URL the index was (last) fetched from.
    pub mirror_used: String,
    /// When a mirror last confirmed this index.
    pub fetched_at: Timestamp,
    /// The refresh did not succeed and this snapshot comes from the local cache.
    pub from_cache: bool,
    /// No mirror could be reached (show the "Çevrimdışı" badge).
    pub offline: bool,
    /// Technical reason the refresh failed, for "copy details". Never shown as UI text.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub problem: Option<String>,
}

/// Result of [`ContentClient::prefetch_media`].
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MediaPrefetch {
    pub requested: usize,
    pub available: usize,
    pub failed: usize,
}

/// Result of cache garbage collection.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GcReport {
    pub removed_docs: usize,
    pub removed_media: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct CacheState {
    /// Exact text of the verified `index.json`.
    index: String,
    signature: SignatureFile,
    mirror_used: String,
    fetched_at: Timestamp,
    /// Mirror base → ETag under which that mirror served exactly `index`.
    #[serde(default)]
    etags: BTreeMap<String, String>,
    /// Files of the snapshot this one replaced; garbage collection keeps them.
    #[serde(default)]
    previous: Retained,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
struct Retained {
    docs: BTreeSet<String>,
    media: BTreeSet<String>,
}

impl Retained {
    fn of(index: &RootIndex, docs: Option<&(LauncherConfig, NewsFeed)>) -> Self {
        let mut retained = Self::default();
        retained.docs.extend(
            index
                .documents()
                .map(|doc| doc.sha256.trim().to_ascii_lowercase()),
        );
        if let Some((config, news)) = docs {
            retained.media.extend(
                config
                    .media_refs()
                    .into_iter()
                    .chain(news.media_refs())
                    .map(|media| media.sha256.trim().to_ascii_lowercase()),
            );
        }
        retained
    }

    fn merge(&mut self, other: &Retained) {
        self.docs.extend(other.docs.iter().cloned());
        self.media.extend(other.media.iter().cloned());
    }
}

/// The verified local cache.
struct Cached {
    state: CacheState,
    index: RootIndex,
    /// Config and news, when both are present and intact in `docs/`.
    docs: Option<(LauncherConfig, NewsFeed)>,
}

enum Fetched {
    NotModified,
    Body {
        bytes: Vec<u8>,
        etag: Option<String>,
    },
}

enum Attempt {
    /// The mirror serves exactly the cached index.
    Unchanged { etag: Option<String> },
    /// A verified index with a higher sequence.
    Newer(Box<NewIndex>),
}

struct NewIndex {
    text: String,
    signature: SignatureFile,
    index: RootIndex,
    etag: Option<String>,
}

/// Client for the signed content tree. Cheap to share behind an `Arc`.
#[derive(Debug)]
pub struct ContentClient {
    http: Http,
    downloader: Downloader,
    mirrors: Vec<Url>,
    cache_dir: PathBuf,
    trusted_keys: Vec<VerifyingKey>,
    request_timeout: Duration,
    refresh_lock: tokio::sync::Mutex<()>,
    media_locks: Mutex<HashMap<String, Arc<tokio::sync::Mutex<()>>>>,
}

impl ContentClient {
    /// `mirrors` are base URLs (`…/v1/`) tried in order; `trusted_keys` are the Ed25519 keys
    /// allowed to sign `index.json` (several during a rotation).
    pub fn new<S: AsRef<str>>(
        http: Http,
        mirrors: &[S],
        cache_dir: impl Into<PathBuf>,
        trusted_keys: Vec<VerifyingKey>,
    ) -> Result<Self> {
        if trusted_keys.is_empty() {
            return Err(Error::InvalidKey(
                "at least one trusted key is required".into(),
            ));
        }
        let mirrors = mirrors
            .iter()
            .map(|mirror| parse_base(mirror.as_ref()))
            .collect::<Result<Vec<_>>>()?;
        if mirrors.is_empty() {
            return Err(Error::Invalid("at least one mirror is required".into()));
        }
        Ok(Self {
            downloader: Downloader::new(http.clone()),
            http,
            mirrors,
            cache_dir: cache_dir.into(),
            trusted_keys,
            request_timeout: DEFAULT_REQUEST_TIMEOUT,
            refresh_lock: tokio::sync::Mutex::new(()),
            media_locks: Mutex::new(HashMap::new()),
        })
    }

    /// Per-request timeout for `index.json` and its signature (documents get at least 30 s).
    /// The launcher uses a short one at startup so offline mode kicks in quickly.
    pub fn with_request_timeout(mut self, timeout: Duration) -> Self {
        self.request_timeout = timeout;
        self
    }

    pub fn mirrors(&self) -> &[Url] {
        &self.mirrors
    }

    pub fn cache_dir(&self) -> &Path {
        &self.cache_dir
    }

    /// Fetches and verifies the newest content. Falls back to the cache on any failure; only
    /// errors when there is no usable cache.
    pub async fn refresh(&self) -> Result<ContentSnapshot> {
        let _guard = self.refresh_lock.lock().await;
        let cached = self.load_verified_cache().await;

        let mut problems = Vec::new();
        let mut all_offline = true;
        let mut content_error = None;
        for base in &self.mirrors {
            match self.refresh_from(base, cached.as_ref()).await {
                Ok(snapshot) => return Ok(snapshot),
                Err(error) => {
                    tracing::warn!(mirror = %base, %error, "content mirror failed");
                    problems.push(format!("{base}: {error}"));
                    if !error.is_offline() {
                        all_offline = false;
                        content_error.get_or_insert(error);
                    }
                }
            }
        }

        let problem = problems.join("; ");
        if let Some(cached) = cached
            && let Some((config, news)) = cached.docs
        {
            return Ok(ContentSnapshot {
                index: cached.index,
                config,
                news,
                mirror_used: cached.state.mirror_used,
                fetched_at: cached.state.fetched_at,
                from_cache: true,
                offline: all_offline,
                problem: Some(problem),
            });
        }
        Err(match content_error {
            Some(error) => error,
            None => Error::Offline(problem),
        })
    }

    /// The cached snapshot without touching the network (offline start). Fails with
    /// [`Error::NoCache`] when nothing valid is cached.
    pub async fn load_cached(&self) -> Result<ContentSnapshot> {
        let cached = self.try_load_cache().await?.ok_or(Error::NoCache)?;
        let (config, news) = cached.docs.ok_or(Error::NoCache)?;
        Ok(ContentSnapshot {
            index: cached.index,
            config,
            news,
            mirror_used: cached.state.mirror_used,
            fetched_at: cached.state.fetched_at,
            from_cache: true,
            offline: false,
            problem: None,
        })
    }

    /// The manifest of a pack listed in the index (from the document cache when present).
    pub async fn pack_manifest(&self, pack: &PackRef) -> Result<PackManifest> {
        let bytes = self.ensure_doc(&pack.manifest, None).await?;
        let manifest: PackManifest = parse_json(&bytes, &pack.manifest.path)?;
        manifest.validate()?;
        Ok(manifest)
    }

    /// The ready-made mod sets the index references (from the document cache when present):
    /// an empty catalog when it references none. Downloaded on first use, not with every
    /// refresh. A set that breaks a rule is left out (and logged); the others stay usable.
    pub async fn mod_sets(&self, index: &RootIndex) -> Result<ModSetCatalog> {
        let Some(doc) = &index.mod_sets else {
            return Ok(ModSetCatalog::default());
        };
        let bytes = self.ensure_doc(doc, None).await?;
        let catalog: ModSetCatalog = parse_json(&bytes, &doc.path)?;
        catalog.check_schema()?;
        let (catalog, dropped) = catalog.without_broken_sets();
        for problem in dropped {
            tracing::warn!(%problem, "leaving out a broken mod set");
        }
        Ok(catalog)
    }

    /// Local path of a media file, downloading and verifying it on first use. Cached copies
    /// are trusted by size; downloads are verified by SHA-256.
    pub async fn media(&self, media: &MediaRef) -> Result<PathBuf> {
        media.validate()?;
        let sha256 = normalize_hex(&media.sha256, 64)
            .ok_or_else(|| Error::Invalid(format!("bad sha256 for {}", media.path)))?;
        let dest = self.cache_dir.join(MEDIA_DIR).join(media.cache_file_name());
        let lock = self.media_lock(&sha256);
        let _guard = lock.lock().await;

        let urls = self
            .mirrors
            .iter()
            .filter_map(|base| base.join(&media.path).ok())
            .map(String::from)
            .collect();
        let item = DownloadItem::with_urls(urls, &dest)
            .checksum(Some(Checksum::sha256(&sha256)))
            .size(Some(media.size));
        let options = DownloadOptions {
            concurrency: 1,
            verify: VerifyMode::Size,
            ..DownloadOptions::default()
        };
        self.downloader.download(item, &options).await?;
        Ok(dest)
    }

    /// Downloads the hero media and the covers of featured news (with posters) so the home
    /// screen renders instantly and offline. Failures are counted, not returned.
    pub async fn prefetch_media(&self, snapshot: &ContentSnapshot) -> MediaPrefetch {
        let now = Timestamp::now();
        let mut seen = HashSet::new();
        let featured = snapshot.news.featured(now);
        let wanted: Vec<&MediaRef> = snapshot
            .config
            .hero
            .media
            .iter()
            .chain(featured.iter().filter_map(|item| item.cover.as_ref()))
            .flat_map(MediaRef::with_posters)
            .filter(|media| seen.insert(media.sha256.trim().to_ascii_lowercase()))
            .collect();
        // Build the futures eagerly: a closure inside the stream combinator would make the
        // returned future higher-ranked and not `Send` for callers that spawn it.
        let downloads: Vec<_> = wanted.iter().map(|media| self.media(media)).collect();
        let results: Vec<Result<PathBuf>> = futures::stream::iter(downloads)
            .buffer_unordered(4)
            .collect()
            .await;
        let failed = results.iter().filter(|result| result.is_err()).count();
        MediaPrefetch {
            requested: wanted.len(),
            available: results.len() - failed,
            failed,
        }
    }

    /// Download URLs for a [`FileRef`] (mirror copies first, then its alternates).
    pub fn file_urls(&self, file: &FileRef) -> Vec<String> {
        file.candidate_urls(&self.mirrors)
    }

    /// A verified [`DownloadItem`] for a [`FileRef`] (AFK client builds).
    pub fn download_item(&self, file: &FileRef, dest: impl Into<PathBuf>) -> Result<DownloadItem> {
        file.download_item(&self.mirrors, dest)
    }

    /// Deletes cached documents and media referenced by neither the current nor the
    /// previous snapshot. Runs automatically after a refresh that brought a new index.
    pub async fn gc(&self) -> Result<GcReport> {
        let _guard = self.refresh_lock.lock().await;
        let Some(cached) = self.load_verified_cache().await else {
            return Ok(GcReport::default());
        };
        let mut keep = Retained::of(&cached.index, cached.docs.as_ref());
        keep.merge(&cached.state.previous);
        self.collect_garbage(&keep).await
    }

    async fn refresh_from(&self, base: &Url, cached: Option<&Cached>) -> Result<ContentSnapshot> {
        let etag = cached
            .and_then(|c| c.state.etags.get(base.as_str()))
            .map(String::as_str);
        let attempt = match self.fetch_index(base, etag, None).await? {
            Fetched::NotModified => match cached {
                Some(_) => Attempt::Unchanged {
                    etag: etag.map(str::to_owned),
                },
                None => return Err(Error::Invalid("unexpected 304 for index.json".into())),
            },
            Fetched::Body { bytes, etag } => {
                match self.classify(base, bytes, etag, cached, None).await {
                    Err(Error::BadSignature) => {
                        // Right after a publish an edge cache can pair a new index with the
                        // old signature (or the reverse); retry once past the cache.
                        let token = Timestamp::now().as_millisecond().to_string();
                        match self.fetch_index(base, None, Some(&token)).await? {
                            Fetched::Body { bytes, etag } => {
                                self.classify(base, bytes, etag, cached, Some(&token))
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
                let (config, news) = match &cached.docs {
                    Some(docs) => docs.clone(),
                    None => self.load_documents(&cached.index, Some(base)).await?,
                };
                let mut state = cached.state.clone();
                state.fetched_at = Timestamp::now();
                if let Some(etag) = etag {
                    state.etags.insert(base.to_string(), etag);
                }
                self.save_state(&state).await;
                Ok(ContentSnapshot {
                    index: cached.index.clone(),
                    config,
                    news,
                    mirror_used: base.to_string(),
                    fetched_at: state.fetched_at,
                    from_cache: false,
                    offline: false,
                    problem: None,
                })
            }
            Attempt::Newer(new) => {
                let NewIndex {
                    text,
                    signature,
                    index,
                    etag,
                } = *new;
                let docs = self.load_documents(&index, Some(base)).await?;
                let previous = cached
                    .map(|c| Retained::of(&c.index, c.docs.as_ref()))
                    .unwrap_or_default();
                let state = CacheState {
                    index: text,
                    signature,
                    mirror_used: base.to_string(),
                    fetched_at: Timestamp::now(),
                    etags: etag.map(|e| (base.to_string(), e)).into_iter().collect(),
                    previous,
                };
                self.save_state(&state).await;
                let mut keep = Retained::of(&index, Some(&docs));
                keep.merge(&state.previous);
                if let Err(error) = self.collect_garbage(&keep).await {
                    tracing::warn!(%error, "content cache cleanup failed");
                }
                tracing::info!(sequence = index.sequence, mirror = %base, "content updated");
                let (config, news) = docs;
                Ok(ContentSnapshot {
                    index,
                    config,
                    news,
                    mirror_used: base.to_string(),
                    fetched_at: state.fetched_at,
                    from_cache: false,
                    offline: false,
                    problem: None,
                })
            }
        }
    }

    /// Verifies a fetched index body and decides whether it is new.
    async fn classify(
        &self,
        base: &Url,
        bytes: Vec<u8>,
        etag: Option<String>,
        cached: Option<&Cached>,
        cache_buster: Option<&str>,
    ) -> Result<Attempt> {
        if let Some(cached) = cached
            && cached.state.index.as_bytes() == bytes.as_slice()
        {
            return Ok(Attempt::Unchanged { etag });
        }
        let signature = self.fetch_signature(base, cache_buster).await?;
        verify_index(&bytes, &signature, &self.trusted_keys)?;
        let text = String::from_utf8(bytes)
            .map_err(|_| Error::Invalid("index.json is not valid UTF-8".into()))?;
        let index: RootIndex = parse_json(text.as_bytes(), INDEX_NAME)?;
        index.validate()?;
        if let Some(cached) = cached {
            let floor = cached.index.sequence;
            if index.sequence < floor {
                return Err(Error::Rollback {
                    received: index.sequence,
                    cached: floor,
                });
            }
            if index.sequence == floor {
                return Err(Error::Invalid(format!(
                    "index sequence {floor} was reused for different content"
                )));
            }
        }
        Ok(Attempt::Newer(Box::new(NewIndex {
            text,
            signature,
            index,
            etag,
        })))
    }

    async fn fetch_index(
        &self,
        base: &Url,
        etag: Option<&str>,
        cache_buster: Option<&str>,
    ) -> Result<Fetched> {
        let url = resource_url(base, INDEX_NAME, cache_buster)?;
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
        let bytes = read_limited(response, MAX_INDEX_BYTES, &url).await?;
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

    async fn load_documents(
        &self,
        index: &RootIndex,
        preferred: Option<&Url>,
    ) -> Result<(LauncherConfig, NewsFeed)> {
        let (config, news) = futures::try_join!(
            self.ensure_doc(&index.config, preferred),
            self.ensure_doc(&index.news, preferred)
        )?;
        parse_documents(index, &config, &news)
    }

    /// Bytes of a document: from `docs/<sha256>.json` when intact, otherwise downloaded from
    /// the mirrors (`preferred` first), verified and cached.
    async fn ensure_doc(&self, doc: &DocRef, preferred: Option<&Url>) -> Result<Vec<u8>> {
        doc.validate()?;
        if doc.size > MAX_DOC_BYTES {
            return Err(Error::Invalid(format!("{} is too large", doc.path)));
        }
        let sha256 = normalize_hex(&doc.sha256, 64)
            .ok_or_else(|| Error::Invalid(format!("bad sha256 for {}", doc.path)))?;
        if let Some(bytes) = self.read_cached_doc(doc).await? {
            return Ok(bytes);
        }

        let timeout = self.request_timeout.max(MIN_DOC_TIMEOUT);
        let mut content_error = None;
        let mut network_error = None;
        for base in self.ordered_mirrors(preferred) {
            let url = base
                .join(&doc.path)
                .map_err(|e| Error::Invalid(format!("bad document URL: {e}")))?;
            match self.get_limited(&url, doc.size, timeout).await {
                Ok(bytes) => {
                    let actual = sha256_hex(&bytes);
                    if bytes.len() as u64 == doc.size && actual == sha256 {
                        if let Err(error) = write_atomic(&self.doc_path(&sha256), &bytes).await {
                            tracing::warn!(%error, "could not cache a content document");
                        }
                        return Ok(bytes);
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
            .unwrap_or_else(|| Error::Offline(doc.path.clone())))
    }

    /// A cached document whose hash still matches; corrupt copies are deleted.
    async fn read_cached_doc(&self, doc: &DocRef) -> Result<Option<Vec<u8>>> {
        let Some(sha256) = normalize_hex(&doc.sha256, 64) else {
            return Ok(None);
        };
        let path = self.doc_path(&sha256);
        let Some(bytes) = read_optional(&path).await? else {
            return Ok(None);
        };
        if sha256_hex(&bytes) == sha256 {
            return Ok(Some(bytes));
        }
        tracing::warn!(path = %path.display(), "cached document is corrupt; dropping it");
        let _ = tokio::fs::remove_file(&path).await;
        Ok(None)
    }

    async fn load_cached_documents(&self, index: &RootIndex) -> Result<(LauncherConfig, NewsFeed)> {
        let config = self
            .read_cached_doc(&index.config)
            .await?
            .ok_or(Error::NoCache)?;
        let news = self
            .read_cached_doc(&index.news)
            .await?
            .ok_or(Error::NoCache)?;
        parse_documents(index, &config, &news)
    }

    async fn load_verified_cache(&self) -> Option<Cached> {
        match self.try_load_cache().await {
            Ok(cached) => cached,
            Err(error) => {
                tracing::warn!(%error, "ignoring the unusable content cache");
                None
            }
        }
    }

    async fn try_load_cache(&self) -> Result<Option<Cached>> {
        let Some(bytes) = read_optional(&self.state_path()).await? else {
            return Ok(None);
        };
        let state: CacheState = parse_json(&bytes, STATE_NAME)?;
        verify_index(state.index.as_bytes(), &state.signature, &self.trusted_keys)?;
        let index: RootIndex = parse_json(state.index.as_bytes(), INDEX_NAME)?;
        index.validate()?;
        let docs = match self.load_cached_documents(&index).await {
            Ok(docs) => Some(docs),
            Err(error) => {
                tracing::debug!(%error, "cached content documents are incomplete");
                None
            }
        };
        Ok(Some(Cached { state, index, docs }))
    }

    async fn save_state(&self, state: &CacheState) {
        let result = match serde_json::to_vec_pretty(state) {
            Ok(bytes) => write_atomic(&self.state_path(), &bytes).await,
            Err(e) => Err(Error::json(STATE_NAME, e)),
        };
        if let Err(error) = result {
            tracing::warn!(%error, "could not persist the content cache state");
        }
    }

    async fn collect_garbage(&self, keep: &Retained) -> Result<GcReport> {
        Ok(GcReport {
            removed_docs: sweep(&self.cache_dir.join(DOCS_DIR), &keep.docs).await?,
            removed_media: sweep(&self.cache_dir.join(MEDIA_DIR), &keep.media).await?,
        })
    }

    fn ordered_mirrors<'a>(&'a self, preferred: Option<&'a Url>) -> Vec<&'a Url> {
        let mut ordered: Vec<&Url> = preferred.into_iter().collect();
        ordered.extend(self.mirrors.iter().filter(|m| Some(*m) != preferred));
        ordered
    }

    fn media_lock(&self, sha256: &str) -> Arc<tokio::sync::Mutex<()>> {
        let mut locks = self
            .media_locks
            .lock()
            .unwrap_or_else(PoisonError::into_inner);
        Arc::clone(locks.entry(sha256.to_owned()).or_default())
    }

    fn state_path(&self) -> PathBuf {
        self.cache_dir.join(STATE_NAME)
    }

    fn doc_path(&self, sha256: &str) -> PathBuf {
        self.cache_dir.join(DOCS_DIR).join(format!("{sha256}.json"))
    }
}

fn parse_base(text: &str) -> Result<Url> {
    let mut url = Url::parse(text.trim())
        .map_err(|e| Error::Invalid(format!("bad mirror URL {text:?}: {e}")))?;
    if !matches!(url.scheme(), "http" | "https") {
        return Err(Error::Invalid(format!("mirror {text:?} is not HTTP(S)")));
    }
    if !url.path().ends_with('/') {
        let path = format!("{}/", url.path());
        url.set_path(&path);
    }
    url.set_query(None);
    url.set_fragment(None);
    Ok(url)
}

pub(crate) fn resource_url(base: &Url, name: &str, cache_buster: Option<&str>) -> Result<Url> {
    let mut url = base
        .join(name)
        .map_err(|e| Error::Invalid(format!("bad content URL: {e}")))?;
    if let Some(token) = cache_buster {
        url.set_query(Some(&format!("t={token}")));
    }
    Ok(url)
}

pub(crate) fn ensure_success(response: reqwest::Response, url: &Url) -> Result<reqwest::Response> {
    if response.status().is_success() {
        Ok(response)
    } else {
        Err(NetError::Status {
            url: url.to_string(),
            status: response.status().as_u16(),
        }
        .into())
    }
}

/// Reads a response body, refusing anything larger than `limit` bytes.
pub(crate) async fn read_limited(
    mut response: reqwest::Response,
    limit: u64,
    url: &Url,
) -> Result<Vec<u8>> {
    if response.content_length().is_some_and(|len| len > limit) {
        return Err(Error::Invalid(format!(
            "{url} is larger than {limit} bytes"
        )));
    }
    let mut body = Vec::new();
    while let Some(chunk) = response.chunk().await.map_err(|source| NetError::Request {
        url: url.to_string(),
        source,
    })? {
        if (body.len() + chunk.len()) as u64 > limit {
            return Err(Error::Invalid(format!(
                "{url} is larger than {limit} bytes"
            )));
        }
        body.extend_from_slice(&chunk);
    }
    Ok(body)
}

pub(crate) fn parse_json<T: DeserializeOwned>(bytes: &[u8], what: &str) -> Result<T> {
    serde_json::from_slice(bytes).map_err(|e| Error::json(what, e))
}

fn parse_documents(
    index: &RootIndex,
    config: &[u8],
    news: &[u8],
) -> Result<(LauncherConfig, NewsFeed)> {
    let config: LauncherConfig = parse_json(config, &index.config.path)?;
    config.check_schema()?;
    let news: NewsFeed = parse_json(news, &index.news.path)?;
    news.check_schema()?;
    Ok((config, news))
}

/// Deletes content-addressed files (`<sha256>.<ext>`) not in `keep`, plus temporary files
/// older than a day. Unrelated files are left alone.
pub(crate) async fn sweep(dir: &Path, keep: &BTreeSet<String>) -> Result<usize> {
    let mut entries = match tokio::fs::read_dir(dir).await {
        Ok(entries) => entries,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(0),
        Err(e) => return Err(Error::io(dir, e)),
    };
    let mut removed = 0;
    while let Some(entry) = entries.next_entry().await.map_err(|e| Error::io(dir, e))? {
        let path = entry.path();
        let Some(name) = path.file_name().and_then(|n| n.to_str()) else {
            continue;
        };
        let remove = if name.ends_with(".part") || name.ends_with(".tmp") {
            is_older_than(&path, STALE_TEMP_AGE).await
        } else {
            let stem = name.split('.').next().unwrap_or_default();
            normalize_hex(stem, 64).is_some() && !keep.contains(stem)
        };
        if !remove {
            continue;
        }
        match tokio::fs::remove_file(&path).await {
            Ok(()) => removed += 1,
            Err(error) => {
                tracing::debug!(path = %path.display(), %error, "could not remove a cached file")
            }
        }
    }
    Ok(removed)
}

async fn is_older_than(path: &Path, age: Duration) -> bool {
    tokio::fs::metadata(path)
        .await
        .and_then(|meta| meta.modified())
        .ok()
        .and_then(|modified| modified.elapsed().ok())
        .is_some_and(|elapsed| elapsed > age)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::model::{MediaKind, ROOT_INDEX_SCHEMA};
    use crate::signing::{SigningKey, sign_index};
    use crate::studio::media_ref;
    use crate::testutil::{Publish, TestServer, dead_base, sample_config, sample_news, test_http};

    fn key(seed: u8) -> SigningKey {
        SigningKey::from_bytes(&[seed; 32])
    }

    fn client(bases: &[String], cache: &Path, key: &SigningKey) -> ContentClient {
        ContentClient::new(test_http(), bases, cache, vec![key.verifying_key()])
            .unwrap()
            .with_request_timeout(Duration::from_secs(5))
    }

    #[tokio::test]
    async fn refresh_verifies_caches_and_reuses_documents() {
        let server = TestServer::start().await;
        let signer = key(1);
        let published = Publish::new(10).put(&server, &signer);
        let dir = tempfile::tempdir().unwrap();

        let content = client(&[server.base()], dir.path(), &signer);
        let snapshot = content.refresh().await.unwrap();
        assert!(!snapshot.from_cache && !snapshot.offline && snapshot.problem.is_none());
        assert_eq!(snapshot.index, published.index);
        assert_eq!(snapshot.config, sample_config(None));
        assert_eq!(snapshot.news, sample_news(None));
        assert_eq!(snapshot.mirror_used, server.base());
        let config_doc = dir
            .path()
            .join(DOCS_DIR)
            .join(format!("{}.json", published.index.config.sha256));
        assert!(config_doc.is_file());

        // Second refresh: conditional request answered with 304, nothing else downloaded.
        let again = content.refresh().await.unwrap();
        assert!(!again.from_cache);
        assert_eq!(server.hits("/v1/index.json"), 2);
        assert_eq!(server.hits("/v1/index.json.sig"), 1);
        let config_url = format!("/v1/{}", published.index.config.path);
        assert_eq!(server.hits(&config_url), 1);

        // A new client (next launch) reuses the cache: offline start, then a 304 refresh.
        let next_run = client(&[server.base()], dir.path(), &signer);
        let offline_start = next_run.load_cached().await.unwrap();
        assert!(offline_start.from_cache && !offline_start.offline);
        assert_eq!(offline_start.index, published.index);
        next_run.refresh().await.unwrap();
        assert_eq!(
            server.hits(&config_url),
            1,
            "documents are reused across runs"
        );
        assert_eq!(server.hits("/v1/index.json.sig"), 1);
    }

    #[tokio::test]
    async fn new_index_replaces_cache() {
        let server = TestServer::start().await;
        let signer = key(1);
        Publish::new(10).put(&server, &signer);
        let dir = tempfile::tempdir().unwrap();
        let content = client(&[server.base()], dir.path(), &signer);
        content.refresh().await.unwrap();

        let mut next = Publish::new(11);
        next.config.hero.title = "Kış etkinliği".into();
        let published = next.put(&server, &signer);
        let snapshot = content.refresh().await.unwrap();
        assert_eq!(snapshot.index.sequence, 11);
        assert_eq!(snapshot.config.hero.title, "Kış etkinliği");
        assert_eq!(content.load_cached().await.unwrap().index, published.index);
    }

    #[tokio::test]
    async fn falls_back_to_the_next_mirror() {
        let good = TestServer::start().await;
        let signer = key(1);
        Publish::new(5).put(&good, &signer);

        // Dead primary.
        let dir = tempfile::tempdir().unwrap();
        let content = client(&[dead_base().await, good.base()], dir.path(), &signer);
        let snapshot = content.refresh().await.unwrap();
        assert_eq!(snapshot.mirror_used, good.base());
        assert!(!snapshot.from_cache);

        // Tampered primary: valid JSON, signature for other bytes.
        let evil = TestServer::start().await;
        let published = Publish::new(6).put(&evil, &signer);
        let mut forged = published.index.clone();
        forged.sequence = 99;
        evil.put(
            "/v1/index.json",
            crate::json::to_canonical_json(&forged).unwrap(),
        );
        Publish::new(6).put(&good, &signer);
        let dir = tempfile::tempdir().unwrap();
        let content = client(&[evil.base(), good.base()], dir.path(), &signer);
        let snapshot = content.refresh().await.unwrap();
        assert_eq!(snapshot.mirror_used, good.base());
        assert_eq!(snapshot.index.sequence, 6);
        assert_eq!(
            evil.hits("/v1/index.json"),
            2,
            "a bad signature is retried once past the edge cache"
        );
    }

    #[tokio::test]
    async fn rollback_is_refused_and_cache_kept() {
        let server = TestServer::start().await;
        let signer = key(1);
        Publish::new(10).put(&server, &signer);
        let dir = tempfile::tempdir().unwrap();
        let content = client(&[server.base()], dir.path(), &signer);
        content.refresh().await.unwrap();

        let mut old = Publish::new(9);
        old.config.hero.title = "Eski".into();
        old.put(&server, &signer);
        let snapshot = content.refresh().await.unwrap();
        assert!(snapshot.from_cache);
        assert!(
            !snapshot.offline,
            "a rollback is not a connectivity problem"
        );
        assert_eq!(snapshot.index.sequence, 10);
        assert!(snapshot.problem.unwrap().contains("older"));

        // Same sequence, different content: refused as well.
        let mut equivocation = Publish::new(10);
        equivocation.config.hero.title = "Başka".into();
        equivocation.put(&server, &signer);
        let snapshot = content.refresh().await.unwrap();
        assert!(snapshot.from_cache);
        assert_ne!(snapshot.config.hero.title, "Başka");
    }

    #[tokio::test]
    async fn unknown_key_is_refused() {
        let server = TestServer::start().await;
        Publish::new(1).put(&server, &key(2));
        let dir = tempfile::tempdir().unwrap();
        let content = client(&[server.base()], dir.path(), &key(1));
        let error = content.refresh().await.unwrap_err();
        assert!(matches!(error, Error::UntrustedKey { .. }), "{error}");
        assert!(!error.is_offline());
    }

    #[tokio::test]
    async fn offline_with_and_without_cache() {
        let server = TestServer::start().await;
        let signer = key(1);
        let published = Publish::new(3).put(&server, &signer);
        let dir = tempfile::tempdir().unwrap();
        client(&[server.base()], dir.path(), &signer)
            .refresh()
            .await
            .unwrap();

        let offline = client(&[dead_base().await, dead_base().await], dir.path(), &signer);
        let snapshot = offline.refresh().await.unwrap();
        assert!(snapshot.from_cache && snapshot.offline);
        assert_eq!(snapshot.index, published.index);
        assert_eq!(snapshot.mirror_used, server.base());

        let empty = tempfile::tempdir().unwrap();
        let error = client(&[dead_base().await], empty.path(), &signer)
            .refresh()
            .await
            .unwrap_err();
        assert!(error.is_offline(), "{error}");
        assert!(matches!(
            client(&[dead_base().await], empty.path(), &signer)
                .load_cached()
                .await,
            Err(Error::NoCache)
        ));
    }

    #[tokio::test]
    async fn tampered_cache_is_ignored() {
        let server = TestServer::start().await;
        let signer = key(1);
        Publish::new(3).put(&server, &signer);
        let dir = tempfile::tempdir().unwrap();
        let content = client(&[server.base()], dir.path(), &signer);
        content.refresh().await.unwrap();

        let state_path = dir.path().join(STATE_NAME);
        let mut state: CacheState =
            serde_json::from_slice(&std::fs::read(&state_path).unwrap()).unwrap();
        state.index = state.index.replace("\"sequence\": 3", "\"sequence\": 4");
        std::fs::write(&state_path, serde_json::to_vec(&state).unwrap()).unwrap();
        assert!(matches!(
            content.load_cached().await,
            Err(Error::BadSignature)
        ));
        // Refresh repairs it from the network.
        assert!(!content.refresh().await.unwrap().from_cache);
        content.load_cached().await.unwrap();

        // A corrupt cached document is dropped and downloaded again.
        let cached = content.load_cached().await.unwrap();
        let doc = dir
            .path()
            .join(DOCS_DIR)
            .join(format!("{}.json", cached.index.news.sha256));
        std::fs::write(&doc, b"{}").unwrap();
        assert!(matches!(content.load_cached().await, Err(Error::NoCache)));
        content.refresh().await.unwrap();
        content.load_cached().await.unwrap();
    }

    #[tokio::test]
    async fn media_is_cached_and_verified() {
        let server = TestServer::start().await;
        let signer = key(1);
        let hero_bytes = b"RIFF....WEBPVP8 hero".to_vec();
        let hero = media_ref(&hero_bytes, "webp", MediaKind::Image);
        let cover_bytes = b"cover image bytes".to_vec();
        let cover = media_ref(&cover_bytes, "png", MediaKind::Image);
        server.put(&format!("/v1/{}", hero.path), hero_bytes.clone());
        server.put(&format!("/v1/{}", cover.path), cover_bytes.clone());
        let mut publish = Publish::new(1);
        publish.config = sample_config(Some(hero.clone()));
        publish.news = sample_news(Some(cover.clone()));
        publish.put(&server, &signer);

        let dir = tempfile::tempdir().unwrap();
        let content = client(&[server.base()], dir.path(), &signer);
        let snapshot = content.refresh().await.unwrap();
        let path = content.media(&hero).await.unwrap();
        assert_eq!(
            path,
            dir.path().join(MEDIA_DIR).join(hero.cache_file_name())
        );
        assert_eq!(std::fs::read(&path).unwrap(), hero_bytes);
        content.media(&hero).await.unwrap();
        assert_eq!(
            server.hits(&format!("/v1/{}", hero.path)),
            1,
            "served from cache"
        );

        let report = content.prefetch_media(&snapshot).await;
        assert_eq!(
            report,
            MediaPrefetch {
                requested: 2,
                available: 2,
                failed: 0
            }
        );
        assert_eq!(server.hits(&format!("/v1/{}", cover.path)), 1);

        // Wrong bytes on the server: verification fails, nothing is cached.
        let mut bogus = media_ref(b"expected", "webp", MediaKind::Image);
        server.put(&format!("/v1/{}", bogus.path), b"tampered".to_vec());
        assert!(content.media(&bogus).await.is_err());
        assert!(
            !dir.path()
                .join(MEDIA_DIR)
                .join(bogus.cache_file_name())
                .exists()
        );
        bogus.path = "../../escape.webp".into();
        assert!(matches!(
            content.media(&bogus).await,
            Err(Error::UnsafePath { .. })
        ));
    }

    #[tokio::test]
    async fn garbage_collection_keeps_current_and_previous() {
        let server = TestServer::start().await;
        let signer = key(1);
        let dir = tempfile::tempdir().unwrap();
        let content = client(&[server.base()], dir.path(), &signer);

        let mut generations = Vec::new();
        for (sequence, title) in [(1u64, "bir"), (2, "iki"), (3, "üç")] {
            let bytes = format!("hero {title}").into_bytes();
            let hero = media_ref(&bytes, "webp", MediaKind::Image);
            server.put(&format!("/v1/{}", hero.path), bytes);
            let mut publish = Publish::new(sequence);
            publish.config = sample_config(Some(hero.clone()));
            publish.config.hero.title = title.into();
            publish.news.items[0].title = title.into();
            let published = publish.put(&server, &signer);
            content.refresh().await.unwrap();
            content.media(&hero).await.unwrap();
            generations.push((published, hero));
        }

        let doc_exists = |doc: &DocRef| {
            dir.path()
                .join(DOCS_DIR)
                .join(format!("{}.json", doc.sha256))
                .exists()
        };
        let media_exists = |media: &MediaRef| {
            dir.path()
                .join(MEDIA_DIR)
                .join(media.cache_file_name())
                .exists()
        };
        let (first, first_hero) = &generations[0];
        assert!(!doc_exists(&first.index.config) && !doc_exists(&first.index.news));
        // Media of generation 2 was downloaded after the refresh that brought generation 3's
        // index ran its cleanup, so collect again explicitly.
        let report = content.gc().await.unwrap();
        assert_eq!(report.removed_docs, 0);
        assert!(!media_exists(first_hero));
        for (published, hero) in &generations[1..] {
            assert!(doc_exists(&published.index.config));
            assert!(media_exists(hero));
        }

        // Unrelated files are never touched.
        let foreign = dir.path().join(DOCS_DIR).join("notes.txt");
        std::fs::write(&foreign, b"x").unwrap();
        content.gc().await.unwrap();
        assert!(foreign.exists());
    }

    #[tokio::test]
    async fn pack_manifests_use_the_document_cache() {
        let server = TestServer::start().await;
        let signer = key(1);
        let mut publish = Publish::new(1);
        publish.pack = Some(crate::testutil::sample_manifest(&server, &[]));
        let published = publish.put(&server, &signer);
        let dir = tempfile::tempdir().unwrap();
        let content = client(&[server.base()], dir.path(), &signer);
        let snapshot = content.refresh().await.unwrap();
        let pack = snapshot.index.pack("projects").unwrap();
        assert_eq!(pack.latest, "1.0.0");
        let manifest = content.pack_manifest(pack).await.unwrap();
        assert_eq!(manifest.projects.pack_id, "projects");
        content.pack_manifest(pack).await.unwrap();
        assert_eq!(server.hits(&format!("/v1/{}", pack.manifest.path)), 1);
        assert_eq!(published.index.packs.len(), 1);
    }

    #[tokio::test]
    async fn mod_sets_use_the_document_cache() {
        use crate::testutil::{sample_mod_sets, sample_variant};
        let server = TestServer::start().await;
        let signer = key(1);
        let dir = tempfile::tempdir().unwrap();
        let content = client(&[server.base()], dir.path(), &signer);

        // An index without sets gives an empty catalog and asks for nothing.
        Publish::new(1).put(&server, &signer);
        let snapshot = content.refresh().await.unwrap();
        assert!(
            content
                .mod_sets(&snapshot.index)
                .await
                .unwrap()
                .sets
                .is_empty()
        );

        let mut publish = Publish::new(2);
        let catalog = sample_mod_sets(sample_variant(None, "26.3", &[("mods/zoom.jar", b"z")]));
        publish.mod_sets = Some(catalog.clone());
        let published = publish.put(&server, &signer);
        let snapshot = content.refresh().await.unwrap();
        let doc = snapshot.index.mod_sets.clone().unwrap();
        assert_eq!(content.mod_sets(&snapshot.index).await.unwrap(), catalog);
        content.mod_sets(&snapshot.index).await.unwrap();
        assert_eq!(server.hits(&format!("/v1/{}", doc.path)), 1, "cached");
        assert_eq!(published.index.mod_sets, Some(doc.clone()));

        // Garbage collection keeps the catalog of the current index.
        content.gc().await.unwrap();
        assert!(
            dir.path()
                .join(DOCS_DIR)
                .join(format!("{}.json", doc.sha256))
                .is_file()
        );
    }

    #[test]
    fn mirror_bases_are_normalized() {
        assert_eq!(
            parse_base("https://cdn.projects.gg/v1").unwrap().as_str(),
            "https://cdn.projects.gg/v1/"
        );
        assert_eq!(
            parse_base("https://x.example/v1/?q=1#f").unwrap().as_str(),
            "https://x.example/v1/"
        );
        assert!(parse_base("ftp://x/v1/").is_err());
        assert!(ContentClient::new(test_http(), &["https://x/v1/"], "cache", Vec::new()).is_err());
        let empty: [&str; 0] = [];
        assert!(
            ContentClient::new(test_http(), &empty, "cache", vec![key(1).verifying_key()]).is_err()
        );
    }

    #[test]
    fn signature_helpers_are_consistent() {
        // The cache stores the signature next to the exact index text.
        let signer = key(9);
        let text = "{\"schema\":1}";
        let sig = sign_index(text.as_bytes(), &signer);
        verify_index(text.as_bytes(), &sig, &[signer.verifying_key()]).unwrap();
        assert_eq!(ROOT_INDEX_SCHEMA, 1);
    }
}
