//! VOICE-0's measurement run (debug builds only).
//!
//! A debug launcher started with `PROJECTS_VOICE_PROBE=<file>` reads a [`ProbeConfig`] from the
//! file, opens the hidden voice window and lets its page run a call against itself (two peer
//! connections: one Opus encode, one decode and playout, a call with one other person) while both
//! sides append what VOICE-0 measures to `out`, one JSON object per line. `k` names the record and
//! `at` is milliseconds since the run began:
//!
//! - `start` (the configuration, the wall clock, the launcher's process id) and `end`;
//! - `page`: the page's report `r`, counters since the call began, except the fields ending in
//!   `Max`, which cover the time since the page's previous report;
//! - `procs`: each WebView2 process of the launcher (kind, the pages it hosts, CPU time, memory)
//!   and the launcher's own process (`self`);
//! - `pong`: how long an event took to reach the hidden page and come back;
//! - `input`: Raw Input reports seen and presses of the watched key (no other key is looked at);
//! - `permission`, `note`, `error`.
//!
//! [`summarize`] turns a stretch of them into the numbers the task note compares with the
//! design's targets (§6.1: the voice window's page under 120 MB, the launcher's processes under
//! 3 % of one core during a call).

use std::collections::{BTreeMap, BTreeSet};
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::gain::Playout;
use crate::input::Key;

/// One 128-frame render quantum at 48 kHz, in milliseconds (the page's pilot meter counts these).
const BLOCK_MS: f64 = 128.0 / 48.0;

#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct ProbeConfig {
    /// Where the records go (appended).
    pub out: PathBuf,
    /// How long the page's call lasts, in seconds.
    pub seconds: u64,
    /// How often the page and the process sampler record, in seconds.
    #[serde(default = "default_every")]
    pub every: u64,
    pub page: PageConfig,
    /// Watch push-to-talk's key through Raw Input meanwhile.
    #[serde(default)]
    pub input: Option<InputConfig>,
}

/// What the page does (sent to it as it is).
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct PageConfig {
    pub ns: NoiseSuppression,
    pub echo: EchoCancellation,
    pub agc: bool,
    /// Silence between words (a speech gate), so Opus sends almost nothing then.
    pub gate: bool,
    /// A 1 kHz tone added after processing: its gaps on arrival show every break on the way.
    pub pilot: bool,
    /// Opus bit rate, bits per second.
    pub bitrate: u32,
    pub playout: Playout,
    /// The other person's volume: 0 plays silence (the output device still runs), 1 as they
    /// arrive, above 1 on the WebAudio path only.
    pub volume: f32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum NoiseSuppression {
    Off,
    /// Chromium's own (WebRTC's) noise suppression.
    Builtin,
    /// RNNoise in an AudioWorklet (WebAssembly).
    Rnnoise,
    /// GTCRN in an AudioWorklet (WebAssembly), the heavier candidate.
    Gtcrn,
}

/// `getUserMedia`'s `echoCancellation`: `on` is `true`; `all` and `remoteOnly` are Chromium's
/// newer values (cancel everything the browser plays, or only what WebRTC receives).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub enum EchoCancellation {
    Off,
    On,
    All,
    RemoteOnly,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub struct InputConfig {
    pub key: Key,
    /// Mouse reports too: needed for mouse buttons, and every movement arrives with them.
    pub mouse: bool,
}

fn default_every() -> u64 {
    10
}

impl ProbeConfig {
    pub fn parse(json: &str) -> Result<Self, String> {
        let config: Self =
            serde_json::from_str(json).map_err(|error| format!("probe config: {error}"))?;
        config.check()?;
        Ok(config)
    }

    fn check(&self) -> Result<(), String> {
        let fail = |what: &str| Err(format!("probe config: {what}"));
        if self.out.as_os_str().is_empty() {
            return fail("out is empty");
        }
        if !(5..=4 * 3600).contains(&self.seconds) {
            return fail("seconds must be 5 to 14400");
        }
        if !(1..=300).contains(&self.every) {
            return fail("every must be 1 to 300");
        }
        if !(6_000..=128_000).contains(&self.page.bitrate) {
            return fail("bitrate must be 6000 to 128000");
        }
        let top = f32::from(self.page.playout.max_percent()) / 100.0;
        if !(0.0..=top).contains(&self.page.volume) {
            return fail("volume is above the playout path's top");
        }
        Ok(())
    }
}

/// Which page a WebView2 frame shows, from its address: the voice page, another page of the
/// launcher (the main page, whatever its route) or something else.
pub fn page_of(source: &str) -> &'static str {
    if !crate::permission::is_app_origin(source) {
        return "other";
    }
    let path = url::Url::parse(source)
        .map(|url| url.path().to_owned())
        .unwrap_or_default();
    if path.ends_with(&format!("/{}", crate::VOICE_PAGE)) {
        "voice"
    } else {
        "main"
    }
}

/// A stretch of a run, in the task note's terms.
#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct Summary {
    /// Seconds since the run began.
    pub from_s: f64,
    pub to_s: f64,
    pub page: Option<PageSummary>,
    /// Percent of one core per role over the stretch ("voice renderer", "main renderer",
    /// "shared renderer", "browser", "gpu", "utility", "launcher", …).
    pub cpu: BTreeMap<String, f64>,
    /// All of them together.
    pub tree_cpu: Option<f64>,
    /// The largest working set seen per role, MB.
    pub working_set_mb: BTreeMap<String, f64>,
    /// The largest private memory seen per role, MB.
    pub private_mb: BTreeMap<String, f64>,
    /// Whether the main and the voice page ran in one renderer process.
    pub shared_renderer: Option<bool>,
    pub pongs: usize,
    pub pong_max_ms: Option<f64>,
    pub input: Option<InputSummary>,
    pub errors: Vec<String>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct PageSummary {
    /// The page's own clock between the two reports compared.
    pub seconds: f64,
    pub reports: usize,
    pub visibility: BTreeSet<String>,
    pub packets_received: f64,
    pub packets_lost: f64,
    /// Audio that arrived (decoded), ms.
    pub received_ms: f64,
    /// Audio the jitter buffer had to make up (loss concealment), ms, and how often it began.
    pub concealed_ms: f64,
    pub silent_concealed_ms: f64,
    pub concealment_events: f64,
    /// Audio the output had to make up because the decoder fell behind (`media-playout`), ms.
    pub synthesized_ms: Option<f64>,
    pub synthesized_events: Option<f64>,
    /// Breaks in the pilot tone on arrival: how many, how long in all, the longest.
    pub pilot_gaps: Option<f64>,
    pub pilot_gap_ms: Option<f64>,
    pub pilot_longest_ms: Option<f64>,
    /// How late a 250 ms page timer fired at most.
    pub timer_late_max_ms: Option<f64>,
    /// What the page sent, kbit/s.
    pub sent_kbps: Option<f64>,
}

#[derive(Debug, Default, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct InputSummary {
    pub seconds: f64,
    /// Raw Input reports received (keyboard and, if watched, mouse), per second.
    pub reports_per_s: f64,
    pub presses: f64,
    /// The input thread's CPU time, percent of one core.
    pub cpu: f64,
}

struct Record {
    at: f64,
    value: Value,
}

fn num(value: &Value, path: &[&str]) -> Option<f64> {
    path.iter()
        .try_fold(value, |value, key| value.get(*key))?
        .as_f64()
}

/// The records between `from_ms` and `to_ms` (since the run began) summed up. Counters are taken
/// as the difference between the last record at or before each end (none before `from_ms`: the
/// run's beginning); per-interval maxima and memory over the records inside.
pub fn summarize<'a>(
    lines: impl IntoIterator<Item = &'a str>,
    from_ms: u64,
    to_ms: u64,
) -> Summary {
    let (from, to) = (from_ms as f64, to_ms as f64);
    let records: Vec<Record> = lines
        .into_iter()
        .filter_map(|line| serde_json::from_str::<Value>(line.trim()).ok())
        .filter_map(|value| {
            let at = num(&value, &["at"])?;
            Some(Record { at, value })
        })
        .filter(|record| record.at <= to)
        .collect();
    let mut summary = Summary {
        from_s: from / 1000.0,
        to_s: to / 1000.0,
        ..Summary::default()
    };

    summary.page = page_summary(&of(&records, "page"), from);

    let procs = of(&records, "procs");
    if let Some((first, last)) = ends(&procs, from) {
        let span = last.at - first.at;
        if span > 0.0 {
            let before = cpu_by_pid(&first.value);
            for (pid, (role, cpu)) in cpu_by_pid(&last.value) {
                if let Some((_, start)) = before.get(&pid)
                    && cpu >= *start
                {
                    *summary.cpu.entry(role).or_default() += (cpu - start) / span * 100.0;
                }
            }
            summary.tree_cpu = Some(summary.cpu.values().sum());
        }
    }
    let mut voice_alone = false;
    let mut main_alone = false;
    for record in procs.iter().filter(|record| record.at > from) {
        for (role, ws, private) in memory(&record.value) {
            let top = summary.working_set_mb.entry(role.clone()).or_default();
            *top = top.max(ws);
            let top = summary.private_mb.entry(role.clone()).or_default();
            *top = top.max(private);
            match role.as_str() {
                "shared renderer" => summary.shared_renderer = Some(true),
                "voice renderer" => voice_alone = true,
                "main renderer" => main_alone = true,
                _ => {}
            }
        }
    }
    if summary.shared_renderer.is_none() && voice_alone && main_alone {
        summary.shared_renderer = Some(false);
    }

    let pongs: Vec<f64> = of(&records, "pong")
        .iter()
        .filter(|record| record.at > from)
        .filter_map(|record| num(&record.value, &["rttMs"]))
        .collect();
    summary.pongs = pongs.len();
    summary.pong_max_ms = pongs.iter().copied().reduce(f64::max);

    let input = of(&records, "input");
    let counted: Vec<&Record> = input
        .iter()
        .copied()
        .filter(|record| num(&record.value, &["events"]).is_some())
        .collect();
    if let Some((first, last)) = ends(&counted, from) {
        let seconds = (last.at - first.at) / 1000.0;
        let delta = |key: &str| {
            num(&last.value, &[key]).unwrap_or(0.0) - num(&first.value, &[key]).unwrap_or(0.0)
        };
        if seconds > 0.0 {
            summary.input = Some(InputSummary {
                seconds,
                reports_per_s: delta("events") / seconds,
                presses: delta("presses"),
                cpu: delta("cpuMs") / (seconds * 1000.0) * 100.0,
            });
        }
    }

    summary.errors = records
        .iter()
        .filter(|record| record.at > from)
        .filter(|record| {
            record.value.get("k").and_then(Value::as_str) == Some("error")
                || record.value.get("error").is_some()
                || record.value.pointer("/r/error").is_some()
        })
        .map(|record| {
            let text = record
                .value
                .get("error")
                .or_else(|| record.value.get("text"))
                .or_else(|| record.value.pointer("/r/error"))
                .map(|value| {
                    value
                        .as_str()
                        .map_or_else(|| value.to_string(), str::to_owned)
                })
                .unwrap_or_default();
            format!("{:.0} s: {text}", record.at / 1000.0)
        })
        .collect();
    summary
}

/// The records of one kind (`k`).
fn of<'r>(records: &'r [Record], kind: &str) -> Vec<&'r Record> {
    records
        .iter()
        .filter(|record| record.value.get("k").and_then(Value::as_str) == Some(kind))
        .collect()
}

/// The last record at or before `from` (or the first one when none is) and the last one.
fn ends<'a>(records: &[&'a Record], from: f64) -> Option<(&'a Record, &'a Record)> {
    let last = *records.last()?;
    let first = records
        .iter()
        .rev()
        .find(|record| record.at <= from)
        .copied()
        .unwrap_or(records[0]);
    (first.at < last.at).then_some((first, last))
}

fn page_summary(pages: &[&Record], from: f64) -> Option<PageSummary> {
    let last = *pages.last()?;
    let first = pages.iter().rev().find(|record| record.at <= from).copied();
    let report = |record: &Record| record.value.get("r").cloned().unwrap_or(Value::Null);
    let end = report(last);
    let start = first.map(report).unwrap_or(Value::Null);
    // A counter the start lacks counts from zero (the call began inside the stretch).
    let delta = |path: &[&str]| -> Option<f64> {
        let end = num(&end, path)?;
        Some(end - num(&start, path).unwrap_or(0.0))
    };
    let inside: Vec<Value> = pages
        .iter()
        .filter(|record| record.at > from)
        .map(|record| report(record))
        .collect();
    let max = |path: &[&str]| {
        inside
            .iter()
            .filter_map(|report| num(report, path))
            .reduce(f64::max)
    };
    let seconds = delta(&["t"]).unwrap_or(0.0);
    Some(PageSummary {
        seconds,
        reports: inside.len(),
        visibility: inside
            .iter()
            .filter_map(|report| report.get("vis").and_then(Value::as_str))
            .map(str::to_owned)
            .collect(),
        packets_received: delta(&["in", "packetsReceived"]).unwrap_or(0.0),
        packets_lost: delta(&["in", "packetsLost"]).unwrap_or(0.0),
        received_ms: delta(&["in", "totalSamplesReceived"]).unwrap_or(0.0) / 48.0,
        concealed_ms: delta(&["in", "concealedSamples"]).unwrap_or(0.0) / 48.0,
        silent_concealed_ms: delta(&["in", "silentConcealedSamples"]).unwrap_or(0.0) / 48.0,
        concealment_events: delta(&["in", "concealmentEvents"]).unwrap_or(0.0),
        synthesized_ms: delta(&["playout", "synthesizedSamplesDuration"]).map(|s| s * 1000.0),
        synthesized_events: delta(&["playout", "synthesizedSamplesEvents"]),
        pilot_gaps: delta(&["pilot", "gaps"]),
        pilot_gap_ms: delta(&["pilot", "gapBlocks"]).map(|blocks| blocks * BLOCK_MS),
        pilot_longest_ms: max(&["pilot", "longestGapBlocksMax"]).map(|blocks| blocks * BLOCK_MS),
        timer_late_max_ms: max(&["timer", "lateMaxMs"]),
        sent_kbps: delta(&["out", "bytesSent"])
            .filter(|_| seconds > 0.0)
            .map(|bytes| bytes * 8.0 / seconds / 1000.0),
    })
}

/// The role of each process of a `procs` record and its CPU time (ms).
fn cpu_by_pid(value: &Value) -> BTreeMap<u64, (String, f64)> {
    let mut out = BTreeMap::new();
    for process in value
        .get("list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
    {
        if let (Some(pid), Some(cpu)) = (
            process.get("pid").and_then(Value::as_u64),
            num(process, &["cpuMs"]),
        ) {
            out.insert(pid, (role(process), cpu));
        }
    }
    if let Some(own) = value.get("self")
        && let (Some(pid), Some(cpu)) =
            (own.get("pid").and_then(Value::as_u64), num(own, &["cpuMs"]))
    {
        out.insert(pid, ("launcher".to_owned(), cpu));
    }
    out
}

/// Each process's role with its working set and private memory (MB).
fn memory(value: &Value) -> Vec<(String, f64, f64)> {
    let mut out: Vec<(String, f64, f64)> = value
        .get("list")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(|process| {
            Some((
                role(process),
                num(process, &["wsMb"])?,
                num(process, &["privMb"]).unwrap_or(0.0),
            ))
        })
        .collect();
    if let Some(own) = value.get("self")
        && let Some(ws) = num(own, &["wsMb"])
    {
        out.push((
            "launcher".to_owned(),
            ws,
            num(own, &["privMb"]).unwrap_or(0.0),
        ));
    }
    out
}

fn role(process: &Value) -> String {
    let kind = process
        .get("kind")
        .and_then(Value::as_str)
        .unwrap_or("other");
    if kind != "renderer" {
        return kind.to_owned();
    }
    let pages: Vec<&str> = process
        .get("pages")
        .and_then(Value::as_array)
        .into_iter()
        .flatten()
        .filter_map(Value::as_str)
        .collect();
    let voice = pages.contains(&"voice");
    let main = pages.contains(&"main");
    match (voice, main) {
        (true, true) => "shared renderer",
        (true, false) => "voice renderer",
        (false, true) => "main renderer",
        (false, false) => "other renderer",
    }
    .to_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    const CONFIG: &str = r#"{
        "out": "C:/x/run.jsonl", "seconds": 1800, "every": 10,
        "page": {"ns": "rnnoise", "echo": "on", "agc": true, "gate": false, "pilot": true,
                 "bitrate": 24000, "playout": "element", "volume": 0},
        "input": {"key": "mouseX1", "mouse": true}
    }"#;

    #[test]
    fn a_config_is_read_and_checked() {
        let config = ProbeConfig::parse(CONFIG).unwrap();
        assert_eq!(config.page.ns, NoiseSuppression::Rnnoise);
        assert_eq!(config.input.unwrap().key, Key::MouseX1);
        assert!(ProbeConfig::parse(&CONFIG.replace("24000", "500")).is_err());
        assert!(ProbeConfig::parse(&CONFIG.replace("\"volume\": 0", "\"volume\": 1.5")).is_err());
        assert!(
            ProbeConfig::parse(
                &CONFIG
                    .replace("\"volume\": 0", "\"volume\": 1.5")
                    .replace("element", "webAudio")
            )
            .is_ok()
        );
        // A typo is an error, not a default.
        assert!(ProbeConfig::parse(&CONFIG.replace("\"gate\"", "\"gates\"")).is_err());
        assert!(ProbeConfig::parse(&CONFIG.replace("1800", "1")).is_err());
    }

    #[test]
    fn pages_by_address() {
        assert_eq!(page_of("http://tauri.localhost/voice.html"), "voice");
        assert_eq!(page_of("http://tauri.localhost/"), "main");
        assert_eq!(page_of("http://tauri.localhost/library?x=1"), "main");
        assert_eq!(page_of("https://example.com/voice.html"), "other");
        assert_eq!(page_of("about:blank"), "other");
    }

    fn procs(at: u64, voice_cpu: f64, main_cpu: f64, own_cpu: f64, shared: bool) -> String {
        let renderers = if shared {
            format!(
                r#"{{"pid": 10, "kind": "renderer", "pages": ["main", "voice"], "cpuMs": {}, "wsMb": 300, "privMb": 200}}"#,
                voice_cpu + main_cpu
            )
        } else {
            format!(
                r#"{{"pid": 10, "kind": "renderer", "pages": ["voice"], "cpuMs": {voice_cpu}, "wsMb": 90, "privMb": 40}},
                   {{"pid": 11, "kind": "renderer", "pages": ["main"], "cpuMs": {main_cpu}, "wsMb": 300, "privMb": 200}}"#
            )
        };
        format!(
            r#"{{"k": "procs", "at": {at}, "list": [{renderers},
                {{"pid": 12, "kind": "browser", "pages": [], "cpuMs": {voice_cpu}, "wsMb": 80, "privMb": 30}}],
                "self": {{"pid": 1, "cpuMs": {own_cpu}, "wsMb": 60, "privMb": 25}}}}"#
        )
    }

    #[test]
    fn cpu_and_memory_per_role() {
        let lines = [
            procs(0, 0.0, 0.0, 0.0, false),
            procs(10_000, 100.0, 20.0, 10.0, false),
            procs(20_000, 300.0, 20.0, 30.0, false),
        ];
        let summary = summarize(lines.iter().map(String::as_str), 10_000, 20_000);
        // 200 ms of CPU over 10 s: 2 % of one core.
        assert!((summary.cpu["voice renderer"] - 2.0).abs() < 1e-9);
        assert!((summary.cpu["main renderer"]).abs() < 1e-9);
        assert!((summary.cpu["launcher"] - 0.2).abs() < 1e-9);
        assert!((summary.tree_cpu.unwrap() - 4.2).abs() < 1e-9);
        assert_eq!(summary.working_set_mb["voice renderer"], 90.0);
        assert_eq!(summary.shared_renderer, Some(false));

        let lines = [
            procs(0, 0.0, 0.0, 0.0, true),
            procs(10_000, 100.0, 50.0, 0.0, true),
        ];
        let summary = summarize(lines.iter().map(String::as_str), 0, 10_000);
        assert_eq!(summary.shared_renderer, Some(true));
        assert!((summary.cpu["shared renderer"] - 1.5).abs() < 1e-9);
    }

    fn page(at: u64, t: f64, concealed: f64, gaps: f64, longest: f64, late: f64) -> String {
        format!(
            r#"{{"k": "page", "at": {at}, "r": {{"t": {t}, "vis": "visible",
                "in": {{"packetsReceived": {p}, "packetsLost": 0, "totalSamplesReceived": {s},
                        "concealedSamples": {concealed}, "concealmentEvents": {gaps}}},
                "out": {{"bytesSent": {b}}},
                "pilot": {{"gaps": {gaps}, "gapBlocks": {gaps}, "longestGapBlocksMax": {longest}}},
                "timer": {{"lateMaxMs": {late}}}}}}}"#,
            p = t * 50.0,
            s = t * 48_000.0,
            b = t * 3_000.0,
        )
    }

    #[test]
    fn page_counters_over_a_stretch() {
        let lines = [
            page(10_000, 10.0, 0.0, 0.0, 0.0, 4.0),
            page(20_000, 20.0, 480.0, 1.0, 3.0, 9.0),
            page(30_000, 30.0, 960.0, 3.0, 6.0, 2.0),
            r#"{"k": "pong", "at": 25000, "rttMs": 7}"#.to_owned(),
            r#"{"k": "pong", "at": 5000, "rttMs": 70}"#.to_owned(),
            r#"{"k": "error", "at": 26000, "text": "device lost"}"#.to_owned(),
        ];
        let summary = summarize(lines.iter().map(String::as_str), 10_000, 30_000);
        let page = summary.page.unwrap();
        assert_eq!(page.seconds, 20.0);
        assert_eq!(page.reports, 2);
        assert_eq!(page.concealed_ms, 20.0);
        assert_eq!(page.received_ms, 20_000.0);
        assert_eq!(page.pilot_gaps, Some(3.0));
        assert_eq!(page.pilot_longest_ms, Some(6.0 * BLOCK_MS));
        assert_eq!(page.timer_late_max_ms, Some(9.0));
        assert_eq!(page.sent_kbps, Some(24.0));
        assert!(page.visibility.contains("visible"));
        assert_eq!(summary.pongs, 1);
        assert_eq!(summary.pong_max_ms, Some(7.0));
        assert_eq!(summary.errors, vec!["26 s: device lost".to_owned()]);
    }

    #[test]
    fn input_counts_and_its_thread() {
        let lines = [
            r#"{"k": "input", "at": 0, "before": []}"#,
            r#"{"k": "input", "at": 1000, "events": 0, "presses": 0, "cpuMs": 0}"#,
            r#"{"k": "input", "at": 11000, "events": 5000, "presses": 2, "cpuMs": 50}"#,
        ];
        let input = summarize(lines, 0, 20_000).input.unwrap();
        assert_eq!(input.seconds, 10.0);
        assert_eq!(input.reports_per_s, 500.0);
        assert_eq!(input.presses, 2.0);
        assert!((input.cpu - 0.5).abs() < 1e-9);
    }
}
