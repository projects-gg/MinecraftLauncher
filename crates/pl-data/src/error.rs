use std::path::{Path, PathBuf};

/// Why a folder cannot receive the data folder.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TargetProblem {
    /// Not an absolute path.
    NotAbsolute,
    /// The data already lives there.
    Same,
    /// Inside the current data folder.
    InsideCurrent,
    /// The current data folder is inside it.
    ContainsCurrent,
    /// A file, not a folder.
    NotAFolder,
    /// Already has something in it.
    NotEmpty,
    /// Cannot be created or written to.
    NotWritable,
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error("{}: {source}", path.display())]
    Io {
        path: PathBuf,
        #[source]
        source: std::io::Error,
    },
    #[error("cancelled")]
    Cancelled,
    #[error("zip: {0}")]
    Zip(#[from] zip::result::ZipError),
    #[error("JSON: {0}")]
    Json(#[from] serde_json::Error),
    #[error("cannot move the data folder to {}: {problem:?}", path.display())]
    Target {
        path: PathBuf,
        problem: TargetProblem,
    },
    #[error("not enough free space: {needed} bytes needed, {available} available")]
    NotEnoughSpace { needed: u64, available: u64 },
    #[error("{count} copied files differ from the original (first: {first})")]
    Mismatch { count: usize, first: String },
    /// A link in the data folder (a symbolic link, or a junction on Windows: e.g. `saves/` kept on
    /// another disk) could not be made again in the copy. `folder`: it stands for a folder.
    #[error("cannot make the link {} in the copy: {source}", path.display())]
    Link {
        path: PathBuf,
        folder: bool,
        #[source]
        source: std::io::Error,
    },
}

impl Error {
    pub(crate) fn io(path: impl AsRef<Path>, source: std::io::Error) -> Self {
        Self::Io {
            path: path.as_ref().to_path_buf(),
            source,
        }
    }

    pub(crate) fn link(path: impl AsRef<Path>, folder: bool, source: std::io::Error) -> Self {
        Self::Link {
            path: path.as_ref().to_path_buf(),
            folder,
            source,
        }
    }

    pub(crate) fn target(path: impl AsRef<Path>, problem: TargetProblem) -> Self {
        Self::Target {
            path: path.as_ref().to_path_buf(),
            problem,
        }
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    /// The underlying I/O error kind, for errors that came from the file system.
    pub fn io_kind(&self) -> Option<std::io::ErrorKind> {
        match self {
            Self::Io { source, .. } => Some(source.kind()),
            _ => None,
        }
    }
}

pub type Result<T, E = Error> = std::result::Result<T, E>;
