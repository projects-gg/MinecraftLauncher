//! The note the launcher leaves for its next start when it closes to install an update.
//!
//! Installing replaces the launcher and quits it, so the AFK accounts have to stop. The next start
//! should look as if nothing happened: the same accounts connect again, the window opens even when
//! the launcher had been started minimized at login, the player is back on the page they were on,
//! and the new version's notes ("Yenilikler") show once.
//!
//! The note is one-shot: the next start [`take`]s it (reads and deletes it). Its timed parts (the
//! accounts, the window, the page) apply only for [`FRESH_FOR`] after it was written, so an update
//! that never finished (the installer failed and the player came back hours later) reconnects
//! nothing by surprise. The notes belong to the version instead: they show when it runs.

use std::path::Path;
use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::Result;

/// The note's file in the data folder.
pub const FILE_NAME: &str = "update-restart.json";

/// How long after it was written the note's timed parts still apply.
pub const FRESH_FOR: Duration = Duration::from_secs(10 * 60);

/// A note this far "in the future" is still fresh (a clock corrected meanwhile).
const CLOCK_SLACK_MS: i64 = 60_000;

/// Longest page address kept; real ones are short (`/library/<id>?tab=mods`).
const MAX_ROUTE_CHARS: usize = 512;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RestartNote {
    /// When the note was written (Unix milliseconds, see [`now_ms`]).
    pub saved_at: i64,
    /// The version being installed.
    pub version: String,
    /// Its release notes (Markdown), shown once that version runs.
    #[serde(default)]
    pub notes: Option<String>,
    /// AFK accounts the update stopped (in game, connecting or waiting in a "connect all"), by id.
    #[serde(default)]
    pub afk_accounts: Vec<String>,
    /// The main window was on screen (not in the tray, not minimized).
    #[serde(default)]
    pub window_visible: bool,
    /// The page the player was on, as a launcher route (`/afk`, `/library/x?tab=mods`).
    #[serde(default)]
    pub route: Option<String>,
}

impl RestartNote {
    /// The note's timed parts still apply at `now` (Unix milliseconds).
    pub fn is_fresh_at(&self, now: i64) -> bool {
        let fresh_for = i64::try_from(FRESH_FOR.as_millis()).unwrap_or(i64::MAX);
        let age = now.saturating_sub(self.saved_at);
        (-CLOCK_SLACK_MS..=fresh_for).contains(&age)
    }
}

/// The current time in Unix milliseconds (what [`RestartNote::saved_at`] holds).
pub fn now_ms() -> i64 {
    jiff::Timestamp::now().as_millisecond()
}

/// A route worth coming back to: a launcher path (`/…`, never `//host`), short, without control
/// characters. `None` for anything else.
pub fn clean_route(route: &str) -> Option<String> {
    let route = route.trim();
    let usable = route.starts_with('/')
        && !route.starts_with("//")
        && !route.starts_with("/\\")
        && route.chars().count() <= MAX_ROUTE_CHARS
        && !route.chars().any(char::is_control);
    usable.then(|| route.to_owned())
}

/// Writes the note atomically and durably: the process that reads it replaced this one.
pub fn save(file: &Path, note: &RestartNote) -> Result<()> {
    crate::relocate::write_json_atomic(file, note)
}

/// Reads and deletes the note. `None` when there is none or it cannot be read; an unreadable note
/// is deleted too, so it never applies at a later start.
pub fn take(file: &Path) -> Option<RestartNote> {
    let bytes = match std::fs::read(file) {
        Ok(bytes) => bytes,
        Err(error) => {
            if error.kind() != std::io::ErrorKind::NotFound {
                tracing::warn!(%error, "cannot read the update restart note");
                discard(file);
            }
            return None;
        }
    };
    discard(file);
    match serde_json::from_slice(&bytes) {
        Ok(note) => Some(note),
        Err(error) => {
            tracing::warn!(%error, "ignoring an unreadable update restart note");
            None
        }
    }
}

/// Deletes the note: the update did not install, nothing waits for a restart.
pub fn discard(file: &Path) {
    if let Err(error) = std::fs::remove_file(file)
        && error.kind() != std::io::ErrorKind::NotFound
    {
        tracing::warn!(%error, "cannot delete the update restart note");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const MINUTE: i64 = 60_000;

    fn note(saved_at: i64) -> RestartNote {
        RestartNote {
            saved_at,
            version: "2.1.0".into(),
            notes: Some("- Daha hızlı indirmeler".into()),
            afk_accounts: vec!["a1".into(), "b2".into()],
            window_visible: true,
            route: Some("/afk".into()),
        }
    }

    #[test]
    fn a_note_is_taken_once() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE_NAME);
        assert_eq!(take(&file), None, "no note, nothing to do");

        let written = note(now_ms());
        save(&file, &written).unwrap();
        assert_eq!(take(&file), Some(written));
        assert!(!file.exists(), "taking deletes it");
        assert_eq!(take(&file), None, "a second start finds nothing");
    }

    #[test]
    fn the_timed_parts_apply_for_ten_minutes() {
        let saved = 1_760_700_000_000;
        let n = note(saved);
        assert!(n.is_fresh_at(saved));
        assert!(n.is_fresh_at(saved + 10 * MINUTE));
        assert!(!n.is_fresh_at(saved + 10 * MINUTE + 1), "too old");
        assert!(
            n.is_fresh_at(saved - MINUTE / 2),
            "a clock set back a little"
        );
        assert!(!n.is_fresh_at(saved - 2 * MINUTE), "written in the future");
        assert!(!n.is_fresh_at(i64::MIN) && !n.is_fresh_at(i64::MAX));
    }

    #[test]
    fn an_unreadable_note_is_dropped() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE_NAME);
        std::fs::write(&file, b"{ not json").unwrap();
        assert_eq!(take(&file), None);
        assert!(!file.exists(), "it never applies later");
    }

    #[test]
    fn a_short_note_reads_with_defaults() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE_NAME);
        std::fs::write(&file, br#"{"savedAt":5,"version":"2.1.0"}"#).unwrap();
        let read = take(&file).unwrap();
        assert_eq!(read.version, "2.1.0");
        assert!(read.afk_accounts.is_empty() && read.notes.is_none() && read.route.is_none());
        assert!(!read.window_visible);
    }

    #[test]
    fn discarding_is_quiet_when_there_is_nothing() {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join(FILE_NAME);
        discard(&file);
        save(&file, &note(1)).unwrap();
        discard(&file);
        assert!(!file.exists());
    }

    #[test]
    fn routes_are_launcher_paths() {
        assert_eq!(clean_route("/afk"), Some("/afk".into()));
        assert_eq!(
            clean_route(" /library/abc?tab=mods "),
            Some("/library/abc?tab=mods".into())
        );
        assert_eq!(clean_route("/"), Some("/".into()));
        for bad in [
            "",
            "afk",
            "//evil.example",
            "/\\evil",
            "https://projects.gg",
            "/a\nb",
        ] {
            assert_eq!(clean_route(bad), None, "{bad:?}");
        }
        assert_eq!(clean_route(&format!("/{}", "x".repeat(600))), None);
    }
}
