//! The launcher's one recorder: counts while the player's switch and Studio's flag are both on,
//! nothing otherwise; hands out the request when one is due; takes back its result.
//!
//! Every method takes the time (`now_ms`, Unix milliseconds) instead of reading a clock, so the
//! rules are tested as they run. Counts that matter after a crash (a start, a launch, a panic) are
//! written at once; frequent ones (feature uses, voice) at most once a minute and when the
//! launcher closes ([`Recorder::flush`]).

use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};

use parking_lot::Mutex;

use crate::batch::{self, Carried, Outgoing};
use crate::facts::AppFacts;
use crate::feature::Feature;
use crate::ledger::{CrashKind, CrashNote, Ledger, START_SAMPLES, Stored, push_sample};
use crate::scrub;
use crate::voice::{VoiceSeen, VoiceTracker};

/// The file in the data folder.
pub const FILE: &str = "telemetry.json";
/// Where a panic leaves its summary when the recorder is busy or not open yet.
pub const SIDE_FILE: &str = "telemetry-crash.json";
/// Counts that wait are written at most this often.
const SAVE_EVERY_MS: i64 = 60_000;
/// Page errors one run counts at most (a page that throws in a loop is one problem).
const PAGE_ERRORS_PER_RUN: u32 = 20;

/// How a launch ended.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Launch<'a> {
    /// The game's process started.
    Started,
    /// It failed before that, with this command error code.
    Failed(&'a str),
}

/// How the service answered a request.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Answer {
    /// It took the batch.
    Taken,
    /// It refused the batch as it is (sending it again would not help).
    Refused,
    /// No answer, or "later": try again after a wait.
    Later,
}

#[derive(Debug)]
struct Inner {
    stored: Stored,
    voice: VoiceTracker,
    dirty: bool,
    saved_ms: i64,
    page_errors: u32,
    sending: bool,
}

/// See the module.
#[derive(Debug)]
pub struct Recorder {
    path: PathBuf,
    side: PathBuf,
    /// Counting is on (the switch and the flag), readable without the lock: the panic hook never
    /// waits for it, and writes nothing while it is off.
    on: AtomicBool,
    inner: Mutex<Inner>,
}

fn secs(ms: i64) -> i64 {
    ms.div_euclid(1000)
}

impl Recorder {
    /// Opens the data folder's file (a missing or broken one starts fresh) and takes in the
    /// summary a panic left beside it (`version`: the launcher's, for a panic before it was known).
    pub fn open(dir: &Path, version: &str, now_ms: i64) -> Self {
        let path = dir.join(FILE);
        let side = dir.join(SIDE_FILE);
        let mut stored = Stored::load(&path);
        let left = std::fs::read(&side)
            .ok()
            .and_then(|bytes| serde_json::from_slice::<CrashNote>(&bytes).ok());
        let had_side = side.exists();
        if let Some(mut note) = left
            && stored.choice
            && stored.flag
        {
            if note.ver.is_empty() {
                note.ver = crate::facts::clean_version(version);
            }
            stored.ledger.counts.panics = stored.ledger.counts.panics.saturating_add(1);
            stored.ledger.add_crash(note, secs(now_ms));
        }
        let recorder = Self {
            path,
            side,
            on: AtomicBool::new(stored.choice && stored.flag),
            inner: Mutex::new(Inner {
                stored,
                voice: VoiceTracker::default(),
                dirty: false,
                saved_ms: now_ms,
                page_errors: 0,
                sending: false,
            }),
        };
        if had_side {
            let mut inner = recorder.inner.lock();
            recorder.save(&mut inner, now_ms);
            let _ = std::fs::remove_file(&recorder.side);
        }
        recorder
    }

    fn save(&self, inner: &mut Inner, now_ms: i64) {
        // Counting goes on in memory when the disk refuses; the next save tries again.
        if inner.stored.save(&self.path).is_ok() {
            inner.dirty = false;
            inner.saved_ms = now_ms;
        }
    }

    /// Counts are kept: the player's switch and Studio's flag are both on.
    pub fn active(&self) -> bool {
        let inner = self.inner.lock();
        inner.stored.choice && inner.stored.flag
    }

    /// Settings → Veri → "Kullanım verisi gönder".
    pub fn choice(&self) -> bool {
        self.inner.lock().stored.choice
    }

    /// The applied content's `features.telemetry`.
    pub fn flag(&self) -> bool {
        self.inner.lock().stored.flag
    }

    /// The player's switch. Off forgets everything not sent yet: nothing of it goes later. When
    /// the disk refuses the choice, counting stays off: an "off" holds in memory and goes with the
    /// next write (at the latest when the launcher closes), an "on" the disk does not hold is none.
    pub fn set_choice(&self, on: bool, now_ms: i64) {
        let mut inner = self.inner.lock();
        if inner.stored.choice == on {
            return;
        }
        inner.stored.choice = on;
        if !on {
            Self::forget(&mut inner);
            let _ = std::fs::remove_file(&self.side);
        }
        inner.dirty = true;
        self.save(&mut inner, now_ms);
        if inner.dirty {
            inner.stored.choice = false;
        }
        self.mirror(&inner);
    }

    /// Studio's flag (`commands::content::apply_snapshot`). Off forgets the counts too.
    pub fn set_flag(&self, on: bool, now_ms: i64) {
        let mut inner = self.inner.lock();
        if inner.stored.flag == on {
            return;
        }
        inner.stored.flag = on;
        if !on {
            Self::forget(&mut inner);
            let _ = std::fs::remove_file(&self.side);
        }
        inner.dirty = true;
        self.save(&mut inner, now_ms);
        self.mirror(&inner);
    }

    fn mirror(&self, inner: &Inner) {
        self.on
            .store(inner.stored.choice && inner.stored.flag, Ordering::SeqCst);
    }

    fn forget(inner: &mut Inner) {
        let ledger = &mut inner.stored.ledger;
        // Only the pace of sending stays: "once a day" and "new computer" mean the same later.
        *ledger = Ledger {
            last_sent: ledger.last_sent,
            sent_before: ledger.sent_before,
            ..Ledger::default()
        };
        inner.voice = VoiceTracker::default();
    }

    /// One count, if counting is on: written at once (`urgent`) or with the next write.
    fn count(&self, now_ms: i64, urgent: bool, change: impl FnOnce(&mut Inner)) {
        let mut inner = self.inner.lock();
        if !(inner.stored.choice && inner.stored.flag) {
            return;
        }
        inner.stored.ledger.touch(secs(now_ms));
        change(&mut inner);
        inner.dirty = true;
        if urgent || now_ms - inner.saved_ms >= SAVE_EVERY_MS {
            self.save(&mut inner, now_ms);
        }
    }

    /// The launcher started and showed its window after `ms`.
    pub fn started(&self, ms: u32, now_ms: i64) {
        self.count(now_ms, true, |inner| {
            let counts = &mut inner.stored.ledger.counts;
            counts.starts = counts.starts.saturating_add(1);
            push_sample(&mut counts.start_ms, ms, START_SAMPLES);
        });
    }

    /// A launch ended in a game, or in an error before it (cancels count nothing).
    pub fn launched(&self, launch: Launch<'_>, now_ms: i64) {
        self.count(now_ms, true, |inner| {
            let counts = &mut inner.stored.ledger.counts;
            counts.launches = counts.launches.saturating_add(1);
            if let Launch::Failed(code) = launch {
                counts.add_fail(scrub::category(code));
            }
        });
    }

    /// A game's window showed.
    pub fn game_shown(&self, now_ms: i64) {
        self.count(now_ms, true, |inner| {
            let counts = &mut inner.stored.ledger.counts;
            counts.shown = counts.shown.saturating_add(1);
        });
    }

    /// A game crashed; `kind` is the crash analysis' (`modError`, `outOfMemory`).
    pub fn game_crashed(&self, kind: &str, now_ms: i64) {
        self.count(now_ms, true, |inner| {
            let counts = &mut inner.stored.ledger.counts;
            counts.game_crashes = counts.game_crashes.saturating_add(1);
            counts.add_fail(scrub::category(&format!("crash.{kind}")));
        });
    }

    /// A feature was used.
    pub fn feature(&self, feature: Feature, now_ms: i64) {
        self.count(now_ms, false, |inner| {
            inner.stored.ledger.counts.add_feature(feature);
        });
    }

    /// One look at the voice state ([`VoiceTracker::observe`]). Watched only while counting; a
    /// stay counts (and the file changes) only when it ends.
    pub fn voice(&self, seen: Option<VoiceSeen<'_>>, now_ms: i64) {
        let mut inner = self.inner.lock();
        // Out of a room with nothing to finish: nothing to do (most states).
        if !(inner.stored.choice && inner.stored.flag) || (seen.is_none() && !inner.voice.in_stay())
        {
            return;
        }
        let Some(stay) = inner.voice.observe(now_ms, seen) else {
            return;
        };
        inner.stored.ledger.touch(secs(now_ms));
        inner.stored.ledger.counts.voice.add(&stay);
        inner.dirty = true;
        if now_ms - inner.saved_ms >= SAVE_EVERY_MS {
            self.save(&mut inner, now_ms);
        }
    }

    /// The page threw and nothing caught it (`source` its script's URL, `line`, `column`).
    pub fn page_error(
        &self,
        message: &str,
        source: &str,
        line: u32,
        column: u32,
        version: &str,
        now_ms: i64,
    ) {
        let note = CrashNote {
            kind: CrashKind::Page,
            at: scrub::page_source(source, line, column),
            msg: scrub::message(message),
            ver: crate::facts::clean_version(version),
            n: 1,
        };
        self.count(now_ms, true, |inner| {
            if inner.page_errors >= PAGE_ERRORS_PER_RUN {
                return;
            }
            inner.page_errors += 1;
            let ledger = &mut inner.stored.ledger;
            ledger.counts.page_errors = ledger.counts.page_errors.saturating_add(1);
            ledger.add_crash(note, secs(now_ms));
        });
    }

    /// From the panic hook: the summary is kept at once (the process may end right after). Never
    /// waits: a recorder held by the panicking thread itself leaves the summary in the side file,
    /// taken in at the next start. True when a crash summary may be sent soon.
    pub fn panicked(&self, note: CrashNote, now_ms: i64) -> bool {
        let Some(mut inner) = self.inner.try_lock() else {
            if self.on.load(Ordering::SeqCst) {
                write_side(&self.side, &note);
            }
            return false;
        };
        if !(inner.stored.choice && inner.stored.flag) {
            return false;
        }
        let ledger = &mut inner.stored.ledger;
        ledger.counts.panics = ledger.counts.panics.saturating_add(1);
        ledger.add_crash(note, secs(now_ms));
        self.save(&mut inner, now_ms);
        true
    }

    /// The launcher closes: a stay in a voice room ends and waiting counts are written.
    pub fn flush(&self, now_ms: i64) {
        let mut inner = self.inner.lock();
        if inner.stored.choice
            && inner.stored.flag
            && let Some(stay) = inner.voice.close(now_ms)
        {
            inner.stored.ledger.counts.voice.add(&stay);
            inner.dirty = true;
        }
        if inner.dirty {
            self.save(&mut inner, now_ms);
        }
    }

    /// The daily batch, when it is due and no other request is on its way.
    pub fn take_daily(&self, facts: &AppFacts, now_ms: i64) -> Option<Outgoing> {
        self.take_daily_when(facts, now_ms, false)
    }

    /// The daily batch whatever the time since the last one, if it has counts (test runs of debug
    /// builds only: `PROJECTS_TELEMETRY_SEND_NOW`).
    pub fn take_daily_now(&self, facts: &AppFacts, now_ms: i64) -> Option<Outgoing> {
        self.take_daily_when(facts, now_ms, true)
    }

    fn take_daily_when(&self, facts: &AppFacts, now_ms: i64, now_anyway: bool) -> Option<Outgoing> {
        let mut inner = self.inner.lock();
        let now = secs(now_ms);
        let ledger = &inner.stored.ledger;
        let due = if now_anyway {
            ledger.has_counts()
        } else {
            batch::daily_due(ledger, now)
        };
        if !(inner.stored.choice && inner.stored.flag) || inner.sending || !due {
            return None;
        }
        inner.sending = true;
        Some(batch::daily(&inner.stored.ledger, facts))
    }

    /// The crash summaries on their own, when they may go.
    pub fn take_crash(&self, facts: &AppFacts, now_ms: i64) -> Option<Outgoing> {
        let mut inner = self.inner.lock();
        if !(inner.stored.choice && inner.stored.flag)
            || inner.sending
            || !batch::crash_due(&inner.stored.ledger, secs(now_ms))
        {
            return None;
        }
        inner.sending = true;
        Some(batch::crash_only(&inner.stored.ledger, facts))
    }

    /// The service's answer to the request that carried `carried`.
    pub fn answered(&self, carried: &Carried, answer: Answer, now_ms: i64) {
        let mut inner = self.inner.lock();
        inner.sending = false;
        if !(inner.stored.choice && inner.stored.flag) {
            // Turned off meanwhile: there is nothing left to take back.
            return;
        }
        let now = secs(now_ms);
        let ledger = &mut inner.stored.ledger;
        match answer {
            Answer::Taken | Answer::Refused => {
                if let Some(counts) = &carried.counts {
                    ledger.counts.subtract(counts);
                    ledger.last_sent = Some(now);
                    ledger.sent_before = true;
                } else {
                    ledger
                        .crash_sends
                        .retain(|at| *at <= now && now - at < 24 * 60 * 60);
                    ledger.crash_sends.push(now);
                }
                ledger.drop_crashes(&carried.crashes);
                // What came while the request was on its way starts the next batch now.
                ledger.since = ledger.has_counts().then_some(now);
                ledger.failures = 0;
                ledger.retry_at = None;
            }
            Answer::Later => {
                ledger.failures = ledger.failures.saturating_add(1);
                ledger.retry_at = Some(now + batch::retry_after(ledger.failures));
            }
        }
        self.save(&mut inner, now_ms);
    }

    /// A copy of the ledger (tests, the debug preview).
    pub fn ledger(&self) -> Ledger {
        self.inner.lock().stored.ledger.clone()
    }
}

/// The summary of a panic that could not reach the recorder (it is taken in by [`Recorder::open`]
/// when counting is on, and deleted either way).
pub fn write_side(path: &Path, note: &CrashNote) {
    if let Ok(bytes) = serde_json::to_vec(note) {
        let _ = std::fs::write(path, bytes);
    }
}

/// A panic before the recorder opened (`dir`: the data folder): its summary is left beside the
/// file only when the file says counting is on, so nothing is written while it is off.
pub fn panicked_early(dir: &Path, note: &CrashNote) {
    let stored = Stored::load(&dir.join(FILE));
    if stored.choice && stored.flag {
        write_side(&dir.join(SIDE_FILE), note);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::voice::Phase;

    const HOUR_MS: i64 = 3_600_000;

    fn facts() -> AppFacts {
        AppFacts::new(
            "2.0.8",
            "windows",
            Some((10, 0, 19_045)),
            "x86_64",
            "en",
            8_000,
        )
    }

    fn on(dir: &Path) -> Recorder {
        let recorder = Recorder::open(dir, "2.0.8", 0);
        recorder.set_flag(true, 0);
        recorder
    }

    fn panic_note() -> CrashNote {
        CrashNote {
            kind: CrashKind::Panic,
            at: "crates/pl-x/src/lib.rs:9".into(),
            msg: "boom".into(),
            ver: "2.0.8".into(),
            n: 1,
        }
    }

    #[test]
    fn nothing_is_counted_until_studio_turns_it_on() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = Recorder::open(dir.path(), "2.0.8", 0);
        assert!(recorder.choice() && !recorder.flag() && !recorder.active());
        recorder.started(1_500, 1_000);
        recorder.feature(Feature::Modrinth, 2_000);
        assert!(!recorder.panicked(panic_note(), 3_000));
        assert_eq!(recorder.ledger(), Ledger::default());
        assert!(recorder.take_daily(&facts(), 30 * HOUR_MS).is_none());
    }

    #[test]
    fn turning_it_off_forgets_what_was_not_sent() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        recorder.started(1_500, 1_000);
        recorder.launched(Launch::Failed("JAVA"), 2_000);
        assert!(recorder.ledger().has_counts());
        recorder.set_choice(false, 3_000);
        assert!(!recorder.ledger().has_counts());
        recorder.launched(Launch::Started, 4_000);
        assert!(!recorder.ledger().has_counts());
        // And it stays off after a restart.
        let again = Recorder::open(dir.path(), "2.0.8", 5_000);
        assert!(!again.choice() && !again.active());
    }

    #[test]
    fn a_day_of_counts_goes_once_and_comes_off_the_ledger() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        recorder.started(1_800, 1_000);
        recorder.launched(Launch::Started, 2_000);
        recorder.game_shown(3_000);
        recorder.game_crashed("modError", 4_000);
        recorder.launched(Launch::Failed("DISK-DOLU"), 5_000);
        recorder.feature(Feature::Worlds, 6_000);
        assert!(
            recorder.take_daily(&facts(), 10 * HOUR_MS).is_none(),
            "too early"
        );
        let out = recorder.take_daily(&facts(), 21 * HOUR_MS).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out.json).unwrap();
        assert_eq!(value["day"]["launches"], 2);
        assert_eq!(value["day"]["ok"], 1);
        assert_eq!(value["fail"]["crash.modError"], 1);
        assert_eq!(value["fail"]["DISK-DOLU"], 1);
        assert_eq!(value["feat"]["worlds"], 1);
        // One request at a time.
        assert!(recorder.take_daily(&facts(), 21 * HOUR_MS).is_none());
        // A count while it is on its way stays for the next batch.
        recorder.feature(Feature::Chat, 21 * HOUR_MS + 1);
        recorder.answered(&out.carried, Answer::Taken, 21 * HOUR_MS + 2);
        let ledger = recorder.ledger();
        assert_eq!(ledger.counts.feat.get("chat"), Some(&1));
        assert_eq!(ledger.counts.launches, 0);
        assert!(ledger.sent_before);
        assert!(
            recorder.take_daily(&facts(), 30 * HOUR_MS).is_none(),
            "a day at most"
        );
        let next = recorder.take_daily(&facts(), 42 * HOUR_MS).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&next.json).unwrap();
        assert!(value.get("new").is_none());
    }

    #[test]
    fn a_test_run_may_send_at_once_but_never_while_off() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        assert!(
            recorder.take_daily_now(&facts(), 1_000).is_none(),
            "nothing counted"
        );
        recorder.started(1_800, 1_000);
        assert!(recorder.take_daily(&facts(), 2_000).is_none());
        let out = recorder.take_daily_now(&facts(), 2_000).unwrap();
        recorder.answered(&out.carried, Answer::Taken, 2_000);
        recorder.set_flag(false, 3_000);
        recorder.started(1_800, 4_000);
        assert!(recorder.take_daily_now(&facts(), 5_000).is_none());
    }

    #[test]
    fn a_failed_send_waits_and_keeps_everything() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        recorder.started(1_800, 1_000);
        let out = recorder.take_daily(&facts(), 21 * HOUR_MS).unwrap();
        recorder.answered(&out.carried, Answer::Later, 21 * HOUR_MS);
        assert_eq!(recorder.ledger().counts.starts, 1);
        assert!(
            recorder.take_daily(&facts(), 21 * HOUR_MS + 1000).is_none(),
            "waits an hour"
        );
        assert!(recorder.take_daily(&facts(), 22 * HOUR_MS).is_some());
    }

    #[test]
    fn a_refused_batch_is_not_sent_again() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        recorder.started(1_800, 1_000);
        let out = recorder.take_daily(&facts(), 21 * HOUR_MS).unwrap();
        recorder.answered(&out.carried, Answer::Refused, 21 * HOUR_MS);
        assert!(!recorder.ledger().has_counts());
    }

    #[test]
    fn panics_go_soon_but_rationed() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        assert!(recorder.panicked(panic_note(), 1_000));
        let out = recorder.take_crash(&facts(), 2_000).unwrap();
        let value: serde_json::Value = serde_json::from_slice(&out.json).unwrap();
        assert_eq!(value["crash"][0]["k"], "panic");
        assert!(value.get("day").is_none());
        recorder.answered(&out.carried, Answer::Taken, 2_000);
        let ledger = recorder.ledger();
        assert!(ledger.crashes.is_empty());
        // The day still counts it.
        assert_eq!(ledger.counts.panics, 1);
        assert!(recorder.panicked(panic_note(), 3_000));
        assert!(
            recorder.take_crash(&facts(), 4_000).is_none(),
            "an hour apart"
        );
        assert!(recorder.take_crash(&facts(), HOUR_MS + 2_000).is_some());
    }

    #[test]
    fn a_panic_before_the_recorder_opened_comes_in_at_the_next_start() {
        let dir = tempfile::tempdir().unwrap();
        on(dir.path());
        // Before `setup` the hook knows no version: the next start's fills it in.
        let early = CrashNote {
            ver: String::new(),
            ..panic_note()
        };
        panicked_early(dir.path(), &early);
        let recorder = Recorder::open(dir.path(), "2.0.9+dev", 10_000);
        let ledger = recorder.ledger();
        assert_eq!(ledger.crashes.len(), 1);
        assert_eq!(ledger.crashes[0].ver, "2.0.9");
        assert_eq!(ledger.counts.panics, 1);
        assert!(!dir.path().join(SIDE_FILE).exists());
        // While counting is off, an early panic leaves nothing, and a left file is only deleted.
        recorder.set_choice(false, 11_000);
        panicked_early(dir.path(), &panic_note());
        assert!(!dir.path().join(SIDE_FILE).exists());
        write_side(&dir.path().join(SIDE_FILE), &panic_note());
        let off = Recorder::open(dir.path(), "2.0.8", 12_000);
        assert!(off.ledger().crashes.is_empty());
        assert!(!dir.path().join(SIDE_FILE).exists());
    }

    #[test]
    fn page_errors_are_scrubbed_and_capped() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        for _ in 0..30 {
            recorder.page_error(
                "Failed to load C:\\Users\\Kaan\\x.png",
                "http://tauri.localhost/assets/index-a1.js",
                1,
                99,
                "2.0.8+dev",
                1_000,
            );
        }
        let ledger = recorder.ledger();
        assert_eq!(ledger.counts.page_errors, 20);
        let note = &ledger.crashes[0];
        assert_eq!(note.msg, "Failed to load <path>");
        assert_eq!(note.at, "assets/index-a1.js:1:99");
        assert_eq!(note.ver, "2.0.8");
        assert_eq!(note.n, 20);
    }

    #[test]
    fn voice_stays_are_counted_when_they_end() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        let seen = |phase| {
            Some(VoiceSeen {
                room: "room-1",
                crew: false,
                phase,
                quality: 0,
            })
        };
        recorder.voice(None, 0);
        recorder.voice(seen(Phase::Joining), 1_000);
        recorder.voice(seen(Phase::Live), 2_000);
        assert_eq!(recorder.ledger().counts.voice.stays, 0);
        recorder.voice(None, 62_000);
        let voice = recorder.ledger().counts.voice;
        assert_eq!(
            (voice.stays, voice.live_secs, voice.setup_ms),
            (1, 60, vec![1_000])
        );
        // A stay still open when the launcher closes ends there.
        recorder.voice(seen(Phase::Live), 70_000);
        recorder.flush(80_000);
        assert_eq!(
            Recorder::open(dir.path(), "2.0.8", 90_000)
                .ledger()
                .counts
                .voice
                .stays,
            2
        );
    }

    #[test]
    fn frequent_counts_are_written_at_most_once_a_minute() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        recorder.feature(Feature::Chat, 1_000);
        assert!(
            Recorder::open(dir.path(), "2.0.8", 1_500)
                .ledger()
                .counts
                .feat
                .is_empty()
        );
        recorder.feature(Feature::Chat, 61_000);
        assert_eq!(
            Recorder::open(dir.path(), "2.0.8", 61_500)
                .ledger()
                .counts
                .feat
                .get("chat"),
            Some(&2)
        );
    }

    #[test]
    fn a_choice_the_disk_does_not_keep_leaves_counting_off() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        recorder.set_choice(false, 1_000);
        // A folder in the file's place: no write lands.
        std::fs::remove_file(dir.path().join(FILE)).unwrap();
        std::fs::create_dir(dir.path().join(FILE)).unwrap();
        recorder.set_choice(true, 2_000);
        assert!(!recorder.choice() && !recorder.active());
        recorder.started(1_000, 3_000);
        assert!(recorder.ledger().counts.is_empty());
    }

    #[test]
    fn a_panic_on_a_busy_recorder_leaves_nothing_while_off() {
        let dir = tempfile::tempdir().unwrap();
        let recorder = on(dir.path());
        {
            let _busy = recorder.inner.lock();
            assert!(!recorder.panicked(panic_note(), 1_000));
        }
        assert!(
            dir.path().join(SIDE_FILE).exists(),
            "on: left for the next start"
        );
        // Turning it off deletes it, and a busy recorder that is off writes none.
        recorder.set_choice(false, 2_000);
        assert!(!dir.path().join(SIDE_FILE).exists());
        {
            let _busy = recorder.inner.lock();
            assert!(!recorder.panicked(panic_note(), 3_000));
        }
        assert!(!dir.path().join(SIDE_FILE).exists());
    }
}
