//! What the computer keeps between sends: the player's choice, Studio's flag and the counts of
//! the batch that is being gathered. One small JSON file in the data folder (`telemetry.json`),
//! bounded in size: every list has a cap, and nothing in it names anyone.

use std::collections::BTreeMap;
use std::io::Write;
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::feature::Feature;
use crate::voice::VoiceStay;

/// Startup samples one batch keeps (the batch sends their middle and their largest).
pub const START_SAMPLES: usize = 16;
/// Failure categories one batch keeps; more add to `other`.
pub const FAIL_KEYS: usize = 12;
/// Distinct crash summaries the file keeps (a batch sends the most frequent [`CRASHES_SENT`]).
pub const CRASHES_KEPT: usize = 8;
pub const CRASHES_SENT: usize = 3;
/// Voice setup samples one batch keeps.
pub const SETUP_SAMPLES: usize = 16;
/// Distinct game crash traces the file keeps (a batch sends the most frequent [`GAMES_SENT`]).
pub const GAMES_KEPT: usize = 6;
pub const GAMES_SENT: usize = 2;

/// What a crash summary is about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum CrashKind {
    /// The launcher's own code panicked.
    Panic,
    /// The page threw and nothing caught it.
    Page,
}

impl CrashKind {
    pub fn key(self) -> &'static str {
        match self {
            Self::Panic => "panic",
            Self::Page => "page",
        }
    }
}

/// One crash summary, already scrubbed ([`crate::scrub`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CrashNote {
    pub kind: CrashKind,
    /// `crates/pl-mc/src/launch/mod.rs:412`, `assets/index-B9x2.js:1:2345`.
    pub at: String,
    pub msg: String,
    /// The launcher version it happened in (an update may come before the batch goes).
    pub ver: String,
    /// How often, since the last send.
    pub n: u32,
}

impl CrashNote {
    fn same(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.at == other.at
            && self.msg == other.msg
            && self.ver == other.ver
    }
}

/// One game crash's coarse trace (TELEM-FIX), already scrubbed ([`crate::scrub::trace`]): what
/// kind of code failed and where, from closed lists (`pl_mc`'s `CrashTrace`), never a path, a
/// name, a world or a stack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct GameCrash {
    /// The crash analysis' kind (`modCrash`, `unknown`).
    pub kind: String,
    /// `java/NullPointerException/sodium`, `native/gpu-amd`, `exit/hung`.
    pub sig: String,
    /// Where the game was, from the crash report (`Rendering overlay`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub phase: Option<String>,
    /// The launcher version it happened in.
    pub ver: String,
    /// How often, since the last send.
    pub n: u32,
}

impl GameCrash {
    fn same(&self, other: &Self) -> bool {
        self.kind == other.kind
            && self.sig == other.sig
            && self.phase == other.phase
            && self.ver == other.ver
    }
}

/// Voice chat's stays since the last send.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct VoiceCounts {
    pub stays: u32,
    pub failed: u32,
    pub crew: u32,
    pub live_secs: u32,
    pub weak_secs: u32,
    pub bad_secs: u32,
    pub reconnects: u32,
    pub setup_ms: Vec<u32>,
}

impl VoiceCounts {
    pub fn add(&mut self, stay: &VoiceStay) {
        self.stays = self.stays.saturating_add(1);
        self.failed = self.failed.saturating_add(u32::from(stay.failed));
        self.crew = self.crew.saturating_add(u32::from(stay.crew));
        self.live_secs = self.live_secs.saturating_add(stay.live_secs);
        self.weak_secs = self.weak_secs.saturating_add(stay.weak_secs);
        self.bad_secs = self.bad_secs.saturating_add(stay.bad_secs);
        self.reconnects = self.reconnects.saturating_add(stay.reconnects);
        if let Some(ms) = stay.setup_ms {
            push_sample(&mut self.setup_ms, ms, SETUP_SAMPLES);
        }
    }

    pub fn is_empty(&self) -> bool {
        self.stays == 0
    }
}

/// The counts of the batch being gathered.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Counts {
    /// Launcher starts, and how long each took to show its window (ms).
    pub starts: u32,
    pub start_ms: Vec<u32>,
    /// Launches that started a game or failed before it (cancels are neither).
    pub launches: u32,
    /// Games whose window showed.
    pub shown: u32,
    /// Games that crashed (any time).
    pub game_crashes: u32,
    /// The launcher's own panics, and the page's uncaught errors.
    pub panics: u32,
    pub page_errors: u32,
    /// Failures by category: a launch error's code (`JAVA`), a game crash's kind (`crash.modError`).
    pub fail: BTreeMap<String, u32>,
    /// Feature uses by wire name ([`Feature::key`]).
    pub feat: BTreeMap<String, u32>,
    pub voice: VoiceCounts,
}

impl Counts {
    pub fn is_empty(&self) -> bool {
        *self == Self::default()
    }

    pub fn add_fail(&mut self, category: String) {
        if !self.fail.contains_key(&category) && self.fail.len() >= FAIL_KEYS {
            let other = self.fail.entry("other".to_owned()).or_default();
            *other = other.saturating_add(1);
            return;
        }
        let count = self.fail.entry(category).or_default();
        *count = count.saturating_add(1);
    }

    pub fn add_feature(&mut self, feature: Feature) {
        let count = self.feat.entry(feature.key().to_owned()).or_default();
        *count = count.saturating_add(1);
    }

    /// Takes away what a batch carried, so what came while it was on its way stays.
    pub fn subtract(&mut self, sent: &Self) {
        self.starts = self.starts.saturating_sub(sent.starts);
        self.start_ms = rest_of(&self.start_ms, &sent.start_ms);
        self.launches = self.launches.saturating_sub(sent.launches);
        self.shown = self.shown.saturating_sub(sent.shown);
        self.game_crashes = self.game_crashes.saturating_sub(sent.game_crashes);
        self.panics = self.panics.saturating_sub(sent.panics);
        self.page_errors = self.page_errors.saturating_sub(sent.page_errors);
        subtract_map(&mut self.fail, &sent.fail);
        subtract_map(&mut self.feat, &sent.feat);
        let voice = &mut self.voice;
        let gone = &sent.voice;
        voice.stays = voice.stays.saturating_sub(gone.stays);
        voice.failed = voice.failed.saturating_sub(gone.failed);
        voice.crew = voice.crew.saturating_sub(gone.crew);
        voice.live_secs = voice.live_secs.saturating_sub(gone.live_secs);
        voice.weak_secs = voice.weak_secs.saturating_sub(gone.weak_secs);
        voice.bad_secs = voice.bad_secs.saturating_sub(gone.bad_secs);
        voice.reconnects = voice.reconnects.saturating_sub(gone.reconnects);
        voice.setup_ms = rest_of(&voice.setup_ms, &gone.setup_ms);
    }
}

/// The samples a batch did not carry (it carried the first ones).
fn rest_of(all: &[u32], sent: &[u32]) -> Vec<u32> {
    if all.starts_with(sent) {
        all[sent.len()..].to_vec()
    } else {
        all.to_vec()
    }
}

fn subtract_map(map: &mut BTreeMap<String, u32>, sent: &BTreeMap<String, u32>) {
    for (key, n) in sent {
        if let Some(count) = map.get_mut(key) {
            *count = count.saturating_sub(*n);
        }
    }
    map.retain(|_, count| *count > 0);
}

/// Keeps the newest `cap` samples.
pub fn push_sample(samples: &mut Vec<u32>, value: u32, cap: usize) {
    samples.push(value);
    if samples.len() > cap {
        let extra = samples.len() - cap;
        samples.drain(..extra);
    }
}

/// The batch being gathered and the state of sending.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Ledger {
    /// When the first count of this batch came (Unix seconds).
    pub since: Option<i64>,
    /// When the service last took a daily batch (Unix seconds).
    pub last_sent: Option<i64>,
    /// The service has taken a daily batch from this computer before.
    pub sent_before: bool,
    /// No send before this (Unix seconds): the wait after a failed one.
    pub retry_at: Option<i64>,
    /// Failed sends in a row.
    pub failures: u32,
    pub counts: Counts,
    pub crashes: Vec<CrashNote>,
    /// When crash summaries went out on their own (Unix seconds, the last day's).
    pub crash_sends: Vec<i64>,
    /// Game crash traces; only the daily batch carries them.
    pub games: Vec<GameCrash>,
}

impl Ledger {
    /// A count came: the batch starts now if it had not.
    pub fn touch(&mut self, now: i64) {
        if self.since.is_none_or(|since| since > now) {
            self.since = Some(now);
        }
    }

    pub fn add_crash(&mut self, note: CrashNote, now: i64) {
        self.touch(now);
        if let Some(known) = self.crashes.iter_mut().find(|known| known.same(&note)) {
            known.n = known.n.saturating_add(note.n.max(1));
            return;
        }
        if self.crashes.len() >= CRASHES_KEPT {
            // A new kind of crash weighs more than the rarest known one.
            if let Some(rarest) = self
                .crashes
                .iter()
                .enumerate()
                .min_by_key(|(_, known)| known.n)
                .map(|(index, _)| index)
            {
                self.crashes.remove(rarest);
            }
        }
        self.crashes.push(note);
    }

    /// The summaries a batch carries: the most frequent first.
    pub fn crashes_to_send(&self) -> Vec<CrashNote> {
        let mut notes = self.crashes.clone();
        notes.sort_by_key(|note| std::cmp::Reverse(note.n));
        notes.truncate(CRASHES_SENT);
        notes
    }

    /// Takes away the summaries a batch carried.
    pub fn drop_crashes(&mut self, sent: &[CrashNote]) {
        for gone in sent {
            if let Some(index) = self.crashes.iter().position(|known| known.same(gone)) {
                let left = self.crashes[index].n.saturating_sub(gone.n);
                if left == 0 {
                    self.crashes.remove(index);
                } else {
                    self.crashes[index].n = left;
                }
            }
        }
    }

    pub fn add_game(&mut self, game: GameCrash, now: i64) {
        self.touch(now);
        if let Some(known) = self.games.iter_mut().find(|known| known.same(&game)) {
            known.n = known.n.saturating_add(game.n.max(1));
            return;
        }
        if self.games.len() >= GAMES_KEPT
            && let Some(rarest) = self
                .games
                .iter()
                .enumerate()
                .min_by_key(|(_, known)| known.n)
                .map(|(index, _)| index)
        {
            self.games.remove(rarest);
        }
        self.games.push(game);
    }

    /// The game crash traces a daily batch carries: the most frequent first.
    pub fn games_to_send(&self) -> Vec<GameCrash> {
        let mut games = self.games.clone();
        games.sort_by_key(|game| std::cmp::Reverse(game.n));
        games.truncate(GAMES_SENT);
        games
    }

    /// Takes away the traces a batch carried.
    pub fn drop_games(&mut self, sent: &[GameCrash]) {
        for gone in sent {
            if let Some(index) = self.games.iter().position(|known| known.same(gone)) {
                let left = self.games[index].n.saturating_sub(gone.n);
                if left == 0 {
                    self.games.remove(index);
                } else {
                    self.games[index].n = left;
                }
            }
        }
    }

    pub fn has_counts(&self) -> bool {
        !self.counts.is_empty() || !self.crashes.is_empty() || !self.games.is_empty()
    }
}

/// The whole file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", default)]
pub struct Stored {
    /// Settings → Veri → "Kullanım verisi gönder" (on until the player turns it off).
    pub choice: bool,
    /// The signed config's `features.telemetry`, as last applied (off until Studio turns it on).
    pub flag: bool,
    pub ledger: Ledger,
}

impl Default for Stored {
    fn default() -> Self {
        Self {
            choice: true,
            flag: false,
            ledger: Ledger::default(),
        }
    }
}

impl Stored {
    /// The file, or the defaults when there is none. A file that is there but cannot be read or
    /// is damaged reads as the switch off (never fatal): it may hold a player's "off", and a broken
    /// file must not turn sending back on. The row then shows the switch off, so it stays honest.
    pub fn load(path: &Path) -> Self {
        match std::fs::read(path) {
            Ok(bytes) => serde_json::from_slice(&bytes).unwrap_or_else(|_| Self::switched_off()),
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => Self::default(),
            Err(_) => Self::switched_off(),
        }
    }

    fn switched_off() -> Self {
        Self {
            choice: false,
            ..Self::default()
        }
    }

    /// Written whole through a temporary file that reaches the disk before it takes the old one's
    /// place, so a crash or a power cut mid-write leaves one whole file.
    pub fn save(&self, path: &Path) -> std::io::Result<()> {
        let bytes = serde_json::to_vec(self).map_err(std::io::Error::other)?;
        let temp = path.with_extension("json.tmp");
        let mut file = std::fs::File::create(&temp)?;
        file.write_all(&bytes)?;
        file.sync_all()?;
        drop(file);
        std::fs::rename(&temp, path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn note(at: &str, n: u32) -> CrashNote {
        CrashNote {
            kind: CrashKind::Panic,
            at: at.to_owned(),
            msg: "boom".to_owned(),
            ver: "2.0.8".to_owned(),
            n,
        }
    }

    #[test]
    fn failure_categories_are_capped() {
        let mut counts = Counts::default();
        for i in 0..20 {
            counts.add_fail(format!("E{i}"));
        }
        counts.add_fail("E0".to_owned());
        assert_eq!(counts.fail.len(), FAIL_KEYS + 1);
        assert_eq!(counts.fail["other"], 8);
        assert_eq!(counts.fail["E0"], 2);
    }

    #[test]
    fn subtracting_keeps_what_came_meanwhile() {
        let mut counts = Counts {
            starts: 3,
            start_ms: vec![1, 2, 3],
            launches: 2,
            ..Counts::default()
        };
        counts.add_fail("JAVA".to_owned());
        counts.add_feature(Feature::Modrinth);
        let sent = counts.clone();
        counts.starts += 1;
        counts.start_ms.push(4);
        counts.add_fail("JAVA".to_owned());
        counts.subtract(&sent);
        assert_eq!(counts.starts, 1);
        assert_eq!(counts.start_ms, vec![4]);
        assert_eq!(counts.launches, 0);
        assert_eq!(counts.fail["JAVA"], 1);
        assert!(counts.feat.is_empty());
    }

    #[test]
    fn samples_keep_the_newest() {
        let mut samples = Vec::new();
        for i in 0..40 {
            push_sample(&mut samples, i, START_SAMPLES);
        }
        assert_eq!(samples.len(), START_SAMPLES);
        assert_eq!(samples[0], 24);
    }

    #[test]
    fn crashes_merge_and_stay_few() {
        let mut ledger = Ledger::default();
        ledger.add_crash(note("a.rs:1", 1), 10);
        ledger.add_crash(note("a.rs:1", 1), 11);
        assert_eq!(ledger.crashes.len(), 1);
        assert_eq!(ledger.crashes[0].n, 2);
        assert_eq!(ledger.since, Some(10));
        for i in 0..20 {
            ledger.add_crash(note(&format!("b{i}.rs:1"), 1), 12);
        }
        assert_eq!(ledger.crashes.len(), CRASHES_KEPT);
        // The frequent one stays.
        assert!(ledger.crashes.iter().any(|c| c.at == "a.rs:1"));
        let sent = ledger.crashes_to_send();
        assert_eq!(sent.len(), CRASHES_SENT);
        assert_eq!(sent[0].at, "a.rs:1");
        ledger.add_crash(note("a.rs:1", 1), 13);
        ledger.drop_crashes(&sent);
        // One came while the batch was on its way.
        assert_eq!(
            ledger.crashes.iter().find(|c| c.at == "a.rs:1").unwrap().n,
            1
        );
    }

    #[test]
    fn the_file_round_trips_and_survives_damage() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("telemetry.json");
        assert_eq!(Stored::load(&path), Stored::default());
        let mut stored = Stored {
            flag: true,
            ..Stored::default()
        };
        stored.ledger.counts.starts = 2;
        stored.ledger.add_crash(note("a.rs:1", 1), 5);
        stored.save(&path).unwrap();
        assert_eq!(Stored::load(&path), stored);
        // A damaged file may have held a player's "off": it never turns sending back on.
        std::fs::write(&path, b"{not json").unwrap();
        let damaged = Stored::load(&path);
        assert!(!damaged.choice && !damaged.flag);
        assert_eq!(damaged.ledger, Ledger::default());
        std::fs::write(&path, br#"{"choice":"no","flag":true}"#).unwrap();
        assert!(!Stored::load(&path).choice);
        // An older or newer file with fields it does not know still reads.
        std::fs::write(&path, br#"{"choice":false,"later":1}"#).unwrap();
        let older = Stored::load(&path);
        assert!(!older.choice && !older.flag);
    }
}
