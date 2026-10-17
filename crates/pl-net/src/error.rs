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
        };
        assert!(batch.is_disk_full(), "{batch}");
        assert!(mentions_disk_full(&format!("download failed: {batch}")));

        let reset = NetError::Batch {
            failed: 1,
            total: 40,
            first: "connection reset by peer".to_owned(),
            offline: true,
        };
        assert!(!reset.is_disk_full());
        assert!(!mentions_disk_full(
            "I/O error at /x: Access is denied. (os error 5)"
        ));
        assert!(!NetError::Cancelled.is_disk_full());
    }
}
