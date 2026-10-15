//! Starting the client. Sessions only see a [`ClientProcess`] (three pipes and an exit future), so
//! tests can run the whole state machine against an in-memory fake under paused tokio time.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;

use pl_net::CancellationToken;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::bridge::BRIDGE_ENV_VAR;

/// Last argument that puts MCC in line-based, color-free console mode.
pub const CONSOLE_MODE_ARG: &str = "BasicIO-NoColor";

/// What to run: `"<exe>" "<ini>" BasicIO-NoColor` in `cwd` (the account folder).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub exe: PathBuf,
    pub ini: PathBuf,
    pub cwd: PathBuf,
}

pub type BoxedReader = Box<dyn AsyncRead + Send + Unpin>;
pub type BoxedWriter = Box<dyn AsyncWrite + Send + Unpin>;
pub type ExitFuture = Pin<Box<dyn Future<Output = Option<i32>> + Send>>;

/// The exit side of a started client.
pub trait ProcessExit: Send {
    /// Resolves with the exit code when the process has ended (`None`: killed by a signal or
    /// unknown). Cancelling `kill` terminates the process first.
    fn wait(self: Box<Self>, kill: CancellationToken) -> ExitFuture;
}

/// A started client.
pub struct ClientProcess {
    pub stdin: BoxedWriter,
    pub stdout: BoxedReader,
    pub stderr: BoxedReader,
    pub exit: Box<dyn ProcessExit>,
}

/// Starts clients. [`ProcessSpawner`] is the real one.
pub trait ClientSpawner: Send + Sync {
    fn spawn(&self, spec: &LaunchSpec) -> std::io::Result<ClientProcess>;
}

/// Spawns the client as a child process: stdin/stdout/stderr piped, `MCC_LAUNCHER_BRIDGE=1`, no
/// console window on Windows, killed if the launcher drops it.
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessSpawner;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

impl ClientSpawner for ProcessSpawner {
    fn spawn(&self, spec: &LaunchSpec) -> std::io::Result<ClientProcess> {
        let mut command = tokio::process::Command::new(&spec.exe);
        command
            .arg(&spec.ini)
            .arg(CONSOLE_MODE_ARG)
            .current_dir(&spec.cwd)
            .env(BRIDGE_ENV_VAR, "1")
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .kill_on_drop(true);
        #[cfg(windows)]
        command.creation_flags(CREATE_NO_WINDOW);

        let mut child = command.spawn()?;
        let missing = || std::io::Error::other("child pipe missing");
        let stdin = child.stdin.take().ok_or_else(missing)?;
        let stdout = child.stdout.take().ok_or_else(missing)?;
        let stderr = child.stderr.take().ok_or_else(missing)?;
        Ok(ClientProcess {
            stdin: Box::new(stdin),
            stdout: Box::new(stdout),
            stderr: Box::new(stderr),
            exit: Box::new(ChildExit(child)),
        })
    }
}

struct ChildExit(tokio::process::Child);

impl ProcessExit for ChildExit {
    fn wait(self: Box<Self>, kill: CancellationToken) -> ExitFuture {
        let mut child = self.0;
        Box::pin(async move {
            tokio::select! {
                status = child.wait() => return status.ok().and_then(|s| s.code()),
                () = kill.cancelled() => {}
            }
            if let Err(error) = child.start_kill() {
                tracing::debug!(%error, "AFK client already gone when killing");
            }
            child.wait().await.ok().and_then(|s| s.code())
        })
    }
}
