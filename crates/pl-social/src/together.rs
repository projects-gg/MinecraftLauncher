//! "Birlikte oynanan süre" (TEAM5): how long the player was on Projects together with crewmates,
//! kept only on this computer and never sent anywhere.
//!
//! Per account it holds, for each crew, the seconds of this week (local Monday to Sunday) in which
//! someone of that crew was on Projects at the same time as the player, and for each crewmate the
//! seconds of this month together. Only the current week and month are kept, so the file stays a
//! few hundred bytes; the player may switch it off (nothing is kept then, and what was kept goes)
//! or delete it. Being on Projects at the same time is not playing the same mode: it is an
//! estimate, and it says so nowhere it would matter.
//!
//! Nothing runs on a timer: the time between two changes of who is there (a presence change, the
//! game starting or ending, the list arriving) is added when the next change comes, and the view
//! adds the interval still open. One interval counts at most [`MAX_INTERVAL_SECS`], so a computer
//! that slept with the game open adds no night.

use std::collections::{BTreeSet, HashMap};
use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// The longest stretch one interval adds.
pub const MAX_INTERVAL_SECS: i64 = 6 * 3_600;

/// Who is on Projects together with the player right now: the crews with someone there, and the
/// people.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Together {
    pub crews: BTreeSet<String>,
    pub people: BTreeSet<String>,
}

impl Together {
    pub fn is_empty(&self) -> bool {
        self.crews.is_empty() && self.people.is_empty()
    }
}

/// The periods the times belong to: this week and this month on the local calendar, as keys
/// (`2026-W43`, `2026-10`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Periods {
    pub week: String,
    pub month: String,
}

impl Periods {
    /// The periods of `unix` seconds on this computer's calendar.
    pub fn at(unix: i64) -> Self {
        let zone = jiff::tz::TimeZone::system();
        let date = jiff::Timestamp::from_second(unix)
            .unwrap_or(jiff::Timestamp::UNIX_EPOCH)
            .to_zoned(zone)
            .date();
        let week = date.iso_week_date();
        Self {
            week: format!("{}-W{:02}", week.year(), week.week()),
            month: format!("{}-{:02}", date.year(), date.month()),
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
struct Period {
    key: String,
    secs: u32,
}

impl Period {
    /// The seconds of `key` (another period's are none).
    fn of(&self, key: &str) -> u32 {
        if self.key == key { self.secs } else { 0 }
    }

    fn add(&mut self, key: &str, secs: u32) {
        if self.key != key {
            self.key = key.to_owned();
            self.secs = 0;
        }
        self.secs = self.secs.saturating_add(secs);
    }
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Account {
    #[serde(default)]
    crews: HashMap<String, Period>,
    #[serde(default)]
    people: HashMap<String, Period>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
struct Document {
    /// The player switched it off on this computer.
    #[serde(default)]
    off: bool,
    /// Per account (Discord user id).
    #[serde(default)]
    accounts: HashMap<String, Account>,
}

#[derive(Debug, Clone)]
struct Open {
    account: String,
    since: i64,
    with: Together,
}

/// The kept times and the interval still open. Reads its file once, writes it when an interval
/// closes with some time in it (and when switched or deleted).
#[derive(Debug, Default)]
pub struct TogetherLog {
    path: Option<PathBuf>,
    document: Document,
    read: bool,
    open: Option<Open>,
}

impl TogetherLog {
    /// Keeps the times in this file (None: for this run only).
    pub fn new(path: Option<PathBuf>) -> Self {
        Self {
            path,
            ..Self::default()
        }
    }

    fn document(&mut self) -> &mut Document {
        if !self.read {
            self.read = true;
            if let Some(path) = &self.path {
                self.document = std::fs::read(path)
                    .ok()
                    .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                    .unwrap_or_default();
            }
        }
        &mut self.document
    }

    /// Whether times are kept (on unless the player switched it off here).
    pub fn enabled(&mut self) -> bool {
        !self.document().off
    }

    /// Switches it on or off. Off closes the open interval without counting it and deletes every
    /// time kept: nothing is kept while it is off.
    pub fn set_enabled(&mut self, on: bool) -> io::Result<()> {
        let document = self.document();
        if document.off == !on {
            return Ok(());
        }
        document.off = !on;
        if !on {
            document.accounts.clear();
            self.open = None;
        }
        self.write()
    }

    /// Deletes every time kept on this computer (the switch stays as it is); the open interval
    /// starts again from now.
    pub fn clear(&mut self, now: i64) -> io::Result<()> {
        self.document().accounts.clear();
        if let Some(open) = &mut self.open {
            open.since = now;
        }
        self.write()
    }

    /// Who is together with the player now (`account`: who is signed in; None signed out). A
    /// change closes the open interval, adding its time to the crews and people that were there
    /// (in the `periods` of now, asked only then), and opens the next one. The same set again
    /// changes nothing.
    pub fn observe(
        &mut self,
        account: Option<&str>,
        with: Together,
        now: i64,
        periods: impl FnOnce() -> Periods,
    ) {
        let on = self.enabled();
        let same = self
            .open
            .as_ref()
            .is_some_and(|open| Some(open.account.as_str()) == account && open.with == with && on);
        if same || (self.open.is_none() && (with.is_empty() || account.is_none() || !on)) {
            return;
        }
        if let Some(open) = self.open.take() {
            let secs = (now - open.since).clamp(0, MAX_INTERVAL_SECS);
            if secs > 0 && on {
                let secs = u32::try_from(secs).unwrap_or(u32::MAX);
                let periods = periods();
                let entry = self
                    .document()
                    .accounts
                    .entry(open.account.clone())
                    .or_default();
                for crew in &open.with.crews {
                    entry
                        .crews
                        .entry(crew.clone())
                        .or_default()
                        .add(&periods.week, secs);
                }
                for person in &open.with.people {
                    entry
                        .people
                        .entry(person.clone())
                        .or_default()
                        .add(&periods.month, secs);
                }
                prune(entry, &periods);
                if let Err(error) = self.write() {
                    tracing::debug!(%error, "time played together is kept for this run only");
                }
            }
        }
        if let Some(account) = account.filter(|_| on && !with.is_empty()) {
            self.open = Some(Open {
                account: account.to_owned(),
                since: now,
                with,
            });
        }
    }

    /// The open interval's seconds so far for what `counts` says is in it.
    fn open_secs(&self, account: &str, now: i64, counts: impl Fn(&Together) -> bool) -> u32 {
        self.open
            .as_ref()
            .filter(|open| open.account == account && counts(&open.with))
            .map_or(0, |open| {
                u32::try_from((now - open.since).clamp(0, MAX_INTERVAL_SECS)).unwrap_or(0)
            })
    }

    /// Seconds this week with someone of `crew` on Projects too (the open interval included).
    pub fn crew_week(&mut self, account: &str, crew: &str, now: i64, periods: &Periods) -> u32 {
        if !self.enabled() {
            return 0;
        }
        let kept = self
            .document()
            .accounts
            .get(account)
            .and_then(|entry| entry.crews.get(crew))
            .map_or(0, |period| period.of(&periods.week));
        kept.saturating_add(self.open_secs(account, now, |with| with.crews.contains(crew)))
    }

    /// Seconds this month together with `person` (the open interval included).
    pub fn person_month(
        &mut self,
        account: &str,
        person: &str,
        now: i64,
        periods: &Periods,
    ) -> u32 {
        if !self.enabled() {
            return 0;
        }
        let kept = self
            .document()
            .accounts
            .get(account)
            .and_then(|entry| entry.people.get(person))
            .map_or(0, |period| period.of(&periods.month));
        kept.saturating_add(self.open_secs(account, now, |with| with.people.contains(person)))
    }

    /// Writes through a temporary file (a crash never leaves half a document); nothing kept and
    /// the switch on removes the file.
    fn write(&self) -> io::Result<()> {
        let Some(path) = &self.path else {
            return Ok(());
        };
        let document = &self.document;
        if document.accounts.is_empty() && !document.off {
            return match std::fs::remove_file(path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        }
        if let Some(dir) = path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temporary = path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec(document)?)?;
        std::fs::rename(&temporary, path)
    }
}

/// Only this week's and this month's times stay.
fn prune(entry: &mut Account, periods: &Periods) {
    entry.crews.retain(|_, period| period.key == periods.week);
    entry.people.retain(|_, period| period.key == periods.month);
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_521_000;

    fn week(week: &str, month: &str) -> Periods {
        Periods {
            week: week.into(),
            month: month.into(),
        }
    }

    fn with(crews: &[&str], people: &[&str]) -> Together {
        Together {
            crews: crews.iter().map(|c| (*c).to_owned()).collect(),
            people: people.iter().map(|p| (*p).to_owned()).collect(),
        }
    }

    /// A folder of its own under the system temp folder, removed when the test ends.
    struct Scratch(PathBuf);

    impl Scratch {
        fn new(name: &str) -> Self {
            let dir = std::env::temp_dir()
                .join(format!("pl-social-together-{}-{name}", std::process::id()));
            let _ = std::fs::remove_dir_all(&dir);
            Self(dir)
        }

        fn file(&self) -> PathBuf {
            self.0.join("social").join("together.json")
        }
    }

    impl Drop for Scratch {
        fn drop(&mut self) {
            let _ = std::fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn counts_the_time_between_changes_per_crew_and_person() {
        let scratch = Scratch::new("counts");
        let periods = week("2026-W43", "2026-10");
        let mut log = TogetherLog::new(Some(scratch.file()));
        // Ayşe (crew A) is there from NOW; Bora (crews A and B) joins ten minutes later.
        log.observe(Some("me"), with(&["a"], &["ayse"]), NOW, || periods.clone());
        log.observe(
            Some("me"),
            with(&["a", "b"], &["ayse", "bora"]),
            NOW + 600,
            || periods.clone(),
        );
        // The open interval counts in the view, without a write.
        assert_eq!(log.crew_week("me", "a", NOW + 900, &periods), 900);
        assert_eq!(log.crew_week("me", "b", NOW + 900, &periods), 300);
        assert_eq!(log.person_month("me", "bora", NOW + 900, &periods), 300);
        // The same set again is no change.
        log.observe(
            Some("me"),
            with(&["a", "b"], &["ayse", "bora"]),
            NOW + 900,
            || periods.clone(),
        );
        // The player leaves Projects: the interval closes.
        log.observe(Some("me"), Together::default(), NOW + 1_800, || {
            periods.clone()
        });
        assert_eq!(log.crew_week("me", "a", NOW + 5_000, &periods), 1_800);
        assert_eq!(log.crew_week("me", "b", NOW + 5_000, &periods), 1_200);
        assert_eq!(log.person_month("me", "ayse", NOW + 5_000, &periods), 1_800);
        // Another launcher run reads them back; another account has none.
        let mut again = TogetherLog::new(Some(scratch.file()));
        assert_eq!(again.crew_week("me", "a", NOW, &periods), 1_800);
        assert_eq!(again.crew_week("other", "a", NOW, &periods), 0);
        // Next week the crews start over; the month goes on until it ends.
        let next = week("2026-W44", "2026-10");
        assert_eq!(again.crew_week("me", "a", NOW, &next), 0);
        assert_eq!(again.person_month("me", "ayse", NOW, &next), 1_800);
        again.observe(Some("me"), with(&["a"], &["ayse"]), NOW, || next.clone());
        again.observe(Some("me"), Together::default(), NOW + 60, || next.clone());
        assert_eq!(again.crew_week("me", "a", NOW, &next), 60);
        assert_eq!(again.person_month("me", "ayse", NOW, &next), 1_860);
        // Only the current week's crews stay in the file.
        let kept: Document =
            serde_json::from_slice(&std::fs::read(scratch.file()).unwrap()).unwrap();
        assert!(
            kept.accounts["me"]
                .crews
                .values()
                .all(|p| p.key == "2026-W44")
        );
    }

    #[test]
    fn a_long_stretch_counts_at_most_six_hours_and_signing_out_closes_it() {
        let periods = week("2026-W43", "2026-10");
        let mut log = TogetherLog::new(None);
        log.observe(Some("me"), with(&["a"], &["ayse"]), NOW, || periods.clone());
        log.observe(None, Together::default(), NOW + 86_400, || periods.clone());
        assert_eq!(
            log.crew_week("me", "a", NOW + 90_000, &periods),
            u32::try_from(MAX_INTERVAL_SECS).unwrap()
        );
        // Signed out nothing is open.
        assert_eq!(log.open_secs("me", NOW + 90_000, |_| true), 0);
    }

    #[test]
    fn switched_off_nothing_is_kept_and_deleting_starts_over() {
        let scratch = Scratch::new("off");
        let periods = week("2026-W43", "2026-10");
        let mut log = TogetherLog::new(Some(scratch.file()));
        log.observe(Some("me"), with(&["a"], &["ayse"]), NOW, || periods.clone());
        log.observe(Some("me"), Together::default(), NOW + 600, || {
            periods.clone()
        });
        log.observe(Some("me"), with(&["a"], &["ayse"]), NOW + 700, || {
            periods.clone()
        });
        log.clear(NOW + 800).unwrap();
        // Deleting keeps counting from now.
        assert_eq!(log.crew_week("me", "a", NOW + 860, &periods), 60);
        log.set_enabled(false).unwrap();
        assert!(!log.enabled());
        assert_eq!(log.crew_week("me", "a", NOW + 900, &periods), 0);
        log.observe(Some("me"), with(&["a"], &["ayse"]), NOW + 1_000, || {
            periods.clone()
        });
        log.observe(Some("me"), Together::default(), NOW + 2_000, || {
            periods.clone()
        });
        // The switch outlives a restart; nothing else is in the file.
        let mut again = TogetherLog::new(Some(scratch.file()));
        assert!(!again.enabled());
        let kept: Document =
            serde_json::from_slice(&std::fs::read(scratch.file()).unwrap()).unwrap();
        assert!(kept.off && kept.accounts.is_empty());
        again.set_enabled(true).unwrap();
        assert!(again.enabled());
        // On again with nothing kept: no file at all.
        assert!(!scratch.file().exists());
        again.observe(Some("me"), with(&["a"], &[]), NOW + 3_000, || {
            periods.clone()
        });
        again.observe(Some("me"), Together::default(), NOW + 3_060, || {
            periods.clone()
        });
        assert_eq!(again.crew_week("me", "a", NOW + 4_000, &periods), 60);
    }

    #[test]
    fn periods_follow_the_local_calendar() {
        let periods = Periods::at(NOW);
        assert!(periods.week.len() == 8 && periods.week.contains("-W"));
        assert_eq!(periods.month.len(), 7);
    }
}
