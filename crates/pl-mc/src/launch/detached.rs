//! A game the launcher left running when it quit (SMALL7: "Oyun açılınca launcher: Kapansın"
//! really quits, and the tray's "Çıkış" during a game leaves it too). The game never depended on
//! the launcher; what the launcher keeps of it is this note in its data folder
//! ([`DetachedGame`]). The next start reads it once: a game that still runs is followed again
//! (`GameProcess::adopt`: the bar says "Oyunda", nothing changes its files meanwhile), and one
//! that ended meanwhile adds its session to the instance's play time, its end read from the
//! game's own log.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::paths::write_durable_blocking;

/// The note's file name, in the launcher's data folder.
pub const DETACHED_FILE: &str = "running-game.json";

/// The log the game writes while it runs, in its folder: its last write is about when it ended.
pub const LATEST_LOG: &str = "logs/latest.log";

/// A session that would be longer than this was not one game (the log was written by a later
/// start, from another launcher): nothing is counted.
pub const LONGEST_SESSION: Duration = Duration::from_secs(24 * 60 * 60);

/// The launcher quit while this game ran.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DetachedGame {
    pub instance_id: String,
    pub pid: u32,
    /// When the OS started the process (seconds since the Unix epoch, as the process table says):
    /// with `pid`, it tells the game from a process that got the same number later.
    pub process_started: u64,
    /// When the game started, and when its window showed (Unix milliseconds).
    pub started_at_ms: i64,
    pub shown_at_ms: Option<i64>,
    /// The instance's game folder.
    pub game_dir: PathBuf,
}

/// `time` in Unix milliseconds (0 before 1970).
pub fn unix_ms(time: SystemTime) -> i64 {
    time.duration_since(UNIX_EPOCH)
        .map(|since| i64::try_from(since.as_millis()).unwrap_or(i64::MAX))
        .unwrap_or(0)
}

/// Unix milliseconds as a time (`ms` ≥ 0).
pub fn from_unix_ms(ms: i64) -> SystemTime {
    UNIX_EPOCH + Duration::from_millis(u64::try_from(ms).unwrap_or(0))
}

impl DetachedGame {
    /// Leaves the note in `root`, at once and durably (the launcher is about to exit).
    pub fn save(&self, root: &Path) -> Result<()> {
        let path = root.join(DETACHED_FILE);
        let bytes = serde_json::to_vec_pretty(self).map_err(|e| Error::json(DETACHED_FILE, e))?;
        write_durable_blocking(&path, &bytes)
    }

    /// The note in `root`, read once: it goes as it is read, a note the launcher cannot read too.
    pub fn take(root: &Path) -> Option<Self> {
        let path = root.join(DETACHED_FILE);
        let bytes = std::fs::read(&path).ok()?;
        if let Err(error) = std::fs::remove_file(&path) {
            tracing::warn!(path = %path.display(), %error, "the detached game's note stays");
        }
        serde_json::from_slice(&bytes)
            .map_err(|error| tracing::warn!(%error, "unreadable note of a detached game"))
            .ok()
    }

    /// When the game ended while no launcher followed it: the last write of its latest log, if
    /// that came after it started.
    pub fn ended_at_ms(&self) -> Option<i64> {
        let modified = std::fs::metadata(self.game_dir.join(LATEST_LOG))
            .and_then(|meta| meta.modified())
            .ok()?;
        let at = unix_ms(modified);
        (at > self.started_at_ms).then_some(at)
    }

    /// What the session adds to the instance's play time when it ended at `ended_at_ms`: the
    /// whole run once its window had shown, as a followed game counts it (FIX2). Nothing when the
    /// window had not shown when the launcher quit, when the end is unknown or not after the
    /// start, or when the session would be longer than [`LONGEST_SESSION`].
    pub fn play_time(&self, ended_at_ms: Option<i64>) -> Option<Duration> {
        self.shown_at_ms?;
        let run = ended_at_ms?.checked_sub(self.started_at_ms)?;
        let run = Duration::from_millis(u64::try_from(run).ok().filter(|ms| *ms > 0)?);
        (run <= LONGEST_SESSION).then_some(run)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn game(dir: &Path) -> DetachedGame {
        DetachedGame {
            instance_id: "projects".into(),
            pid: 4242,
            process_started: 1_792_000_000,
            started_at_ms: 1_792_000_000_500,
            shown_at_ms: Some(1_792_000_030_000),
            game_dir: dir.join("game"),
        }
    }

    #[test]
    fn the_note_is_read_once() {
        let dir = tempfile::tempdir().unwrap();
        let note = game(dir.path());
        note.save(dir.path()).unwrap();
        assert_eq!(DetachedGame::take(dir.path()), Some(note));
        assert_eq!(DetachedGame::take(dir.path()), None);
        // An unreadable note goes too.
        std::fs::write(dir.path().join(DETACHED_FILE), b"{").unwrap();
        assert_eq!(DetachedGame::take(dir.path()), None);
        assert!(!dir.path().join(DETACHED_FILE).exists());
    }

    #[test]
    fn a_session_counts_once_the_window_had_shown() {
        let dir = tempfile::tempdir().unwrap();
        let note = game(dir.path());
        let hour = 3_600_000;
        assert_eq!(
            note.play_time(Some(note.started_at_ms + hour)),
            Some(Duration::from_secs(3600))
        );
        // Unknown, earlier or absurd ends count nothing.
        assert_eq!(note.play_time(None), None);
        assert_eq!(note.play_time(Some(note.started_at_ms)), None);
        assert_eq!(note.play_time(Some(note.started_at_ms - 5)), None);
        assert_eq!(note.play_time(Some(note.started_at_ms + 25 * hour)), None);
        // Left while it was still opening: whether it was played is not known.
        let opening = DetachedGame {
            shown_at_ms: None,
            ..note
        };
        assert_eq!(opening.play_time(Some(opening.started_at_ms + hour)), None);
    }

    #[test]
    fn the_end_is_the_logs_last_write() {
        let dir = tempfile::tempdir().unwrap();
        let mut note = game(dir.path());
        assert_eq!(note.ended_at_ms(), None);
        std::fs::create_dir_all(note.game_dir.join("logs")).unwrap();
        std::fs::write(note.game_dir.join(LATEST_LOG), b"[12:00:00] Stopping!\n").unwrap();
        let written = unix_ms(
            std::fs::metadata(note.game_dir.join(LATEST_LOG))
                .unwrap()
                .modified()
                .unwrap(),
        );
        // A log from before the game started is another session's.
        note.started_at_ms = written + 60_000;
        assert_eq!(note.ended_at_ms(), None);
        note.started_at_ms = written - 60_000;
        assert_eq!(note.ended_at_ms(), Some(written));
        assert_eq!(
            from_unix_ms(written),
            UNIX_EPOCH + Duration::from_millis(written as u64)
        );
    }
}
