//! FABRIC-PART: the wait for Fabric API before a start. A fixed timeout that dropped the download
//! killed one that was still moving and left its `.part` file in the player's `mods/`. Here the
//! wait follows progress (bytes), ends a stalled or overlong download through its cancel token (so
//! the downloader cleans up), and sweeps what is left of Fabric API's parts.

use std::future::Future;
use std::path::Path;
use std::sync::Arc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::time::Duration;

use pl_net::CancellationToken;
use tokio::time::Instant;

/// How long a wait lasts.
#[derive(Debug, Clone, Copy)]
pub struct Limits {
    /// No new byte (and no finished step) for this long: the download is stuck.
    pub stall: Duration,
    /// The whole wait never goes past this, however steadily it moves.
    pub total: Duration,
    /// How often progress is looked at.
    pub poll: Duration,
}

/// How a watched job ended.
#[derive(Debug, PartialEq, Eq)]
pub enum Watched<T> {
    Done(T),
    Stalled,
    TooLong,
}

/// Runs `job` (given a token to stop it and a byte counter to bump as it progresses) until it
/// finishes, stalls or runs past the total. A job ended by the watch is cancelled and awaited, so
/// it cleans up after itself; the outer `cancel` (the launch's) stops it the same way.
pub async fn watch<T, F, Fut>(cancel: &CancellationToken, limits: Limits, job: F) -> Watched<T>
where
    F: FnOnce(CancellationToken, Arc<AtomicU64>) -> Fut,
    Fut: Future<Output = T>,
{
    let stop = cancel.child_token();
    let progress = Arc::new(AtomicU64::new(0));
    let mut job = std::pin::pin!(job(stop.clone(), progress.clone()));
    let started = Instant::now();
    let mut last_change = started;
    let mut seen = 0u64;
    loop {
        tokio::select! {
            out = &mut job => return Watched::Done(out),
            () = tokio::time::sleep(limits.poll) => {}
        }
        let now = Instant::now();
        let bytes = progress.load(Ordering::Relaxed);
        if bytes != seen {
            seen = bytes;
            last_change = now;
        }
        let verdict = if now.duration_since(started) >= limits.total {
            Some(Watched::TooLong)
        } else if now.duration_since(last_change) >= limits.stall {
            Some(Watched::Stalled)
        } else {
            None
        };
        if let Some(verdict) = verdict {
            stop.cancel();
            // The job notices the token and removes its own part; a job that never does is let go.
            let _ = tokio::time::timeout(Duration::from_secs(2), &mut job).await;
            return verdict;
        }
    }
}

/// Removes the unfinished downloads of Fabric API in `mods` (`fabric-api*.part`, any version): the
/// game ignores them, but they are junk in the player's folder. Answers how many went.
pub fn sweep_parts(mods: &Path) -> usize {
    let Ok(entries) = std::fs::read_dir(mods) else {
        return 0;
    };
    let mut removed = 0;
    for entry in entries.flatten() {
        let name = entry.file_name().to_string_lossy().to_ascii_lowercase();
        if name.starts_with("fabric-api")
            && name.ends_with(".part")
            && entry.file_type().is_ok_and(|kind| kind.is_file())
            && std::fs::remove_file(entry.path()).is_ok()
        {
            removed += 1;
        }
    }
    removed
}

#[cfg(test)]
mod tests {
    use super::*;

    fn limits(stall: u64, total: u64) -> Limits {
        Limits {
            stall: Duration::from_millis(stall),
            total: Duration::from_millis(total),
            poll: Duration::from_millis(10),
        }
    }

    #[tokio::test]
    async fn a_finished_job_is_done() {
        let out = watch(&CancellationToken::new(), limits(200, 1000), |_, _| async {
            7
        })
        .await;
        assert_eq!(out, Watched::Done(7));
    }

    #[tokio::test]
    async fn a_moving_job_outlives_the_stall_limit() {
        let out = watch(
            &CancellationToken::new(),
            limits(60, 2000),
            |_, bytes| async move {
                for _ in 0..15 {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                    bytes.fetch_add(1, Ordering::Relaxed);
                }
                "ok"
            },
        )
        .await;
        assert_eq!(out, Watched::Done("ok"));
    }

    #[tokio::test]
    async fn a_silent_job_stalls_and_is_cancelled() {
        let cleaned = Arc::new(AtomicU64::new(0));
        let flag = cleaned.clone();
        let out = watch(
            &CancellationToken::new(),
            limits(50, 2000),
            |stop, _| async move {
                stop.cancelled().await;
                flag.store(1, Ordering::Relaxed);
            },
        )
        .await;
        assert_eq!(out, Watched::Stalled);
        assert_eq!(cleaned.load(Ordering::Relaxed), 1);
    }

    #[tokio::test]
    async fn a_steady_but_endless_job_hits_the_total() {
        let out = watch(
            &CancellationToken::new(),
            limits(100, 120),
            |stop, bytes| async move {
                loop {
                    tokio::select! {
                        () = stop.cancelled() => return,
                        () = tokio::time::sleep(Duration::from_millis(10)) => {
                            bytes.fetch_add(1, Ordering::Relaxed);
                        }
                    }
                }
            },
        )
        .await;
        assert_eq!(out, Watched::TooLong);
    }

    #[tokio::test]
    async fn the_launch_cancel_reaches_the_job() {
        let cancel = CancellationToken::new();
        let stopper = cancel.clone();
        tokio::spawn(async move {
            tokio::time::sleep(Duration::from_millis(30)).await;
            stopper.cancel();
        });
        let out = watch(&cancel, limits(500, 2000), |stop, _| async move {
            stop.cancelled().await;
            "cancelled"
        })
        .await;
        assert_eq!(out, Watched::Done("cancelled"));
    }

    #[test]
    fn sweep_removes_only_fabric_api_parts() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "fabric-api-0.155.3+26.1.2.jar.part",
            "Fabric-API-0.1.jar.part",
            "fabric-api-0.155.3+26.1.2.jar",
            "sodium.jar.part",
            "notes.txt",
        ] {
            std::fs::write(dir.path().join(name), b"x").unwrap();
        }
        assert_eq!(sweep_parts(dir.path()), 2);
        assert!(dir.path().join("fabric-api-0.155.3+26.1.2.jar").is_file());
        assert!(dir.path().join("sodium.jar.part").is_file());
        assert!(
            !dir.path()
                .join("fabric-api-0.155.3+26.1.2.jar.part")
                .exists()
        );
        assert_eq!(sweep_parts(&dir.path().join("missing")), 0);
    }
}
