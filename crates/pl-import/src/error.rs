use std::path::PathBuf;

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("I/O error at {path}: {source}")]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },

    #[error(transparent)]
    Mc(#[from] pl_mc::Error),

    #[error("the import was cancelled")]
    Cancelled,

    #[error("not enough free space: {needed} bytes needed, {free} free")]
    NoSpace { needed: u64, free: u64 },

    #[error("{0}")]
    Invalid(String),
}

impl Error {
    pub(crate) fn io(path: impl Into<PathBuf>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.into(),
            source,
        }
    }

    /// The disk filled up while copying (the OS's own "disk full").
    pub(crate) fn is_disk_full(&self) -> bool {
        match self {
            Self::NoSpace { .. } => true,
            Self::Io { source, .. } => is_disk_full_io(source),
            _ => false,
        }
    }
}

pub(crate) fn is_disk_full_io(error: &std::io::Error) -> bool {
    // ERROR_DISK_FULL (112) and ERROR_HANDLE_DISK_FULL (39) on Windows, ENOSPC elsewhere.
    error.kind() == std::io::ErrorKind::StorageFull
        || (cfg!(windows) && matches!(error.raw_os_error(), Some(39 | 112)))
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
