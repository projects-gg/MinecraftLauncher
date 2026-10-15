use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Crate-wide error. Variants are matched by callers (offline detection, UI hints), so new
/// ones go at the end.
#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Net(#[from] pl_net::NetError),

    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error("invalid JSON in {what}: {source}")]
    Json {
        what: String,
        #[source]
        source: serde_json::Error,
    },

    #[error("invalid key: {0}")]
    InvalidKey(String),

    #[error("the content index is signed by an untrusted key ({key_id})")]
    UntrustedKey { key_id: String },

    #[error("the content index signature is invalid")]
    BadSignature,

    #[error("content index sequence {received} is older than the cached sequence {cached}")]
    Rollback { received: u64, cached: u64 },

    #[error("{what} uses schema {found}; this build understands up to {supported}")]
    UnsupportedSchema {
        what: String,
        found: u32,
        supported: u32,
    },

    #[error("{what} failed verification: expected {expected}, got {actual}")]
    Integrity {
        what: String,
        expected: String,
        actual: String,
    },

    #[error("unsafe path {path:?}: {reason}")]
    UnsafePath { path: String, reason: &'static str },

    #[error("invalid data: {0}")]
    Invalid(String),

    #[error("no cached content is available")]
    NoCache,

    #[error("no content mirror could be reached: {0}")]
    Offline(String),

    #[error("operation cancelled")]
    Cancelled,
}

impl Error {
    pub fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    pub fn json(what: impl Into<String>, source: serde_json::Error) -> Self {
        Self::Json {
            what: what.into(),
            source,
        }
    }

    pub(crate) fn unsafe_path(path: &str, reason: &'static str) -> Self {
        Self::UnsafePath {
            path: path.to_owned(),
            reason,
        }
    }

    /// True when the failure means "no connectivity" rather than bad content, so callers can
    /// switch to offline mode.
    pub fn is_offline(&self) -> bool {
        match self {
            Self::Offline(_) => true,
            Self::Net(e) => e.is_network(),
            _ => false,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        match self {
            Self::Cancelled => true,
            Self::Net(e) => e.is_cancelled(),
            _ => false,
        }
    }
}
