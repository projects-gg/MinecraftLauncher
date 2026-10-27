//! Games the launcher left running when it quit (SMALL7: "Oyun açılınca launcher: Kapansın"
//! really quits, and the tray's "Çıkış" during a game leaves it too). A game never depended on
//! the launcher; what the launcher keeps of it is this note in its data folder
//! ([`DetachedGame`]). The next start reads it once: a game that still runs is followed again
//! (`GameProcess::adopt`: the bar says "Oyunda", nothing changes its files meanwhile), and one
//! that ended meanwhile adds its session to the instance's play time, its end read from the
//! game's own log.
//!
//! Several games may run at once (PLAY-AGAIN): the note keeps the first one at its top, as it
//! always did (an older launcher reads that one and follows it), and the others under `more`
//! ([`save_all`], [`take_all`]).

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use serde::{Deserialize, Serialize};

use crate::error::{Error, Result};
use crate::launch::GameAccount;
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
    /// The account the game plays with, so the next start says "Oyunda" for that account alone.
    /// A note left by an older launcher has none: its game counts as every account's.
    #[serde(default)]
    pub account: Option<GameAccount>,
    /// The other games that ran when the launcher quit (PLAY-AGAIN), in the order they started.
    /// Only the note's first game has any; an older launcher ignores them.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub more: Vec<DetachedGame>,
}

/// Leaves the note of `games` (in the order they started) in `root`, at once and durably (the
/// launcher is about to exit); with none, an older note goes.
pub fn save_all(root: &Path, games: &[DetachedGame]) -> Result<()> {
    let Some((first, rest)) = games.split_first() else {
        let _ = std::fs::remove_file(root.join(DETACHED_FILE));
        return Ok(());
    };
    let note = DetachedGame {
        more: rest
            .iter()
            .map(|game| DetachedGame {
                more: Vec::new(),
                ..game.clone()
            })
            .collect(),
        ..first.clone()
    };
    note.save(root)
}

/// Every game of the note in `root`, in the order they started, read once ([`DetachedGame::take`]).
pub fn take_all(root: &Path) -> Vec<DetachedGame> {
    let Some(mut first) = DetachedGame::take(root) else {
        return Vec::new();
    };
    let more = std::mem::take(&mut first.more);
    std::iter::once(first).chain(more).collect()
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
    /// Leaves the note in `root`, at once and durably (the launcher is about to exit). Several
    /// games go through [`save_all`].
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
            account: Some(GameAccount {
                id: "0b7d6c1e-3f42-4c55-9d1a-2f0e8c4b6a10".into(),
                name: "EjderAvcisi".into(),
            }),
            more: Vec::new(),
        }
    }

    #[test]
    fn the_note_keeps_every_game_and_the_first_on_top() {
        // PLAY-AGAIN: two games of one account (and one of another) ran when the launcher quit.
        let dir = tempfile::tempdir().unwrap();
        let first = game(dir.path());
        let second = DetachedGame {
            pid: 4343,
            started_at_ms: first.started_at_ms + 60_000,
            ..first.clone()
        };
        let third = DetachedGame {
            pid: 4444,
            instance_id: "hardcore".into(),
            account: Some(GameAccount {
                id: "5c1e0d2a-7b44-4e10-8f6a-3d2c1b0a9e88".into(),
                name: "KorAtesi_".into(),
            }),
            ..first.clone()
        };
        save_all(dir.path(), &[first.clone(), second.clone(), third.clone()]).unwrap();
        // An older launcher reads the top of the note: the first game, nothing else.
        let text = std::fs::read_to_string(dir.path().join(DETACHED_FILE)).unwrap();
        let top: serde_json::Value = serde_json::from_str(&text).unwrap();
        assert_eq!(top["pid"], 4242);
        assert_eq!(top["more"].as_array().map(Vec::len), Some(2));
        assert_eq!(
            take_all(dir.path()),
            vec![first.clone(), second, third],
            "in the order they started, read once"
        );
        assert!(take_all(dir.path()).is_empty());
        // One game writes the note as it always was (no `more`).
        save_all(dir.path(), std::slice::from_ref(&first)).unwrap();
        let text = std::fs::read_to_string(dir.path().join(DETACHED_FILE)).unwrap();
        assert!(!text.contains("more"));
        assert_eq!(take_all(dir.path()), vec![first.clone()]);
        // None takes an older note away.
        first.save(dir.path()).unwrap();
        save_all(dir.path(), &[]).unwrap();
        assert!(take_all(dir.path()).is_empty());
    }

    #[test]
    fn the_note_keeps_the_account_the_game_plays_with() {
        let dir = tempfile::tempdir().unwrap();
        let note = game(dir.path());
        note.save(dir.path()).unwrap();
        let read = DetachedGame::take(dir.path()).unwrap();
        assert_eq!(read.account, note.account);
    }

    #[test]
    fn a_note_from_an_older_launcher_has_no_account() {
        // PLAY-ACCT added `account`; a game left running by an older launcher is still followed.
        let dir = tempfile::tempdir().unwrap();
        let old = r#"{"instanceId":"projects","pid":4242,"processStarted":1792000000,
            "startedAtMs":1792000000500,"shownAtMs":null,"gameDir":"C:/games/projects"}"#;
        std::fs::write(dir.path().join(DETACHED_FILE), old).unwrap();
        let read = DetachedGame::take(dir.path()).unwrap();
        assert_eq!(read.instance_id, "projects");
        assert_eq!(read.account, None);
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
