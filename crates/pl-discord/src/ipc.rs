//! Local transport: Windows named pipes or Unix domain sockets.

use std::collections::HashSet;
use std::path::{Path, PathBuf};

use tokio::io::{AsyncRead, AsyncWrite};

use crate::error::{Error, Result};

pub(crate) trait IpcStream: AsyncRead + AsyncWrite + Send + Unpin {}
impl<T: AsyncRead + AsyncWrite + Send + Unpin> IpcStream for T {}
pub(crate) type BoxedStream = Box<dyn IpcStream>;

/// Directories (from these variables, then `/tmp`) that may hold Discord's Unix socket.
const UNIX_ENV_DIRS: [&str; 4] = ["XDG_RUNTIME_DIR", "TMPDIR", "TMP", "TEMP"];
/// Sub-folders used by sandboxed installs (Flatpak, Snap, Vesktop's Flatpak).
const UNIX_SUBDIRS: [&str; 4] = [
    "",
    "app/com.discordapp.Discord",
    "snap.discord",
    ".flatpak/dev.vencord.Vesktop/xdg-run",
];
const SLOTS: u8 = 10;

/// IPC endpoints Discord may listen on, in the order they are tried.
pub fn default_endpoints() -> Vec<PathBuf> {
    #[cfg(windows)]
    {
        (0..SLOTS)
            .map(|slot| PathBuf::from(format!(r"\\.\pipe\discord-ipc-{slot}")))
            .collect()
    }
    #[cfg(not(windows))]
    {
        unix_endpoints(&|name| std::env::var(name).ok())
    }
}

/// Unix socket candidates: every base directory × sandbox sub-folder × `discord-ipc-{0..9}`.
#[cfg_attr(windows, allow(dead_code))]
pub(crate) fn unix_endpoints(env: &dyn Fn(&str) -> Option<String>) -> Vec<PathBuf> {
    let mut bases: Vec<PathBuf> = UNIX_ENV_DIRS
        .iter()
        .filter_map(|name| env(name))
        .filter(|value| !value.is_empty())
        .map(PathBuf::from)
        .collect();
    bases.push(PathBuf::from("/tmp"));

    let mut seen = HashSet::new();
    let mut endpoints = Vec::new();
    for base in &bases {
        for sub in UNIX_SUBDIRS {
            let dir = sub
                .split('/')
                .filter(|part| !part.is_empty())
                .fold(base.clone(), |dir, part| dir.join(part));
            for slot in 0..SLOTS {
                let path = dir.join(format!("discord-ipc-{slot}"));
                if seen.insert(path.clone()) {
                    endpoints.push(path);
                }
            }
        }
    }
    endpoints
}

/// Connects to the first endpoint that accepts.
pub(crate) async fn connect_any(endpoints: &[PathBuf]) -> Result<(BoxedStream, PathBuf)> {
    for endpoint in endpoints {
        match connect(endpoint).await {
            Ok(stream) => return Ok((stream, endpoint.clone())),
            Err(error) => {
                tracing::trace!(endpoint = %endpoint.display(), %error, "Discord IPC endpoint unavailable");
            }
        }
    }
    Err(Error::NotRunning)
}

#[cfg(windows)]
async fn connect(path: &Path) -> std::io::Result<BoxedStream> {
    let client = tokio::net::windows::named_pipe::ClientOptions::new().open(path)?;
    Ok(Box::new(client))
}

#[cfg(unix)]
async fn connect(path: &Path) -> std::io::Result<BoxedStream> {
    Ok(Box::new(tokio::net::UnixStream::connect(path).await?))
}

#[cfg(not(any(windows, unix)))]
async fn connect(_path: &Path) -> std::io::Result<BoxedStream> {
    Err(std::io::Error::new(
        std::io::ErrorKind::Unsupported,
        "Discord IPC is not supported on this platform",
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unix_candidates_cover_sandboxes_in_order() {
        let env = |name: &str| match name {
            "XDG_RUNTIME_DIR" => Some("/run/user/1000".to_owned()),
            "TMPDIR" => Some(String::new()),
            "TMP" => Some("/tmp".to_owned()),
            _ => None,
        };
        let endpoints = unix_endpoints(&env);
        let normalized: Vec<String> = endpoints
            .iter()
            .map(|p| p.to_string_lossy().replace('\\', "/"))
            .collect();
        assert_eq!(normalized[0], "/run/user/1000/discord-ipc-0");
        assert_eq!(normalized[9], "/run/user/1000/discord-ipc-9");
        assert_eq!(
            normalized[10],
            "/run/user/1000/app/com.discordapp.Discord/discord-ipc-0"
        );
        assert_eq!(normalized[20], "/run/user/1000/snap.discord/discord-ipc-0");
        assert_eq!(
            normalized[30],
            "/run/user/1000/.flatpak/dev.vencord.Vesktop/xdg-run/discord-ipc-0"
        );
        assert_eq!(normalized[40], "/tmp/discord-ipc-0");
        // `TMP=/tmp` and the final `/tmp` fallback are deduplicated; empty TMPDIR is skipped.
        assert_eq!(normalized.len(), 80);
    }

    #[cfg(windows)]
    #[test]
    fn windows_pipes() {
        let endpoints = default_endpoints();
        assert_eq!(endpoints.len(), 10);
        assert_eq!(endpoints[0], PathBuf::from(r"\\.\pipe\discord-ipc-0"));
    }

    #[tokio::test]
    async fn nothing_listening_means_not_running() {
        let dir = tempfile::tempdir().unwrap();
        #[cfg(windows)]
        let endpoint = PathBuf::from(format!(
            r"\\.\pipe\pl-discord-missing-{}",
            uuid::Uuid::new_v4()
        ));
        #[cfg(not(windows))]
        let endpoint = dir.path().join("discord-ipc-0");
        let result = connect_any(&[endpoint]).await;
        assert!(matches!(result, Err(Error::NotRunning)));
        drop(dir);
    }
}
