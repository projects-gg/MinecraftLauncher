//! Game logs the launcher captured (`<root>/logs/game-<instance>-<timestamp>.log`, written by
//! [`crate::launch::GameProcess`]) and the game's own logs (`<game>/logs/latest.log`,
//! `debug.log` and their `.log.gz` archives): listing them, reading one back as parsed entries
//! (archives are inflated on demand, with a cap) and sharing a log on mclo.gs.
//!
//! Everything a user can see or upload goes through [`mask_secrets`] ([`crate::mask`]) first: logs
//! written by older launchers, by the game itself or printed by old game versions may contain
//! access tokens.

use std::borrow::Cow;
use std::fs::File;
use std::io::{BufReader, Read, Seek, SeekFrom};
use std::path::{Path, PathBuf};
use std::sync::LazyLock;
use std::time::Duration;

use pl_net::NetError;
use regex::Regex;
use serde::{Deserialize, Serialize};

use super::fsops::{blocking, modified_ms, read_dir_or_empty};
use crate::error::{Error, Result};
use crate::launch::log::{LogLevel, LogLine, parse_plain_line};
pub use crate::mask::mask_secrets;

/// Logs larger than this are read from the end (the launcher caps its own logs at 8 MB).
pub const MAX_READ_BYTES: u64 = 16 * 1024 * 1024;
/// mclo.gs keeps at most this many bytes of a log.
pub const MCLOGS_MAX_BYTES: usize = 10 * 1024 * 1024;
/// mclo.gs keeps at most this many lines of a log.
pub const MCLOGS_MAX_LINES: usize = 25_000;
/// mclo.gs "create a log" endpoint (form field `content`).
pub const MCLOGS_API: &str = "https://api.mclo.gs/1/log";
/// An archived log is inflated at most this far (a damaged or hostile archive cannot make the
/// launcher unpack gigabytes); the viewer keeps the last [`MAX_READ_BYTES`] of it.
pub const MAX_ARCHIVE_INFLATE_BYTES: u64 = 256 * 1024 * 1024;

/// Where a log comes from.
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum LogSource {
    /// Captured by the launcher from the game's output (`<root>/logs/game-<instance>-….log`).
    #[default]
    Launcher,
    /// Written by the game itself into its `logs/` folder (`latest.log`, `debug.log` and their
    /// `.log.gz` archives).
    Game,
}

/// One captured game session of an instance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GameLogFile {
    pub file_name: String,
    pub path: PathBuf,
    /// When the session started (from the file name), ms since the Unix epoch.
    pub started_ms: Option<i64>,
    /// Last write (the session's end for finished games).
    pub modified_ms: Option<i64>,
    pub size: u64,
}

/// The file-name id of an instance, spelled like the launcher writes it (characters other than
/// `A-Z a-z 0-9 - _` become `_`).
pub fn game_log_id(instance_id: &str) -> String {
    instance_id
        .chars()
        .map(|c| {
            if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                c
            } else {
                '_'
            }
        })
        .collect()
}

/// `game-<id>-<YYYYMMDD>-<HHMMSS>.log` → (`<id>`, `<YYYYMMDD-HHMMSS>`). The stamp is matched
/// from the end, so `game-projects-2-…` belongs to `projects-2`, never to `projects`.
fn split_log_name(name: &str) -> Option<(&str, &str)> {
    const STAMP_LEN: usize = 15;
    let stem = name.strip_prefix("game-")?.strip_suffix(".log")?;
    let cut = stem.len().checked_sub(STAMP_LEN)?;
    let id = stem.get(..cut)?.strip_suffix('-')?;
    let stamp = stem.get(cut..)?;
    let bytes = stamp.as_bytes();
    let valid = bytes[8] == b'-'
        && bytes[..8].iter().all(u8::is_ascii_digit)
        && bytes[9..].iter().all(u8::is_ascii_digit);
    (valid && !id.is_empty()).then_some((id, stamp))
}

/// The session start encoded in a log file name (local time) as ms since the Unix epoch.
fn stamp_ms(stamp: &str) -> Option<i64> {
    let civil = jiff::civil::DateTime::strptime("%Y%m%d-%H%M%S", stamp).ok()?;
    let zoned = civil.to_zoned(jiff::tz::TimeZone::system()).ok()?;
    Some(zoned.timestamp().as_millisecond())
}

/// Sessions of `instance_id` in `logs_dir`, newest first.
pub async fn list_game_logs(logs_dir: &Path, instance_id: &str) -> Result<Vec<GameLogFile>> {
    let dir = logs_dir.to_path_buf();
    let wanted = game_log_id(instance_id);
    blocking(move || {
        let mut logs: Vec<(String, GameLogFile)> = read_dir_or_empty(&dir)?
            .into_iter()
            .filter_map(|entry| {
                let file_name = entry.file_name().to_string_lossy().into_owned();
                let (id, stamp) = split_log_name(&file_name)?;
                if id != wanted {
                    return None;
                }
                let stamp = stamp.to_owned();
                let meta = entry.metadata().ok().filter(|m| m.is_file())?;
                Some((
                    stamp.clone(),
                    GameLogFile {
                        path: entry.path(),
                        started_ms: stamp_ms(&stamp),
                        modified_ms: modified_ms(&meta),
                        size: meta.len(),
                        file_name,
                    },
                ))
            })
            .collect();
        logs.sort_by(|(a, x), (b, y)| b.cmp(a).then_with(|| y.modified_ms.cmp(&x.modified_ms)));
        Ok(logs.into_iter().map(|(_, log)| log).collect())
    })
    .await
}

/// A file of the game's own `logs/` folder: `latest.log`, `debug.log`, `2026-10-15-1.log.gz`,
/// `debug-2.log.gz` (plain names, no hidden files).
pub fn is_game_log_name(name: &str) -> bool {
    let lower = name.to_ascii_lowercase();
    !name.starts_with('.')
        && !name.contains(['/', '\\'])
        && (lower.ends_with(".log") || lower.ends_with(".log.gz"))
}

fn is_archive(path: &Path) -> bool {
    path.file_name()
        .is_some_and(|name| name.to_string_lossy().to_ascii_lowercase().ends_with(".gz"))
}

/// The day of a dated archive (`2026-10-15-1.log.gz`, the game's rotation name) as local
/// midnight in ms since the Unix epoch.
fn archive_day_ms(name: &str) -> Option<i64> {
    if name.as_bytes().get(10) != Some(&b'-') {
        return None;
    }
    let day: jiff::civil::Date = name.get(..10)?.parse().ok()?;
    let zoned = day.to_zoned(jiff::tz::TimeZone::system()).ok()?;
    Some(zoned.timestamp().as_millisecond())
}

/// The game's own logs in `<game_dir>/logs`, newest first; empty files are left out.
pub async fn list_game_folder_logs(game_dir: &Path) -> Result<Vec<GameLogFile>> {
    let dir = game_dir.join("logs");
    blocking(move || {
        let mut logs: Vec<GameLogFile> = read_dir_or_empty(&dir)?
            .into_iter()
            .filter_map(|entry| {
                let file_name = entry.file_name().to_string_lossy().into_owned();
                if !is_game_log_name(&file_name) {
                    return None;
                }
                let meta = entry
                    .metadata()
                    .ok()
                    .filter(|m| m.is_file() && m.len() > 0)?;
                Some(GameLogFile {
                    path: entry.path(),
                    started_ms: archive_day_ms(&file_name),
                    modified_ms: modified_ms(&meta),
                    size: meta.len(),
                    file_name,
                })
            })
            .collect();
        logs.sort_by(|a, b| {
            b.modified_ms
                .cmp(&a.modified_ms)
                .then_with(|| b.file_name.cmp(&a.file_name))
        });
        Ok(logs)
    })
    .await
}

/// A log read back for the viewer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GameLogContent {
    /// Parsed entries; stack traces and other continuation lines stay inside `message`.
    pub lines: Vec<LogLine>,
    /// Only the end of the file was read (it was larger than the read limit).
    pub truncated: bool,
    /// Size of the whole file.
    pub size: u64,
}

/// Read a log (at most `max_bytes` from its end; a `.gz` archive is inflated first), mask secrets
/// and parse it.
pub async fn read_game_log(path: &Path, max_bytes: u64) -> Result<GameLogContent> {
    let path = path.to_path_buf();
    blocking(move || {
        let (text, truncated, size) = read_log_text(&path, max_bytes)?;
        let masked = mask_secrets(&text);
        Ok(GameLogContent {
            lines: parse_log_text(&masked),
            truncated,
            size,
        })
    })
    .await
}

/// The masked text of a log, shortened to what mclo.gs accepts.
pub async fn read_log_for_upload(path: &Path) -> Result<String> {
    let path = path.to_path_buf();
    blocking(move || {
        let (text, _, _) = read_log_text(&path, MAX_READ_BYTES)?;
        let masked = mask_secrets(&text);
        Ok(shorten_for_upload(&masked, MCLOGS_MAX_LINES, MCLOGS_MAX_BYTES).into_owned())
    })
    .await
}

/// The last `max_bytes` of a log as text, whether it was cut, and the file's size on disk.
fn read_log_text(path: &Path, max_bytes: u64) -> Result<(String, bool, u64)> {
    if is_archive(path) {
        read_archive_tail(path, max_bytes)
    } else {
        read_tail(path, max_bytes)
    }
}

/// [`read_tail`] for a `.log.gz` archive: inflated in chunks (at most
/// [`MAX_ARCHIVE_INFLATE_BYTES`]), keeping only the end. A damaged archive (the game was closed
/// while writing it) gives what could be read.
fn read_archive_tail(path: &Path, max_bytes: u64) -> Result<(String, bool, u64)> {
    let file = File::open(path).map_err(|e| Error::io(path, e))?;
    let size = file.metadata().map_err(|e| Error::io(path, e))?.len();
    let keep = usize::try_from(max_bytes).unwrap_or(usize::MAX);
    let mut decoder = flate2::read::MultiGzDecoder::new(BufReader::new(file));
    let mut chunk = vec![0u8; 64 * 1024];
    let mut tail: Vec<u8> = Vec::new();
    let mut inflated = 0u64;
    loop {
        let read = match decoder.read(&mut chunk) {
            Ok(0) => break,
            Ok(read) => read,
            Err(error) if inflated > 0 => {
                tracing::debug!(%error, path = %path.display(), "damaged log archive");
                break;
            }
            Err(error) => return Err(Error::io(path, error)),
        };
        inflated += read as u64;
        tail.extend_from_slice(&chunk[..read]);
        // Drop the front now and then rather than on every chunk.
        if tail.len() > keep.saturating_mul(2).max(chunk.len()) {
            tail.drain(..tail.len() - keep);
        }
        if inflated >= MAX_ARCHIVE_INFLATE_BYTES {
            break;
        }
    }
    if tail.len() > keep {
        tail.drain(..tail.len() - keep);
    }
    let truncated = inflated > max_bytes;
    if truncated && let Some(newline) = tail.iter().position(|b| *b == b'\n') {
        tail.drain(..=newline);
    }
    Ok((String::from_utf8_lossy(&tail).into_owned(), truncated, size))
}

/// The last `max_bytes` of a text file (starting at a line boundary), lossily decoded.
fn read_tail(path: &Path, max_bytes: u64) -> Result<(String, bool, u64)> {
    let mut file = File::open(path).map_err(|e| Error::io(path, e))?;
    let size = file.metadata().map_err(|e| Error::io(path, e))?.len();
    let truncated = size > max_bytes;
    if truncated {
        file.seek(SeekFrom::Start(size - max_bytes))
            .map_err(|e| Error::io(path, e))?;
    }
    let mut bytes = Vec::with_capacity(usize::try_from(size.min(max_bytes)).unwrap_or(0));
    file.take(max_bytes)
        .read_to_end(&mut bytes)
        .map_err(|e| Error::io(path, e))?;
    if truncated && let Some(newline) = bytes.iter().position(|b| *b == b'\n') {
        bytes.drain(..=newline);
    }
    Ok((
        String::from_utf8_lossy(&bytes).into_owned(),
        truncated,
        size,
    ))
}

/// `java.lang.IllegalStateException: …`, `com.mojang.authlib.exceptions.X` (exception headers).
static EXCEPTION_HEADER: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^(?:[A-Za-z_$][\w$]*\.)+[A-Za-z_$][\w$]*(?:Exception|Error|Throwable)(?::|$)")
        .expect("valid regex")
});

/// True for a line that belongs to the entry above it: indented lines (stack frames, mod lists,
/// multi-line messages), `Caused by:`/`Suppressed:`, `... 12 more` and exception headers.
pub fn is_continuation(line: &str) -> bool {
    line.starts_with([' ', '\t'])
        || line.starts_with("Caused by: ")
        || line.starts_with("Suppressed: ")
        || line.starts_with("... ")
        || EXCEPTION_HEADER.is_match(line)
}

/// Parse captured log text (one formatted [`LogLine`] per line) back into entries. The
/// launcher's `# …` header is skipped; continuation lines join the entry above, and a stack
/// trace under a plain informational entry raises it to an error.
pub fn parse_log_text(text: &str) -> Vec<LogLine> {
    let mut out: Vec<LogLine> = Vec::new();
    let mut in_header = true;
    for raw in text.lines() {
        let line = raw.trim_end_matches('\r');
        if in_header {
            if line.starts_with('#') {
                continue;
            }
            in_header = false;
        }
        if line.trim().is_empty() {
            continue;
        }
        if let Some(last) = out.last_mut()
            && is_continuation(line)
        {
            if last.level < LogLevel::Warn && parse_plain_line(line).level >= LogLevel::Error {
                last.level = LogLevel::Error;
            }
            last.message.push('\n');
            last.message.push_str(line);
            continue;
        }
        out.push(parse_plain_line(line));
    }
    out
}

/// Fit a log into `max_lines` lines and `max_bytes` bytes: the start (versions, mod list) and
/// as much of the end (where crashes are) as fits, with a marker line where lines were left out.
pub fn shorten_for_upload(text: &str, max_lines: usize, max_bytes: usize) -> Cow<'_, str> {
    let lines: Vec<&str> = text.lines().collect();
    if lines.len() <= max_lines && text.len() <= max_bytes {
        return Cow::Borrowed(text);
    }
    let max_lines = max_lines.max(3);
    let head_budget = max_bytes / 10;
    let mut head = 0;
    let mut head_bytes = 0;
    while head < lines.len()
        && head < max_lines / 10
        && head_bytes + lines[head].len() < head_budget
    {
        head_bytes += lines[head].len() + 1;
        head += 1;
    }
    // Room for the tail: what is left of both budgets after the head and the marker line.
    let marker_room = 80;
    let tail_lines = max_lines - head - 1;
    let tail_budget = max_bytes.saturating_sub(head_bytes + marker_room);
    let mut tail_start = lines.len();
    let mut tail_bytes = 0;
    while tail_start > head
        && lines.len() - tail_start < tail_lines
        && tail_bytes + lines[tail_start - 1].len() < tail_budget
    {
        tail_start -= 1;
        tail_bytes += lines[tail_start].len() + 1;
    }
    let omitted = tail_start - head;
    let mut out = String::with_capacity(head_bytes + tail_bytes + marker_room);
    for line in &lines[..head] {
        out.push_str(line);
        out.push('\n');
    }
    if omitted > 0 {
        out.push_str(&format!(
            "[... {omitted} lines omitted by Projects Launcher ...]\n"
        ));
    }
    for line in &lines[tail_start..] {
        out.push_str(line);
        out.push('\n');
    }
    Cow::Owned(out)
}

/// A log published on mclo.gs.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct SharedLog {
    pub id: String,
    /// Page to share (`https://mclo.gs/<id>`).
    pub url: String,
    /// Plain text version.
    pub raw: Option<String>,
}

#[derive(Debug, Deserialize)]
struct McLogsAnswer {
    #[serde(default)]
    success: bool,
    id: Option<String>,
    url: Option<String>,
    raw: Option<String>,
    error: Option<String>,
}

/// Interpret the answer of mclo.gs' "create a log" endpoint.
fn parse_mclogs_answer(body: &str) -> Result<SharedLog> {
    let answer: McLogsAnswer =
        serde_json::from_str(body).map_err(|e| Error::json("mclo.gs answer", e))?;
    match answer {
        McLogsAnswer {
            success: true,
            id: Some(id),
            url,
            raw,
            ..
        } if !id.is_empty() => Ok(SharedLog {
            url: url.unwrap_or_else(|| format!("https://mclo.gs/{id}")),
            raw,
            id,
        }),
        McLogsAnswer { error, .. } => Err(Error::Invalid(format!(
            "mclo.gs refused the log: {}",
            error.unwrap_or_else(|| "no reason given".to_owned())
        ))),
    }
}

/// Upload `content` (already masked and shortened) to mclo.gs. The deletion token mclo.gs
/// returns is not kept.
pub async fn upload_to_mclogs(
    client: &reqwest::Client,
    api_url: &str,
    content: &str,
) -> Result<SharedLog> {
    let request_error = |source| {
        Error::Net(NetError::Request {
            url: api_url.to_owned(),
            source,
        })
    };
    let body = url::form_urlencoded::Serializer::new(String::new())
        .append_pair("content", content)
        .finish();
    let response = client
        .post(api_url)
        .header(
            reqwest::header::CONTENT_TYPE,
            "application/x-www-form-urlencoded",
        )
        .header(reqwest::header::ACCEPT, "application/json")
        .timeout(Duration::from_secs(90))
        .body(body)
        .send()
        .await
        .map_err(request_error)?;
    let status = response.status();
    let text = response.text().await.map_err(request_error)?;
    match parse_mclogs_answer(&text) {
        Err(Error::Json { .. }) if !status.is_success() => Err(Error::Net(NetError::Status {
            url: api_url.to_owned(),
            status: status.as_u16(),
        })),
        other => other,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn log_names_match_exact_instances() {
        assert_eq!(
            split_log_name("game-projects-20261015-195921.log"),
            Some(("projects", "20261015-195921"))
        );
        assert_eq!(
            split_log_name("game-projects-2-20261015-224432.log"),
            Some(("projects-2", "20261015-224432"))
        );
        for bad in [
            "game-projects.log",
            "game--20261015-195921.log",
            "game-x-2026101-1959210.log",
            "game-x-20261015-19592a.log",
            "launcher.log",
            "game-ü-2026101ş-195921.log",
        ] {
            assert_eq!(split_log_name(bad), None, "{bad}");
        }
        assert_eq!(game_log_id("my instance.1"), "my_instance_1");
        assert!(stamp_ms("20261015-195921").is_some());
        assert!(stamp_ms("20261399-195921").is_none());
    }

    #[tokio::test]
    async fn lists_sessions_of_one_instance_newest_first() {
        let dir = tempfile::tempdir().unwrap();
        for name in [
            "game-projects-20261015-195921.log",
            "game-projects-20261016-080000.log",
            "game-projects-2-20261015-224432.log",
            "launcher.log",
        ] {
            std::fs::write(dir.path().join(name), b"[12:00:00] [main/INFO]: hi\n").unwrap();
        }
        let logs = list_game_logs(dir.path(), "projects").await.unwrap();
        let names: Vec<&str> = logs.iter().map(|l| l.file_name.as_str()).collect();
        assert_eq!(
            names,
            [
                "game-projects-20261016-080000.log",
                "game-projects-20261015-195921.log"
            ]
        );
        assert!(logs[0].started_ms > logs[1].started_ms);
        assert_eq!(logs[0].size, 27);
        assert_eq!(
            list_game_logs(dir.path(), "projects-2")
                .await
                .unwrap()
                .len(),
            1
        );
        assert!(
            list_game_logs(&dir.path().join("missing"), "projects")
                .await
                .unwrap()
                .is_empty()
        );
    }

    #[test]
    fn continuation_lines_join_their_entry() {
        let text = "# Projects Launcher game log\n# command: java ********\n\
            [19:59:25] [main/INFO] [FabricLoader]: Loading 2 mods:\n\
            \t- fabricloader 0.19.5\n\
            \t- java 25\n\
            [19:59:40] [Download-2/ERROR] [net.minecraft.client.Minecraft]: Failed to fetch user properties\n\
            com.mojang.authlib.exceptions.InvalidCredentialsException: Status: 401\n\
            \tat knot//com.mojang.authlib.Thing.run(Thing.java:59)\n\
            Caused by: java.io.IOException: boom\n\
            \t... 4 more\n\
            [19:59:41] [Render thread/INFO]: Printing a trace\n\
            java.lang.IllegalStateException: bad\n\
            \tat a.b.C.d(C.java:1)\n\
            Picked up _JAVA_OPTIONS: -Xmx2G\n\
            \n\
            [19:59:42] [Render thread/WARN]: careful\n";
        let lines = parse_log_text(text);
        assert_eq!(lines.len(), 5);
        assert_eq!(lines[0].level, LogLevel::Info);
        assert_eq!(lines[0].message.lines().count(), 3);
        assert_eq!(lines[1].level, LogLevel::Error);
        assert!(lines[1].message.ends_with("\t... 4 more"));
        assert_eq!(lines[1].message.lines().count(), 5);
        // A stack trace under an informational entry makes it an error.
        assert_eq!(lines[2].level, LogLevel::Error);
        assert_eq!(lines[3].message, "Picked up _JAVA_OPTIONS: -Xmx2G");
        assert_eq!(lines[3].time, None);
        assert_eq!(lines[4].level, LogLevel::Warn);
        // A leading continuation line has nothing to join and stays on its own.
        assert_eq!(parse_log_text("\tat x.Y.z(Y.java:1)\n").len(), 1);
        assert!(!is_continuation("[12:00:00] [main/INFO]: x"));
        assert!(!is_continuation(
            "Exception in thread \"main\" java.lang.Error"
        ));
        assert!(is_continuation(
            "java.lang.OutOfMemoryError: Java heap space"
        ));
        assert!(is_continuation("org.example.BoomException"));
        assert!(!is_continuation("org.example.Boom"));
    }

    #[test]
    fn long_logs_keep_start_and_end() {
        let text: String = (0..1000).map(|i| format!("line {i}\n")).collect();
        assert!(matches!(
            shorten_for_upload(&text, 1000, 1 << 20),
            Cow::Borrowed(_)
        ));
        let short = shorten_for_upload(&text, 100, 1 << 20);
        let lines: Vec<&str> = short.lines().collect();
        assert_eq!(lines.len(), 100);
        assert_eq!(lines[0], "line 0");
        assert!(lines.iter().any(|l| l.contains("lines omitted")));
        assert_eq!(*lines.last().unwrap(), "line 999");

        let by_bytes = shorten_for_upload(&text, 25_000, 2000);
        assert!(by_bytes.len() <= 2000, "{}", by_bytes.len());
        assert!(by_bytes.ends_with("line 999\n"));
        assert!(by_bytes.starts_with("line 0\n"));
    }

    #[tokio::test]
    async fn reads_masked_entries_and_tail_of_big_files() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("game-x-20261015-195921.log");
        let mut text = String::from("# command: java --accessToken ********\n");
        for i in 0..200 {
            text.push_str(&format!("[12:00:00] [main/INFO]: line {i} token=abc{i}\n"));
        }
        std::fs::write(&path, &text).unwrap();
        let all = read_game_log(&path, MAX_READ_BYTES).await.unwrap();
        assert!(!all.truncated);
        assert_eq!(all.lines.len(), 200);
        assert_eq!(all.lines[5].message, "line 5 token=********");
        assert_eq!(all.size, text.len() as u64);

        let tail = read_game_log(&path, 1000).await.unwrap();
        assert!(tail.truncated);
        assert!(tail.lines.len() < 200);
        assert_eq!(
            tail.lines.last().unwrap().message,
            "line 199 token=********"
        );
        assert!(tail.lines[0].message.starts_with("line "));

        let upload = read_log_for_upload(&path).await.unwrap();
        assert!(upload.starts_with("# command: java --accessToken ********"));
        assert!(!upload.contains("abc1"));
        assert!(
            read_game_log(&dir.path().join("none.log"), 10)
                .await
                .is_err()
        );
    }

    fn gzip(text: &str) -> Vec<u8> {
        use std::io::Write;
        let mut encoder = flate2::write::GzEncoder::new(Vec::new(), flate2::Compression::default());
        encoder.write_all(text.as_bytes()).unwrap();
        encoder.finish().unwrap()
    }

    fn set_age(path: &Path, secs: u64) {
        let file = File::options().write(true).open(path).unwrap();
        file.set_modified(std::time::SystemTime::now() - Duration::from_secs(secs))
            .unwrap();
    }

    #[tokio::test]
    async fn lists_the_games_own_logs_newest_first() {
        let game = tempfile::tempdir().unwrap();
        let logs = game.path().join("logs");
        std::fs::create_dir_all(logs.join("telemetry")).unwrap();
        std::fs::write(logs.join("latest.log"), "[12:00:00] [main/INFO]: hi\n").unwrap();
        std::fs::write(logs.join("debug.log"), "[12:00:00] [main/DEBUG]: hi\n").unwrap();
        std::fs::write(logs.join("2026-10-14-1.log.gz"), gzip("old\n")).unwrap();
        std::fs::write(logs.join("2026-10-15-2.log.gz"), gzip("newer\n")).unwrap();
        std::fs::write(logs.join("empty.log"), "").unwrap();
        std::fs::write(logs.join(".hidden.log"), "x").unwrap();
        std::fs::write(logs.join("notes.txt"), "x").unwrap();
        set_age(&logs.join("latest.log"), 10);
        set_age(&logs.join("debug.log"), 12);
        set_age(&logs.join("2026-10-15-2.log.gz"), 3600);
        set_age(&logs.join("2026-10-14-1.log.gz"), 90_000);

        let list = list_game_folder_logs(game.path()).await.unwrap();
        let names: Vec<&str> = list.iter().map(|l| l.file_name.as_str()).collect();
        assert_eq!(
            names,
            [
                "latest.log",
                "debug.log",
                "2026-10-15-2.log.gz",
                "2026-10-14-1.log.gz"
            ]
        );
        assert_eq!(list[0].started_ms, None);
        assert!(list[2].started_ms > list[3].started_ms);
        assert!(
            list_game_folder_logs(&game.path().join("none"))
                .await
                .unwrap()
                .is_empty()
        );
        assert!(is_game_log_name("debug-3.log.gz"));
        assert!(!is_game_log_name("../latest.log"));
        assert!(!is_game_log_name("latest.txt"));
    }

    #[tokio::test]
    async fn reads_archives_masked_and_from_the_end() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("2026-10-15-1.log.gz");
        let mut text = String::new();
        for i in 0..300 {
            text.push_str(&format!("[12:00:00] [main/INFO]: line {i} token=abc{i}\n"));
        }
        std::fs::write(&path, gzip(&text)).unwrap();
        let size = std::fs::metadata(&path).unwrap().len();

        let all = read_game_log(&path, MAX_READ_BYTES).await.unwrap();
        assert!(!all.truncated);
        assert_eq!(all.lines.len(), 300);
        assert_eq!(all.lines[7].message, "line 7 token=********");
        assert_eq!(all.size, size, "the size on disk");

        let tail = read_game_log(&path, 1000).await.unwrap();
        assert!(tail.truncated);
        assert!(tail.lines.len() < 300);
        assert_eq!(
            tail.lines.last().unwrap().message,
            "line 299 token=********"
        );
        assert!(
            tail.lines[0].message.starts_with("line "),
            "starts at a line"
        );

        let upload = read_log_for_upload(&path).await.unwrap();
        assert!(upload.contains("line 0 token=********"));
        assert!(!upload.contains("abc1"));

        // A damaged archive gives what could be read; garbage is an error.
        let mut damaged = gzip(&text);
        damaged.truncate(damaged.len() / 2);
        let cut = dir.path().join("cut.log.gz");
        std::fs::write(&cut, damaged).unwrap();
        let partial = read_game_log(&cut, MAX_READ_BYTES).await.unwrap();
        assert!(!partial.lines.is_empty() && partial.lines.len() < 300);
        let garbage = dir.path().join("bad.log.gz");
        std::fs::write(&garbage, b"not a gzip stream").unwrap();
        assert!(read_game_log(&garbage, MAX_READ_BYTES).await.is_err());
    }

    #[test]
    fn mclogs_answers() {
        let ok = parse_mclogs_answer(
            r#"{"success":true,"id":"WnMMikq","url":"https://mclo.gs/WnMMikq","raw":"https://api.mclo.gs/1/raw/WnMMikq","token":"x"}"#,
        )
        .unwrap();
        assert_eq!(ok.id, "WnMMikq");
        assert_eq!(ok.url, "https://mclo.gs/WnMMikq");
        let no_url = parse_mclogs_answer(r#"{"success":true,"id":"abc"}"#).unwrap();
        assert_eq!(no_url.url, "https://mclo.gs/abc");
        let refused = parse_mclogs_answer(
            r#"{"success":false,"error":"Required field 'content' not found."}"#,
        );
        assert!(matches!(refused, Err(Error::Invalid(m)) if m.contains("content")));
        assert!(matches!(
            parse_mclogs_answer("<html>"),
            Err(Error::Json { .. })
        ));
    }

    /// Reads the captured logs of a real data folder and prints a summary (nothing secret).
    /// Run with `PL_LOGS_DIR=<root>/logs PL_LOGS_INSTANCE=<id>
    /// cargo test -p pl-mc --lib -- --ignored real_logs --nocapture`.
    #[tokio::test]
    #[ignore = "needs a real data folder"]
    async fn real_logs() {
        let dir = PathBuf::from(std::env::var("PL_LOGS_DIR").expect("PL_LOGS_DIR"));
        let instance = std::env::var("PL_LOGS_INSTANCE").expect("PL_LOGS_INSTANCE");
        let logs = list_game_logs(&dir, &instance).await.unwrap();
        assert!(!logs.is_empty(), "no sessions for {instance}");
        let jwt = Regex::new(r"\beyJ[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}\.[A-Za-z0-9_-]{6,}")
            .expect("valid regex");
        for log in &logs {
            let content = read_game_log(&log.path, MAX_READ_BYTES).await.unwrap();
            let multi = content
                .lines
                .iter()
                .filter(|l| l.message.contains('\n'))
                .count();
            let errors = content
                .lines
                .iter()
                .filter(|l| l.level >= LogLevel::Error)
                .count();
            println!(
                "{}: {} bytes, {} entries ({multi} multi-line, {errors} errors)",
                log.file_name,
                log.size,
                content.lines.len()
            );
            let upload = read_log_for_upload(&log.path).await.unwrap();
            assert!(upload.len() <= MCLOGS_MAX_BYTES);
            assert!(upload.lines().count() <= MCLOGS_MAX_LINES);
            assert!(jwt.find(&upload).is_none());
        }
    }

    /// Publishes a tiny synthetic log on mclo.gs to check the API contract. Run with
    /// `cargo test -p pl-mc --lib -- --ignored mclogs_upload --nocapture`.
    #[tokio::test]
    #[ignore = "network; publishes a public test log"]
    async fn mclogs_upload() {
        let client = reqwest::Client::builder()
            .user_agent("ProjectsLauncher-test (+https://projects.gg)")
            .build()
            .unwrap();
        let text = "[12:00:00] [main/INFO] [Projects Launcher]: upload test, no real data\n\
                    [12:00:01] [main/ERROR] [Projects Launcher]: example error line\n";
        let shared = upload_to_mclogs(&client, MCLOGS_API, text).await.unwrap();
        println!("{} {}", shared.id, shared.url);
        assert!(shared.url.starts_with("https://mclo.gs/"));
    }
}
