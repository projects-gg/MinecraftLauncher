//! Usage data (TELEMETRY): the launcher's side of `pl_telemetry`, which holds the rules.
//!
//! Off until Studio turns on the signed config's `features.telemetry`, and off for good with the
//! player's "Kullanım verisi gönder" (Settings → Veri). While on, the launcher counts what it does
//! anyway: its starts (how long until the window showed), its launches and their failures (the
//! command error's code, the crash analysis' kind), the feature commands the page invokes (one
//! table, `pl_telemetry::feature`), voice chat's stays (the voice state the page gets) and its own
//! panics and page errors, scrubbed. Nothing looks at other programs, windows, keys or files.
//!
//! Nothing wakes the launcher for it: the daily batch goes with the daily tidy-up's round
//! (`commands::cleanup::start_housekeeping`, 90 s after the start, then daily), a crash summary
//! with the crash (a task that sleeps until a panic wakes it). Requests go to our own service over
//! HTTPS with the launcher's usual client; debug builds send only to `PROJECTS_TELEMETRY_URL` (a
//! local `wrangler dev`) and otherwise write the batch to `telemetry-preview.json` in the data
//! folder instead. For test runs, debug builds also take `PROJECTS_TELEMETRY=1` (count as if
//! Studio's flag were on) and `PROJECTS_TELEMETRY_SEND_NOW=1` (the daily batch goes at the next
//! round, 90 s after the start, instead of after a day); release builds read neither.

use std::path::Path;
use std::sync::OnceLock;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use pl_telemetry::{
    Answer, AppFacts, CrashKind, CrashNote, Feature, Launch, Outgoing, Phase, Recorder, VoiceSeen,
};
use pl_voice::view::{VoicePhase, VoiceRoomKind, VoiceState};
use serde::Serialize;
use tauri::{AppHandle, Manager};

use crate::state::AppState;

/// The service (release builds).
const ENDPOINT: &str = "https://studio-team.projects.gg/v1/telemetry";
/// How long a request may take.
const TIMEOUT: Duration = Duration::from_secs(20);
/// A burst of panics goes as one request.
const CRASH_SETTLE: Duration = Duration::from_secs(10);
/// The longest page error text taken in (only its first line is kept anyway).
const PAGE_TEXT_MAX: usize = 4096;

static STARTED: OnceLock<Instant> = OnceLock::new();
static RECORDER: OnceLock<Recorder> = OnceLock::new();
static VERSION: OnceLock<String> = OnceLock::new();
static PAGE_READY: AtomicBool = AtomicBool::new(false);
static NUDGE: tokio::sync::Notify = tokio::sync::Notify::const_new();

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_millis()).unwrap_or(i64::MAX))
}

fn recorder() -> Option<&'static Recorder> {
    RECORDER.get()
}

/// A test run's switch (debug builds only; see the module).
fn debug_env(name: &str) -> bool {
    cfg!(debug_assertions) && std::env::var(name).is_ok_and(|value| value == "1")
}

/// The process began: the first thing `run()` does (the startup time counts from here).
pub fn process_started() {
    STARTED.get_or_init(Instant::now);
}

/// Opens the recorder over the data folder (once, in `setup`, before the content is applied) and
/// starts the task a panic wakes.
pub fn open(app: &AppHandle, root: &Path) {
    let version = app.package_info().version.to_string();
    let _ = VERSION.set(version.clone());
    let _ = RECORDER.set(Recorder::open(root, &version, now_ms()));
    if debug_env("PROJECTS_TELEMETRY") {
        apply_flag(true);
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        loop {
            NUDGE.notified().await;
            tokio::time::sleep(CRASH_SETTLE).await;
            let Some(recorder) = recorder() else {
                continue;
            };
            let facts = facts(&app);
            if let Some(out) = recorder.take_crash(&facts, now_ms()) {
                send(&app, recorder, out).await;
            }
        }
    });
}

/// From the panic hook (`lib.rs`): the summary is kept at once, and goes soon if it may.
pub fn panicked(info: &std::panic::PanicHookInfo<'_>) {
    let payload = info.payload();
    let message = payload
        .downcast_ref::<&str>()
        .map(|text| (*text).to_owned())
        .or_else(|| payload.downcast_ref::<String>().cloned())
        .unwrap_or_default();
    let note = CrashNote {
        kind: CrashKind::Panic,
        at: info.location().map_or_else(
            || "?".to_owned(),
            |at| pl_telemetry::scrub::location(at.file(), at.line()),
        ),
        msg: pl_telemetry::scrub::message(&message),
        // Before `setup` the version is not known yet: the next start fills it in.
        ver: VERSION
            .get()
            .map(|v| pl_telemetry::facts::clean_version(v))
            .unwrap_or_default(),
        n: 1,
    };
    match recorder() {
        Some(recorder) => {
            if recorder.panicked(note, now_ms()) {
                NUDGE.notify_one();
            }
        }
        // Before `setup`: left beside the file, only while the file says counting is on.
        None => pl_telemetry::recorder::panicked_early(&crate::state::default_root(), &note),
    }
}

/// The applied content's `features.telemetry` (`commands::content::apply_snapshot`).
pub fn apply_flag(on: bool) {
    if let Some(recorder) = recorder() {
        recorder.set_flag(on || debug_env("PROJECTS_TELEMETRY"), now_ms());
    }
}

/// Every command the page invokes passes here (`lib.rs`'s invoke handler): the feature ones count.
pub fn command(name: &str) {
    if let Some(feature) = Feature::of_command(name)
        && let Some(recorder) = recorder()
    {
        recorder.feature(feature, now_ms());
    }
}

/// A launch ended: the game started (`Ok`), or an error with this code came first.
pub fn launch_ended(result: Result<(), &str>) {
    if let Some(recorder) = recorder() {
        let launch = match result {
            Ok(()) => Launch::Started,
            Err(code) => Launch::Failed(code),
        };
        recorder.launched(launch, now_ms());
    }
}

/// A game's window showed.
pub fn game_shown() {
    if let Some(recorder) = recorder() {
        recorder.game_shown(now_ms());
    }
}

/// A game crashed: its kind as the crash analysis names it (`modError`).
pub fn game_crashed(kind: pl_mc::launch::crash::CrashKind) {
    let Some(recorder) = recorder() else {
        return;
    };
    let name = serde_json::to_value(kind)
        .ok()
        .and_then(|value| value.as_str().map(str::to_owned))
        .unwrap_or_else(|| "unknown".to_owned());
    recorder.game_crashed(&name, now_ms());
}

/// Every voice state (`voice::AppPlatform::state`): a stay's quality is read from them.
pub fn voice_state(state: &VoiceState) {
    let Some(recorder) = recorder() else {
        return;
    };
    let seen = state.room.as_ref().map(|room| VoiceSeen {
        room: &room.id,
        crew: room.kind == VoiceRoomKind::Crew,
        phase: match room.phase {
            VoicePhase::Joining => Phase::Joining,
            VoicePhase::Live => Phase::Live,
            VoicePhase::Reconnecting => Phase::Reconnecting,
        },
        quality: room.me.quality.wire(),
    });
    recorder.voice(seen, now_ms());
}

/// The launcher exits: a voice stay ends and waiting counts are written.
pub fn flush() {
    if let Some(recorder) = recorder() {
        recorder.flush(now_ms());
    }
}

/// The daily tidy-up's round: the daily batch goes if it is due (or crash summaries that wait).
/// Off the round's own task: a slow network never holds the tidy-up.
pub fn tick(app: &AppHandle) {
    let Some(recorder) = recorder() else {
        return;
    };
    if !recorder.active() {
        return;
    }
    let app = app.clone();
    tauri::async_runtime::spawn(async move {
        let facts = facts(&app);
        let now = now_ms();
        let daily = if debug_env("PROJECTS_TELEMETRY_SEND_NOW") {
            recorder.take_daily_now(&facts, now)
        } else {
            recorder.take_daily(&facts, now)
        };
        let out = daily.or_else(|| recorder.take_crash(&facts, now));
        if let Some(out) = out {
            send(&app, recorder, out).await;
        }
    });
}

async fn send(app: &AppHandle, recorder: &Recorder, out: Outgoing) {
    let answer = post(app, &out).await;
    recorder.answered(&out.carried, answer, now_ms());
}

/// Debug builds send only to a local test service, the address parsed: a user part
/// (`http://localhost:@example.com/`) names another host.
fn endpoint() -> Option<String> {
    if !cfg!(debug_assertions) {
        return Some(ENDPOINT.to_owned());
    }
    let url = tauri::Url::parse(&std::env::var("PROJECTS_TELEMETRY_URL").ok()?).ok()?;
    let local = matches!(url.host_str(), Some("127.0.0.1" | "localhost" | "[::1]"));
    (url.scheme() == "http" && local && url.username().is_empty() && url.password().is_none())
        .then(|| url.to_string())
}

async fn post(app: &AppHandle, out: &Outgoing) -> Answer {
    let state = app.state::<AppState>();
    let Some(url) = endpoint() else {
        // What would have gone, for a look in the data folder.
        let path = state.root.join("telemetry-preview.json");
        if let Err(error) = std::fs::write(&path, &out.json) {
            tracing::debug!(%error, "usage data preview not written");
        }
        return Answer::Taken;
    };
    let request = state
        .http
        .client()
        .post(url)
        .header("Content-Type", "application/json")
        .header("Content-Encoding", "gzip")
        .timeout(TIMEOUT)
        .body(out.body.clone());
    match request.send().await {
        Ok(response) if response.status().is_success() => Answer::Taken,
        // The service refuses the batch as it is: sending it again would not help.
        Ok(response) if matches!(response.status().as_u16(), 400 | 413 | 415 | 422) => {
            tracing::warn!(status = response.status().as_u16(), "usage data refused");
            Answer::Refused
        }
        Ok(response) => {
            tracing::debug!(status = response.status().as_u16(), "usage data: later");
            Answer::Later
        }
        Err(error) => {
            tracing::debug!(%error, "usage data not sent");
            Answer::Later
        }
    }
}

fn facts(app: &AppHandle) -> AppFacts {
    AppFacts::new(
        &app.package_info().version.to_string(),
        std::env::consts::OS,
        os_version(),
        std::env::consts::ARCH,
        pl_lang::Lang::current().code(),
        pl_mc::instance::system_memory_mb(),
    )
}

/// Windows' version as `RtlGetVersion` gives it (11 still says 10.0, with a build of 22000 or
/// later).
#[cfg(windows)]
fn os_version() -> Option<(u64, u64, u64)> {
    match os_info::get().version() {
        os_info::Version::Semantic(major, minor, build) => Some((*major, *minor, *build)),
        _ => None,
    }
}

/// macOS' version from its own file (no program runs for it).
#[cfg(target_os = "macos")]
fn os_version() -> Option<(u64, u64, u64)> {
    let text = std::fs::read_to_string("/System/Library/CoreServices/SystemVersion.plist").ok()?;
    let after = text.split_once("<key>ProductVersion</key>")?.1;
    let version = after.split_once("<string>")?.1.split_once("</string>")?.0;
    let mut parts = version
        .trim()
        .split('.')
        .map(|part| part.parse::<u64>().ok());
    Some((parts.next()??, parts.next().flatten().unwrap_or(0), 0))
}

/// Linux: only the family goes.
#[cfg(not(any(windows, target_os = "macos")))]
fn os_version() -> Option<(u64, u64, u64)> {
    None
}

/// What Settings → Veri shows.
#[derive(Debug, Clone, Copy, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct TelemetryView {
    /// Studio's flag is on: the row shows.
    pub shown: bool,
    /// "Kullanım verisi gönder".
    pub enabled: bool,
}

#[tauri::command]
#[specta::specta]
pub fn telemetry_state() -> TelemetryView {
    recorder().map_or(
        TelemetryView {
            shown: false,
            enabled: true,
        },
        |recorder| TelemetryView {
            shown: recorder.flag(),
            enabled: recorder.choice(),
        },
    )
}

/// The player's switch. Off forgets what was not sent yet.
#[tauri::command]
#[specta::specta]
pub fn telemetry_configure(enabled: bool) -> TelemetryView {
    if let Some(recorder) = recorder() {
        recorder.set_choice(enabled, now_ms());
    }
    telemetry_state()
}

/// The page painted and showed the window (`main.tsx`): the start's time, once per run.
#[tauri::command]
#[specta::specta]
pub fn telemetry_page_ready() {
    if PAGE_READY.swap(true, Ordering::SeqCst) {
        return;
    }
    if let (Some(started), Some(recorder)) = (STARTED.get(), recorder()) {
        let ms = u32::try_from(started.elapsed().as_millis()).unwrap_or(u32::MAX);
        recorder.started(ms, now_ms());
    }
}

/// An error the page threw and nothing caught (`features/telemetry/page-errors.ts`).
#[tauri::command]
#[specta::specta]
pub fn telemetry_page_error(message: String, source: String, line: u32, column: u32) {
    let (Some(recorder), Some(version)) = (recorder(), VERSION.get()) else {
        return;
    };
    let message: String = message.chars().take(PAGE_TEXT_MAX).collect();
    let source: String = source.chars().take(PAGE_TEXT_MAX).collect();
    recorder.page_error(&message, &source, line, column, version, now_ms());
}
