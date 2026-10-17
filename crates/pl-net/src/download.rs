use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::time::{Duration, Instant};

use futures::StreamExt;
use tokio::io::AsyncWriteExt;
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
            #[cfg(test)]
            disk_full_after: None,
        }
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
        let response = tokio::select! {
            _ = options.cancel.cancelled() => return Err(NetError::Cancelled),
            r = self.http.client().get(url).send() => r.map_err(request_error)?,
        };
        if !response.status().is_success() {
            return Err(NetError::Status {
                url: url.to_owned(),
                status: response.status().as_u16(),
            });
        }
        if !*size_accounted && let Some(len) = response.content_length() {
            counters.total_bytes.fetch_add(len, Ordering::Relaxed);
            *size_accounted = true;
        }

        if let Some(parent) = item.dest.parent() {
            tokio::fs::create_dir_all(parent)
                .await
                .map_err(|e| NetError::io(parent, e))?;
        }
        let tmp = part_path(&item.dest);
        let mut written = 0u64;

        let result: Result<()> = async {
            let mut file = tokio::fs::File::create(&tmp)
                .await
                .map_err(|e| NetError::io(&tmp, e))?;
            let mut hasher = item.checksum.as_ref().map(|c| Hasher::new(c.algo));
            let mut stream = response.bytes_stream();
            loop {
                let next = tokio::select! {
                    _ = options.cancel.cancelled() => return Err(NetError::Cancelled),
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
                // A slow disk (a busy USB drive) must not hold up a cancel either.
                tokio::select! {
                    biased;
                    _ = options.cancel.cancelled() => return Err(NetError::Cancelled),
                    result = file.write_all(&chunk) => result.map_err(|e| NetError::io(&tmp, e))?,
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
            let _ = tokio::fs::remove_file(&tmp).await;
            return Err(error);
        }
        Ok(written)
    }
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
