//! What happens after the client stops on its own.
//!
//! A hosted client (`ExitOnFailure = true`) exits on every disconnect, so reconnecting is the
//! launcher's job. Legacy behavior is kept (15, 30, 45, 60 s, then every 60 s; the counter resets
//! when the game is joined) with the legacy bugs fixed: the AutoRelog retry limit (-1 = unlimited)
//! and the kick-message filter are honored, and the decision always uses the latest settings.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::bridge::DisconnectReason;
use crate::model::AutoRelogOptions;
use crate::util::{contains_ignore_case, one_line, strip_color_codes};

/// Longest server message kept in a failure reason (characters).
const MAX_MESSAGE_CHARS: usize = 300;

/// Back-off between automatic restarts: `step × attempt`, capped at `max`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartSchedule {
    pub step: Duration,
    pub max: Duration,
}

impl Default for RestartSchedule {
    fn default() -> Self {
        Self {
            step: Duration::from_secs(15),
            max: Duration::from_secs(60),
        }
    }
}

impl RestartSchedule {
    /// Delay before restart number `attempt` (1-based).
    pub fn delay(&self, attempt: u32) -> Duration {
        self.step.saturating_mul(attempt.max(1)).min(self.max)
    }
}

/// Why a session stopped or is reconnecting. Messages come from the server or the client
/// (formatting codes removed, flattened to one line) and are meant for display.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum FailureReason {
    /// Kicked by the server while in game.
    Kicked {
        message: Option<String>,
    },
    /// The server refused the login.
    LoginRejected {
        message: Option<String>,
    },
    ConnectionLost {
        message: Option<String>,
    },
    LoggedOut {
        message: Option<String>,
    },
    /// The client reported a disconnect reason this launcher does not know.
    Disconnected {
        message: Option<String>,
    },
    /// The server's version is not supported by the client (pick a version in the settings).
    UnsupportedVersion {
        message: Option<String>,
    },
    /// The client reported a failure without a reason.
    Failed {
        message: Option<String>,
    },
    /// The client exited without reporting anything (crash, killed, unknown exit code).
    UnexpectedExit {
        code: Option<i32>,
    },
    /// The client could not be started.
    LaunchFailed {
        message: String,
    },
    /// No client build is installed.
    ClientMissing,
    /// The login commands could not be read from the OS keychain.
    SecretsUnavailable,
}

impl FailureReason {
    /// `DescribeExitCode`: what an exit code means when the client reported nothing else.
    pub fn from_exit_code(code: Option<i32>) -> Self {
        match code {
            Some(1) => Self::LoggedOut { message: None },
            Some(2) => Self::Kicked { message: None },
            Some(3) => Self::ConnectionLost { message: None },
            Some(4) => Self::LoginRejected { message: None },
            other => Self::UnexpectedExit { code: other },
        }
    }

    fn message(&self) -> Option<&str> {
        match self {
            Self::Kicked { message }
            | Self::LoginRejected { message }
            | Self::ConnectionLost { message }
            | Self::LoggedOut { message }
            | Self::Disconnected { message }
            | Self::UnsupportedVersion { message }
            | Self::Failed { message } => message.as_deref(),
            _ => None,
        }
    }
}

/// The last problem the running client reported through the bridge.
#[derive(Debug, Clone, PartialEq)]
pub(crate) struct Failure {
    pub reason: Option<DisconnectReason>,
    /// Verbatim (formatting codes removed), one line, length-limited.
    pub message: Option<String>,
    pub version_error: bool,
}

impl Failure {
    pub(crate) fn new(
        reason: Option<DisconnectReason>,
        message: Option<&str>,
        version_error: bool,
    ) -> Self {
        let message = message
            .map(|m| one_line(&strip_color_codes(m), MAX_MESSAGE_CHARS))
            .filter(|m| !m.is_empty());
        Self {
            reason,
            message,
            version_error,
        }
    }

    /// Combines a new report with the previous one: MCC sends `disconnected` (with the kick
    /// message) and then `error` (same reason, no message); the message must survive.
    pub(crate) fn merge(previous: Option<Failure>, next: Failure) -> Failure {
        match previous {
            Some(previous) if next.message.is_none() && previous.reason == next.reason => Failure {
                message: previous.message,
                version_error: previous.version_error || next.version_error,
                ..next
            },
            _ => next,
        }
    }

    pub(crate) fn to_reason(&self) -> FailureReason {
        let message = self.message.clone();
        if self.version_error {
            return FailureReason::UnsupportedVersion { message };
        }
        match &self.reason {
            Some(DisconnectReason::InGameKick) => FailureReason::Kicked { message },
            Some(DisconnectReason::LoginRejected) => FailureReason::LoginRejected { message },
            Some(DisconnectReason::ConnectionLost) => FailureReason::ConnectionLost { message },
            Some(DisconnectReason::UserLogout) => FailureReason::LoggedOut { message },
            Some(DisconnectReason::Other(_)) => FailureReason::Disconnected { message },
            None => FailureReason::Failed { message },
        }
    }
}

/// Outcome once the client process is gone.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum ExitDecision {
    Stopped,
    Fail(FailureReason),
    /// Retry limit reached.
    GiveUp(FailureReason),
    Restart {
        attempt: u32,
        reason: FailureReason,
    },
}

/// MCC's `AutoRelogRetryPolicy.ShouldReconnect` adapted to the launcher: with "ignore kick
/// message" off, kicks and login rejections only reconnect when the server message contains one of
/// the configured kick messages; connection losses, crashes and other failures always do.
pub(crate) fn kick_filter_allows(
    relog: &AutoRelogOptions,
    reason: Option<&DisconnectReason>,
    message: Option<&str>,
) -> bool {
    if relog.ignore_kick_message {
        return true;
    }
    match reason {
        Some(DisconnectReason::InGameKick | DisconnectReason::LoginRejected) => {
            let message = message.unwrap_or_default();
            relog
                .kick_messages
                .iter()
                .any(|k| !k.is_empty() && contains_ignore_case(message, k))
        }
        _ => true,
    }
}

/// Restart (or not) after a failure, honoring the retry limit. `attempts` counts restarts since the
/// last successful join.
pub(crate) fn decide_failure(
    relog: &AutoRelogOptions,
    reason: Option<&DisconnectReason>,
    display: FailureReason,
    attempts: u32,
) -> ExitDecision {
    if !relog.enabled || !kick_filter_allows(relog, reason, display.message()) {
        return ExitDecision::Fail(display);
    }
    if relog.retries >= 0 && attempts >= relog.retries.unsigned_abs() {
        return ExitDecision::GiveUp(display);
    }
    ExitDecision::Restart {
        attempt: attempts + 1,
        reason: display,
    }
}

/// `OnProcessExited`, with the fixes described in the module docs.
pub(crate) fn decide_exit(
    relog: &AutoRelogOptions,
    stop_requested: bool,
    exit_code: Option<i32>,
    failure: Option<&Failure>,
    attempts: u32,
) -> ExitDecision {
    if stop_requested {
        return ExitDecision::Stopped;
    }

    if !relog.enabled {
        // Legacy: errors stay visible, a clean exit without a reported problem is a plain stop.
        return match (failure, exit_code) {
            (Some(failure), _) => ExitDecision::Fail(failure.to_reason()),
            (None, Some(0)) => ExitDecision::Stopped,
            (None, code) => ExitDecision::Fail(FailureReason::from_exit_code(code)),
        };
    }

    let (reason, display) = match (failure, exit_code) {
        (Some(failure), _) => (failure.reason.clone(), failure.to_reason()),
        // Legacy: a clean exit nobody asked for counts as a lost connection.
        (None, Some(0)) => (None, FailureReason::ConnectionLost { message: None }),
        (None, code) => (
            code.and_then(DisconnectReason::from_exit_code),
            FailureReason::from_exit_code(code),
        ),
    };
    decide_failure(relog, reason.as_ref(), display, attempts)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn relog(retries: i32, ignore_kick_message: bool) -> AutoRelogOptions {
        AutoRelogOptions {
            retries,
            ignore_kick_message,
            kick_messages: vec!["Server is restarting".into(), String::new()],
            ..AutoRelogOptions::default()
        }
    }

    #[test]
    fn schedule_is_15_30_45_60_then_60() {
        let schedule = RestartSchedule::default();
        let delays: Vec<u64> = (1..=7).map(|a| schedule.delay(a).as_secs()).collect();
        assert_eq!(delays, vec![15, 30, 45, 60, 60, 60, 60]);
        assert_eq!(schedule.delay(u32::MAX).as_secs(), 60);
    }

    #[test]
    fn retry_limit_counts_restarts_since_last_join() {
        let limited = relog(2, true);
        let kicked = Failure::new(Some(DisconnectReason::InGameKick), Some("bye"), false);
        assert_eq!(
            decide_exit(&limited, false, Some(2), Some(&kicked), 0),
            ExitDecision::Restart {
                attempt: 1,
                reason: FailureReason::Kicked {
                    message: Some("bye".into())
                }
            }
        );
        assert!(matches!(
            decide_exit(&limited, false, Some(2), Some(&kicked), 1),
            ExitDecision::Restart { attempt: 2, .. }
        ));
        assert!(matches!(
            decide_exit(&limited, false, Some(2), Some(&kicked), 2),
            ExitDecision::GiveUp(FailureReason::Kicked { .. })
        ));
        let unlimited = relog(-1, true);
        assert!(matches!(
            decide_exit(&unlimited, false, Some(3), None, 10_000),
            ExitDecision::Restart {
                attempt: 10_001,
                ..
            }
        ));
        assert!(matches!(
            decide_exit(&relog(0, true), false, Some(3), None, 0),
            ExitDecision::GiveUp(_)
        ));
    }

    #[test]
    fn kick_filter_only_applies_to_kicks() {
        let filtered = relog(-1, false);
        let restart = Failure::new(
            Some(DisconnectReason::InGameKick),
            Some("§cserver is RESTARTING, brb"),
            false,
        );
        assert!(matches!(
            decide_exit(&filtered, false, Some(2), Some(&restart), 0),
            ExitDecision::Restart { .. }
        ));
        let banned = Failure::new(Some(DisconnectReason::InGameKick), Some("Banned"), false);
        assert_eq!(
            decide_exit(&filtered, false, Some(2), Some(&banned), 0),
            ExitDecision::Fail(FailureReason::Kicked {
                message: Some("Banned".into())
            })
        );
        // Exit code 2 alone (no message) does not match any kick message.
        assert!(matches!(
            decide_exit(&filtered, false, Some(2), None, 0),
            ExitDecision::Fail(FailureReason::Kicked { message: None })
        ));
        // Connection losses and crashes always reconnect.
        assert!(matches!(
            decide_exit(&filtered, false, Some(3), None, 0),
            ExitDecision::Restart { .. }
        ));
        assert!(matches!(
            decide_exit(&filtered, false, Some(-1073741819), None, 0),
            ExitDecision::Restart {
                reason: FailureReason::UnexpectedExit { .. },
                ..
            }
        ));
    }

    #[test]
    fn without_auto_relog_the_legacy_outcomes_remain() {
        let off = AutoRelogOptions {
            enabled: false,
            ..AutoRelogOptions::default()
        };
        assert_eq!(
            decide_exit(&off, false, Some(0), None, 0),
            ExitDecision::Stopped
        );
        assert_eq!(
            decide_exit(&off, false, Some(4), None, 0),
            ExitDecision::Fail(FailureReason::LoginRejected { message: None })
        );
        let lost = Failure::new(Some(DisconnectReason::ConnectionLost), None, false);
        assert_eq!(
            decide_exit(&off, false, Some(0), Some(&lost), 0),
            ExitDecision::Fail(FailureReason::ConnectionLost { message: None })
        );
        assert_eq!(
            decide_exit(&off, true, Some(2), Some(&lost), 0),
            ExitDecision::Stopped
        );
        assert_eq!(
            decide_exit(&relog(-1, true), false, Some(0), None, 0),
            ExitDecision::Restart {
                attempt: 1,
                reason: FailureReason::ConnectionLost { message: None }
            }
        );
    }

    #[test]
    fn merge_keeps_the_kick_message() {
        let disconnected = Failure::new(
            Some(DisconnectReason::InGameKick),
            Some("Sunucu\nyeniden başlatılıyor"),
            false,
        );
        let error = Failure::new(Some(DisconnectReason::InGameKick), None, false);
        let merged = Failure::merge(Some(disconnected), error);
        assert_eq!(
            merged.message.as_deref(),
            Some("Sunucu yeniden başlatılıyor")
        );
        let version = Failure::new(None, Some("Unsupported"), true);
        assert_eq!(
            version.to_reason(),
            FailureReason::UnsupportedVersion {
                message: Some("Unsupported".into())
            }
        );
        let other = Failure::merge(Some(version), Failure::new(None, Some("x"), false));
        assert_eq!(other.message.as_deref(), Some("x"));
    }
}
