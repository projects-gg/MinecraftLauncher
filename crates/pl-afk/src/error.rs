use std::path::PathBuf;

use crate::form::ValidationIssue;

pub type Result<T, E = AfkError> = std::result::Result<T, E>;

/// Crate-wide error. Messages are English and technical: the UI maps variants to Turkish text and
/// never shows these strings directly. No variant ever carries a login command or password.
#[derive(Debug, thiserror::Error)]
pub enum AfkError {
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

    #[error("invalid data: {0}")]
    InvalidData(String),

    #[error("archive error in {path}: {message}")]
    Archive { path: PathBuf, message: String },

    /// The OS keychain could not be read or written (locked, missing Secret Service, ...).
    #[error("secret store error: {0}")]
    Secret(String),

    #[error("nickname must be 3-16 characters of letters, digits and underscores")]
    InvalidNickname,

    #[error("an AFK account with this nickname already exists")]
    DuplicateNickname,

    #[error("AFK account {0} not found")]
    AccountNotFound(String),

    #[error("AFK settings are incomplete: {0:?}")]
    InvalidSettings(Vec<ValidationIssue>),

    #[error("the AFK client is not installed")]
    ClientNotInstalled,

    #[error("invalid AFK client release: {0}")]
    InvalidRelease(String),

    #[error("the AFK session is not running")]
    NotRunning,

    #[error("a bulk connect is already running")]
    Busy,

    #[error("operation cancelled")]
    Cancelled,
}

impl AfkError {
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

    pub fn archive(path: impl Into<PathBuf>, message: impl std::fmt::Display) -> Self {
        Self::Archive {
            path: path.into(),
            message: message.to_string(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        match self {
            Self::Cancelled => true,
            Self::Net(e) => e.is_cancelled(),
            _ => false,
        }
    }

    /// True when the failure looks like missing connectivity.
    pub fn is_network(&self) -> bool {
        matches!(self, Self::Net(e) if e.is_network())
    }
}
