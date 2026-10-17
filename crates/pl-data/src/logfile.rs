//! The launcher's own log files: `logs/launcher.<YYYY-MM-DD>.log`, one per **local** day (the
//! date players and support talk in, like the game logs' `game-<instance>-<local time>.log`
//! names), the newest [`KEEP_FILES`] kept. [`DailyLog`] is a plain [`std::io::Write`]; the app
//! wraps it in a mutex for its log subscriber. Lines are written synchronously: the last lines
//! before a crash are the ones support needs.

use std::fs::{File, OpenOptions};
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use jiff::civil::Date;

/// Launcher log files kept (one per day).
pub const KEEP_FILES: usize = 7;
const PREFIX: &str = "launcher.";
const SUFFIX: &str = ".log";

type Clock = Box<dyn Fn() -> Date + Send>;

/// Appends to today's launcher log, switching files at local midnight.
pub struct DailyLog {
    dir: PathBuf,
    keep: usize,
    today: Clock,
    current: Option<(Date, File)>,
}

impl std::fmt::Debug for DailyLog {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DailyLog")
            .field("dir", &self.dir)
            .field("keep", &self.keep)
            .finish_non_exhaustive()
    }
}

impl DailyLog {
    /// Logs in `dir` (created when missing), dated by the system's local time zone.
    pub fn new(dir: impl Into<PathBuf>) -> io::Result<Self> {
        Self::with_clock(dir, KEEP_FILES, Box::new(|| jiff::Zoned::now().date()))
    }

    /// `today` names the current local date (tests use a fixed one).
    pub fn with_clock(dir: impl Into<PathBuf>, keep: usize, today: Clock) -> io::Result<Self> {
        let dir = dir.into();
        std::fs::create_dir_all(&dir)?;
        Ok(Self {
            dir,
            keep: keep.max(1),
            today,
            current: None,
        })
    }

    /// The file for `date`.
    pub fn file_for(dir: &Path, date: Date) -> PathBuf {
        dir.join(format!("{PREFIX}{date}{SUFFIX}"))
    }

    fn file(&mut self) -> io::Result<&mut File> {
        let today = (self.today)();
        let stale = self.current.as_ref().is_none_or(|(date, _)| *date != today);
        if stale {
            let file = OpenOptions::new()
                .create(true)
                .append(true)
                .open(Self::file_for(&self.dir, today))?;
            self.current = Some((today, file));
            self.prune(today);
        }
        match &mut self.current {
            Some((_, file)) => Ok(file),
            None => Err(io::Error::other("no log file")),
        }
    }

    /// Keeps the newest `keep` launcher logs (by the date in their names, so files named in UTC
    /// by older launchers age out the same way). Today's file always stays.
    fn prune(&self, today: Date) {
        let Ok(entries) = std::fs::read_dir(&self.dir) else {
            return;
        };
        let mut dated: Vec<(Date, PathBuf)> = entries
            .filter_map(Result::ok)
            .filter_map(|entry| {
                let name = entry.file_name().to_string_lossy().into_owned();
                let stamp = name.strip_prefix(PREFIX)?.strip_suffix(SUFFIX)?;
                let date: Date = stamp.parse().ok()?;
                Some((date, entry.path()))
            })
            .collect();
        dated.sort_by_key(|(date, _)| std::cmp::Reverse(*date));
        for (date, path) in dated.into_iter().skip(self.keep) {
            if date != today {
                let _ = std::fs::remove_file(path);
            }
        }
    }
}

impl Write for DailyLog {
    fn write(&mut self, buf: &[u8]) -> io::Result<usize> {
        self.file()?.write(buf)
    }

    fn flush(&mut self) -> io::Result<()> {
        match &mut self.current {
            Some((_, file)) => file.flush(),
            None => Ok(()),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use super::*;

    fn names(dir: &Path) -> Vec<String> {
        let mut names: Vec<String> = std::fs::read_dir(dir)
            .unwrap()
            .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        names.sort();
        names
    }

    #[test]
    fn one_file_per_local_day_and_old_ones_go() {
        let dir = tempfile::tempdir().unwrap();
        // Older files, one named by an older launcher, and something that is not a log.
        for name in [
            "launcher.2026-10-01.log",
            "launcher.2026-10-02.log",
            "launcher.2026-10-10.log",
            "game-x-20261001-120000.log",
            "launcher.notes.txt",
        ] {
            std::fs::write(dir.path().join(name), b"old\n").unwrap();
        }
        let day = Arc::new(Mutex::new(jiff::civil::date(2026, 10, 15)));
        let clock = Arc::clone(&day);
        let mut log =
            DailyLog::with_clock(dir.path(), 3, Box::new(move || *clock.lock().unwrap())).unwrap();
        writeln!(log, "first line").unwrap();
        writeln!(log, "second line").unwrap();
        log.flush().unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("launcher.2026-10-15.log")).unwrap(),
            "first line\nsecond line\n"
        );
        // Kept: today plus the two newest older ones; other files are left alone.
        assert_eq!(
            names(dir.path()),
            [
                "game-x-20261001-120000.log",
                "launcher.2026-10-02.log",
                "launcher.2026-10-10.log",
                "launcher.2026-10-15.log",
                "launcher.notes.txt",
            ]
        );

        // Local midnight passes: the next line starts tomorrow's file.
        *day.lock().unwrap() = jiff::civil::date(2026, 10, 16);
        writeln!(log, "after midnight").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("launcher.2026-10-16.log")).unwrap(),
            "after midnight\n"
        );
        assert!(!dir.path().join("launcher.2026-10-02.log").exists());

        // Reopening appends to today's file.
        drop(log);
        let mut again =
            DailyLog::with_clock(dir.path(), 3, Box::new(|| jiff::civil::date(2026, 10, 16)))
                .unwrap();
        writeln!(again, "restarted").unwrap();
        assert_eq!(
            std::fs::read_to_string(dir.path().join("launcher.2026-10-16.log")).unwrap(),
            "after midnight\nrestarted\n"
        );
    }

    #[test]
    fn uses_the_local_date_by_default() {
        let dir = tempfile::tempdir().unwrap();
        let mut log = DailyLog::new(dir.path().join("logs")).unwrap();
        writeln!(log, "hello").unwrap();
        let today = jiff::Zoned::now().date();
        assert!(DailyLog::file_for(&dir.path().join("logs"), today).is_file());
    }
}
