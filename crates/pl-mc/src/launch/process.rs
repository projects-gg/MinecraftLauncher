//! Starting the game and watching it: output is parsed into [`LogLine`]s and masked
//! ([`masked_line`]), the last [`MAX_TAIL_LINES`] are kept in memory and everything is written to
//! `logs/game-<instance>-<timestamp>.log` (capped at [`MAX_LOG_BYTES`]). Events go through a
//! callback; when the run fails, the crash analyzer produces a [`CrashDiagnosis`].

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
use std::time::{Duration, Instant, SystemTime};

use parking_lot::Mutex;
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader};
use tokio::task::JoinHandle;

use crate::error::{Error, Result};
use crate::launch::command::{LaunchCommand, java_argfile};
use crate::launch::crash::{
    CrashDiagnosis, CrashInput, analyze_crash, find_crash_report, find_jvm_error_log, is_failure,
};
use crate::launch::log::{Log4jParser, LogLine};
use crate::mask::mask_secrets;
use crate::paths::write_atomic;

/// Lines kept in memory for the log view and crash analysis.
pub const MAX_TAIL_LINES: usize = 3000;
/// Size cap of the captured log file.
pub const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;
/// Without a recognizable log line, assume the window is up after this long (legacy rule).
pub const WINDOW_FALLBACK: Duration = Duration::from_secs(12);
/// How long to wait for output pipes to close after the process exited.
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
/// Crash report bytes read for analysis.
const MAX_REPORT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GameEvent {
    Started {
        pid: Option<u32>,
    },
    Line(LogLine),
    /// The game window has most likely appeared (time to hide/minimize the launcher).
    WindowLikelyShown,
    Exited {
        code: Option<i32>,
        runtime_secs: u64,
    },
    Crashed(CrashDiagnosis),
}

pub type GameEventFn = Arc<dyn Fn(GameEvent) + Send + Sync>;

#[derive(Debug, Clone, Default)]
pub struct SpawnOptions {
    /// Used in the log file name (`logs/game-<instance>-<timestamp>.log`).
    pub instance_id: String,
    /// Folder for captured logs (`DataDirs::logs()`).
    pub logs_dir: PathBuf,
    /// For crash analysis: Java that runs the game, Java the version wants, `-Xmx`.
    pub java_major: Option<u32>,
    pub required_java: Option<u32>,
    pub max_memory_mb: Option<u32>,
}

/// Final state of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GameExit {
    pub code: Option<i32>,
    pub runtime_secs: u64,
    /// Ended by [`GameProcess::kill`].
    pub killed: bool,
    pub crash: Option<CrashDiagnosis>,
    pub log_file: Option<PathBuf>,
}

struct LogSink {
    file: Option<std::fs::File>,
    path: Option<PathBuf>,
    written: u64,
    truncated: bool,
}

impl LogSink {
    fn open(dir: &Path, instance_id: &str, header: &str) -> Self {
        let stamp = jiff::Zoned::now().strftime("%Y%m%d-%H%M%S").to_string();
        let safe_id: String = instance_id
            .chars()
            .map(|c| {
                if c.is_ascii_alphanumeric() || c == '-' || c == '_' {
                    c
                } else {
                    '_'
                }
            })
            .collect();
        let path = dir.join(format!("game-{safe_id}-{stamp}.log"));
        let opened = std::fs::create_dir_all(dir).and_then(|_| std::fs::File::create(&path));
        match opened {
            Ok(file) => {
                let mut sink = Self {
                    file: Some(file),
                    path: Some(path),
                    written: 0,
                    truncated: false,
                };
                sink.write(header);
                sink
            }
            Err(error) => {
                tracing::warn!(path = %path.display(), %error, "cannot create the game log file");
                Self {
                    file: None,
                    path: None,
                    written: 0,
                    truncated: false,
                }
            }
        }
    }

    fn write(&mut self, line: &str) {
        let Some(file) = self.file.as_mut() else {
            return;
        };
        if self.written >= MAX_LOG_BYTES {
            if !self.truncated {
                let _ = writeln!(
                    file,
                    "[log truncated: {} MB limit reached]",
                    MAX_LOG_BYTES / 1024 / 1024
                );
                self.truncated = true;
            }
            return;
        }
        if writeln!(file, "{line}").is_err() {
            self.file = None;
            return;
        }
        self.written += line.len() as u64 + 1;
    }

    fn close(&mut self) {
        if let Some(mut file) = self.file.take() {
            let _ = file.flush();
        }
    }
}

struct Shared {
    tail: Mutex<VecDeque<LogLine>>,
    log: Mutex<LogSink>,
    window_shown: AtomicBool,
    on_event: GameEventFn,
}

/// The line with its secrets masked ([`mask_secrets`]): what the game prints is masked before it
/// is written to the captured log, kept for crash analysis or handed to the event callback.
pub fn masked_line(mut line: LogLine) -> LogLine {
    if let Cow::Owned(message) = mask_secrets(&line.message) {
        line.message = message;
    }
    for field in [&mut line.thread, &mut line.logger] {
        if let Some(value) = field
            && let Cow::Owned(masked) = mask_secrets(value)
        {
            *value = masked;
        }
    }
    line
}

impl Shared {
    fn emit_line(&self, line: LogLine) {
        let line = masked_line(line);
        self.log.lock().write(&line.format());
        let window = !self.window_shown.load(Ordering::Relaxed) && looks_like_window(&line.message);
        {
            let mut tail = self.tail.lock();
            if tail.len() >= MAX_TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line.clone());
        }
        (self.on_event)(GameEvent::Line(line));
        if window {
            self.mark_window_shown();
        }
    }

    fn mark_window_shown(&self) {
        if !self.window_shown.swap(true, Ordering::SeqCst) {
            (self.on_event)(GameEvent::WindowLikelyShown);
        }
    }

    fn tail_text(&self) -> String {
        let tail = self.tail.lock();
        let mut out = String::with_capacity(tail.len() * 80);
        for line in tail.iter() {
            out.push_str(&line.format());
            out.push('\n');
        }
        out
    }
}

/// The executable the game is started with. On Windows `javaw.exe` becomes the `java.exe` next to
/// it: when the JVM cannot start (a mistyped option, more memory than the computer can give),
/// `javaw` reports it in a modal "Java Virtual Machine Launcher" box and exits only once the
/// player closes it, while `java.exe` writes the same text to stderr (captured, so the crash
/// dialog comes at once) and exits. `CREATE_NO_WINDOW` keeps its console hidden. Elsewhere the
/// executable is used as it is.
pub fn launch_program(program: &Path) -> PathBuf {
    if cfg!(windows) {
        crate::java::detect::console_java(program)
    } else {
        program.to_path_buf()
    }
}

/// Log lines printed right around window creation, from LWJGL 2 era versions to today.
pub fn looks_like_window(message: &str) -> bool {
    message.contains("Backend library: LWJGL")
        || message.starts_with("LWJGL Version")
        || message.contains("Reloading ResourceManager")
        || message.contains("Sound engine started")
        || message.contains("OpenAL initialized")
        || (message.starts_with("Created: ") && message.contains("atlas"))
}

/// A running (or finished) game.
pub struct GameProcess {
    pid: Option<u32>,
    kill: CancellationToken,
    shared: Arc<Shared>,
    exit: tokio::sync::watch::Receiver<Option<GameExit>>,
}

impl GameProcess {
    /// Start `command` (stdout/stderr piped, working directory = game directory) and supervise
    /// it in the background. Events are delivered on tokio worker threads.
    pub async fn spawn(
        command: &LaunchCommand,
        options: SpawnOptions,
        on_event: GameEventFn,
    ) -> Result<Self> {
        tokio::fs::create_dir_all(&command.cwd)
            .await
            .map_err(|e| Error::io(&command.cwd, e))?;
        let (args, argfile) = prepare_args(command, &options).await?;
        let program = launch_program(&command.program);
        let mut cmd = tokio::process::Command::new(&program);
        cmd.args(&args)
            .current_dir(&command.cwd)
            .envs(&command.env)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        cmd.creation_flags(crate::java::detect::CREATE_NO_WINDOW);

        let shown = command.display_redacted_as(&program);
        let header = format!(
            "# Projects Launcher game log\n# command: {}",
            mask_secrets(&shown)
        );
        let sink = LogSink::open(&options.logs_dir, &options.instance_id, &header);
        let started = Instant::now();
        let started_at = SystemTime::now();
        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(e) => {
                if let Some(path) = &argfile {
                    let _ = tokio::fs::remove_file(path).await;
                }
                return Err(Error::Process(format!(
                    "cannot start {}: {e}",
                    program.display()
                )));
            }
        };
        let pid = child.id();
        tracing::info!(?pid, command = %shown, "game started");

        let shared = Arc::new(Shared {
            tail: Mutex::new(VecDeque::with_capacity(512)),
            log: Mutex::new(sink),
            window_shown: AtomicBool::new(false),
            on_event,
        });
        (shared.on_event)(GameEvent::Started { pid });

        let readers: Vec<JoinHandle<()>> = [
            child
                .stdout
                .take()
                .map(|s| tokio::spawn(read_stream(s, Arc::clone(&shared)))),
            child
                .stderr
                .take()
                .map(|s| tokio::spawn(read_stream(s, Arc::clone(&shared)))),
        ]
        .into_iter()
        .flatten()
        .collect();

        let kill = CancellationToken::new();
        let (tx, rx) = tokio::sync::watch::channel(None);
        tokio::spawn(supervise(Supervision {
            child,
            kill: kill.clone(),
            shared: Arc::clone(&shared),
            readers,
            exit: tx,
            started,
            started_at,
            game_dir: command.cwd.clone(),
            argfile,
            player_jvm_args: command.player_jvm_options().to_vec(),
            options,
        }));
        Ok(Self {
            pid,
            kill,
            shared,
            exit: rx,
        })
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// Ask the supervisor to kill the game. A killed run is never reported as a crash.
    pub fn kill(&self) {
        self.kill.cancel();
    }

    pub fn is_running(&self) -> bool {
        self.exit.borrow().is_none()
    }

    /// The last [`MAX_TAIL_LINES`] lines.
    pub fn tail(&self) -> Vec<LogLine> {
        self.shared.tail.lock().iter().cloned().collect()
    }

    /// Captured log file of this run.
    pub fn log_file(&self) -> Option<PathBuf> {
        self.shared.log.lock().path.clone()
    }

    /// Wait until the game has exited and the run has been analyzed.
    pub async fn wait(&self) -> GameExit {
        let mut exit = self.exit.clone();
        match exit.wait_for(Option::is_some).await {
            Ok(value) => value.clone().unwrap_or_else(|| lost_exit(&self.shared)),
            Err(_) => lost_exit(&self.shared),
        }
    }
}

fn lost_exit(shared: &Shared) -> GameExit {
    GameExit {
        code: None,
        runtime_secs: 0,
        killed: false,
        crash: None,
        log_file: shared.log.lock().path.clone(),
    }
}

async fn read_stream<R: AsyncRead + Unpin>(stream: R, shared: Arc<Shared>) {
    let mut reader = BufReader::new(stream);
    let mut parser = Log4jParser::new();
    let mut buffer = Vec::with_capacity(1024);
    loop {
        buffer.clear();
        match reader.read_until(b'\n', &mut buffer).await {
            Ok(0) => break,
            Ok(_) => {
                let text = String::from_utf8_lossy(&buffer);
                for line in parser.push_line(text.trim_end_matches(['\n', '\r'])) {
                    shared.emit_line(line);
                }
            }
            Err(error) => {
                tracing::debug!(%error, "game output stream closed");
                break;
            }
        }
    }
    for line in parser.finish() {
        shared.emit_line(line);
    }
}

struct Supervision {
    child: tokio::process::Child,
    kill: CancellationToken,
    shared: Arc<Shared>,
    readers: Vec<JoinHandle<()>>,
    exit: tokio::sync::watch::Sender<Option<GameExit>>,
    started: Instant,
    started_at: SystemTime,
    game_dir: PathBuf,
    argfile: Option<PathBuf>,
    /// The JVM arguments the player typed (crash analysis).
    player_jvm_args: Vec<String>,
    options: SpawnOptions,
}

/// Windows rejects command lines longer than 32 767 characters (big modpacks, deep data
/// folders). Above this the JVM options move into a Java argument file (Java 9+).
const WINDOWS_COMMAND_LINE_LIMIT: usize = 30_000;

/// Arguments to spawn with, and the temporary argument file when one was needed.
async fn prepare_args(
    command: &LaunchCommand,
    options: &SpawnOptions,
) -> Result<(Vec<String>, Option<PathBuf>)> {
    let range = command.jvm_option_range();
    let needed = cfg!(windows)
        && command.command_line_len() > WINDOWS_COMMAND_LINE_LIMIT
        && options.java_major.is_some_and(|major| major >= 9)
        && !range.is_empty();
    if !needed {
        return Ok((command.args.clone(), None));
    }
    let path = options
        .logs_dir
        .join(format!("launch-{}.args", uuid::Uuid::new_v4().simple()));
    write_atomic(&path, java_argfile(&command.args[range.clone()]).as_bytes()).await?;
    let mut args = command.args.clone();
    args.splice(range, [format!("@{}", path.display())]);
    tracing::info!(path = %path.display(), "long command line, using a Java argument file");
    Ok((args, Some(path)))
}

async fn supervise(mut s: Supervision) {
    let fallback = tokio::time::sleep(WINDOW_FALLBACK);
    tokio::pin!(fallback);
    let mut killed = false;
    let status = loop {
        tokio::select! {
            status = s.child.wait() => break status,
            _ = s.kill.cancelled(), if !killed => {
                killed = true;
                if let Err(error) = s.child.start_kill() {
                    tracing::warn!(%error, "could not kill the game");
                }
            }
            _ = &mut fallback, if !s.shared.window_shown.load(Ordering::Relaxed) => {
                s.shared.mark_window_shown();
            }
        }
    };
    let runtime = s.started.elapsed();
    for reader in s.readers {
        let abort = reader.abort_handle();
        if tokio::time::timeout(DRAIN_TIMEOUT, reader).await.is_err() {
            abort.abort();
        }
    }
    let code = match &status {
        Ok(status) => status.code(),
        Err(error) => {
            tracing::warn!(%error, "waiting for the game failed");
            None
        }
    };
    s.shared.log.lock().close();
    if let Some(argfile) = &s.argfile {
        let _ = tokio::fs::remove_file(argfile).await;
    }
    let log_file = s.shared.log.lock().path.clone();
    tracing::info!(
        ?code,
        runtime_secs = runtime.as_secs(),
        killed,
        "game exited"
    );
    (s.shared.on_event)(GameEvent::Exited {
        code,
        runtime_secs: runtime.as_secs(),
    });

    let crash_report = find_crash_report(&s.game_dir, s.started_at);
    let jvm_log = find_jvm_error_log(&s.game_dir, s.started_at);
    let crash = if !killed && is_failure(code, runtime, crash_report.is_some()) {
        let report_path = crash_report.or(jvm_log);
        let report_text = report_path.as_deref().and_then(read_report);
        let output = s.shared.tail_text();
        let mut diagnosis = analyze_crash(&CrashInput {
            output: &output,
            crash_report: report_text.as_deref(),
            exit_code: code,
            runtime,
            java_major: s.options.java_major,
            required_java: s.options.required_java,
            max_memory_mb: s.options.max_memory_mb,
            player_jvm_args: &s.player_jvm_args,
        });
        diagnosis.crash_report = report_path;
        diagnosis.log_file = log_file.clone();
        tracing::warn!(kind = ?diagnosis.kind, ?code, "game crashed");
        (s.shared.on_event)(GameEvent::Crashed(diagnosis.clone()));
        Some(diagnosis)
    } else {
        None
    };
    let _ = s.exit.send(Some(GameExit {
        code,
        runtime_secs: runtime.as_secs(),
        killed,
        crash,
        log_file,
    }));
}

fn read_report(path: &Path) -> Option<String> {
    let bytes = std::fs::read(path).ok()?;
    let end = bytes.len().min(MAX_REPORT_BYTES);
    Some(String::from_utf8_lossy(&bytes[..end]).into_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn games_start_with_the_console_java_on_windows() {
        let tmp = tempfile::tempdir().unwrap();
        let bin = tmp.path().join("bin");
        std::fs::create_dir_all(&bin).unwrap();
        let javaw = bin.join("javaw.exe");
        std::fs::write(&javaw, b"").unwrap();
        // Without a java.exe next to it, javaw stays.
        assert_eq!(launch_program(&javaw), javaw);
        let java = bin.join("java.exe");
        std::fs::write(&java, b"").unwrap();
        let expected = if cfg!(windows) { &java } else { &javaw };
        assert_eq!(&launch_program(&javaw), expected);
        // Other executables (a wrapper, java itself, macOS/Linux java) are left alone.
        assert_eq!(launch_program(&java), java);
        assert_eq!(
            launch_program(Path::new("mangohud")),
            PathBuf::from("mangohud")
        );
        let unix = tmp.path().join("bin").join("java");
        assert_eq!(launch_program(&unix), unix);
    }

    /// A JVM that refuses one of the player's arguments exits at once; the run is reported with
    /// "reset the Java arguments" as the fix.
    #[tokio::test]
    async fn a_refused_player_argument_is_reported_at_once() {
        let tmp = tempfile::tempdir().unwrap();
        let (program, args, player) = if cfg!(windows) {
            let script = tmp.path().join("fake-java.cmd");
            std::fs::write(
                &script,
                "@echo Unrecognized VM option 'UseFancyThing' 1>&2\r\n@echo Error: Could not create the Java Virtual Machine. 1>&2\r\n@exit /b 1\r\n",
            )
            .unwrap();
            (
                PathBuf::from("cmd"),
                vec![
                    "/C".to_owned(),
                    script.display().to_string(),
                    "-XX:+UseFancyThing".to_owned(),
                ],
                2..3,
            )
        } else {
            (
                PathBuf::from("sh"),
                vec![
                    "-c".to_owned(),
                    "echo \"Unrecognized VM option 'UseFancyThing'\" >&2; echo 'Error: Could not create the Java Virtual Machine.' >&2; exit 1".to_owned(),
                    "sh".to_owned(),
                    "-XX:+UseFancyThing".to_owned(),
                ],
                3..4,
            )
        };
        let command = LaunchCommand::for_tests(program, args, tmp.path().join("game"))
            .with_player_jvm_options(player);
        assert_eq!(command.player_jvm_options(), ["-XX:+UseFancyThing"]);
        let started = Instant::now();
        let process = GameProcess::spawn(
            &command,
            SpawnOptions {
                instance_id: "jvm".into(),
                logs_dir: tmp.path().join("logs"),
                max_memory_mb: Some(4096),
                ..SpawnOptions::default()
            },
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
        let exit = tokio::time::timeout(Duration::from_secs(10), process.wait())
            .await
            .unwrap();
        assert_eq!(exit.code, Some(1));
        let crash = exit.crash.unwrap();
        assert_eq!(
            crash.kind,
            crate::launch::crash::CrashKind::InvalidJvmArgument
        );
        assert_eq!(
            crash.suggested_actions[0],
            crate::launch::crash::SuggestedAction::ResetJvmArguments
        );
        assert!(started.elapsed() < DRAIN_TIMEOUT, "{:?}", started.elapsed());
    }

    #[test]
    fn window_markers() {
        assert!(looks_like_window("Backend library: LWJGL version 3.3.3+5"));
        assert!(looks_like_window("LWJGL Version: 2.9.4"));
        assert!(looks_like_window(
            "Created: 1024x512x4 minecraft:textures/atlas/blocks.png-atlas"
        ));
        assert!(looks_like_window("Sound engine started"));
        assert!(!looks_like_window("Setting user: Steve"));
    }

    #[test]
    fn log_sink_caps_size() {
        let tmp = tempfile::tempdir().unwrap();
        let mut sink = LogSink::open(tmp.path(), "my instance/1", "# header");
        let path = sink.path.clone().unwrap();
        assert!(
            path.file_name()
                .unwrap()
                .to_string_lossy()
                .starts_with("game-my_instance_1-")
        );
        let line = "x".repeat(1024);
        for _ in 0..(9 * 1024) {
            sink.write(&line);
        }
        sink.close();
        let size = std::fs::metadata(&path).unwrap().len();
        assert!(size <= MAX_LOG_BYTES + 2048, "{size}");
        let text = std::fs::read_to_string(&path).unwrap();
        assert!(text.starts_with("# header"));
        assert!(text.trim_end().ends_with("limit reached]"));
    }

    /// Runs a real child process (`java -version` style output via the system shell) to check
    /// spawning, streaming, exit codes and crash reporting.
    #[tokio::test]
    async fn supervises_a_failing_process() {
        let tmp = tempfile::tempdir().unwrap();
        let game_dir = tmp.path().join("game");
        let script = if cfg!(windows) {
            (
                PathBuf::from("cmd"),
                vec![
                    "/C".to_owned(),
                    "echo Error: Could not reserve enough space for object heap & exit 1"
                        .to_owned(),
                ],
            )
        } else {
            (
                PathBuf::from("sh"),
                vec![
                    "-c".to_owned(),
                    "echo 'Error: Could not reserve enough space for object heap'; exit 1"
                        .to_owned(),
                ],
            )
        };
        let command = LaunchCommand::for_tests(script.0, script.1, game_dir.clone());
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let process = GameProcess::spawn(
            &command,
            SpawnOptions {
                instance_id: "test".into(),
                logs_dir: tmp.path().join("logs"),
                max_memory_mb: Some(4096),
                ..SpawnOptions::default()
            },
            Arc::new(move |event| sink.lock().push(event)),
        )
        .await
        .unwrap();
        let exit = process.wait().await;
        assert_eq!(exit.code, Some(1));
        assert!(!exit.killed);
        let crash = exit.crash.unwrap();
        assert_eq!(crash.kind, crate::launch::crash::CrashKind::MemoryReserve);
        assert!(exit.log_file.as_ref().unwrap().is_file());
        let events = events.lock();
        assert!(matches!(events.first(), Some(GameEvent::Started { .. })));
        assert!(events.iter().any(
            |e| matches!(e, GameEvent::Line(l) if l.message.contains("reserve enough space"))
        ));
        assert!(
            events
                .iter()
                .any(|e| matches!(e, GameEvent::Exited { code: Some(1), .. }))
        );
        assert!(matches!(events.last(), Some(GameEvent::Crashed(_))));
        assert_eq!(process.tail().len(), 1);
    }

    /// Secrets the game prints never reach the captured log, the tail or the event callback.
    #[tokio::test]
    async fn output_is_masked_at_the_source() {
        let tmp = tempfile::tempdir().unwrap();
        let (program, args) = if cfg!(windows) {
            (
                PathBuf::from("cmd"),
                vec![
                    "/C".to_owned(),
                    "echo [12:00:00] [main/INFO]: session token=abc123def & echo /login hunter2 & exit 0"
                        .to_owned(),
                ],
            )
        } else {
            (
                PathBuf::from("sh"),
                vec![
                    "-c".to_owned(),
                    "echo '[12:00:00] [main/INFO]: session token=abc123def'; echo '/login hunter2'"
                        .to_owned(),
                ],
            )
        };
        let command = LaunchCommand::for_tests(program, args, tmp.path().join("game"));
        let events = Arc::new(Mutex::new(Vec::new()));
        let sink = Arc::clone(&events);
        let process = GameProcess::spawn(
            &command,
            SpawnOptions {
                instance_id: "mask".into(),
                logs_dir: tmp.path().join("logs"),
                ..SpawnOptions::default()
            },
            Arc::new(move |event| sink.lock().push(event)),
        )
        .await
        .unwrap();
        let exit = process.wait().await;
        let saved = std::fs::read_to_string(exit.log_file.unwrap()).unwrap();
        let shown: Vec<String> = events
            .lock()
            .iter()
            .filter_map(|event| match event {
                GameEvent::Line(line) => Some(line.message.clone()),
                _ => None,
            })
            .collect();
        let kept: Vec<String> = process.tail().into_iter().map(|l| l.message).collect();
        for secret in ["abc123def", "hunter2"] {
            assert!(!saved.contains(secret), "{saved}");
            assert!(shown.iter().all(|l| !l.contains(secret)), "{shown:?}");
            assert!(kept.iter().all(|l| !l.contains(secret)), "{kept:?}");
        }
        assert!(
            shown.iter().any(|l| l.contains("session token=********")),
            "{shown:?}"
        );
        assert!(
            shown.iter().any(|l| l.contains("/login ********")),
            "{shown:?}"
        );
        assert!(
            saved.contains("[main/INFO]: session token=********"),
            "{saved}"
        );
    }

    #[test]
    fn masked_lines_keep_ordinary_text() {
        let line = LogLine {
            time: Some("12:00:00".into()),
            level: crate::launch::log::LogLevel::Info,
            thread: Some("Render thread".into()),
            logger: None,
            message: r"Loaded C:\Users\Ali Veli\mods\sodium.jar --accessToken eyJx".into(),
        };
        let masked = masked_line(line.clone());
        assert_eq!(
            masked.message,
            r"Loaded C:\Users\********\mods\sodium.jar --accessToken ********"
        );
        assert_eq!(masked.thread, line.thread);
        let plain = LogLine {
            message: "Sound engine started".into(),
            ..line
        };
        assert_eq!(masked_line(plain.clone()), plain);
    }

    #[tokio::test]
    async fn long_windows_command_lines_use_an_argfile() {
        let tmp = tempfile::tempdir().unwrap();
        let classpath = vec![r"C:\libraries\some\library.jar"; 2000].join(";");
        let command = LaunchCommand::for_tests(
            PathBuf::from("java"),
            vec!["-cp".into(), classpath, "Main".into(), "--x".into()],
            tmp.path().to_path_buf(),
        )
        .with_jvm_options(0..2);
        let options = SpawnOptions {
            logs_dir: tmp.path().to_path_buf(),
            java_major: Some(21),
            ..SpawnOptions::default()
        };
        let (args, file) = prepare_args(&command, &options).await.unwrap();
        if cfg!(windows) {
            let file = file.unwrap();
            assert_eq!(args.len(), 3);
            assert!(args[0].starts_with('@'));
            assert_eq!(&args[1..], ["Main", "--x"]);
            let text = std::fs::read_to_string(&file).unwrap();
            assert!(text.starts_with("\"-cp\"\n\"C:\\\\libraries"));
            // Java 8 has no argument files: leave the command alone.
            let java8 = SpawnOptions {
                java_major: Some(8),
                ..options.clone()
            };
            let (args8, none) = prepare_args(&command, &java8).await.unwrap();
            assert!(none.is_none());
            assert_eq!(args8, command.args);
        } else {
            assert!(file.is_none());
            assert_eq!(args, command.args);
        }
    }

    #[tokio::test]
    async fn killed_runs_are_not_crashes() {
        let tmp = tempfile::tempdir().unwrap();
        let (program, args) = if cfg!(windows) {
            (
                PathBuf::from("cmd"),
                vec!["/C".to_owned(), "ping -n 30 127.0.0.1 >NUL".to_owned()],
            )
        } else {
            (
                PathBuf::from("sh"),
                vec!["-c".to_owned(), "sleep 30".to_owned()],
            )
        };
        let command = LaunchCommand::for_tests(program, args, tmp.path().join("game"));
        let process = GameProcess::spawn(
            &command,
            SpawnOptions {
                instance_id: "kill".into(),
                logs_dir: tmp.path().join("logs"),
                ..SpawnOptions::default()
            },
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
        assert!(process.is_running());
        process.kill();
        let exit = tokio::time::timeout(Duration::from_secs(20), process.wait())
            .await
            .unwrap();
        assert!(exit.killed);
        assert!(exit.crash.is_none());
    }
}
