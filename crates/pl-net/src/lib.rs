//! Networking for Projects Launcher.
//!
//! - [`Http`]: one shared HTTP client with sane timeouts, retries and mirror fallback.
//! - [`Downloader`]: parallel, checksum-verified, cancellable downloads that write to a
//!   temporary file and atomically move it into place, reporting aggregated progress.
//! - [`hash_file`] / [`verify_file`]: streaming SHA-1/SHA-256/SHA-512 helpers.
//! - [`is_disk_full`] / [`mentions_disk_full`]: a full disk, however the error reached the caller.
//! - [`background`]: when downloads nobody asked for yet may run (connection cost, battery saver,
//!   other work) and when the next one is due.
//! - [`probe`]: one short request that tells how an address answered (or why it did not), for the
//!   troubleshooter.

pub mod background;
mod client;
mod download;
mod error;
mod hash;
pub mod probe;

pub use client::{Http, HttpConfig};
pub use download::{
    DownloadItem, DownloadOptions, DownloadProgress, DownloadSummary, Downloader, ProgressFn,
    VerifyMode, until_cancelled,
};
pub use error::{NetError, Result, is_disk_full, mentions_disk_full};
pub use hash::{Checksum, HashAlgo, hash_bytes, hash_file, verify_file};
pub use probe::{Probe, ProbeMethod, Reach};
pub use tokio_util::sync::CancellationToken;
