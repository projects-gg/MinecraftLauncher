//! The launcher's data folder as a whole (Settings → Veri):
//!
//! - [`usage`]: space taken by caches and logs, free space and disk size.
//! - [`housekeeping`]: the cleanup policy (every age and size rule): the automatic tidy-up
//!   (launcher trash, unfinished downloads, temporary files, old update installers) and the
//!   cleanup on request with a size preview (caches, old logs, crash reports). Instances, game
//!   files in use, accounts and AFK data are never touched; unused versions, Java runtimes,
//!   libraries and assets are judged by `pl_mc::shared_files`.
//! - [`bundle`]: support bundles (logs, crash reports, system and instance reports) with every
//!   secret masked by [`mask::mask_secrets`] (the launcher's one masking code, `pl_mc::mask`).
//! - [`logfile`]: the launcher's own daily log files, named by the local date.
//! - [`relocate`]: moving the data folder to another disk through a pointer file kept outside
//!   it: copy with progress and cancel, flush, verify, switch, and delete the old folders only
//!   after the next start succeeded. Links inside it (junctions, symbolic links) are made again
//!   in the copy, never copied through.
//! - [`restart`]: the one-shot note an update leaves for the next start (AFK accounts to bring
//!   back, the window, the page, the release notes).
//! - [`troubleshoot`]: the troubleshooter's rules (Settings → Sorun yardımcısı): what each check
//!   found, its code for support and its one-tap fix, put together as the checks come in.
//!
//! Everything here is blocking file work, besides [`troubleshoot`]'s pure rules; the app runs it on
//! a blocking thread.

pub mod bundle;
mod error;
pub mod housekeeping;
mod links;
pub mod logfile;
pub mod mask;
pub mod relocate;
pub mod restart;
pub mod troubleshoot;
pub mod usage;

pub use error::{Error, Result, TargetProblem};
pub use mask::{MASK, mask_secrets};
pub use tokio_util::sync::CancellationToken;
