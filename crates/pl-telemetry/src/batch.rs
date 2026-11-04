//! The request body and when it may go.
//!
//! One daily batch: at most one every [`DAY_GAP_SECS`], each covering at least that long, so a
//! computer sends about once a day whatever it does in between. A panic's summary may go on its
//! own soon after it, at most once an hour and [`CRASH_SENDS_PER_DAY`] times a day; the rest (and
//! every page error) wait for the daily batch. A failed send waits longer each time (1, 2, 4 … 24
//! hours). The body is JSON (`v: 1`, described in `services/studio-team/README.md` →
//! "Telemetry"), gzipped. It says nothing about when the last one went: the service keeps each
//! batch's time, and two such times would let batches be chained into one computer's history.

use std::collections::BTreeMap;
use std::io::Write;

use serde::Serialize;

use crate::facts::AppFacts;
use crate::ledger::{Counts, CrashKind, CrashNote, Ledger};

/// The wire format's version.
pub const WIRE_VERSION: u8 = 1;
/// Least time between two daily batches, and least time a batch gathers.
pub const DAY_GAP_SECS: i64 = 20 * 60 * 60;
/// Least time between two crash summaries sent on their own.
pub const CRASH_GAP_SECS: i64 = 60 * 60;
pub const CRASH_SENDS_PER_DAY: usize = 3;
/// The longest wait after failed sends.
pub const RETRY_MAX_SECS: i64 = 24 * 60 * 60;

#[derive(Debug, Serialize)]
struct Wire<'a> {
    v: u8,
    app: &'a AppFacts,
    #[serde(skip_serializing_if = "std::ops::Not::not")]
    new: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    day: Option<WireDay>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    fail: BTreeMap<String, u32>,
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    feat: BTreeMap<String, u32>,
    #[serde(skip_serializing_if = "Option::is_none")]
    voice: Option<WireVoice>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    crash: Vec<WireCrash>,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireDay {
    starts: u32,
    /// The middle and the largest startup time (ms); 0 when none was measured.
    start_ms: u32,
    start_max: u32,
    launches: u32,
    ok: u32,
    crashes: u32,
    panics: u32,
    perr: u32,
}

#[derive(Debug, Serialize)]
#[serde(rename_all = "camelCase")]
struct WireVoice {
    n: u32,
    fail: u32,
    crew: u32,
    live: u32,
    weak: u32,
    bad: u32,
    rec: u32,
    setup_ms: u32,
}

#[derive(Debug, Serialize)]
struct WireCrash {
    k: &'static str,
    at: String,
    msg: String,
    ver: String,
    n: u32,
}

/// The middle of the samples (the upper one of an even count); 0 for none.
pub fn median(samples: &[u32]) -> u32 {
    let mut sorted = samples.to_vec();
    sorted.sort_unstable();
    sorted.get(sorted.len() / 2).copied().unwrap_or(0)
}

fn crashes(notes: &[CrashNote]) -> Vec<WireCrash> {
    notes
        .iter()
        .map(|note| WireCrash {
            k: note.kind.key(),
            at: note.at.clone(),
            msg: note.msg.clone(),
            ver: note.ver.clone(),
            n: note.n,
        })
        .collect()
}

/// What a request carried: taken away from the ledger once the service has it.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Carried {
    /// The daily counts (none for a crash summary on its own).
    pub counts: Option<Counts>,
    pub crashes: Vec<CrashNote>,
}

/// A request ready to go.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Outgoing {
    /// The JSON before compression (debug builds show it).
    pub json: Vec<u8>,
    /// What goes over the wire.
    pub body: Vec<u8>,
    pub carried: Carried,
}

/// The daily batch of `ledger` (whether it is due is [`daily_due`]'s).
pub fn daily(ledger: &Ledger, facts: &AppFacts) -> Outgoing {
    let counts = ledger.counts.clone();
    let notes = ledger.crashes_to_send();
    let voice = &counts.voice;
    let wire = Wire {
        v: WIRE_VERSION,
        app: facts,
        new: !ledger.sent_before,
        day: Some(WireDay {
            starts: counts.starts,
            start_ms: median(&counts.start_ms),
            start_max: counts.start_ms.iter().copied().max().unwrap_or(0),
            launches: counts.launches,
            ok: counts.shown,
            crashes: counts.game_crashes,
            panics: counts.panics,
            perr: counts.page_errors,
        }),
        fail: counts.fail.clone(),
        feat: counts.feat.clone(),
        voice: (!voice.is_empty()).then(|| WireVoice {
            n: voice.stays,
            fail: voice.failed,
            crew: voice.crew,
            live: voice.live_secs,
            weak: voice.weak_secs,
            bad: voice.bad_secs,
            rec: voice.reconnects,
            setup_ms: median(&voice.setup_ms),
        }),
        crash: crashes(&notes),
    };
    outgoing(
        &wire,
        Carried {
            counts: Some(counts),
            crashes: notes,
        },
    )
}

/// The panics' summaries alone, soon after a panic (page errors wait for the daily batch).
pub fn crash_only(ledger: &Ledger, facts: &AppFacts) -> Outgoing {
    let mut notes: Vec<CrashNote> = ledger
        .crashes
        .iter()
        .filter(|note| note.kind == CrashKind::Panic)
        .cloned()
        .collect();
    notes.sort_by_key(|note| std::cmp::Reverse(note.n));
    notes.truncate(crate::ledger::CRASHES_SENT);
    let wire = Wire {
        v: WIRE_VERSION,
        app: facts,
        new: false,
        day: None,
        fail: BTreeMap::new(),
        feat: BTreeMap::new(),
        voice: None,
        crash: crashes(&notes),
    };
    outgoing(
        &wire,
        Carried {
            counts: None,
            crashes: notes,
        },
    )
}

fn outgoing(wire: &Wire<'_>, carried: Carried) -> Outgoing {
    // Plain data: serialising it cannot fail.
    let json = serde_json::to_vec(wire).unwrap_or_default();
    Outgoing {
        body: gzip(&json),
        json,
        carried,
    }
}

/// Gzip, as `Content-Encoding: gzip` says.
pub fn gzip(bytes: &[u8]) -> Vec<u8> {
    let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
    // Writing into memory cannot fail.
    let _ = encoder.write_all(bytes);
    encoder.finish().unwrap_or_default()
}

/// No failed send asks to wait now. A wait longer than the longest one is a clock that went
/// back: it does not hold the batch for months.
fn may_retry(ledger: &Ledger, now: i64) -> bool {
    ledger
        .retry_at
        .is_none_or(|at| now >= at || at - now > RETRY_MAX_SECS)
}

/// Whether the daily batch may go now: it has counts, gathered for [`DAY_GAP_SECS`], the last one
/// went that long ago (or the clock went back past it), and no failed send asks to wait.
pub fn daily_due(ledger: &Ledger, now: i64) -> bool {
    let Some(since) = ledger.since else {
        return false;
    };
    ledger.has_counts()
        && may_retry(ledger, now)
        && now - since >= DAY_GAP_SECS
        && ledger
            .last_sent
            .is_none_or(|last| now - last >= DAY_GAP_SECS || last > now)
}

/// Whether crash summaries may go on their own now: only for a panic (the launcher may be about to
/// go); page errors wait for the daily batch, so they never add a request.
pub fn crash_due(ledger: &Ledger, now: i64) -> bool {
    let recent: Vec<i64> = ledger
        .crash_sends
        .iter()
        .copied()
        .filter(|at| *at <= now && now - at < 24 * 60 * 60)
        .collect();
    ledger
        .crashes
        .iter()
        .any(|note| note.kind == CrashKind::Panic)
        && may_retry(ledger, now)
        && recent.len() < CRASH_SENDS_PER_DAY
        && recent.iter().all(|at| now - at >= CRASH_GAP_SECS)
}

/// The wait after `failures` failed sends in a row.
pub fn retry_after(failures: u32) -> i64 {
    let hours = 1_i64 << failures.saturating_sub(1).min(5);
    (hours * 60 * 60).min(RETRY_MAX_SECS)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::feature::Feature;

    fn facts() -> AppFacts {
        AppFacts::new(
            "2.0.8",
            "windows",
            Some((10, 0, 26_100)),
            "x86_64",
            "tr",
            16_000,
        )
    }

    fn note() -> CrashNote {
        CrashNote {
            kind: CrashKind::Panic,
            at: "crates/pl-mc/src/a.rs:3".into(),
            msg: "called `Option::unwrap()` on a `None` value".into(),
            ver: "2.0.7".into(),
            n: 2,
        }
    }

    fn json(out: &Outgoing) -> serde_json::Value {
        serde_json::from_slice(&out.json).unwrap()
    }

    #[test]
    fn a_daily_batch_says_what_was_counted() {
        let mut ledger = Ledger {
            since: Some(0),
            ..Ledger::default()
        };
        ledger.counts.starts = 3;
        ledger.counts.start_ms = vec![2_000, 1_500, 4_000];
        ledger.counts.launches = 4;
        ledger.counts.shown = 3;
        ledger.counts.game_crashes = 1;
        ledger.counts.add_fail("JAVA".into());
        ledger.counts.add_feature(Feature::Modrinth);
        ledger.add_crash(note(), 0);
        let out = daily(&ledger, &facts());
        assert_eq!(
            json(&out),
            serde_json::json!({
                "v": 1,
                "app": {"ver": "2.0.8", "os": "windows", "osv": "11", "arch": "x86_64", "lang": "tr", "ram": 16},
                "new": true,
                "day": {"starts": 3, "startMs": 2000, "startMax": 4000, "launches": 4, "ok": 3,
                        "crashes": 1, "panics": 0, "perr": 0},
                "fail": {"JAVA": 1},
                "feat": {"modrinth": 1},
                "crash": [{"k": "panic", "at": "crates/pl-mc/src/a.rs:3",
                           "msg": "called `Option::unwrap()` on a `None` value", "ver": "2.0.7", "n": 2}]
            })
        );
        assert_eq!(out.carried.counts.as_ref(), Some(&ledger.counts));
        assert_eq!(out.carried.crashes.len(), 1);
        // The body is the JSON, gzipped.
        let mut plain = Vec::new();
        std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(&out.body[..]), &mut plain)
            .unwrap();
        assert_eq!(plain, out.json);
    }

    #[test]
    fn voice_goes_only_when_there_were_stays() {
        let mut ledger = Ledger {
            since: Some(0),
            sent_before: true,
            ..Ledger::default()
        };
        ledger.counts.voice.add(&crate::voice::VoiceStay {
            crew: true,
            connected: true,
            setup_ms: Some(900),
            live_secs: 600,
            weak_secs: 30,
            ..Default::default()
        });
        let value = json(&daily(&ledger, &facts()));
        assert_eq!(
            value["voice"],
            serde_json::json!({"n": 1, "fail": 0, "crew": 1, "live": 600, "weak": 30, "bad": 0, "rec": 0, "setupMs": 900})
        );
        assert!(value.get("new").is_none());
        assert!(value.get("fail").is_none() && value.get("crash").is_none());
    }

    #[test]
    fn a_crash_summary_alone_carries_nothing_else() {
        let mut ledger = Ledger::default();
        ledger.counts.starts = 5;
        ledger.add_crash(note(), 0);
        ledger.add_crash(
            CrashNote {
                kind: CrashKind::Page,
                n: 9,
                ..note()
            },
            0,
        );
        let out = crash_only(&ledger, &facts());
        let value = json(&out);
        assert!(value.get("day").is_none() && value.get("new").is_none());
        // Only the panic: the page error waits for the daily batch.
        assert_eq!(value["crash"].as_array().map(Vec::len), Some(1));
        assert_eq!(value["crash"][0]["n"], 2);
        assert_eq!(out.carried.counts, None);
        assert_eq!(out.carried.crashes.len(), 1);
    }

    #[test]
    fn a_day_at_most() {
        let mut ledger = Ledger {
            since: Some(0),
            ..Ledger::default()
        };
        assert!(!daily_due(&ledger, 30 * 3600), "nothing counted");
        ledger.counts.starts = 1;
        assert!(!daily_due(&ledger, 19 * 3600));
        assert!(daily_due(&ledger, 20 * 3600));
        ledger.last_sent = Some(10 * 3600);
        assert!(!daily_due(&ledger, 25 * 3600));
        assert!(daily_due(&ledger, 30 * 3600));
        ledger.retry_at = Some(40 * 3600);
        assert!(!daily_due(&ledger, 35 * 3600));
        assert!(daily_due(&ledger, 40 * 3600));
        assert!(!daily_due(&Ledger::default(), 99 * 3600));
    }

    #[test]
    fn crash_summaries_alone_are_rationed() {
        let mut ledger = Ledger::default();
        assert!(!crash_due(&ledger, 0), "no summary");
        // A page error waits for the daily batch: it never adds a request.
        ledger.add_crash(
            CrashNote {
                kind: CrashKind::Page,
                ..note()
            },
            0,
        );
        assert!(!crash_due(&ledger, 0), "a page error alone");
        ledger.add_crash(note(), 0);
        assert!(crash_due(&ledger, 0));
        ledger.crash_sends = vec![0];
        assert!(!crash_due(&ledger, 1800));
        assert!(crash_due(&ledger, 3600));
        ledger.crash_sends = vec![0, 3600, 7200];
        assert!(!crash_due(&ledger, 12_000));
        assert!(crash_due(&ledger, 24 * 3600 + 1));
    }

    #[test]
    fn waits_grow_to_a_day() {
        assert_eq!(retry_after(1), 3600);
        assert_eq!(retry_after(2), 2 * 3600);
        assert_eq!(retry_after(4), 8 * 3600);
        assert_eq!(retry_after(9), RETRY_MAX_SECS);
        assert_eq!(median(&[]), 0);
        assert_eq!(median(&[5, 1, 3, 2]), 3);
    }
}
