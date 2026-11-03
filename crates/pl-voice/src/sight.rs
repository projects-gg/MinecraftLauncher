//! Whether the launcher's main window is on screen, looked at once a second at most.
//!
//! The answer comes from the window's own thread (the main thread), and that thread asks too: the
//! live levels of a synchronous command (`voice_page_report`) run on it. So the cache's lock is never
//! held while the window is asked: held across the question, it deadlocked the main thread and the
//! `voice-effects` thread a few moments into a call, and the launcher stopped answering (BATCH-206).

use std::sync::{Mutex, MutexGuard, PoisonError};
use std::time::{Duration, Instant};

/// How long one look at the window counts.
pub const SIGHT_FOR: Duration = Duration::from_secs(1);

/// The last look at the window: whether it was on screen, and when.
#[derive(Debug, Default)]
pub struct SightCache(Mutex<Option<(bool, Instant)>>);

impl SightCache {
    pub const fn new() -> Self {
        Self(Mutex::new(None))
    }

    /// The last answer while it is fresh, else `ask`'s, kept for [`SIGHT_FOR`]. `ask` runs with no
    /// lock of this cache held (two threads asking at once may both look; that is all).
    pub fn get(&self, ask: impl FnOnce() -> bool) -> bool {
        self.get_at(Instant::now(), ask)
    }

    fn get_at(&self, now: Instant, ask: impl FnOnce() -> bool) -> bool {
        let last = *self.lock();
        if let Some((seen, at)) = last
            && now.saturating_duration_since(at) < SIGHT_FOR
        {
            return seen;
        }
        let seen = ask();
        *self.lock() = Some((seen, now));
        seen
    }

    fn lock(&self) -> MutexGuard<'_, Option<(bool, Instant)>> {
        self.0.lock().unwrap_or_else(PoisonError::into_inner)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_window_is_asked_with_the_cache_free() {
        let cache = SightCache::new();
        // Another thread (here: the question itself) may look at the cache meanwhile.
        let free = cache.get(|| cache.0.try_lock().is_ok());
        assert!(free, "the cache's lock was held while the window was asked");
    }

    #[test]
    fn one_look_counts_for_a_second() {
        let cache = SightCache::new();
        let start = Instant::now();
        assert!(cache.get_at(start, || true));
        // Fresh: the window is not asked again.
        assert!(cache.get_at(start + Duration::from_millis(900), || panic!("asked again")));
        // Stale: asked again, and the new answer is kept.
        assert!(!cache.get_at(start + SIGHT_FOR, || false));
        assert!(!cache.get_at(start + SIGHT_FOR + Duration::from_millis(10), || true));
    }

    #[test]
    fn another_thread_gets_an_answer_while_one_waits_for_the_window() {
        // The deadlock's shape: one thread's question waits on another thread, which asks too.
        let cache = std::sync::Arc::new(SightCache::new());
        let (asked, waiting) = std::sync::mpsc::channel::<()>();
        let (answered, answer) = std::sync::mpsc::channel::<bool>();
        let other = std::sync::Arc::clone(&cache);
        let thread = std::thread::spawn(move || {
            other.get(|| {
                asked.send(()).unwrap();
                // Waits for the other thread's own answer, as the window waits for the main thread.
                answer.recv_timeout(Duration::from_secs(5)).unwrap_or(false)
            })
        });
        waiting.recv_timeout(Duration::from_secs(5)).unwrap();
        let mine = cache.get(|| true);
        answered.send(mine).unwrap();
        assert!(mine);
        assert!(thread.join().unwrap());
    }
}
