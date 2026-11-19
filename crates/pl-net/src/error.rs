use std::path::PathBuf;

pub type Result<T, E = NetError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum NetError {
    #[error("request to {url} failed: {source}")]
    Request {
        url: String,
        #[source]
        source: reqwest::Error,
    },

    #[error("{url} returned HTTP {status}")]
    Status { url: String, status: u16 },

    #[error("invalid JSON from {url}: {source}")]
    Json {
        url: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("checksum mismatch for {path}: expected {expected}, got {actual}")]
    Checksum {
        path: PathBuf,
        expected: String,
        actual: String,
    },

    #[error("size mismatch for {path}: expected {expected} bytes, got {actual}")]
    Size {
        path: PathBuf,
        expected: u64,
        actual: u64,
    },

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("no download URL for {0}")]
    NoSource(PathBuf),

    #[error("all sources failed for {what}: {last}")]
    AllMirrorsFailed { what: String, last: String },

    #[error("{failed} of {total} downloads failed; first error: {first}")]
    Batch {
        failed: usize,
        total: usize,
        first: String,
        /// True when every failure looked like a connectivity problem.
        offline: bool,
        /// The first failure's [`NetError::cause`].
        cause: &'static str,
        /// Where the first failed file came from ([`url_origin`] of its first URL).
        origin: &'static str,
    },

    #[error("operation cancelled")]
    Cancelled,

    #[error("HTTP client setup failed: {0}")]
    Setup(String),
}

impl NetError {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    /// True when the failure looks like missing connectivity (DNS, connect, timeout)
    /// rather than a bad file or server answer. Used to switch into offline mode.
    pub fn is_network(&self) -> bool {
        match self {
            Self::Request { source, .. } => {
                source.is_connect() || source.is_timeout() || source.is_request()
            }
            Self::AllMirrorsFailed { last, .. } => looks_offline(last),
            Self::Batch { offline, .. } => *offline,
            _ => false,
        }
    }

    /// What went wrong, in closed words for usage data (DL-CAUSE): never a URL, a path or a text
    /// from the server. `http5xx`, `http404`, `timeout`, `dropped` (the connection broke during
    /// the body), `checksum`, `size` and so on.
    pub fn cause(&self) -> &'static str {
        match self {
            Self::Request { source, .. } => {
                if source.is_timeout() {
                    "timeout"
                } else if source.is_connect() {
                    "connect"
                } else if source.is_body() || source.is_decode() {
                    "dropped"
                } else {
                    "request"
                }
            }
            Self::Status { status, .. } => status_cause(*status),
            Self::Json { .. } => "json",
            Self::Checksum { .. } => "checksum",
            Self::Size { .. } => "size",
            Self::Io { .. } => "io",
            Self::NoSource(_) => "nosource",
            Self::AllMirrorsFailed { last, .. } => text_cause(last),
            Self::Batch { cause, .. } => cause,
            Self::Cancelled => "cancelled",
            Self::Setup(_) => "setup",
        }
    }

    /// Where the failed request went, in closed words for usage data (DL-CAUSE): see
    /// [`url_origin`]; `none` when the error names no URL (a file's checksum or size).
    pub fn origin(&self) -> &'static str {
        match self {
            Self::Request { url, .. } | Self::Status { url, .. } | Self::Json { url, .. } => {
                url_origin(url)
            }
            Self::AllMirrorsFailed { what, .. } => url_origin(what),
            Self::Batch { origin, .. } => origin,
            _ => "none",
        }
    }

    /// True when the disk (or the user's disk quota) is full: nothing more can be written until
    /// room is made. Failures that only kept a text (a batch, all mirrors) are read too.
    pub fn is_disk_full(&self) -> bool {
        match self {
            Self::Io { source, .. } => is_disk_full(source),
            Self::Batch { first: text, .. } | Self::AllMirrorsFailed { last: text, .. } => {
                mentions_disk_full(text)
            }
            _ => false,
        }
    }
}

/// OS error codes for a full disk or a used-up disk quota (the same numbers mean other things on
/// other systems: 28 is "out of paper" on Windows).
#[cfg(windows)]
const DISK_FULL_CODES: &[i32] = &[
    39,   // ERROR_HANDLE_DISK_FULL
    112,  // ERROR_DISK_FULL
    1295, // ERROR_DISK_QUOTA_EXCEEDED
];
#[cfg(any(target_os = "linux", target_os = "android"))]
const DISK_FULL_CODES: &[i32] = &[28 /* ENOSPC */, 122 /* EDQUOT */];
#[cfg(all(unix, not(any(target_os = "linux", target_os = "android"))))]
const DISK_FULL_CODES: &[i32] = &[28 /* ENOSPC */, 69 /* EDQUOT */];
#[cfg(not(any(windows, unix)))]
const DISK_FULL_CODES: &[i32] = &[];

/// True when `error` means the disk (or the user's disk quota) is full.
pub fn is_disk_full(error: &std::io::Error) -> bool {
    use std::io::ErrorKind;
    matches!(
        error.kind(),
        ErrorKind::StorageFull | ErrorKind::QuotaExceeded
    ) || error
        .raw_os_error()
        .is_some_and(|code| DISK_FULL_CODES.contains(&code))
}

/// True when an error's text carries a full-disk OS error (`… (os error 112)`; the sentence before
/// it is in the system's language, the number is not). For failures that only kept their text: a
/// batch of downloads, a step that ran in another process.
pub fn mentions_disk_full(text: &str) -> bool {
    DISK_FULL_CODES
        .iter()
        .any(|code| text.contains(&format!("(os error {code})")))
}

/// An HTTP status in closed words: the few that mean something on their own, else its class.
fn status_cause(status: u16) -> &'static str {
    match status {
        403 => "http403",
        404 => "http404",
        429 => "http429",
        400..=499 => "http4xx",
        500..=599 => "http5xx",
        _ => "http",
    }
}

/// The cause of a failure that only kept its text (every mirror failed): the status of
/// `... returned HTTP 503`, a timeout or a refused connection, else `mirrors`.
fn text_cause(text: &str) -> &'static str {
    const STATUS: &str = "returned HTTP ";
    if let Some(at) = text.find(STATUS)
        && let Ok(status) = text[at + STATUS.len()..]
            .chars()
            .take(3)
            .collect::<String>()
            .parse::<u16>()
    {
        return status_cause(status);
    }
    let lower = text.to_ascii_lowercase();
    if lower.contains("timed out") || lower.contains("timeout") {
        "timeout"
    } else if lower.contains("connect") || lower.contains("dns") {
        "connect"
    } else {
        "mirrors"
    }
}

/// The service a URL belongs to, in closed words for usage data (DL-CAUSE): `mojang` (version
/// files, client jars, Mojang's Java), `assets`, `libraries`, `loader` (Fabric, Quilt, Forge,
/// NeoForge), `modrinth`, `projects` (our own files), `maven`, `github`, else `other`.
pub fn url_origin(url: &str) -> &'static str {
    let Some(host) = url::Url::parse(url)
        .ok()
        .and_then(|url| url.host_str().map(str::to_ascii_lowercase))
    else {
        return "other";
    };
    let is = |domain: &str| host == domain || host.ends_with(&format!(".{domain}"));
    if host == "resources.download.minecraft.net" {
        "assets"
    } else if host == "libraries.minecraft.net" {
        "libraries"
    } else if is("mojang.com") || is("minecraft.net") {
        "mojang"
    } else if is("fabricmc.net")
        || is("quiltmc.org")
        || is("minecraftforge.net")
        || is("neoforged.net")
    {
        "loader"
    } else if is("modrinth.com") {
        "modrinth"
    } else if is("projects.gg") {
        "projects"
    } else if host == "repo1.maven.org" || host == "repo.maven.apache.org" {
        "maven"
    } else if is("github.com") || is("githubusercontent.com") {
        "github"
    } else {
        "other"
    }
}

pub(crate) fn looks_offline(message: &str) -> bool {
    let m = message.to_ascii_lowercase();
    [
        "dns",
        "connect",
        "timed out",
        "timeout",
        "unreachable",
        "network",
        "no route",
    ]
    .iter()
    .any(|needle| m.contains(needle))
}

#[cfg(test)]
mod tests {
    use std::io;

    use super::*;

    #[test]
    fn full_disks_are_recognised() {
        assert!(is_disk_full(&io::Error::from(io::ErrorKind::StorageFull)));
        assert!(is_disk_full(&io::Error::from(io::ErrorKind::QuotaExceeded)));
        assert!(!is_disk_full(&io::Error::from(
            io::ErrorKind::PermissionDenied
        )));
        for &code in DISK_FULL_CODES {
            assert!(is_disk_full(&io::Error::from_raw_os_error(code)), "{code}");
        }
        #[cfg(windows)]
        {
            assert!(is_disk_full(&io::Error::from_raw_os_error(112)));
            assert!(is_disk_full(&io::Error::from_raw_os_error(39)));
            // ERROR_OUT_OF_PAPER, ENOSPC's number elsewhere.
            assert!(!is_disk_full(&io::Error::from_raw_os_error(28)));
        }
        #[cfg(unix)]
        assert!(is_disk_full(&io::Error::from_raw_os_error(28)));
    }

    #[test]
    fn a_batch_that_hit_a_full_disk_says_so() {
        let full = io::Error::from_raw_os_error(DISK_FULL_CODES[0]);
        let first = NetError::io("/data/assets/objects/ab/abc.part", full);
        assert!(first.is_disk_full());
        let batch = NetError::Batch {
            failed: 3,
            total: 40,
            first: first.to_string(),
            offline: false,
            cause: first.cause(),
            origin: "assets",
        };
        assert!(batch.is_disk_full(), "{batch}");
        assert!(mentions_disk_full(&format!("download failed: {batch}")));

        let reset = NetError::Batch {
            failed: 1,
            total: 40,
            first: "connection reset by peer".to_owned(),
            offline: true,
            cause: "dropped",
            origin: "libraries",
        };
        assert!(!reset.is_disk_full());
        assert!(!mentions_disk_full(
            "I/O error at /x: Access is denied. (os error 5)"
        ));
        assert!(!NetError::Cancelled.is_disk_full());
    }

    #[test]
    fn causes_and_origins_are_closed_words() {
        let status = |status| NetError::Status {
            url: "https://libraries.minecraft.net/org/lwjgl/lwjgl/3.4.1/lwjgl-3.4.1.jar".into(),
            status,
        };
        assert_eq!(status(404).cause(), "http404");
        assert_eq!(status(410).cause(), "http4xx");
        assert_eq!(status(503).cause(), "http5xx");
        assert_eq!(status(503).origin(), "libraries");
        let checksum = NetError::Checksum {
            path: "/home/korates/x.jar".into(),
            expected: "a".into(),
            actual: "b".into(),
        };
        assert_eq!((checksum.cause(), checksum.origin()), ("checksum", "none"));
        let mirrors = NetError::AllMirrorsFailed {
            what: "https://cdn.projects.gg/v1/files/abc".into(),
            last: "https://cdn.projects.gg/v1/files/abc returned HTTP 502".into(),
        };
        assert_eq!((mirrors.cause(), mirrors.origin()), ("http5xx", "projects"));
        let silent = NetError::AllMirrorsFailed {
            what: String::new(),
            last: "no URLs given".into(),
        };
        assert_eq!((silent.cause(), silent.origin()), ("mirrors", "other"));
        for (url, origin) in [
            (
                "https://resources.download.minecraft.net/ab/abcdef",
                "assets",
            ),
            (
                "https://piston-meta.mojang.com/mc/game/version_manifest_v2.json",
                "mojang",
            ),
            (
                "https://piston-data.mojang.com/v1/objects/abc/client.jar",
                "mojang",
            ),
            ("https://meta.fabricmc.net/v2/versions/loader", "loader"),
            ("https://maven.neoforged.net/releases/x.jar", "loader"),
            (
                "https://cdn.modrinth.com/data/AANobbMI/versions/x/sodium.jar",
                "modrinth",
            ),
            ("https://repo1.maven.org/maven2/x.jar", "maven"),
            ("https://evilprojects.gg/x", "other"),
            ("https://korates.example/x", "other"),
            ("not a url", "other"),
        ] {
            assert_eq!(url_origin(url), origin, "{url}");
        }
    }
}
