use std::path::PathBuf;

pub type Result<T, E = Error> = std::result::Result<T, E>;

/// Crate-wide error. Prefer the existing variants; add new ones at the end only when a
/// caller genuinely needs to match on them.
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

    #[error("archive error in {path}: {message}")]
    Archive { path: PathBuf, message: String },

    #[error("Minecraft version {0} was not found")]
    VersionNotFound(String),

    #[error("{0} not found")]
    NotFound(String),

    #[error("invalid data: {0}")]
    Invalid(String),

    #[error("not supported: {0}")]
    Unsupported(String),

    #[error("no suitable Java runtime: {0}")]
    Java(String),

    #[error("process error: {0}")]
    Process(String),

    #[error("this operation needs an internet connection")]
    Offline,

    #[error("operation cancelled")]
    Cancelled,

    /// A file or folder with this name already exists (imports, renames and backups never
    /// overwrite).
    #[error("{} already exists", .0.display())]
    AlreadyExists(PathBuf),

    /// The OS trash could not take the item; nothing was deleted. The caller may ask the user
    /// and retry with permanent deletion.
    #[error("could not move {} to the trash: {message}", .path.display())]
    TrashUnavailable { path: PathBuf, message: String },

    /// Server List Ping failure, classified for the UI.
    #[error("server ping to {address} failed: {message}")]
    Ping {
        address: String,
        kind: crate::server::PingFailure,
        message: String,
    },

    /// The disk cannot hold what an install is about to download or unpack; nothing was written
    /// ([`crate::install::space::ensure_room`]).
    #[error(
        "not enough room: {} bytes needed for {} bytes, {} bytes free",
        .0.needed,
        .0.missing,
        .0.free
    )]
    NoRoom(crate::install::space::Shortfall),
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

    pub fn archive(path: impl Into<PathBuf>, message: impl std::fmt::Display) -> Self {
        Self::Archive {
            path: path.into(),
            message: message.to_string(),
        }
    }

    /// True when the error means "no connectivity", so callers can fall back to offline mode.
    pub fn is_offline(&self) -> bool {
        match self {
            Self::Offline => true,
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

    /// True when the disk (or the user's disk quota) is full. Steps that ran elsewhere (an
    /// archive, a mod loader installer's process) only kept the text of what failed.
    pub fn is_disk_full(&self) -> bool {
        match self {
            Self::Io { source, .. } => pl_net::is_disk_full(source),
            Self::Net(e) => e.is_disk_full(),
            Self::Archive { message, .. } | Self::Process(message) => {
                pl_net::mentions_disk_full(message)
            }
            _ => false,
        }
    }
}
