//! Starting the game and watching it: output is parsed into [`LogLine`]s and masked
//! ([`masked_line`]), the last [`MAX_TAIL_LINES`] are kept in memory and everything is written to
//! `logs/game-<instance>-<timestamp>.log` (capped at [`MAX_LOG_BYTES`]). Events go through a
//! callback; [`GameEvent::WindowShown`] tells when the game's window is on screen
//! ([`crate::launch::window`]); when the run fails, the crash analyzer produces a
//! [`CrashDiagnosis`].

use std::borrow::Cow;
use std::collections::VecDeque;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::pin::Pin;
use std::process::Stdio;
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::task::{Context, Poll};
use std::time::{Duration, Instant, SystemTime};

use parking_lot::Mutex;
use pl_net::CancellationToken;
use serde::{Deserialize, Serialize};
use tokio::io::{AsyncBufReadExt, AsyncRead, BufReader, ReadBuf};
use tokio::task::JoinHandle;

use crate::error::{Error, Result};
use crate::launch::command::{LaunchCommand, java_argfile};
use crate::launch::crash::{
    CrashDiagnosis, CrashInput, SHORT_RUN, analyze_crash, crash_report_for, is_failure,
    jvm_error_log_for,
};
use crate::launch::log::{Log4jParser, LogLine};
use crate::launch::window::{GIVE_UP_AFTER, ShownBy, WindowProbe, log_marker, probe_interval};
use crate::mask::mask_secrets;
use crate::paths::write_atomic;

/// Lines kept in memory for the log view and crash analysis.
pub const MAX_TAIL_LINES: usize = 3000;
/// Size cap of the captured log file.
pub const MAX_LOG_BYTES: u64 = 8 * 1024 * 1024;
/// How long a reader still waits, once the game has exited and its own output is read, for a pipe
/// another process holds open (something the game started and left running); counted only while
/// that is so ([`drain_output`]).
const DRAIN_TIMEOUT: Duration = Duration::from_secs(5);
/// The longest the game's own last output may take to be read once it has exited (a stalled
/// machine; [`drain_output`]).
const DRAIN_LIMIT: Duration = Duration::from_secs(60);
/// How often a reader that has not ended is looked at once the game has exited.
const DRAIN_CHECK: Duration = Duration::from_millis(100);
/// Crash report bytes read for analysis.
const MAX_REPORT_BYTES: usize = 1024 * 1024;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum GameEvent {
    Started {
        pid: Option<u32>,
    },
    Line(LogLine),
    /// The game's window is on screen (once per run, never after the exit): "Oyunda", the
    /// presence and the launcher's own window follow it ([`crate::launch::window`]).
    WindowShown,
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
    /// Looks for the game's window while it opens (Windows); without one the log alone tells.
    pub window_probe: Option<WindowProbe>,
    /// Set (by the launcher) once another game runs in the same folder beside this one
    /// (PLAY-AGAIN): a crash report or JVM log this run's output does not name could then be the
    /// other game's, so none is taken by its date alone.
    pub shared_folder: Arc<AtomicBool>,
}

/// A crash within this long of the game's window showing is a failed start, not a session: the
/// game was still on its loading screen (it takes up to about a minute after the window shows).
pub const LOADING_GRACE: Duration = Duration::from_secs(60);

/// Final state of a run.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GameExit {
    pub code: Option<i32>,
    pub runtime_secs: u64,
    /// How long the game's window was on screen before the exit ([`GameEvent::WindowShown`]);
    /// `None` when it never showed.
    #[serde(default)]
    pub on_screen_secs: Option<u64>,
    /// Ended by [`GameProcess::kill`].
    pub killed: bool,
    pub crash: Option<CrashDiagnosis>,
    pub log_file: Option<PathBuf>,
}

impl GameExit {
    /// What this run adds to its instance's play time, the whole run as it always was; `None`
    /// when the player never got to play (FIX2): the window never showed (the game was closed or
    /// crashed while it opened, the launcher still said "Açılıyor"), or it crashed within
    /// [`LOADING_GRACE`] of showing (still loading). Such a run is not "last played" either.
    pub fn play_time(&self) -> Option<Duration> {
        let on_screen = Duration::from_secs(self.on_screen_secs?);
        if self.crash.is_some() && on_screen < LOADING_GRACE {
            return None;
        }
        Some(Duration::from_secs(self.runtime_secs))
    }
}

struct LogSink {
    file: Option<std::fs::File>,
    path: Option<PathBuf>,
    written: u64,
    truncated: bool,
}

/// How many numbered names a log file tries when games of one instance start in the same second.
const LOG_NAME_TRIES: u32 = 20;

/// A new log file in `dir` for `instance_id`'s game: `game-<instance>-<time>.log`, or with `-2`,
/// `-3`… when another game of the instance started in the same second (PLAY-AGAIN: two games
/// never write into one file).
fn create_log_file(dir: &Path, instance_id: &str) -> std::io::Result<(std::fs::File, PathBuf)> {
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
    std::fs::create_dir_all(dir)?;
    let mut last = std::io::Error::from(std::io::ErrorKind::AlreadyExists);
    for n in 1..=LOG_NAME_TRIES {
        let name = if n == 1 {
            format!("game-{safe_id}-{stamp}.log")
        } else {
            format!("game-{safe_id}-{stamp}-{n}.log")
        };
        let path = dir.join(name);
        match std::fs::OpenOptions::new()
            .write(true)
            .create_new(true)
            .open(&path)
        {
            Ok(file) => return Ok((file, path)),
            Err(error) if error.kind() == std::io::ErrorKind::AlreadyExists => last = error,
            Err(error) => return Err(error),
        }
    }
    Err(last)
}

impl LogSink {
    fn open(dir: &Path, instance_id: &str, header: &str) -> Self {
        match create_log_file(dir, instance_id) {
            Ok((file, path)) => {
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
                tracing::warn!(dir = %dir.display(), %error, "cannot create the game log file");
                Self {
                    file: None,
                    path: None,
                    written: 0,
                    truncated: false,
                }
            }
        }
    }

    /// No file: a game followed without its output ([`GameProcess::adopt`]).
    fn none() -> Self {
        Self {
            file: None,
            path: None,
            written: 0,
            truncated: false,
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
    /// When the window showed, counted from the start.
    shown_after: Mutex<Option<Duration>>,
    /// A window probe looks too: only the lines logged once the window exists count.
    probing: bool,
    /// The game is being closed ([`GameProcess::kill`]) or has exited: its window no longer
    /// counts as shown.
    ending: AtomicBool,
    started: Instant,
    /// `started` by the wall clock.
    started_at: SystemTime,
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
        let window = !self.window_shown.load(Ordering::Relaxed)
            && log_marker(&line.message).is_some_and(|marker| marker.counts(self.probing));
        {
            let mut tail = self.tail.lock();
            if tail.len() >= MAX_TAIL_LINES {
                tail.pop_front();
            }
            tail.push_back(line.clone());
        }
        (self.on_event)(GameEvent::Line(line));
        if window {
            self.mark_window_shown(ShownBy::Log);
        }
    }

    fn mark_window_shown(&self, by: ShownBy) {
        // Lines read while the game is closed or after its exit (the pipes drain) never make it
        // "shown".
        if self.ending.load(Ordering::SeqCst) || self.window_shown.swap(true, Ordering::SeqCst) {
            return;
        }
        let after = self.started.elapsed();
        *self.shown_after.lock() = Some(after);
        tracing::info!(
            ?by,
            after_ms = after.as_millis() as u64,
            "game window shown"
        );
        (self.on_event)(GameEvent::WindowShown);
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
        Self::spawn_draining(command, options, DRAIN_TIMEOUT, on_event).await
    }

    /// [`Self::spawn`] with `drain` in place of [`DRAIN_TIMEOUT`]. Tests that check the last lines
    /// a process printed give it longer than a loaded machine takes to read them (where the pipe
    /// cannot be asked, [`drain_output`], the drain runs from the exit).
    async fn spawn_draining(
        command: &LaunchCommand,
        options: SpawnOptions,
        drain: Duration,
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
            shown_after: Mutex::new(None),
            probing: options.window_probe.is_some() && pid.is_some(),
            ending: AtomicBool::new(false),
            started,
            started_at,
            on_event,
        });
        (shared.on_event)(GameEvent::Started { pid });

        let readers: Vec<Output> = [
            child
                .stdout
                .take()
                .map(|s| Output::spawn(pipe::Pipe::of(&s), s, &shared)),
            child
                .stderr
                .take()
                .map(|s| Output::spawn(pipe::Pipe::of(&s), s, &shared)),
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
            drain,
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

    /// A game this launcher started before it quit and left running ([`super::detached`]),
    /// followed again until it ends. Its output went with the old launcher's pipes, so there is
    /// no log, no tail and no crash analysis. `wait` resolves once the process has ended (with its
    /// exit code when the OS still tells it); `kill` ends it. `started` and `shown` are when the
    /// game started and when its window showed, by the wall clock; a game left while it was still
    /// opening counts as shown from now on (it is past its loading screen by the time anyone
    /// looks). Must be called within a Tokio runtime.
    pub fn adopt(
        pid: u32,
        started: SystemTime,
        shown: Option<SystemTime>,
        wait: impl std::future::Future<Output = Option<i32>> + Send + 'static,
        kill: impl FnOnce() + Send + 'static,
    ) -> Self {
        let now = SystemTime::now();
        let elapsed = now.duration_since(started).unwrap_or_default();
        let started_instant = Instant::now()
            .checked_sub(elapsed)
            .unwrap_or_else(Instant::now);
        let shown_after = shown
            .unwrap_or(now)
            .duration_since(started)
            .unwrap_or_default();
        let shared = Arc::new(Shared {
            tail: Mutex::new(VecDeque::new()),
            log: Mutex::new(LogSink::none()),
            window_shown: AtomicBool::new(true),
            shown_after: Mutex::new(Some(shown_after)),
            probing: false,
            ending: AtomicBool::new(false),
            started: started_instant,
            started_at: started,
            on_event: Arc::new(|_| {}),
        });
        let token = CancellationToken::new();
        let (tx, rx) = tokio::sync::watch::channel(None);
        let follow = Arc::clone(&shared);
        let cancelled = token.clone();
        tokio::spawn(async move {
            tokio::pin!(wait);
            let mut kill = Some(kill);
            let code = loop {
                tokio::select! {
                    code = &mut wait => break code,
                    () = cancelled.cancelled(), if kill.is_some() => {
                        follow.ending.store(true, Ordering::SeqCst);
                        if let Some(kill) = kill.take() {
                            kill();
                        }
                    }
                }
            };
            follow.ending.store(true, Ordering::SeqCst);
            let runtime = follow.started.elapsed();
            let on_screen = runtime.saturating_sub(shown_after);
            tracing::info!(
                ?code,
                runtime_secs = runtime.as_secs(),
                "adopted game exited"
            );
            let _ = tx.send(Some(GameExit {
                code,
                runtime_secs: runtime.as_secs(),
                on_screen_secs: Some(on_screen.as_secs()),
                killed: kill.is_none(),
                crash: None,
                log_file: None,
            }));
        });
        Self {
            pid: Some(pid),
            kill: token,
            shared,
            exit: rx,
        }
    }

    pub fn pid(&self) -> Option<u32> {
        self.pid
    }

    /// When the game started, by the wall clock.
    pub fn started_at(&self) -> SystemTime {
        self.shared.started_at
    }

    /// When the game's window showed, by the wall clock (`None` while it opens).
    pub fn shown_at(&self) -> Option<SystemTime> {
        (*self.shared.shown_after.lock()).map(|after| self.shared.started_at + after)
    }

    /// Ask the supervisor to kill the game. A killed run is never reported as a crash.
    pub fn kill(&self) {
        self.kill.cancel();
    }

    pub fn is_running(&self) -> bool {
        self.exit.borrow().is_none()
    }

    /// The game's window has been on screen ([`GameEvent::WindowShown`] was sent).
    pub fn window_shown(&self) -> bool {
        self.shared.window_shown.load(Ordering::SeqCst)
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
        on_screen_secs: None,
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

/// One of the game's output streams and its reader ([`drain_output`]).
struct Output {
    task: JoinHandle<()>,
    /// Bytes the reader has taken from the pipe.
    taken: Arc<AtomicU64>,
    pipe: Option<pipe::Pipe>,
}

impl Output {
    fn spawn<R: AsyncRead + Unpin + Send + 'static>(
        pipe: Option<pipe::Pipe>,
        stream: R,
        shared: &Arc<Shared>,
    ) -> Self {
        let taken = Arc::new(AtomicU64::new(0));
        let counted = Counted {
            inner: stream,
            taken: Arc::clone(&taken),
        };
        Self {
            task: tokio::spawn(read_stream(counted, Arc::clone(shared))),
            taken,
            pipe,
        }
    }
}

/// A stream that counts the bytes taken from it.
struct Counted<R> {
    inner: R,
    taken: Arc<AtomicU64>,
}

impl<R: AsyncRead + Unpin> AsyncRead for Counted<R> {
    fn poll_read(
        self: Pin<&mut Self>,
        cx: &mut Context<'_>,
        buf: &mut ReadBuf<'_>,
    ) -> Poll<std::io::Result<()>> {
        let this = self.get_mut();
        let before = buf.filled().len();
        let polled = Pin::new(&mut this.inner).poll_read(cx, buf);
        let read = buf.filled().len().saturating_sub(before);
        this.taken
            .fetch_add(u64::try_from(read).unwrap_or(u64::MAX), Ordering::SeqCst);
        polled
    }
}

/// Waits for the reader of one of the game's output streams once the game has exited. The pipe
/// closes when the last process holding its writing end does, and what the game wrote is taken
/// already or still in the pipe (its exit ended its writes): that is read to its end however long
/// a stalled machine takes, up to [`DRAIN_LIMIT`]. A pipe another process still holds open
/// (something the game started and left running) gets `drain` more, counted only while that is
/// so, and then its reader is stopped: what comes from there is not the game's. Where the pipe
/// cannot be asked, the reader gets `drain` from the exit.
async fn drain_output(output: Output, drain: Duration) {
    let Output {
        mut task,
        taken,
        pipe,
    } = output;
    let abort = task.abort_handle();
    let Some(pipe) = pipe else {
        if tokio::time::timeout(drain, &mut task).await.is_err() {
            abort.abort();
        }
        return;
    };
    // Where the game's own output ends in the stream.
    let owed = match pipe.left() {
        Some(PipeLeft::Unread(unread)) => taken.load(Ordering::SeqCst).saturating_add(unread),
        Some(PipeLeft::Closed) | None => 0,
    };
    let limit = Instant::now() + DRAIN_LIMIT;
    let mut held = Duration::ZERO;
    while tokio::time::timeout(DRAIN_CHECK, &mut task).await.is_err() {
        let reading = match pipe.left() {
            // Every writer is gone: the reader's next turn ends it.
            Some(PipeLeft::Closed) => true,
            Some(PipeLeft::Unread(_)) => taken.load(Ordering::SeqCst) < owed,
            None => false,
        };
        if reading && Instant::now() < limit {
            continue;
        }
        held += DRAIN_CHECK;
        if held >= drain {
            abort.abort();
            return;
        }
    }
}

/// What is left in one of the game's output pipes ([`drain_output`]).
#[cfg_attr(not(windows), allow(dead_code))]
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum PipeLeft {
    /// Every writing end is closed and every byte taken.
    Closed,
    /// Bytes not taken yet (none: a writing end is still open somewhere).
    Unread(u64),
}

/// The reading end of one of the game's output pipes, asked what is left in it
/// (`PeekNamedPipe`).
#[cfg(windows)]
mod pipe {
    use std::os::windows::io::{AsHandle, AsRawHandle, OwnedHandle};

    use windows_sys::Win32::Foundation::{ERROR_BROKEN_PIPE, ERROR_PIPE_NOT_CONNECTED};
    use windows_sys::Win32::System::Pipes::PeekNamedPipe;

    use super::PipeLeft;

    /// A handle of its own to the pipe: the reader owns the stream's.
    pub(super) struct Pipe(OwnedHandle);

    impl Pipe {
        pub(super) fn of(stream: &impl AsHandle) -> Option<Self> {
            stream.as_handle().try_clone_to_owned().ok().map(Self)
        }

        /// None when the pipe cannot tell.
        pub(super) fn left(&self) -> Option<PipeLeft> {
            let mut unread = 0u32;
            // SAFETY: the handle stays open while `self` lives; nothing is copied out of the pipe
            // (no buffer), and the one out pointer is a local that outlives the call.
            let peeked = unsafe {
                PeekNamedPipe(
                    self.0.as_raw_handle(),
                    std::ptr::null_mut(),
                    0,
                    std::ptr::null_mut(),
                    &mut unread,
                    std::ptr::null_mut(),
                )
            };
            if peeked != 0 {
                return Some(PipeLeft::Unread(u64::from(unread)));
            }
            let code = std::io::Error::last_os_error().raw_os_error();
            match code.and_then(|code| u32::try_from(code).ok()) {
                Some(ERROR_BROKEN_PIPE | ERROR_PIPE_NOT_CONNECTED) => Some(PipeLeft::Closed),
                _ => None,
            }
        }
    }
}

/// Elsewhere the pipe is not asked: a reader gets the drain from the game's exit.
#[cfg(not(windows))]
mod pipe {
    use super::PipeLeft;

    pub(super) enum Pipe {}

    impl Pipe {
        pub(super) fn of<T>(_stream: &T) -> Option<Self> {
            None
        }

        pub(super) fn left(&self) -> Option<PipeLeft> {
            match *self {}
        }
    }
}

struct Supervision {
    child: tokio::process::Child,
    kill: CancellationToken,
    shared: Arc<Shared>,
    readers: Vec<Output>,
    /// How long a reader may wait for a pipe another process holds open ([`DRAIN_TIMEOUT`]).
    drain: Duration,
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
    // The process id while the child still has one: its JVM error log is named after it.
    let pid = s.child.id();
    // While the game opens its window is looked for (the probe, else the log alone tells) and,
    // after `GIVE_UP_AFTER` without a sign, counted as shown. Nothing runs once it showed.
    let probe = s.options.window_probe.zip(pid);
    let look = tokio::time::sleep(probe_interval(Duration::ZERO));
    tokio::pin!(look);
    let give_up = tokio::time::sleep(GIVE_UP_AFTER);
    tokio::pin!(give_up);
    let mut killed = false;
    let status = loop {
        let opening = !killed && !s.shared.window_shown.load(Ordering::Relaxed);
        tokio::select! {
            status = s.child.wait() => break status,
            _ = s.kill.cancelled(), if !killed => {
                killed = true;
                s.shared.ending.store(true, Ordering::SeqCst);
                if let Err(error) = s.child.start_kill() {
                    tracing::warn!(%error, "could not kill the game");
                }
            }
            _ = &mut look, if opening && probe.is_some() => {
                // The log may have told meanwhile; then this was the last look.
                if let Some((probe, pid)) = probe
                    && !s.shared.window_shown.load(Ordering::Relaxed)
                {
                    if probe(pid) {
                        s.shared.mark_window_shown(ShownBy::Probe);
                    } else {
                        let next = probe_interval(s.started.elapsed());
                        look.as_mut().reset(tokio::time::Instant::now() + next);
                    }
                }
            }
            _ = &mut give_up, if opening => s.shared.mark_window_shown(ShownBy::GaveUp),
        }
    };
    s.shared.ending.store(true, Ordering::SeqCst);
    let runtime = s.started.elapsed();
    let on_screen = (*s.shared.shown_after.lock()).map(|after| runtime.saturating_sub(after));
    for output in s.readers {
        drain_output(output, s.drain).await;
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

    // This run's own report: the one its output names; another game of the same folder may have
    // written the newest one (PLAY-AGAIN). Read only when the run may have failed (a clean exit
    // after a full session never is one).
    let may_have_failed = !killed && (code != Some(0) || runtime < SHORT_RUN);
    let output = if may_have_failed {
        s.shared.tail_text()
    } else {
        String::new()
    };
    let shared_folder = s.options.shared_folder.load(Ordering::SeqCst);
    let crash_report = may_have_failed
        .then(|| crash_report_for(&s.game_dir, s.started_at, &output, shared_folder))
        .flatten();
    let jvm_log = may_have_failed
        .then(|| jvm_error_log_for(&s.game_dir, s.started_at, pid, shared_folder))
        .flatten();
    let crash = if may_have_failed && is_failure(code, runtime, crash_report.is_some()) {
        let report_path = crash_report.or(jvm_log);
        let report_text = report_path.as_deref().and_then(read_report);
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
        on_screen_secs: on_screen.map(|time| time.as_secs()),
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
    fn two_games_of_an_instance_never_share_a_log_file() {
        // PLAY-AGAIN: a second game of the instance may start within the same second.
        let tmp = tempfile::tempdir().unwrap();
        let mut first = LogSink::open(tmp.path(), "projects", "# first");
        let mut second = LogSink::open(tmp.path(), "projects", "# second");
        let (a, b) = (first.path.clone().unwrap(), second.path.clone().unwrap());
        assert_ne!(a, b);
        first.write("one");
        second.write("two");
        first.close();
        second.close();
        let read = |path: &Path| std::fs::read_to_string(path).unwrap();
        assert!(read(&a).contains("# first") && read(&a).contains("one"));
        assert!(read(&b).contains("# second") && read(&b).contains("two"));
        assert!(!read(&a).contains("two") && !read(&b).contains("one"));
    }

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
        // "At once": the exit is told right after the JVM's last line, once its pipes close, not
        // when the drain runs out. Counted between those two events, not from the start: starting
        // a process (a fresh script the virus scanner reads first) took 5 s on a busy machine.
        let times = Arc::new(Mutex::new((None::<Instant>, None::<Instant>)));
        let seen = Arc::clone(&times);
        let process = GameProcess::spawn_draining(
            &command,
            SpawnOptions {
                instance_id: "jvm".into(),
                logs_dir: tmp.path().join("logs"),
                max_memory_mb: Some(4096),
                ..SpawnOptions::default()
            },
            SLOW_DRAIN,
            Arc::new(move |event| {
                let mut seen = seen.lock();
                match event {
                    GameEvent::Line(_) => seen.0 = Some(Instant::now()),
                    GameEvent::Exited { .. } => seen.1 = Some(Instant::now()),
                    _ => {}
                }
            }),
        )
        .await
        .unwrap();
        let exit = tokio::time::timeout(DEADLINE, process.wait())
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
        let (last_line, exited) = *times.lock();
        let since = exited
            .expect("the exit was told")
            .duration_since(last_line.expect("the JVM's lines were read"));
        assert!(since < SLOW_DRAIN, "the drain ran out: {since:?}");
    }

    /// Deadline of a wait for a real process: it only runs out when something is broken (on a
    /// loaded machine a fake game that lived 2 s once took more than 20 s to be reported; beside a
    /// busy-loop on 12 of 16 threads, real-process tests ran some 30 times slower).
    const DEADLINE: Duration = Duration::from_secs(300);

    /// Drain of the tests that check the last lines a process printed before it exited: longer
    /// than a loaded machine takes to read them (beside a busy-loop on 12 of 16 threads the
    /// production 5 s ran out and a line still in the pipe was lost), shorter than [`DEADLINE`],
    /// so a drain that is waited out fails on its own.
    const SLOW_DRAIN: Duration = Duration::from_secs(120);

    /// Checks `condition` until it holds, failing after [`DEADLINE`].
    async fn eventually(what: &str, condition: impl Fn() -> bool) {
        let deadline = Instant::now() + DEADLINE;
        let mut pause = Duration::from_millis(5);
        while !condition() {
            assert!(Instant::now() < deadline, "timed out waiting for: {what}");
            tokio::time::sleep(pause).await;
            pause = (pause * 2).min(Duration::from_millis(100));
        }
    }

    /// A fake game that prints `line` (a log line, or nothing) and then runs until it is closed:
    /// the test decides what happens while it opens, not a timer racing a loaded machine. It looks
    /// for its quit file in its game folder about once a second.
    struct OpeningGame {
        command: LaunchCommand,
        quit_file: PathBuf,
    }

    impl OpeningGame {
        fn new(dir: &Path, line: Option<&str>) -> Self {
            let id = uuid::Uuid::new_v4().simple().to_string();
            let game_dir = dir.join("game");
            let quit = format!("quit-{id}");
            let (program, args) = if cfg!(windows) {
                let print = line.map(|l| format!("echo {l}\r\n")).unwrap_or_default();
                let script = dir.join(format!("game-{id}.cmd"));
                std::fs::write(
                    &script,
                    format!(
                        "@echo off\r\n{print}:open\r\nif exist {quit} exit /b 0\r\nping -n 2 127.0.0.1 >NUL\r\ngoto open\r\n"
                    ),
                )
                .unwrap();
                (
                    PathBuf::from("cmd"),
                    vec!["/C".to_owned(), script.display().to_string()],
                )
            } else {
                let print = line.map(|l| format!("echo '{l}'; ")).unwrap_or_default();
                (
                    PathBuf::from("sh"),
                    vec![
                        "-c".to_owned(),
                        format!("{print}while [ ! -e {quit} ]; do sleep 0.05; done"),
                    ],
                )
            };
            Self {
                command: LaunchCommand::for_tests(program, args, game_dir.clone()),
                quit_file: game_dir.join(quit),
            }
        }

        /// The game exits by itself within about a second.
        fn close(&self) {
            std::fs::write(&self.quit_file, b"").unwrap();
        }
    }

    /// A started [`OpeningGame`] and every event it sent.
    struct Watched {
        process: GameProcess,
        events: Arc<Mutex<Vec<GameEvent>>>,
    }

    impl Watched {
        async fn start(game: &OpeningGame, logs: &Path, window_probe: Option<WindowProbe>) -> Self {
            let events = Arc::new(Mutex::new(Vec::new()));
            let sink = Arc::clone(&events);
            let process = GameProcess::spawn(
                &game.command,
                SpawnOptions {
                    instance_id: "window".into(),
                    logs_dir: logs.to_path_buf(),
                    window_probe,
                    ..SpawnOptions::default()
                },
                Arc::new(move |event| sink.lock().push(event)),
            )
            .await
            .unwrap();
            Self { process, events }
        }

        fn saw(&self, check: impl Fn(&GameEvent) -> bool) -> bool {
            self.events.lock().iter().any(check)
        }

        /// Closes the game and waits for the end of its run: its events (the lines left out),
        /// whether its window showed, and the exit.
        async fn close(self, game: &OpeningGame) -> (Vec<GameEvent>, bool, GameExit) {
            game.close();
            let exit = tokio::time::timeout(DEADLINE, self.process.wait())
                .await
                .expect("the game exits once closed");
            let events = self
                .events
                .lock()
                .iter()
                .filter(|event| !matches!(event, GameEvent::Line(_)))
                .cloned()
                .collect();
            (events, self.process.window_shown(), exit)
        }
    }

    fn window_shown(event: &GameEvent) -> bool {
        *event == GameEvent::WindowShown
    }

    static LOOKS: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

    /// The window shows at the probe's second look.
    fn second_look(_pid: u32) -> bool {
        LOOKS.fetch_add(1, Ordering::SeqCst) >= 1
    }

    fn never(_pid: u32) -> bool {
        false
    }

    static LOOKS_BEFORE_CLOSING: std::sync::atomic::AtomicUsize =
        std::sync::atomic::AtomicUsize::new(0);

    /// No window yet (counting the looks).
    fn not_yet(_pid: u32) -> bool {
        LOOKS_BEFORE_CLOSING.fetch_add(1, Ordering::SeqCst);
        false
    }

    /// The probe finds the window while the game opens: told once, between start and exit, and
    /// nothing is asked after that.
    #[tokio::test]
    async fn the_probe_tells_when_the_window_shows() {
        let tmp = tempfile::tempdir().unwrap();
        let game = OpeningGame::new(tmp.path(), None);
        let run = Watched::start(&game, tmp.path(), Some(second_look)).await;
        // The probe looks every 250 ms; the game runs until it is closed.
        eventually("the window shows", || run.saw(window_shown)).await;
        let (events, shown, exit) = run.close(&game).await;
        assert!(shown);
        // The window was on screen for the rest of the run, which counts as played.
        assert!(
            exit.on_screen_secs
                .is_some_and(|secs| secs <= exit.runtime_secs)
        );
        assert_eq!(
            exit.play_time(),
            Some(Duration::from_secs(exit.runtime_secs))
        );
        let shown_at = events
            .iter()
            .position(|e| *e == GameEvent::WindowShown)
            .unwrap();
        assert!(matches!(events[0], GameEvent::Started { .. }), "{events:?}");
        assert!(shown_at > 0 && matches!(events[shown_at + 1], GameEvent::Exited { .. }));
        assert_eq!(
            events
                .iter()
                .filter(|e| **e == GameEvent::WindowShown)
                .count(),
            1
        );
        assert_eq!(
            LOOKS.load(Ordering::SeqCst),
            2,
            "no look after the window showed"
        );
    }

    /// With a probe, a line logged right before the window is created does not count (the probe
    /// sees the window itself); without one it does.
    #[tokio::test]
    async fn lines_before_the_window_count_only_without_a_probe() {
        let tmp = tempfile::tempdir().unwrap();
        let line = "[12:00:00] [Render thread/INFO]: Backend library: LWJGL version 3.3.3+5";
        let game = OpeningGame::new(tmp.path(), Some(line));
        let run = Watched::start(&game, tmp.path(), Some(never)).await;
        // Closed only once the line was taken in while the game ran (lines read after the exit
        // never count, for another reason).
        eventually("the line is read", || {
            run.saw(|e| matches!(e, GameEvent::Line(l) if l.message.contains("LWJGL")))
        })
        .await;
        let (events, shown, _) = run.close(&game).await;
        assert!(!shown, "{events:?}");
        assert!(!events.contains(&GameEvent::WindowShown));
        let game = OpeningGame::new(tmp.path(), Some(line));
        let run = Watched::start(&game, tmp.path(), None).await;
        eventually("the window shows", || run.saw(window_shown)).await;
        let (events, shown, _) = run.close(&game).await;
        assert!(shown, "{events:?}");
        // A line logged once the window exists counts either way.
        let line = "[12:00:01] [Render thread/INFO]: Sound engine started";
        let game = OpeningGame::new(tmp.path(), Some(line));
        let run = Watched::start(&game, tmp.path(), Some(never)).await;
        eventually("the window shows", || run.saw(window_shown)).await;
        let (events, shown, _) = run.close(&game).await;
        assert!(shown, "{events:?}");
    }

    /// A game that closes before any window showed is never "shown": its exit takes over, and
    /// the run adds no play time.
    #[tokio::test]
    async fn a_game_that_closes_first_never_shows() {
        let tmp = tempfile::tempdir().unwrap();
        let game = OpeningGame::new(tmp.path(), None);
        let run = Watched::start(&game, tmp.path(), Some(not_yet)).await;
        // The probe looks and finds nothing; then the game closes.
        eventually("the probe looks", || {
            LOOKS_BEFORE_CLOSING.load(Ordering::SeqCst) >= 2
        })
        .await;
        let (events, shown, exit) = run.close(&game).await;
        assert!(!shown);
        assert!(!events.contains(&GameEvent::WindowShown), "{events:?}");
        assert!(matches!(events.last(), Some(GameEvent::Exited { .. })));
        assert_eq!(exit.on_screen_secs, None);
        assert_eq!(exit.play_time(), None);
    }

    /// Play time (FIX2): a run counts once the player got to play: its window showed, and a crash
    /// came after the loading screen. A start that crashed or was closed while the game opened,
    /// or that crashed while it was still loading, adds nothing.
    #[test]
    fn only_runs_that_reached_the_game_count_as_played() {
        let crash = analyze_crash(&CrashInput {
            output: "",
            crash_report: None,
            exit_code: Some(1),
            runtime: Duration::from_secs(30),
            java_major: None,
            required_java: None,
            max_memory_mb: None,
            player_jvm_args: &[],
        });
        let run = |runtime_secs, on_screen_secs, crashed: bool, killed| GameExit {
            code: Some(i32::from(crashed)),
            runtime_secs,
            on_screen_secs,
            killed,
            crash: crashed.then(|| crash.clone()),
            log_file: None,
        };
        let secs = Duration::from_secs;
        // Played and closed, or closed from the launcher while playing.
        assert_eq!(
            run(3600, Some(3570), false, false).play_time(),
            Some(secs(3600))
        );
        assert_eq!(
            run(900, Some(870), false, true).play_time(),
            Some(secs(900))
        );
        // Crashed after a long session: every minute of it counts.
        assert_eq!(
            run(7200, Some(7170), true, false).play_time(),
            Some(secs(7200))
        );
        let grace = LOADING_GRACE.as_secs();
        assert_eq!(
            run(40 + grace, Some(grace), true, false).play_time(),
            Some(secs(40 + grace))
        );
        // Crashed (or was closed) before the window showed, or on the loading screen.
        assert_eq!(run(27, None, true, false).play_time(), None);
        assert_eq!(run(15, None, false, true).play_time(), None);
        assert_eq!(run(70, Some(grace - 1), true, false).play_time(), None);
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
        let process = GameProcess::spawn_draining(
            &command,
            SpawnOptions {
                instance_id: "test".into(),
                logs_dir: tmp.path().join("logs"),
                max_memory_mb: Some(4096),
                ..SpawnOptions::default()
            },
            SLOW_DRAIN,
            Arc::new(move |event| sink.lock().push(event)),
        )
        .await
        .unwrap();
        let exit = tokio::time::timeout(DEADLINE, process.wait())
            .await
            .unwrap();
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
        let process = GameProcess::spawn_draining(
            &command,
            SpawnOptions {
                instance_id: "mask".into(),
                logs_dir: tmp.path().join("logs"),
                ..SpawnOptions::default()
            },
            SLOW_DRAIN,
            Arc::new(move |event| sink.lock().push(event)),
        )
        .await
        .unwrap();
        let exit = tokio::time::timeout(DEADLINE, process.wait())
            .await
            .unwrap();
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

    /// A game the launcher left running when it quit is followed again by the next start
    /// (SMALL7): shown, its whole run counted once it ends, no log of its own.
    #[tokio::test]
    async fn an_adopted_game_is_followed_to_its_end() {
        let (end, ended) = tokio::sync::oneshot::channel::<Option<i32>>();
        let started = SystemTime::now() - Duration::from_secs(600);
        let shown = started + Duration::from_secs(30);
        let process = GameProcess::adopt(
            77,
            started,
            Some(shown),
            async move { ended.await.ok().flatten() },
            || {},
        );
        assert!(process.is_running() && process.window_shown());
        assert_eq!(process.pid(), Some(77));
        assert_eq!(process.started_at(), started);
        assert_eq!(process.shown_at(), Some(shown));
        assert!(process.log_file().is_none() && process.tail().is_empty());
        end.send(Some(0)).unwrap();
        let exit = tokio::time::timeout(DEADLINE, process.wait())
            .await
            .unwrap();
        assert_eq!(exit.code, Some(0));
        assert!(!exit.killed && exit.crash.is_none() && exit.log_file.is_none());
        assert!(exit.runtime_secs >= 600, "{exit:?}");
        assert_eq!(exit.on_screen_secs, Some(exit.runtime_secs - 30));
        assert_eq!(
            exit.play_time(),
            Some(Duration::from_secs(exit.runtime_secs))
        );
        assert!(!process.is_running());
    }

    #[tokio::test]
    async fn an_adopted_game_is_closed_through_the_os() {
        let (end, ended) = tokio::sync::oneshot::channel::<Option<i32>>();
        let asked = Arc::new(AtomicBool::new(false));
        let kill = {
            let asked = Arc::clone(&asked);
            move || {
                asked.store(true, Ordering::SeqCst);
                let _ = end.send(Some(1));
            }
        };
        // Left while it was still opening: it counts as shown from the moment it was adopted.
        let process = GameProcess::adopt(
            78,
            SystemTime::now(),
            None,
            async move { ended.await.ok().flatten() },
            kill,
        );
        assert!(process.shown_at().is_some());
        process.kill();
        let exit = tokio::time::timeout(DEADLINE, process.wait())
            .await
            .unwrap();
        assert!(asked.load(Ordering::SeqCst));
        assert!(exit.killed && exit.crash.is_none());
    }

    /// A machine stalled when the game exits: its last lines are read however long the reader
    /// takes, the drain (here 200 ms) counts only while another process holds a pipe open. The
    /// reader takes 5 ms a line, a second for all two hundred.
    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_slow_reader_still_gets_the_last_lines() {
        let tmp = tempfile::tempdir().unwrap();
        let command = LaunchCommand::for_tests(
            PathBuf::from("cmd"),
            vec![
                "/C".to_owned(),
                "for /L %i in (1,1,200) do @echo line %i".to_owned(),
            ],
            tmp.path().join("game"),
        );
        let process = GameProcess::spawn_draining(
            &command,
            SpawnOptions {
                instance_id: "slow".into(),
                logs_dir: tmp.path().join("logs"),
                ..SpawnOptions::default()
            },
            Duration::from_millis(200),
            Arc::new(|event| {
                if matches!(event, GameEvent::Line(_)) {
                    std::thread::sleep(Duration::from_millis(5));
                }
            }),
        )
        .await
        .unwrap();
        let exit = tokio::time::timeout(DEADLINE, process.wait())
            .await
            .unwrap();
        assert_eq!(exit.code, Some(0));
        let lines: Vec<String> = process.tail().into_iter().map(|l| l.message).collect();
        assert_eq!(lines.len(), 200, "{lines:?}");
        assert_eq!(lines.last().map(String::as_str), Some("line 200"));
    }

    /// Something the game started and left running keeps writing to its output: the exit is told
    /// a drain after, not once the longest wait for the game's own lines is over.
    #[cfg(windows)]
    #[tokio::test(flavor = "multi_thread", worker_threads = 2)]
    async fn a_writer_left_running_does_not_hold_the_exit() {
        let tmp = tempfile::tempdir().unwrap();
        let command = LaunchCommand::for_tests(
            PathBuf::from("cmd"),
            vec![
                "/C".to_owned(),
                "start /B ping -n 12 127.0.0.1 & exit 0".to_owned(),
            ],
            tmp.path().join("game"),
        );
        let started = Instant::now();
        let process = GameProcess::spawn(
            &command,
            SpawnOptions {
                instance_id: "left".into(),
                logs_dir: tmp.path().join("logs"),
                ..SpawnOptions::default()
            },
            Arc::new(|_| {}),
        )
        .await
        .unwrap();
        let exit = tokio::time::timeout(DEADLINE, process.wait())
            .await
            .unwrap();
        assert_eq!(exit.code, Some(0));
        let took = started.elapsed();
        assert!(took < DRAIN_LIMIT, "the exit was told after {took:?}");
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
        let exit = tokio::time::timeout(DEADLINE, process.wait())
            .await
            .unwrap();
        assert!(exit.killed);
        assert!(exit.crash.is_none());
    }
}
