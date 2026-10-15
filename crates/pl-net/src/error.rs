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
