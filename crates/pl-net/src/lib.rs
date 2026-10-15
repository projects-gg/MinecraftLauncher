//! Networking for Projects Launcher.
//!
//! - [`Http`]: one shared HTTP client with sane timeouts, retries and mirror fallback.
//! - [`Downloader`]: parallel, checksum-verified, cancellable downloads that write to a
//!   temporary file and atomically move it into place, reporting aggregated progress.
//! - [`hash_file`] / [`verify_file`]: streaming SHA-1/SHA-256/SHA-512 helpers.

mod client;
mod download;
mod error;
mod hash;

pub use client::{Http, HttpConfig};
pub use download::{
    DownloadItem, DownloadOptions, DownloadProgress, DownloadSummary, Downloader, ProgressFn,
    VerifyMode,
};
pub use error::{NetError, Result};
pub use hash::{Checksum, HashAlgo, hash_bytes, hash_file, verify_file};
pub use tokio_util::sync::CancellationToken;
