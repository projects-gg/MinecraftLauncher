//! Starting the client. Sessions only see a [`ClientProcess`] (three pipes and an exit future), so
//! tests can run the whole state machine against an in-memory fake under paused tokio time.

use std::future::Future;
use std::path::PathBuf;
use std::pin::Pin;
use std::process::Stdio;

use pl_net::CancellationToken;
use tokio::io::{AsyncRead, AsyncWrite};

use crate::bridge::{BRIDGE_ENV_VAR, CONSOLE_KINDS_ENV_VAR, NONCE_ENV_VAR};

/// Last argument that puts MCC in line-based, color-free console mode.
pub const CONSOLE_MODE_ARG: &str = "BasicIO-NoColor";

/// The fork's other environment variables that steer a client: its agent channel, console kinds,
/// auto-reply variables, MCP token and file input. The launcher sets none of them, so a value in
/// its own environment never reaches the client.
const CONTROL_ENV_VARS: [&str; 8] = [
    "MCC_CONSOLE_KINDS",
    "MCC_AGENT_EVENTS",
    "MCC_AGENT_FD_IN",
    "MCC_AGENT_FD_OUT",
    "MCC_MATCH_VARS",
    "MCC_MCP_AUTH_TOKEN",
    "MCC_FILE_INPUT",
    "MCC_INPUT_FILE",
];

/// What to run: `"<exe>" "<ini>" BasicIO-NoColor` in `cwd` (the account folder).
#[derive(Clone, PartialEq, Eq)]
pub struct LaunchSpec {
    pub exe: PathBuf,
    pub ini: PathBuf,
    pub cwd: PathBuf,
    /// The bridge nonce of this process ([`NONCE_ENV_VAR`]), for clients 4.1.0 and later: only its
    /// event lines count (`bridge::parse_line_with_nonce`). Never shown or logged.
    pub nonce: Option<String>,
    /// Console writes as `line` events, with their looks and the server's dialogs
    /// ([`CONSOLE_KINDS_ENV_VAR`]; with a nonce only, clients 4.2.4 and later).
    pub console_kinds: bool,
}

/// The nonce stays out of every log line and panic message.
impl std::fmt::Debug for LaunchSpec {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("LaunchSpec")
            .field("exe", &self.exe)
            .field("ini", &self.ini)
            .field("cwd", &self.cwd)
            .field("nonce", &self.nonce.as_ref().map(|_| "<redacted>"))
            .field("console_kinds", &self.console_kinds)
            .finish()
    }
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

/// Spawns the client as a child process: stdin/stdout/stderr piped, `MCC_LAUNCHER_BRIDGE=1` (and
/// the spec's `MCC_BRIDGE_NONCE`), no console window on Windows, killed if the launcher drops it.
/// The running client is recorded in its account folder until it exits, so a launcher that crashed
/// can stop it at its next start (see `orphans`).
#[derive(Debug, Clone, Copy, Default)]
pub struct ProcessSpawner;

#[cfg(windows)]
const CREATE_NO_WINDOW: u32 = 0x0800_0000;

/// The client's command. Of the fork's variables it gets only the bridge and the spec's nonce: one
/// in this process's own environment is removed (or replaced by ours), never passed on.
fn client_command(spec: &LaunchSpec) -> tokio::process::Command {
    let mut command = tokio::process::Command::new(&spec.exe);
    command
        .arg(&spec.ini)
        .arg(CONSOLE_MODE_ARG)
        .current_dir(&spec.cwd)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .kill_on_drop(true);
    for name in CONTROL_ENV_VARS {
        command.env_remove(name);
    }
    command.env(BRIDGE_ENV_VAR, "1");
    match &spec.nonce {
        Some(nonce) => command.env(NONCE_ENV_VAR, nonce),
        None => command.env_remove(NONCE_ENV_VAR),
    };
    // The client tells its events from text by the nonce alone: never the kinds without it.
    if spec.console_kinds && spec.nonce.is_some() {
        command.env(CONSOLE_KINDS_ENV_VAR, "1");
    }
    #[cfg(windows)]
    command.creation_flags(CREATE_NO_WINDOW);
    command
}

impl ClientSpawner for ProcessSpawner {
    fn spawn(&self, spec: &LaunchSpec) -> std::io::Result<ClientProcess> {
        let mut child = client_command(spec).spawn()?;
        let record = child.id().map(|pid| {
            crate::orphans::record_spawn(&spec.cwd, pid, &spec.exe, &spec.ini);
            (spec.cwd.clone(), pid)
        });
        let missing = || std::io::Error::other("child pipe missing");
        let stdin = child.stdin.take().ok_or_else(missing)?;
        let stdout = child.stdout.take().ok_or_else(missing)?;
        let stderr = child.stderr.take().ok_or_else(missing)?;
        Ok(ClientProcess {
            stdin: Box::new(stdin),
            stdout: Box::new(stdout),
            stderr: Box::new(stderr),
            exit: Box::new(ChildExit { child, record }),
        })
    }
}

struct ChildExit {
    child: tokio::process::Child,
    /// Account folder and process id of the client's record (see `orphans`).
    record: Option<(PathBuf, u32)>,
}

impl ProcessExit for ChildExit {
    fn wait(self: Box<Self>, kill: CancellationToken) -> ExitFuture {
        let ChildExit { mut child, record } = *self;
        Box::pin(async move {
            let code = tokio::select! {
                status = child.wait() => status.ok().and_then(|s| s.code()),
                () = kill.cancelled() => {
                    if let Err(error) = child.start_kill() {
                        tracing::debug!(%error, "AFK client already gone when killing");
                    }
                    child.wait().await.ok().and_then(|s| s.code())
                }
            };
            if let Some((dir, pid)) = record {
                crate::orphans::clear_record(&dir, pid);
            }
            code
        })
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;

    const NONCE: &str = "0123456789abcdef0123456789abcdef";

    fn spec(nonce: Option<&str>) -> LaunchSpec {
        LaunchSpec {
            exe: PathBuf::from("MinecraftClient.exe"),
            ini: PathBuf::from("MinecraftClient.ini"),
            cwd: PathBuf::from("account"),
            nonce: nonce.map(str::to_owned),
            console_kinds: false,
        }
    }

    /// What the command sets (`Some`) or removes (`None`) on top of this process's environment.
    fn envs(spec: &LaunchSpec) -> HashMap<String, Option<String>> {
        client_command(spec)
            .as_std()
            .get_envs()
            .map(|(name, value)| {
                let value = value.map(|v| v.to_string_lossy().into_owned());
                (name.to_string_lossy().into_owned(), value)
            })
            .collect()
    }

    #[test]
    fn the_client_gets_the_bridge_and_its_own_nonce_and_no_inherited_control_variable() {
        let with = envs(&spec(Some(NONCE)));
        assert_eq!(with.get(BRIDGE_ENV_VAR), Some(&Some("1".to_owned())));
        assert_eq!(with.get(NONCE_ENV_VAR), Some(&Some(NONCE.to_owned())));
        // Without a nonce of its own, one in the launcher's environment is removed, not passed on.
        let without = envs(&spec(None));
        assert_eq!(without.get(BRIDGE_ENV_VAR), Some(&Some("1".to_owned())));
        assert_eq!(without.get(NONCE_ENV_VAR), Some(&None));
        for set in [&with, &without] {
            for name in CONTROL_ENV_VARS {
                assert_eq!(set.get(name), Some(&None), "{name}");
            }
            assert_eq!(set.len(), CONTROL_ENV_VARS.len() + 2);
        }

        // The console kinds: ours with a nonce only, never the launcher's own.
        let kinds = envs(&LaunchSpec {
            console_kinds: true,
            ..spec(Some(NONCE))
        });
        assert_eq!(
            kinds.get(CONSOLE_KINDS_ENV_VAR),
            Some(&Some("1".to_owned()))
        );
        let no_nonce = envs(&LaunchSpec {
            console_kinds: true,
            ..spec(None)
        });
        assert_eq!(no_nonce.get(CONSOLE_KINDS_ENV_VAR), Some(&None));
    }

    #[test]
    fn debug_output_never_shows_the_nonce() {
        let text = format!("{:?}", spec(Some(NONCE)));
        assert!(!text.contains(NONCE));
        assert!(text.contains("<redacted>"));
        assert!(text.contains("MinecraftClient.ini"));
        assert!(format!("{:?}", spec(None)).contains("nonce: None"));
    }
}
