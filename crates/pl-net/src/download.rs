use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures::StreamExt;
use reqwest::StatusCode;
use tokio::io::{AsyncSeekExt, AsyncWriteExt};
use tokio_util::sync::CancellationToken;

use crate::client::{Http, backoff};
use crate::error::{NetError, Result, looks_offline};
use crate::hash::{Checksum, Hasher, verify_file};

/// How much to trust files that already exist on disk.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum VerifyMode {
    /// Re-hash existing files when a checksum is known. Safest; default for installs.
    #[default]
    Full,
    /// Trust existing files whose size matches. Fast repeat launches.
    Size,
    /// Trust any existing file. Offline launches.
    Exists,
}

/// One file to fetch. `urls` are tried in order (primary first, then mirrors).
#[derive(Debug, Clone)]
pub struct DownloadItem {
    pub urls: Vec<String>,
    pub dest: PathBuf,
    pub checksum: Option<Checksum>,
    pub size: Option<u64>,
    pub executable: bool,
}

impl DownloadItem {
    pub fn new(url: impl Into<String>, dest: impl Into<PathBuf>) -> Self {
        Self {
            urls: vec![url.into()],
            dest: dest.into(),
            checksum: None,
            size: None,
            executable: false,
        }
    }

    pub fn with_urls(urls: Vec<String>, dest: impl Into<PathBuf>) -> Self {
        Self {
            urls,
            dest: dest.into(),
            checksum: None,
            size: None,
            executable: false,
        }
    }

    pub fn mirror(mut self, url: impl Into<String>) -> Self {
        self.urls.push(url.into());
        self
    }

    pub fn checksum(mut self, checksum: Option<Checksum>) -> Self {
        self.checksum = checksum;
        self
    }

    pub fn sha1(self, hex: impl AsRef<str>) -> Self {
        self.checksum(Some(Checksum::sha1(hex)))
    }

    pub fn size(mut self, size: Option<u64>) -> Self {
        self.size = size;
        self
    }

    pub fn executable(mut self, executable: bool) -> Self {
        self.executable = executable;
        self
    }
}

/// Aggregated progress for a batch. Emitted at most every `progress_interval`.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "camelCase")]
pub struct DownloadProgress {
    pub total_files: usize,
    pub done_files: usize,
    pub total_bytes: u64,
    pub done_bytes: u64,
    pub bytes_per_sec: u64,
}

pub type ProgressFn = Arc<dyn Fn(DownloadProgress) + Send + Sync>;

#[derive(Clone)]
pub struct DownloadOptions {
    pub concurrency: usize,
    pub verify: VerifyMode,
    pub cancel: CancellationToken,
    pub progress: Option<ProgressFn>,
    pub progress_interval: Duration,
}

impl Default for DownloadOptions {
    fn default() -> Self {
        Self {
            concurrency: 16,
            verify: VerifyMode::Full,
            cancel: CancellationToken::new(),
            progress: None,
            progress_interval: Duration::from_millis(120),
        }
    }
}

impl std::fmt::Debug for DownloadOptions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DownloadOptions")
            .field("concurrency", &self.concurrency)
            .field("verify", &self.verify)
            .field("has_progress", &self.progress.is_some())
            .finish()
    }
}

/// Runs `future` until it finishes or `cancel` fires. A cancelled future is dropped (a request
/// in flight is aborted) and [`NetError::Cancelled`] comes back. For single requests (metadata,
/// listings) that do not take a token themselves; downloads check the token on their own.
pub async fn until_cancelled<T, E>(
    cancel: &CancellationToken,
    future: impl std::future::Future<Output = std::result::Result<T, E>>,
) -> std::result::Result<T, E>
where
    E: From<NetError>,
{
    tokio::select! {
        biased;
        () = cancel.cancelled() => Err(NetError::Cancelled.into()),
        result = future => result,
    }
}

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DownloadSummary {
    pub total: usize,
    pub downloaded: usize,
    pub skipped: usize,
    pub bytes: u64,
}

#[derive(Debug, Clone)]
pub struct Downloader {
    http: Http,
    /// A file a cancel or a dropped connection interrupted keeps what it got (its `.part` file).
    keep_partial: bool,
    /// Tests only: a write that would take a file past this many bytes fails as if the disk were
    /// full.
    #[cfg(test)]
    disk_full_after: Option<u64>,
}

struct Counters {
    total_files: usize,
    done_files: AtomicUsize,
    total_bytes: AtomicU64,
    done_bytes: AtomicU64,
}

impl Counters {
    fn snapshot(&self, bytes_per_sec: u64) -> DownloadProgress {
        DownloadProgress {
            total_files: self.total_files,
            done_files: self.done_files.load(Ordering::Relaxed),
            total_bytes: self.total_bytes.load(Ordering::Relaxed),
            done_bytes: self.done_bytes.load(Ordering::Relaxed),
            bytes_per_sec,
        }
    }
}

enum Outcome {
    Skipped,
    Downloaded(u64),
}

impl Downloader {
    pub fn new(http: Http) -> Self {
        Self {
            http,
            keep_partial: false,
            #[cfg(test)]
            disk_full_after: None,
        }
    }

    /// The same downloader for background downloads: a file a cancel or a dropped connection
    /// interrupts keeps what it got, and the next attempt (or the player's own launch) goes on
    /// from there. Any downloader resumes such a file when the whole file can be checked.
    pub fn keeping_partial_files(mut self) -> Self {
        self.keep_partial = true;
        self
    }

    pub fn http(&self) -> &Http {
        &self.http
    }

    /// Download a single item. Returns `true` if it was fetched, `false` if the
    /// existing file was already valid.
    pub async fn download(&self, item: DownloadItem, options: &DownloadOptions) -> Result<bool> {
        let summary = self.download_all(vec![item], options).await?;
        Ok(summary.downloaded == 1)
    }

    /// Download many items in parallel. Items sharing a destination are fetched once.
    /// Fails with [`NetError::Batch`] if any item could not be fetched, after all
    /// other items have been attempted, so a retry only redoes the failures.
    ///
    /// A full disk stops the batch at once instead ([`NetError::is_disk_full`], the I/O error
    /// itself): every other file would fail the same way, each after a request of its own. The
    /// files already finished stay (verified, skipped by the next attempt) and partial files go.
    pub async fn download_all(
        &self,
        items: Vec<DownloadItem>,
        options: &DownloadOptions,
    ) -> Result<DownloadSummary> {
        let mut seen = HashSet::new();
        let items: Vec<DownloadItem> = items
            .into_iter()
            .filter(|item| seen.insert(item.dest.clone()))
            .collect();

        let counters = Arc::new(Counters {
            total_files: items.len(),
            done_files: AtomicUsize::new(0),
            total_bytes: AtomicU64::new(items.iter().filter_map(|i| i.size).sum()),
            done_bytes: AtomicU64::new(0),
        });

        // Stops with the batch, and at once when the batch is cancelled (no progress after that).
        let ticker_stop = options.cancel.child_token();
        let ticker = options.progress.clone().map(|progress| {
            tokio::spawn(run_ticker(
                Arc::clone(&counters),
                progress,
                options.progress_interval,
                ticker_stop.clone(),
            ))
        });

        // The files' own token: a full disk stops the files still running or queued, while
        // the caller's token keeps its meaning (a cancel of the whole batch).
        let disk_full = options.cancel.child_token();
        let file_options = DownloadOptions {
            cancel: disk_full.clone(),
            ..options.clone()
        };

        // Futures are built eagerly: a mapping closure inside the stream would make this
        // future higher-ranked and not `Send` for callers that spawn it (Tauri commands).
        let jobs: Vec<_> = items
            .iter()
            .map(|item| {
                let counters = Arc::clone(&counters);
                let file_options = &file_options;
                let disk_full = &disk_full;
                async move {
                    let outcome = self.fetch_one(item, file_options, &counters).await;
                    if outcome.as_ref().is_err_and(NetError::is_disk_full) {
                        disk_full.cancel();
                    }
                    (item.dest.clone(), outcome)
                }
            })
            .collect();
        let mut results: Vec<(PathBuf, Result<Outcome>)> = futures::stream::iter(jobs)
            .buffer_unordered(options.concurrency.max(1))
            .collect()
            .await;

        ticker_stop.cancel();
        if let Some(handle) = ticker {
            let _ = handle.await;
        }
        if options.cancel.is_cancelled() {
            return Err(NetError::Cancelled);
        }
        if let Some(progress) = &options.progress {
            progress(counters.snapshot(0));
        }
        // The files it stopped report a cancel; the batch reports the full disk.
        let full = results
            .iter()
            .position(|(_, result)| result.as_ref().is_err_and(NetError::is_disk_full));
        if let Some(index) = full
            && let (dest, Err(error)) = results.swap_remove(index)
        {
            tracing::warn!(dest = %dest.display(), %error, "downloads stopped: the disk is full");
            return Err(error);
        }

        let mut summary = DownloadSummary {
            total: items.len(),
            ..Default::default()
        };
        let mut failures = Vec::new();
        for (dest, result) in results {
            match result {
                Ok(Outcome::Skipped) => summary.skipped += 1,
                Ok(Outcome::Downloaded(bytes)) => {
                    summary.downloaded += 1;
                    summary.bytes += bytes;
                }
                Err(NetError::Cancelled) => return Err(NetError::Cancelled),
                Err(e) => {
                    tracing::warn!(dest = %dest.display(), error = %e, "download failed");
                    failures.push(e);
                }
            }
        }

        if let Some(first) = failures.first() {
            let offline = failures
                .iter()
                .all(|e| e.is_network() || looks_offline(&e.to_string()));
            return Err(NetError::Batch {
                failed: failures.len(),
                total: summary.total,
                first: first.to_string(),
                offline,
            });
        }
        Ok(summary)
    }

    async fn fetch_one(
        &self,
        item: &DownloadItem,
        options: &DownloadOptions,
        counters: &Counters,
    ) -> Result<Outcome> {
        // Hashing a big existing file (a Java runtime's `lib/modules`) must not hold up a cancel:
        // the check is dropped (it only reads).
        let valid = tokio::select! {
            biased;
            () = options.cancel.cancelled() => return Err(NetError::Cancelled),
            valid = verify_file(&item.dest, item.checksum.as_ref(), item.size, options.verify) => valid,
        };
        if valid {
            if let Some(size) = item.size {
                counters.done_bytes.fetch_add(size, Ordering::Relaxed);
            }
            counters.done_files.fetch_add(1, Ordering::Relaxed);
            return Ok(Outcome::Skipped);
        }
        if item.urls.is_empty() {
            return Err(NetError::NoSource(item.dest.clone()));
        }

        let mut size_accounted = item.size.is_some();
        let mut last_error = None;
        for url in &item.urls {
            for attempt in 0..=self.http.retries() {
                if options.cancel.is_cancelled() {
                    return Err(NetError::Cancelled);
                }
                match self
                    .stream_to_file(url, item, options, counters, &mut size_accounted)
                    .await
                {
                    Ok(bytes) => {
                        counters.done_files.fetch_add(1, Ordering::Relaxed);
                        return Ok(Outcome::Downloaded(bytes));
                    }
                    Err(NetError::Cancelled) => return Err(NetError::Cancelled),
                    // No mirror can write to a full disk.
                    Err(error) if error.is_disk_full() => return Err(error),
                    Err(error) => {
                        let transient = is_transient(&error);
                        tracing::debug!(%url, attempt, %error, transient, "download attempt failed");
                        last_error = Some(error);
                        if !transient {
                            break; // try the next mirror right away
                        }
                        tokio::select! {
                            _ = options.cancel.cancelled() => return Err(NetError::Cancelled),
                            _ = tokio::time::sleep(backoff(attempt)) => {}
                        }
                    }
                }
            }
        }
        Err(last_error.unwrap_or_else(|| NetError::NoSource(item.dest.clone())))
    }

    async fn stream_to_file(
        &self,
        url: &str,
        item: &DownloadItem,
        options: &DownloadOptions,
        counters: &Counters,
        size_accounted: &mut bool,
    ) -> Result<u64> {
        let request_error = |source| NetError::Request {
            url: url.to_owned(),
            source,
        };
        let tmp = part_path(&item.dest);
        // What an interrupted download left goes on where it stopped, when the whole file can be
        // checked at the end (a part that does not belong to this file fails the check and goes).
        let mut resume_from = match &item.checksum {
            Some(_) => partial_len(&tmp, item.size).await,
            None => 0,
        };
        let mut response = tokio::select! {
            _ = options.cancel.cancelled() => return Err(NetError::Cancelled),
            r = self.request(url, resume_from) => r.map_err(request_error)?,
        };
        if resume_from > 0 && !resumes(&response, resume_from) {
            let status = response.status();
            resume_from = 0;
            if status == StatusCode::PARTIAL_CONTENT || status == StatusCode::RANGE_NOT_SATISFIABLE
            {
                // The part does not fit the file the server has: start over without it.
                tracing::debug!(%url, status = status.as_u16(), "partial download not resumable");
                let _ = tokio::fs::remove_file(&tmp).await;
                response = tokio::select! {
                    _ = options.cancel.cancelled() => return Err(NetError::Cancelled),
                    r = self.request(url, 0) => r.map_err(request_error)?,
                };
            }
            // Otherwise the whole file came (no range support) and replaces the part, or an
            // error status is reported below and the part waits for the next attempt.
        }
        if !response.status().is_success() {
            return Err(NetError::Status {
                url: url.to_owned(),
                status: response.status().as_u16(),
            });
        }
        if !*size_accounted && let Some(len) = response.content_length() {
            counters
                .total_bytes
                .fetch_add(len + resume_from, Ordering::Relaxed);
            *size_accounted = true;
        }

        if let Some(parent) = item.dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| NetError::io(parent, e))?;
        }
        let mut written = 0u64;

        let result: Result<()> = async {
            let mut hasher = item.checksum.as_ref().map(|c| Hasher::new(c.algo));
            let mut file = if resume_from > 0 {
                // The part's bytes are checked again with the rest at the end.
                if let Some(fresh) = hasher.take() {
                    hasher = Some(hash_prefix(&tmp, resume_from, fresh).await?);
                }
                let mut file = tokio::fs::OpenOptions::new()
                    .write(true)
                    .open(&tmp)
                    .await
                    .map_err(|e| NetError::io(&tmp, e))?;
                file.set_len(resume_from)
                    .await
                    .map_err(|e| NetError::io(&tmp, e))?;
                file.seek(std::io::SeekFrom::End(0))
                    .await
                    .map_err(|e| NetError::io(&tmp, e))?;
                written = resume_from;
                counters
                    .done_bytes
                    .fetch_add(resume_from, Ordering::Relaxed);
                tracing::debug!(dest = %item.dest.display(), from = resume_from, "download resumed");
                file
            } else {
                tokio::fs::File::create(&tmp)
                    .await
                    .map_err(|e| NetError::io(&tmp, e))?
            };
            let mut stream = response.bytes_stream();
            loop {
                let next = tokio::select! {
                    _ = options.cancel.cancelled() => {
                        // What was written stays whole for the next attempt.
                        if self.keep_partial {
                            let _ = tokio::time::timeout(KEEP_FLUSH, file.flush()).await;
                        }
                        return Err(NetError::Cancelled);
                    }
                    next = stream.next() => next,
                };
                let Some(chunk) = next else { break };
                let chunk = chunk.map_err(request_error)?;
                if let Some(hasher) = hasher.as_mut() {
                    hasher.update(&chunk);
                }
                #[cfg(test)]
                if self
                    .disk_full_after
                    .is_some_and(|limit| written + chunk.len() as u64 > limit)
                {
                    let full = std::io::Error::from(std::io::ErrorKind::StorageFull);
                    return Err(NetError::io(&tmp, full));
                }
                // A slow disk (a busy USB drive) must not hold up a cancel either (a background
                // download still lets the write in progress end, so its part stays whole).
                let cancelled = tokio::select! {
                    biased;
                    _ = options.cancel.cancelled() => true,
                    result = file.write_all(&chunk) => {
                        result.map_err(|e| NetError::io(&tmp, e))?;
                        false
                    }
                };
                if cancelled {
                    if self.keep_partial {
                        let _ = tokio::time::timeout(KEEP_FLUSH, file.flush()).await;
                    }
                    return Err(NetError::Cancelled);
                }
                written += chunk.len() as u64;
                counters
                    .done_bytes
                    .fetch_add(chunk.len() as u64, Ordering::Relaxed);
            }
            file.flush().await.map_err(|e| NetError::io(&tmp, e))?;
            drop(file);

            if let Some(expected) = item.size
                && expected != written
            {
                return Err(NetError::Size {
                    path: item.dest.clone(),
                    expected,
                    actual: written,
                });
            }
            if let (Some(sum), Some(hasher)) = (&item.checksum, hasher) {
                let actual = hasher.finalize_hex();
                if actual != sum.hex {
                    return Err(NetError::Checksum {
                        path: item.dest.clone(),
                        expected: sum.hex.clone(),
                        actual,
                    });
                }
            }
            if item.executable {
                set_executable(&tmp).await?;
            }
            tokio::fs::rename(&tmp, &item.dest)
                .await
                .map_err(|e| NetError::io(&item.dest, e))
        }
        .await;

        if let Err(error) = result {
            counters.done_bytes.fetch_sub(written, Ordering::Relaxed);
            // A background download keeps what a cancel or a dropped connection interrupted
            // (only when the whole file can be checked once it is complete).
            let keep = self.keep_partial
                && written > 0
                && item.checksum.is_some()
                && matches!(error, NetError::Cancelled | NetError::Request { .. });
            if !keep {
                let _ = tokio::fs::remove_file(&tmp).await;
            }
            return Err(error);
        }
        Ok(written)
    }

    /// A GET, asking for the rest of the file from `from` on (and the bytes as stored, so the
    /// range means the same as the part on disk) when there is a part to go on with.
    async fn request(
        &self,
        url: &str,
        from: u64,
    ) -> std::result::Result<reqwest::Response, reqwest::Error> {
        let mut request = self.http.client().get(url);
        if from > 0 {
            request = request
                .header(reqwest::header::RANGE, format!("bytes={from}-"))
                .header(reqwest::header::ACCEPT_ENCODING, "identity");
        }
        request.send().await
    }
}

/// How long a cancelled background download may take to write out what it already got.
const KEEP_FLUSH: Duration = Duration::from_secs(2);

/// The bytes an interrupted download left in its part file: some, and fewer than the whole file.
async fn partial_len(part: &Path, size: Option<u64>) -> u64 {
    match tokio::fs::metadata(part).await {
        Ok(meta)
            if meta.is_file() && meta.len() > 0 && size.is_none_or(|size| meta.len() < size) =>
        {
            meta.len()
        }
        _ => 0,
    }
}

/// The server sends the rest of the file from `from` on.
fn resumes(response: &reqwest::Response, from: u64) -> bool {
    response.status() == StatusCode::PARTIAL_CONTENT
        && response
            .headers()
            .get(reqwest::header::CONTENT_RANGE)
            .and_then(|value| value.to_str().ok())
            .and_then(range_start)
            == Some(from)
}

/// The first byte of a `Content-Range` value (`bytes 1000-4999/5000`).
fn range_start(value: &str) -> Option<u64> {
    let rest = value.trim().strip_prefix("bytes")?.trim_start();
    rest.split('-').next()?.trim().parse().ok()
}

/// `hasher` fed with the first `len` bytes of `part` (a resumed download), on the blocking pool.
async fn hash_prefix(part: &Path, len: u64, mut hasher: Hasher) -> Result<Hasher> {
    use std::io::Read;
    let path = part.to_path_buf();
    tokio::task::spawn_blocking(move || {
        let file = std::fs::File::open(&path).map_err(|e| NetError::io(&path, e))?;
        let mut reader = file.take(len);
        let mut buf = vec![0u8; 256 * 1024];
        loop {
            let n = reader.read(&mut buf).map_err(|e| NetError::io(&path, e))?;
            if n == 0 {
                break;
            }
            hasher.update(&buf[..n]);
        }
        Ok(hasher)
    })
    .await
    .map_err(|e| NetError::Setup(format!("hash task panicked: {e}")))?
}

/// Network hiccups, server errors and corrupted transfers are worth retrying;
/// 404s and local I/O errors are not.
fn is_transient(error: &NetError) -> bool {
    match error {
        NetError::Request { .. } => true,
        NetError::Status { status, .. } => *status >= 500 || *status == 429 || *status == 408,
        NetError::Checksum { .. } | NetError::Size { .. } => true,
        _ => false,
    }
}

fn part_path(dest: &Path) -> PathBuf {
    let mut name = dest
        .file_name()
        .map(|n| n.to_os_string())
        .unwrap_or_default();
    name.push(".part");
    dest.with_file_name(name)
}

#[cfg(unix)]
async fn set_executable(path: &Path) -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    tokio::fs::set_permissions(path, std::fs::Permissions::from_mode(0o755))
        .await
        .map_err(|e| NetError::io(path, e))
}

#[cfg(not(unix))]
async fn set_executable(_path: &Path) -> Result<()> {
    Ok(())
}

async fn run_ticker(
    counters: Arc<Counters>,
    progress: ProgressFn,
    interval: Duration,
    stop: CancellationToken,
) {
    let mut last_bytes = 0u64;
    let mut last_at = Instant::now();
    let mut speed = 0f64;
    let mut last_emitted = None;
    loop {
        tokio::select! {
            _ = stop.cancelled() => break,
            _ = tokio::time::sleep(interval) => {}
        }
        let now = Instant::now();
        let bytes = counters.done_bytes.load(Ordering::Relaxed);
        let elapsed = now.duration_since(last_at).as_secs_f64();
        if elapsed > 0.0 {
            let instant = bytes.saturating_sub(last_bytes) as f64 / elapsed;
            // Exponential moving average keeps the speed readout calm.
            speed = if speed == 0.0 {
                instant
            } else {
                speed * 0.75 + instant * 0.25
            };
        }
        last_bytes = bytes;
        last_at = now;
        let snapshot = counters.snapshot(speed as u64);
        if last_emitted != Some(snapshot) {
            progress(snapshot);
            last_emitted = Some(snapshot);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn part_path_appends_suffix() {
        assert_eq!(
            part_path(Path::new("/a/b/client.jar")),
            PathBuf::from("/a/b/client.jar.part")
        );
        assert_eq!(
            part_path(Path::new("/a/b/obj")),
            PathBuf::from("/a/b/obj.part")
        );
    }

    #[test]
    fn transient_classification() {
        assert!(is_transient(&NetError::Status {
            url: String::new(),
            status: 503
        }));
        assert!(!is_transient(&NetError::Status {
            url: String::new(),
            status: 404
        }));
        assert!(!is_transient(&NetError::NoSource(PathBuf::new())));
    }

    #[tokio::test]
    async fn existing_valid_files_are_skipped() {
        let dir = tempfile::tempdir().unwrap();
        let dest = dir.path().join("abc.txt");
        std::fs::write(&dest, b"abc").unwrap();
        let http = Http::new(crate::HttpConfig::new("pl-net-test")).unwrap();
        let downloader = Downloader::new(http);
        // Unreachable URL: the item must be satisfied from disk without any request.
        let item = DownloadItem::new("http://127.0.0.1:9/never", &dest)
            .sha1("a9993e364706816aba3e25717850c26c9cd0d89d")
            .size(Some(3));
        let summary = downloader
            .download_all(vec![item.clone(), item], &DownloadOptions::default())
            .await
            .unwrap();
        assert_eq!(summary.total, 1, "duplicate destinations are merged");
        assert_eq!(summary.skipped, 1);
        assert_eq!(summary.downloaded, 0);
    }

    /// A local HTTP server that answers every request with `size` bytes, sent slowly (1 KiB
    /// every 20 ms), and counts the requests it got.
    async fn slow_server(size: usize) -> (String, Arc<AtomicUsize>) {
        use tokio::io::AsyncReadExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let requests = Arc::new(AtomicUsize::new(0));
        let counter = Arc::clone(&requests);
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let counter = Arc::clone(&counter);
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => head.extend_from_slice(&buf[..n]),
                        }
                    }
                    counter.fetch_add(1, Ordering::SeqCst);
                    let header = format!(
                        "HTTP/1.1 200 OK\r\nContent-Length: {size}\r\nContent-Type: application/octet-stream\r\n\r\n"
                    );
                    if socket.write_all(header.as_bytes()).await.is_err() {
                        return;
                    }
                    let chunk = [b'x'; 1024];
                    let mut sent = 0;
                    while sent < size {
                        let n = chunk.len().min(size - sent);
                        if socket.write_all(&chunk[..n]).await.is_err() {
                            return;
                        }
                        sent += n;
                        tokio::time::sleep(Duration::from_millis(20)).await;
                    }
                });
            }
        });
        (format!("http://{address}"), requests)
    }

    fn leftovers(dir: &Path) -> Vec<String> {
        std::fs::read_dir(dir)
            .unwrap()
            .flatten()
            .map(|e| e.file_name().to_string_lossy().into_owned())
            .collect()
    }

    /// Cancelling a running batch: the request in flight is aborted, queued files never start,
    /// the partial file goes, no progress is reported afterwards, and it all takes well under a
    /// second.
    #[tokio::test]
    async fn cancelling_stops_a_batch_at_once() {
        let dir = tempfile::tempdir().unwrap();
        let size = 4 * 1024 * 1024;
        let (base, requests) = slow_server(size).await;
        let items: Vec<DownloadItem> = (0..4)
            .map(|i| {
                DownloadItem::new(
                    format!("{base}/file{i}"),
                    dir.path().join(format!("file{i}")),
                )
                .size(Some(size as u64))
            })
            .collect();
        let downloader = Downloader::new(Http::new(crate::HttpConfig::new("pl-net-test")).unwrap());
        let cancel = CancellationToken::new();
        let reports = Arc::new(parking_lot::Mutex::new(Vec::<Instant>::new()));
        let seen = Arc::clone(&reports);
        let options = DownloadOptions {
            concurrency: 1,
            cancel: cancel.clone(),
            progress: Some(Arc::new(move |_| seen.lock().push(Instant::now()))),
            progress_interval: Duration::from_millis(20),
            ..DownloadOptions::default()
        };
        let run = tokio::spawn({
            let downloader = downloader.clone();
            async move { downloader.download_all(items, &options).await }
        });
        // Wait until the first file is being written.
        let part = dir.path().join("file0.part");
        let deadline = Instant::now() + Duration::from_secs(10);
        while !(part.is_file() && std::fs::metadata(&part).unwrap().len() > 0) {
            assert!(Instant::now() < deadline, "the download never started");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        let cancelled_at = Instant::now();
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("the batch kept running")
            .unwrap();
        let took = cancelled_at.elapsed();
        assert!(matches!(result, Err(NetError::Cancelled)), "{result:?}");
        assert!(took < Duration::from_secs(1), "cancel took {took:?}");
        // Give a stray request or tick the chance to show up.
        tokio::time::sleep(Duration::from_millis(150)).await;
        assert_eq!(requests.load(Ordering::SeqCst), 1, "queued files started");
        assert!(
            leftovers(dir.path()).is_empty(),
            "{:?}",
            leftovers(dir.path())
        );
        assert!(
            reports.lock().iter().all(|at| *at <= cancelled_at + took),
            "progress was reported after the cancel"
        );
    }

    /// A batch cancelled before it starts makes no request at all.
    #[tokio::test]
    async fn a_cancelled_batch_makes_no_requests() {
        let dir = tempfile::tempdir().unwrap();
        let (base, requests) = slow_server(1024).await;
        let downloader = Downloader::new(Http::new(crate::HttpConfig::new("pl-net-test")).unwrap());
        let cancel = CancellationToken::new();
        cancel.cancel();
        let options = DownloadOptions {
            cancel,
            ..DownloadOptions::default()
        };
        let items = vec![DownloadItem::new(format!("{base}/a"), dir.path().join("a"))];
        let result = downloader.download_all(items, &options).await;
        assert!(matches!(result, Err(NetError::Cancelled)), "{result:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(requests.load(Ordering::SeqCst), 0);
        assert!(leftovers(dir.path()).is_empty());
    }

    /// A full disk stops a batch at once: the file being written goes, its mirror and the queued
    /// files are never asked for, files finished before stay, and the batch reports the full disk
    /// itself rather than a batch of failures.
    #[tokio::test]
    async fn a_full_disk_stops_the_batch() {
        let dir = tempfile::tempdir().unwrap();
        let size = 16 * 1024;
        let (base, requests) = slow_server(size).await;
        let done = dir.path().join("done");
        std::fs::write(&done, vec![b'x'; size]).unwrap();
        let mut items = vec![
            DownloadItem::new(format!("{base}/done"), &done).size(Some(size as u64)),
            DownloadItem::new(format!("{base}/file0"), dir.path().join("file0"))
                .mirror(format!("{base}/mirror0"))
                .size(Some(size as u64)),
        ];
        items.extend((1..4).map(|i| {
            DownloadItem::new(
                format!("{base}/file{i}"),
                dir.path().join(format!("file{i}")),
            )
            .size(Some(size as u64))
        }));
        let mut downloader =
            Downloader::new(Http::new(crate::HttpConfig::new("pl-net-test")).unwrap());
        downloader.disk_full_after = Some(4 * 1024);
        let options = DownloadOptions {
            concurrency: 1,
            ..DownloadOptions::default()
        };
        let result = tokio::time::timeout(
            Duration::from_secs(10),
            downloader.download_all(items, &options),
        )
        .await
        .expect("the batch kept running");
        let error = result.unwrap_err();
        assert!(error.is_disk_full(), "{error:?}");
        assert!(matches!(error, NetError::Io { .. }), "{error:?}");
        tokio::time::sleep(Duration::from_millis(100)).await;
        assert_eq!(
            requests.load(Ordering::SeqCst),
            1,
            "more files were asked for"
        );
        assert_eq!(leftovers(dir.path()), ["done"]);
    }

    #[test]
    fn content_ranges_are_read() {
        assert_eq!(range_start("bytes 1000-4999/5000"), Some(1000));
        assert_eq!(range_start("  bytes 0-9/*"), Some(0));
        assert_eq!(range_start("bytes */5000"), None);
        assert_eq!(range_start("items 1-2/3"), None);
    }

    /// The bytes a range test serves: not all the same, so a misplaced part shows.
    fn sample(len: usize) -> Vec<u8> {
        (0..len).map(|i| (i % 251) as u8).collect()
    }

    type Asked = Arc<parking_lot::Mutex<Vec<Option<u64>>>>;

    /// A local HTTP server for `content` that answers `Range: bytes=N-` with the rest when
    /// `ranges` is set (and with 416 past the end), and the whole file otherwise. Records the
    /// range of every request (`None` for a plain one). `slow`: 1 KiB every 20 ms.
    async fn range_server(content: Vec<u8>, ranges: bool, slow: bool) -> (String, Asked) {
        use tokio::io::AsyncReadExt;
        let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
        let address = listener.local_addr().unwrap();
        let asked: Asked = Arc::default();
        let log = Arc::clone(&asked);
        let content = Arc::new(content);
        tokio::spawn(async move {
            while let Ok((mut socket, _)) = listener.accept().await {
                let log = Arc::clone(&log);
                let content = Arc::clone(&content);
                tokio::spawn(async move {
                    let mut head = Vec::new();
                    let mut buf = [0u8; 1024];
                    while !head.windows(4).any(|w| w == b"\r\n\r\n") {
                        match socket.read(&mut buf).await {
                            Ok(0) | Err(_) => return,
                            Ok(n) => head.extend_from_slice(&buf[..n]),
                        }
                    }
                    let text = String::from_utf8_lossy(&head).to_ascii_lowercase();
                    let from = text
                        .lines()
                        .find_map(|line| line.strip_prefix("range: bytes="))
                        .and_then(|value| value.trim().trim_end_matches('-').parse::<usize>().ok());
                    log.lock().push(from.map(|from| from as u64));
                    let total = content.len();
                    let (status, range, body): (&str, Option<String>, &[u8]) = match from {
                        Some(from) if ranges && from >= total => (
                            "416 Range Not Satisfiable",
                            Some(format!("bytes */{total}")),
                            &[],
                        ),
                        Some(from) if ranges => (
                            "206 Partial Content",
                            Some(format!("bytes {from}-{}/{total}", total - 1)),
                            &content[from..],
                        ),
                        _ => ("200 OK", None, &content[..]),
                    };
                    let mut header = format!(
                        "HTTP/1.1 {status}\r\nContent-Length: {}\r\nConnection: close\r\n",
                        body.len()
                    );
                    if let Some(range) = range {
                        header.push_str(&format!("Content-Range: {range}\r\n"));
                    }
                    header.push_str("\r\n");
                    if socket.write_all(header.as_bytes()).await.is_err() {
                        return;
                    }
                    for chunk in body.chunks(1024) {
                        if socket.write_all(chunk).await.is_err() {
                            return;
                        }
                        if slow {
                            tokio::time::sleep(Duration::from_millis(20)).await;
                        }
                    }
                });
            }
        });
        (format!("http://{address}"), asked)
    }

    fn downloader() -> Downloader {
        Downloader::new(Http::new(crate::HttpConfig::new("pl-net-test")).unwrap())
    }

    fn checked_item(base: &str, content: &[u8], dest: &Path) -> DownloadItem {
        DownloadItem::new(format!("{base}/file"), dest)
            .sha1(crate::hash_bytes(content, crate::HashAlgo::Sha1))
            .size(Some(content.len() as u64))
    }

    /// A part an interrupted download left goes on from where it stopped, and the whole file is
    /// checked at the end.
    #[tokio::test]
    async fn a_part_left_behind_goes_on_where_it_stopped() {
        let dir = tempfile::tempdir().unwrap();
        let content = sample(64 * 1024);
        let (base, asked) = range_server(content.clone(), true, false).await;
        let dest = dir.path().join("file");
        std::fs::write(dir.path().join("file.part"), &content[..20_000]).unwrap();
        let summary = downloader()
            .download_all(
                vec![checked_item(&base, &content, &dest)],
                &DownloadOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(summary.downloaded, 1);
        assert_eq!(summary.bytes, content.len() as u64);
        assert_eq!(*asked.lock(), [Some(20_000)]);
        assert_eq!(std::fs::read(&dest).unwrap(), content);
        assert_eq!(leftovers(dir.path()), ["file"]);
    }

    /// A server without ranges sends the whole file, which replaces the part.
    #[tokio::test]
    async fn a_server_without_ranges_sends_the_whole_file() {
        let dir = tempfile::tempdir().unwrap();
        let content = sample(32 * 1024);
        let (base, asked) = range_server(content.clone(), false, false).await;
        let dest = dir.path().join("file");
        std::fs::write(dir.path().join("file.part"), &content[..5_000]).unwrap();
        downloader()
            .download_all(
                vec![checked_item(&base, &content, &dest)],
                &DownloadOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(*asked.lock(), [Some(5_000)]);
        assert_eq!(std::fs::read(&dest).unwrap(), content);
        assert_eq!(leftovers(dir.path()), ["file"]);
    }

    /// A part that does not belong to the file fails the check, goes, and the file comes again
    /// whole.
    #[tokio::test]
    async fn a_part_of_another_file_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let content = sample(32 * 1024);
        let (base, asked) = range_server(content.clone(), true, false).await;
        let dest = dir.path().join("file");
        std::fs::write(dir.path().join("file.part"), vec![b'?'; 10_000]).unwrap();
        downloader()
            .download_all(
                vec![checked_item(&base, &content, &dest)],
                &DownloadOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(*asked.lock(), [Some(10_000), None]);
        assert_eq!(std::fs::read(&dest).unwrap(), content);
    }

    /// A part longer than the file the server has (its size unknown here) gets 416 and goes.
    #[tokio::test]
    async fn a_part_past_the_end_starts_over() {
        let dir = tempfile::tempdir().unwrap();
        let content = sample(8 * 1024);
        let (base, asked) = range_server(content.clone(), true, false).await;
        let dest = dir.path().join("file");
        std::fs::write(dir.path().join("file.part"), sample(9 * 1024)).unwrap();
        let item = DownloadItem::new(format!("{base}/file"), &dest)
            .sha1(crate::hash_bytes(&content, crate::HashAlgo::Sha1));
        downloader()
            .download_all(vec![item], &DownloadOptions::default())
            .await
            .unwrap();
        assert_eq!(*asked.lock(), [Some(9 * 1024), None]);
        assert_eq!(std::fs::read(&dest).unwrap(), content);
    }

    /// Without a checksum a part could never be told apart from another file's: it is not used.
    #[tokio::test]
    async fn without_a_checksum_nothing_resumes() {
        let dir = tempfile::tempdir().unwrap();
        let content = sample(16 * 1024);
        let (base, asked) = range_server(content.clone(), true, false).await;
        let dest = dir.path().join("file");
        std::fs::write(dir.path().join("file.part"), &content[..4_000]).unwrap();
        let item =
            DownloadItem::new(format!("{base}/file"), &dest).size(Some(content.len() as u64));
        downloader()
            .download_all(vec![item], &DownloadOptions::default())
            .await
            .unwrap();
        assert_eq!(*asked.lock(), [None]);
        assert_eq!(std::fs::read(&dest).unwrap(), content);
    }

    /// A background download keeps what it got when it is cancelled; the next download (here an
    /// ordinary one, like the player's own launch) only asks for the rest.
    #[tokio::test]
    async fn a_background_download_keeps_its_part_for_the_next_one() {
        let dir = tempfile::tempdir().unwrap();
        let content = sample(256 * 1024);
        let (base, asked) = range_server(content.clone(), true, true).await;
        let dest = dir.path().join("file");
        let part = dir.path().join("file.part");
        let cancel = CancellationToken::new();
        let options = DownloadOptions {
            cancel: cancel.clone(),
            ..DownloadOptions::default()
        };
        let background = downloader().keeping_partial_files();
        let run = tokio::spawn({
            let item = checked_item(&base, &content, &dest);
            async move { background.download_all(vec![item], &options).await }
        });
        let deadline = Instant::now() + Duration::from_secs(10);
        while !(part.is_file() && std::fs::metadata(&part).unwrap().len() >= 8 * 1024) {
            assert!(Instant::now() < deadline, "the download never started");
            tokio::time::sleep(Duration::from_millis(10)).await;
        }
        cancel.cancel();
        let result = tokio::time::timeout(Duration::from_secs(5), run)
            .await
            .expect("the batch kept running")
            .unwrap();
        assert!(matches!(result, Err(NetError::Cancelled)), "{result:?}");
        let kept = std::fs::metadata(&part).unwrap().len();
        assert!(kept > 0 && kept < content.len() as u64, "kept {kept}");
        assert_eq!(
            &std::fs::read(&part).unwrap()[..],
            &content[..kept as usize]
        );

        downloader()
            .download_all(
                vec![checked_item(&base, &content, &dest)],
                &DownloadOptions::default(),
            )
            .await
            .unwrap();
        assert_eq!(*asked.lock(), [None, Some(kept)]);
        assert_eq!(std::fs::read(&dest).unwrap(), content);
        assert_eq!(leftovers(dir.path()), ["file"]);
    }

    /// A single request that takes no token is dropped as soon as the token fires.
    #[tokio::test]
    async fn until_cancelled_aborts_a_request() {
        let (base, requests) = slow_server(4 * 1024 * 1024).await;
        let http = Http::new(crate::HttpConfig::new("pl-net-test")).unwrap();
        let cancel = CancellationToken::new();
        let trigger = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(100)).await;
            trigger.cancel();
        });
        let started = Instant::now();
        let result = until_cancelled(&cancel, http.get_bytes(&format!("{base}/big"))).await;
        assert!(matches!(result, Err(NetError::Cancelled)), "{result:?}");
        assert!(started.elapsed() < Duration::from_secs(1));
        assert_eq!(requests.load(Ordering::SeqCst), 1);
        // A finished future wins over a token cancelled later.
        let done: Result<u8> = until_cancelled(&CancellationToken::new(), async { Ok(7) }).await;
        assert_eq!(done.unwrap(), 7);
    }
}
