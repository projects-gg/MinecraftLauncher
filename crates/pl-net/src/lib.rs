//! Networking for Projects Launcher.
//!
//! - [`Http`]: one shared HTTP client with sane timeouts, retries and mirror fallback.
//! - [`Downloader`]: parallel, checksum-verified, cancellable downloads that write to a
//!   temporary file and atomically move it into place, reporting aggregated progress.
//! - [`hash_file`] / [`verify_file`]: streaming SHA-1/SHA-256/SHA-512 helpers.
//! - [`is_disk_full`] / [`mentions_disk_full`]: a full disk, however the error reached the caller.

mod client;
mod download;
mod error;
mod hash;

pub use client::{Http, HttpConfig};
pub use download::{
    DownloadItem, DownloadOptions, DownloadProgress, DownloadSummary, Downloader, ProgressFn,
    VerifyMode, until_cancelled,
};
pub use error::{NetError, Result, is_disk_full, mentions_disk_full};
pub use hash::{Checksum, HashAlgo, hash_bytes, hash_file, verify_file};
pub use tokio_util::sync::CancellationToken;
