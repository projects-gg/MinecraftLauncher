//! Getting the AFK client ready for a connect, as the accounts' cards show it.
//!
//! A connect needs an installed client build. When none is installed and the launcher knows no
//! build (its content index predates the first publish, or it was offline when it last looked),
//! the manager refreshes the index once through the app's [`ReleaseSource`] and installs what it
//! lists; a known build that cannot be had while nothing is installed gets one more look too. Every
//! account waiting meanwhile shows the same [`ClientStep`]; a client that cannot be had leaves each
//! of them a [`ClientProblem`] to show instead of a code.

use std::sync::Arc;

use futures::future::BoxFuture;
use serde::{Deserialize, Serialize};

use crate::error::{AfkError, Result};
use crate::runtime::AfkClientRelease;

/// Refreshes the content index now (not waiting for the app's own schedule) and returns the AFK
/// client build it publishes for this computer: `Ok(None)` when a mirror answered and the index
/// lists none, [`AfkError::ContentUnavailable`] when no mirror could be reached.
pub type ReleaseSource =
    Arc<dyn Fn() -> BoxFuture<'static, Result<Option<AfkClientRelease>>> + Send + Sync>;

/// Where an account's connect stands while the AFK client is made ready, or why the last one
/// could not get it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "step", rename_all = "camelCase")]
pub enum ClientStep {
    /// Looking for the client build in a freshly refreshed content index.
    Checking,
    /// Downloading the build: `done` of `total` bytes (`total` is 0 while unknown).
    Downloading { done: u64, total: u64 },
    /// Verifying and unpacking the downloaded build.
    Installing,
    /// The last connect could not get the client. Cleared by the account's next connect, by
    /// stopping it, and by any preparation that succeeds.
    Failed { problem: ClientProblem },
}

impl ClientStep {
    /// The client is being made ready (not a failure).
    pub fn is_preparing(&self) -> bool {
        !matches!(self, Self::Failed { .. })
    }
}

/// Why the AFK client could not be had, in the player's terms (the UI words each one; the cause
/// itself goes to the log).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ClientProblem {
    /// The content index lists no client for this computer: none published yet (or not for this
    /// platform).
    NotPublished,
    /// No connection: neither the content index nor the download could be reached.
    Offline,
    /// The disk is full.
    DiskFull,
    /// Writing the client was refused or failed (permissions, an antivirus).
    Blocked,
    /// The download or the package was broken (a server error, a checksum or size mismatch, a
    /// damaged archive, no executable in it).
    Broken,
}

impl ClientProblem {
    /// The player's side of a failure to get the client.
    pub fn of(error: &AfkError) -> Self {
        match error {
            AfkError::NoClientRelease | AfkError::ClientNotInstalled => Self::NotPublished,
            AfkError::ContentUnavailable(_) => Self::Offline,
            AfkError::ClientUnavailable { problem, .. } => *problem,
            AfkError::Net(net) if net.is_disk_full() => Self::DiskFull,
            AfkError::Net(net) if net.is_network() => Self::Offline,
            AfkError::Net(pl_net::NetError::Io { .. }) => Self::Blocked,
            AfkError::Io { source, .. } if pl_net::is_disk_full(source) => Self::DiskFull,
            AfkError::Io { .. } => Self::Blocked,
            _ => Self::Broken,
        }
    }
}

#[cfg(test)]
mod tests {
    use std::path::PathBuf;

    use super::*;

    fn io(kind: std::io::ErrorKind) -> std::io::Error {
        std::io::Error::new(kind, "test")
    }

    #[test]
    fn nothing_published_and_no_index_say_so() {
        assert_eq!(
            ClientProblem::of(&AfkError::NoClientRelease),
            ClientProblem::NotPublished
        );
        assert_eq!(
            ClientProblem::of(&AfkError::ClientNotInstalled),
            ClientProblem::NotPublished
        );
        assert_eq!(
            ClientProblem::of(&AfkError::ContentUnavailable("all mirrors down".into())),
            ClientProblem::Offline
        );
    }

    #[test]
    fn download_failures_split_by_cause() {
        let offline = AfkError::Net(pl_net::NetError::AllMirrorsFailed {
            what: "windows-x86_64.zip".into(),
            last: "error trying to connect: dns error".into(),
        });
        assert_eq!(ClientProblem::of(&offline), ClientProblem::Offline);

        let missing = AfkError::Net(pl_net::NetError::Status {
            url: "https://cdn.projects.gg/v1/afk/4.0.0/windows-x86_64.zip".into(),
            status: 404,
        });
        assert_eq!(ClientProblem::of(&missing), ClientProblem::Broken);

        let checksum = AfkError::Net(pl_net::NetError::Checksum {
            path: PathBuf::from("x.zip"),
            expected: "a".into(),
            actual: "b".into(),
        });
        assert_eq!(ClientProblem::of(&checksum), ClientProblem::Broken);

        let full = AfkError::Net(pl_net::NetError::Io {
            path: PathBuf::from("x.zip"),
            source: io(std::io::ErrorKind::StorageFull),
        });
        assert_eq!(ClientProblem::of(&full), ClientProblem::DiskFull);

        let denied = AfkError::Net(pl_net::NetError::Io {
            path: PathBuf::from("x.zip"),
            source: io(std::io::ErrorKind::PermissionDenied),
        });
        assert_eq!(ClientProblem::of(&denied), ClientProblem::Blocked);
    }

    #[test]
    fn install_failures_split_by_cause() {
        let full = AfkError::io("bin/4.0.0", io(std::io::ErrorKind::StorageFull));
        assert_eq!(ClientProblem::of(&full), ClientProblem::DiskFull);
        let denied = AfkError::io("bin/4.0.0", io(std::io::ErrorKind::PermissionDenied));
        assert_eq!(ClientProblem::of(&denied), ClientProblem::Blocked);
        let archive = AfkError::archive("x.zip", "invalid Zip archive");
        assert_eq!(ClientProblem::of(&archive), ClientProblem::Broken);
        let release = AfkError::InvalidRelease("the package has no client at its root".into());
        assert_eq!(ClientProblem::of(&release), ClientProblem::Broken);
    }

    #[test]
    fn a_wrapped_problem_stays_itself() {
        let error = AfkError::ClientUnavailable {
            problem: ClientProblem::DiskFull,
            details: "I/O error".into(),
        };
        assert_eq!(ClientProblem::of(&error), ClientProblem::DiskFull);
    }

    #[test]
    fn steps_serialize_for_the_ui() {
        let json = |step: ClientStep| serde_json::to_string(&step).unwrap();
        assert_eq!(json(ClientStep::Checking), r#"{"step":"checking"}"#);
        assert_eq!(
            json(ClientStep::Downloading { done: 5, total: 10 }),
            r#"{"step":"downloading","done":5,"total":10}"#
        );
        assert_eq!(
            json(ClientStep::Failed {
                problem: ClientProblem::NotPublished
            }),
            r#"{"step":"failed","problem":"notPublished"}"#
        );
        assert!(ClientStep::Installing.is_preparing());
        assert!(
            !ClientStep::Failed {
                problem: ClientProblem::Offline
            }
            .is_preparing()
        );
    }
}
