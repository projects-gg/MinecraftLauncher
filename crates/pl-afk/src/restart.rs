//! What happens after the client stops on its own.
//!
//! A hosted client (`ExitOnFailure = true`) exits on every disconnect, so reconnecting is the
//! launcher's job. Restarts back off: the first waits the account's shortest AutoRelog delay and
//! each further one in a row waits longer, doubling towards a minute (see [`RestartSchedule`]).
//! The count starts over only after a lasting connection: joined and kicked a second later is a
//! failure like any other, so a login plugin or an anti-bot kick no longer brings the client
//! back every few seconds forever. Kicks reconnecting cannot fix (a ban, the whitelist, a wrong
//! password, a premium-only server, a login from elsewhere; "already connected" and login
//! timeouts after several in a row) stop the session with a [`KickCause`]. The legacy bugs stay
//! fixed: the AutoRelog retry limit (-1 = unlimited) and the kick-message filter are honored, and
//! every decision uses the latest settings.

use std::time::Duration;

use serde::{Deserialize, Serialize};

use crate::bridge::DisconnectReason;
use crate::form::{MAX_RELOG_DELAY_SECONDS, MIN_RELOG_DELAY_SECONDS, ordered};
use crate::model::AutoRelogOptions;
use crate::util::{contains_ignore_case, one_line, strip_color_codes};

/// Longest server message kept in a failure reason (characters).
const MAX_MESSAGE_CHARS: usize = 300;

/// The wait before an automatic restart. It comes from the account's AutoRelog delay ("Bekleme
/// süresi", `delay_min`..`delay_max` seconds): the first restart waits the shortest delay and
/// restart number `ramp` and later ones the longest, with steps in between. Failures in a row also
/// double the shortest delay each time, up to `backoff_cap`, so a client that cannot get in waits
/// longer and longer whatever the account says (the account's own longer delays still win).
/// Whatever the stored settings say, the delays stay between `floor` and `ceiling`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct RestartSchedule {
    /// Shortest wait (servers and proxies throttle logins from one address for a few seconds,
    /// and every attempt starts a client process).
    pub floor: Duration,
    /// Longest wait.
    pub ceiling: Duration,
    /// Restart number that first waits the longest delay (1: every restart does).
    pub ramp: u32,
    /// Where the doubling of failures in a row stops.
    pub backoff_cap: Duration,
    /// Time in game after which the connection counts as working: the next restart starts over
    /// from the shortest delay.
    pub stable_after: Duration,
}

impl Default for RestartSchedule {
    fn default() -> Self {
        Self {
            floor: Duration::from_secs_f64(MIN_RELOG_DELAY_SECONDS),
            ceiling: Duration::from_secs_f64(MAX_RELOG_DELAY_SECONDS),
            ramp: 4,
            backoff_cap: Duration::from_secs(60),
            stable_after: Duration::from_secs(120),
        }
    }
}

impl RestartSchedule {
    /// Delay before restart number `attempt` (1-based, counted since the last lasting
    /// connection) of an account with `relog` settings. Inverted ranges are swapped; non-finite
    /// values fall back to the AutoRelog defaults.
    pub fn delay(&self, relog: &AutoRelogOptions, attempt: u32) -> Duration {
        let floor = self.floor.as_secs_f64();
        let ceiling = self.ceiling.as_secs_f64().max(floor);
        let defaults = AutoRelogOptions::default();
        let bound = |seconds: f64, fallback: f64| {
            let seconds = if seconds.is_finite() {
                seconds
            } else {
                fallback
            };
            seconds.clamp(floor, ceiling)
        };
        let (shortest, longest) = ordered(
            bound(relog.delay_min, defaults.delay_min),
            bound(relog.delay_max, defaults.delay_max),
        );
        let ramp = self.ramp.max(1);
        let fraction = if ramp == 1 {
            1.0
        } else {
            f64::from(attempt.clamp(1, ramp) - 1) / f64::from(ramp - 1)
        };
        let ramped = shortest + (longest - shortest) * fraction;
        // 1, 2, 4, 8, ... times the shortest delay (the exponent is capped long before f64 is).
        let doubled = shortest * 2f64.powi(attempt.clamp(1, 31).cast_signed() - 1);
        let backoff = doubled.min(self.backoff_cap.as_secs_f64());
        Duration::from_secs_f64(ramped.max(backoff).clamp(floor, ceiling))
    }

    /// [`Self::delay`] spread by ±20 % (`spread` in `0.0..=1.0`, normally random). When the
    /// server restarts every AFK client drops at the same moment; without the spread they would
    /// all reconnect in the same second, again and again.
    pub fn spread_delay(&self, relog: &AutoRelogOptions, attempt: u32, spread: f64) -> Duration {
        let spread = if spread.is_finite() {
            spread.clamp(0.0, 1.0)
        } else {
            0.5
        };
        self.delay(relog, attempt).mul_f64(0.8 + 0.4 * spread)
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
    /// The server turned the account away for a reason reconnecting does not fix (a ban, the
    /// whitelist, ...): the session stopped instead of trying again.
    Refused {
        cause: KickCause,
        message: Option<String>,
    },
}

/// Why the server keeps turning an account away, read from its kick message (English, Turkish,
/// and the ASCII spelling AuthMe's Turkish messages use).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum KickCause {
    Banned,
    NotWhitelisted,
    /// The login plugin refused the password the login commands sent.
    WrongPassword,
    /// The server only lets in Microsoft (premium) accounts.
    PremiumOnly,
    /// Someone joined with this name from elsewhere (the player's own game, another computer):
    /// reconnecting would kick them in turn.
    LoggedInElsewhere,
    /// The name is still connected. The server may hold a dropped session for a few seconds, so
    /// this stops the session only after [`REPEATED_KICKS`] in a row.
    AlreadyConnected,
    /// The login plugin's time ran out (no or wrong login commands). Can be a slow server once,
    /// so it stops the session only after [`REPEATED_KICKS`] in a row.
    LoginTimeout,
}

/// Kicks in a row with a cause that can also be passing before the session stops.
pub const REPEATED_KICKS: u32 = 5;

impl KickCause {
    fn kicks_to_stop(self) -> u32 {
        match self {
            Self::AlreadyConnected | Self::LoginTimeout => REPEATED_KICKS,
            _ => 1,
        }
    }
}

/// Messages about a passing stop (maintenance, a server restart) never name a cause, even when
/// they mention the whitelist.
const PASSING: &[&str] = &[
    "MAINTENANCE",
    "BAKIM",
    "SERVER IS RESTARTING",
    "SERVER RESTART",
    "RESTARTING SERVER",
    "YENIDEN BASLATILIYOR",
    // projects.gg's whitelist text while a server is being worked on.
    "GELISTIRME MODU",
];

/// Phrases of each cause, upper case with Turkish letters in ASCII (see [`plain_upper`]). Plain
/// words such as `banned` or `yasaklı` are left out on purpose: "kicked for a banned word" is not
/// a ban. Includes projects.gg's own AuthMe and LiteBans texts (checked against the server's
/// message files, October 2026).
const CAUSES: &[(KickCause, &[&str])] = &[
    (
        KickCause::Banned,
        &[
            "YOU ARE BANNED",
            "YOU'RE BANNED",
            "YOU HAVE BEEN BANNED",
            "YOU WERE BANNED",
            "BANNED FROM",
            "BANNED BY",
            "PERMANENTLY BANNED",
            "TEMPORARILY BANNED",
            "IP BANNED",
            "IP IS BANNED",
            "IP ADDRESS IS BANNED",
            "ACCOUNT IS BANNED",
            "YASAKLANDIN",
            "YASAKLANMISSIN",
            "BANLANDIN",
            "BANLANMISSIN",
            "UZAKLASTIRILDIN",
            "HESABINIZ KULLANIMA KAPATILMIS",
            "BOLGENIZ YASAKLI",
        ],
    ),
    (
        KickCause::NotWhitelisted,
        &["WHITELIST", "WHITE-LIST", "BEYAZ LISTE"],
    ),
    (
        KickCause::WrongPassword,
        &[
            "WRONG PASSWORD",
            "INCORRECT PASSWORD",
            "INVALID PASSWORD",
            "PASSWORD IS WRONG",
            "PASSWORD IS INCORRECT",
            "YANLIS SIFRE",
            "HATALI SIFRE",
            "SIFRE YANLIS",
            "SIFREN YANLIS",
            "SIFRENIZ YANLIS",
            "YANLIS PAROLA",
            "HATALI PAROLA",
            "PAROLA YANLIS",
        ],
    ),
    (
        KickCause::PremiumOnly,
        &[
            "FAILED TO VERIFY USERNAME",
            "INVALID SESSION",
            "NOT AUTHENTICATED WITH MINECRAFT",
        ],
    ),
    (
        KickCause::LoggedInElsewhere,
        &[
            "LOGGED IN FROM ANOTHER LOCATION",
            "BASKA BIR YERDEN GIRIS",
            "BASKA BIR KONUMDAN GIRIS",
        ],
    ),
    (
        KickCause::AlreadyConnected,
        &[
            "ALREADY CONNECTED",
            "ALREADY ONLINE",
            "ALREADY LOGGED IN",
            "ALREADY PLAYING",
            "ZATEN BAGLI",
            "ZATEN CEVRIMICI",
            "ZATEN OYUNDA",
            "ZATEN SUNUCUDA",
            "ZATEN BIR OYUNCU",
            "ADINIZDA BIR OYUNCU",
        ],
    ),
    (
        KickCause::LoginTimeout,
        &[
            "LOGIN TIMEOUT",
            "LOGIN TIMED OUT",
            "TOOK TOO LONG TO LOG",
            "GIRIS ZAMAN ASIMI",
            "GIRIS SURESI",
            "GIRIS YAPMAK ICIN SURE",
            "SIFRE GIRILMEDIGI ICIN",
        ],
    ),
];

/// Upper case with the Turkish letters spelled in ASCII, so `Yanlış şifre`, `YANLIŞ ŞİFRE` and
/// AuthMe's `Yanlis sifre` all read `YANLIS SIFRE`.
fn plain_upper(text: &str) -> String {
    text.chars()
        .flat_map(char::to_uppercase)
        .map(|c| match c {
            'İ' => 'I',
            'Ş' => 'S',
            'Ç' => 'C',
            'Ğ' => 'G',
            'Ö' => 'O',
            'Ü' => 'U',
            other => other,
        })
        .collect()
}

/// The cause behind a server's kick or login refusal, if its message names one. Connection losses
/// and other failures never have one (their messages come from the client, not the server).
pub fn kick_cause(reason: Option<&DisconnectReason>, message: Option<&str>) -> Option<KickCause> {
    if !matches!(
        reason,
        Some(DisconnectReason::InGameKick | DisconnectReason::LoginRejected)
    ) {
        return None;
    }
    let text = plain_upper(message?);
    if PASSING.iter().any(|phrase| text.contains(phrase)) {
        return None;
    }
    CAUSES
        .iter()
        .find(|(_, phrases)| phrases.iter().any(|phrase| text.contains(phrase)))
        .map(|(cause, _)| *cause)
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
            | Self::Failed { message }
            | Self::Refused { message, .. } => message.as_deref(),
            _ => None,
        }
    }
}

/// The last problem the running client reported through the bridge.
#[derive(Debug, Clone, PartialEq)]
pub struct Failure {
    pub reason: Option<DisconnectReason>,
    /// Verbatim (formatting codes removed), one line, length-limited.
    pub message: Option<String>,
    pub version_error: bool,
}

impl Failure {
    /// A report of the bridge's `disconnected` or `error` event (the message is cleaned up and
    /// length-limited; mask secrets before passing it in).
    pub fn new(
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
    pub fn merge(previous: Option<Failure>, next: Failure) -> Failure {
        match previous {
            Some(previous) if next.message.is_none() && previous.reason == next.reason => Failure {
                message: previous.message,
                version_error: previous.version_error || next.version_error,
                ..next
            },
            _ => next,
        }
    }

    /// The reason shown for this report.
    pub fn to_reason(&self) -> FailureReason {
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
pub enum ExitDecision {
    Stopped,
    Fail(FailureReason),
    /// Retry limit reached.
    GiveUp(FailureReason),
    Restart {
        attempt: u32,
        reason: FailureReason,
    },
}

/// The message contains one of the account's kick messages ("reconnect on these").
fn listed_kick_message(relog: &AutoRelogOptions, message: Option<&str>) -> bool {
    let message = message.unwrap_or_default();
    relog
        .kick_messages
        .iter()
        .any(|k| !k.is_empty() && contains_ignore_case(message, k))
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
            listed_kick_message(relog, message)
        }
        _ => true,
    }
}

/// Restart (or not) after a failure, honoring the retry limit. `attempts` counts restarts since the
/// last lasting connection; `repeats` how many failures right before this one had the same
/// [`KickCause`]. A kick with a cause stops the session (at once, or after [`REPEATED_KICKS`] in a
/// row for the causes that can be passing) unless it contains one of the account's own kick
/// messages; a version the client does not support stops it too.
pub fn decide_failure(
    relog: &AutoRelogOptions,
    reason: Option<&DisconnectReason>,
    display: FailureReason,
    attempts: u32,
    repeats: u32,
) -> ExitDecision {
    if let Some(cause) = kick_cause(reason, display.message())
        && !listed_kick_message(relog, display.message())
        && (!relog.enabled || repeats + 1 >= cause.kicks_to_stop())
    {
        return ExitDecision::Fail(FailureReason::Refused {
            cause,
            message: display.message().map(str::to_owned),
        });
    }
    if !relog.enabled
        || matches!(display, FailureReason::UnsupportedVersion { .. })
        || !kick_filter_allows(relog, reason, display.message())
    {
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

/// `OnProcessExited`, with the fixes described in the module docs. `failure` is what the bridge
/// reported before the exit ([`Failure::merge`] of its `disconnected` and `error` events).
pub fn decide_exit(
    relog: &AutoRelogOptions,
    stop_requested: bool,
    exit_code: Option<i32>,
    failure: Option<&Failure>,
    attempts: u32,
    repeats: u32,
) -> ExitDecision {
    if stop_requested {
        return ExitDecision::Stopped;
    }

    if !relog.enabled {
        // Legacy: errors stay visible, a clean exit without a reported problem is a plain stop.
        return match (failure, exit_code) {
            (Some(failure), _) => decide_failure(
                relog,
                failure.reason.as_ref(),
                failure.to_reason(),
                attempts,
                repeats,
            ),
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
    decide_failure(relog, reason.as_ref(), display, attempts, repeats)
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

    fn delays(min: f64, max: f64) -> AutoRelogOptions {
        AutoRelogOptions {
            delay_min: min,
            delay_max: max,
            ..AutoRelogOptions::default()
        }
    }

    fn secs(schedule: &RestartSchedule, relog: &AutoRelogOptions, attempts: u32) -> Vec<f64> {
        (1..=attempts)
            .map(|a| (schedule.delay(relog, a).as_secs_f64() * 100.0).round() / 100.0)
            .collect()
    }

    #[test]
    fn failures_in_a_row_back_off_towards_a_minute() {
        let schedule = RestartSchedule::default();
        // New accounts (5-10 s): the shortest delay doubles up to a minute.
        assert_eq!(
            secs(&schedule, &AutoRelogOptions::default(), 7),
            vec![5.0, 10.0, 20.0, 40.0, 60.0, 60.0, 60.0]
        );
        // The legacy range 15-60 s.
        assert_eq!(
            secs(&schedule, &delays(15.0, 60.0), 5),
            vec![15.0, 30.0, 60.0, 60.0, 60.0]
        );
        // The account's own longer delays still win.
        assert_eq!(
            secs(&schedule, &delays(120.0, 300.0), 5),
            vec![120.0, 180.0, 240.0, 300.0, 300.0]
        );
        assert_eq!(schedule.delay(&delays(15.0, 60.0), u32::MAX).as_secs(), 60);
        assert_eq!(schedule.delay(&delays(15.0, 60.0), 0).as_secs(), 15);
        // One value: the first restart waits it, failures in a row then back off.
        assert_eq!(
            secs(&schedule, &delays(30.0, 30.0), 3),
            vec![30.0, 60.0, 60.0]
        );
    }

    #[test]
    fn stored_delays_are_kept_in_sane_bounds() {
        let schedule = RestartSchedule::default();
        // Inverted, too short, too long and broken values.
        assert_eq!(
            secs(&schedule, &delays(60.0, 15.0), 4),
            secs(&schedule, &delays(15.0, 60.0), 4)
        );
        assert_eq!(secs(&schedule, &delays(0.0, 1.0), 2), vec![5.0, 10.0]);
        assert_eq!(schedule.delay(&delays(1e9, 1e12), 1).as_secs(), 3600);
        assert_eq!(
            secs(&schedule, &delays(f64::NAN, f64::INFINITY), 4),
            vec![5.0, 10.0, 20.0, 40.0],
            "the defaults stand in"
        );
        // Tests shrink the bounds; a ramp of 1 waits the longest delay right away.
        let fast = RestartSchedule {
            floor: Duration::from_millis(100),
            ceiling: Duration::from_millis(600),
            ramp: 1,
            ..RestartSchedule::default()
        };
        assert_eq!(
            fast.delay(&AutoRelogOptions::default(), 1),
            Duration::from_millis(600)
        );
        assert_eq!(
            fast.delay(&AutoRelogOptions::default(), 9),
            Duration::from_millis(600)
        );
    }

    #[test]
    fn spread_keeps_reconnects_apart_within_twenty_percent() {
        let schedule = RestartSchedule::default();
        let relog = delays(15.0, 60.0);
        assert_eq!(schedule.spread_delay(&relog, 4, 0.0).as_secs(), 48);
        assert_eq!(schedule.spread_delay(&relog, 4, 0.5).as_secs(), 60);
        assert_eq!(schedule.spread_delay(&relog, 4, 1.0).as_secs(), 72);
        // Out-of-range input is clamped rather than producing a zero or huge delay.
        assert_eq!(schedule.spread_delay(&relog, 1, -3.0).as_secs(), 12);
        assert_eq!(schedule.spread_delay(&relog, 1, 9.0).as_secs(), 18);
        assert_eq!(schedule.spread_delay(&relog, 1, f64::NAN).as_secs(), 15);
        // Even the shortest wait keeps the spread, so equal settings never line clients up.
        let quick = AutoRelogOptions::default();
        assert_eq!(
            schedule.spread_delay(&quick, 1, 0.0),
            Duration::from_secs(4)
        );
        assert_eq!(
            schedule.spread_delay(&quick, 1, 1.0),
            Duration::from_secs(6)
        );
    }

    #[test]
    fn retry_limit_counts_restarts_since_last_join() {
        let limited = relog(2, true);
        let kicked = Failure::new(Some(DisconnectReason::InGameKick), Some("bye"), false);
        assert_eq!(
            decide_exit(&limited, false, Some(2), Some(&kicked), 0, 0),
            ExitDecision::Restart {
                attempt: 1,
                reason: FailureReason::Kicked {
                    message: Some("bye".into())
                }
            }
        );
        assert!(matches!(
            decide_exit(&limited, false, Some(2), Some(&kicked), 1, 0),
            ExitDecision::Restart { attempt: 2, .. }
        ));
        assert!(matches!(
            decide_exit(&limited, false, Some(2), Some(&kicked), 2, 0),
            ExitDecision::GiveUp(FailureReason::Kicked { .. })
        ));
        let unlimited = relog(-1, true);
        assert!(matches!(
            decide_exit(&unlimited, false, Some(3), None, 10_000, 0),
            ExitDecision::Restart {
                attempt: 10_001,
                ..
            }
        ));
        assert!(matches!(
            decide_exit(&relog(0, true), false, Some(3), None, 0, 0),
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
            decide_exit(&filtered, false, Some(2), Some(&restart), 0, 0),
            ExitDecision::Restart { .. }
        ));
        let banned = Failure::new(Some(DisconnectReason::InGameKick), Some("Banned"), false);
        assert_eq!(
            decide_exit(&filtered, false, Some(2), Some(&banned), 0, 0),
            ExitDecision::Fail(FailureReason::Kicked {
                message: Some("Banned".into())
            })
        );
        // Exit code 2 alone (no message) does not match any kick message.
        assert!(matches!(
            decide_exit(&filtered, false, Some(2), None, 0, 0),
            ExitDecision::Fail(FailureReason::Kicked { message: None })
        ));
        // Connection losses and crashes always reconnect.
        assert!(matches!(
            decide_exit(&filtered, false, Some(3), None, 0, 0),
            ExitDecision::Restart { .. }
        ));
        assert!(matches!(
            decide_exit(&filtered, false, Some(-1073741819), None, 0, 0),
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
            decide_exit(&off, false, Some(0), None, 0, 0),
            ExitDecision::Stopped
        );
        assert_eq!(
            decide_exit(&off, false, Some(4), None, 0, 0),
            ExitDecision::Fail(FailureReason::LoginRejected { message: None })
        );
        let lost = Failure::new(Some(DisconnectReason::ConnectionLost), None, false);
        assert_eq!(
            decide_exit(&off, false, Some(0), Some(&lost), 0, 0),
            ExitDecision::Fail(FailureReason::ConnectionLost { message: None })
        );
        assert_eq!(
            decide_exit(&off, true, Some(2), Some(&lost), 0, 0),
            ExitDecision::Stopped
        );
        assert_eq!(
            decide_exit(&relog(-1, true), false, Some(0), None, 0, 0),
            ExitDecision::Restart {
                attempt: 1,
                reason: FailureReason::ConnectionLost { message: None }
            }
        );
    }

    fn kicked(message: &str) -> Failure {
        Failure::new(Some(DisconnectReason::InGameKick), Some(message), false)
    }

    fn refused(message: &str) -> Failure {
        Failure::new(Some(DisconnectReason::LoginRejected), Some(message), false)
    }

    fn cause_of(decision: &ExitDecision) -> Option<KickCause> {
        match decision {
            ExitDecision::Fail(FailureReason::Refused { cause, .. }) => Some(*cause),
            _ => None,
        }
    }

    #[test]
    fn kick_messages_name_the_causes_reconnecting_cannot_fix() {
        let cases = [
            ("§cYou are banned from this server!", KickCause::Banned),
            ("Banned by an operator.", KickCause::Banned),
            (
                "You are permanently banned. Reason: hile",
                KickCause::Banned,
            ),
            ("Sunucudan YASAKLANDIN! Sebep: küfür", KickCause::Banned),
            ("Banlandınız.", KickCause::Banned),
            (
                "You are not whitelisted on this server!",
                KickCause::NotWhitelisted,
            ),
            (
                "You are not white-listed on this server!",
                KickCause::NotWhitelisted,
            ),
            ("Beyaz listede değilsin.", KickCause::NotWhitelisted),
            ("&cWrong password!", KickCause::WrongPassword),
            ("Yanlış şifre!", KickCause::WrongPassword),
            ("YANLIŞ ŞİFRE!", KickCause::WrongPassword),
            ("Yanlis sifre!", KickCause::WrongPassword),
            ("Failed to verify username!", KickCause::PremiumOnly),
            (
                "Invalid session (Try restarting your game)",
                KickCause::PremiumOnly,
            ),
            (
                "You logged in from another location",
                KickCause::LoggedInElsewhere,
            ),
            (
                "Başka bir konumdan giriş yaptınız",
                KickCause::LoggedInElsewhere,
            ),
            (
                "You are already connected to this proxy!",
                KickCause::AlreadyConnected,
            ),
            (
                "Bu isimle zaten bir oyuncu sunucuda bulunmakta!",
                KickCause::AlreadyConnected,
            ),
            (
                "Login timeout exceeded, you have been kicked from the server",
                KickCause::LoginTimeout,
            ),
            (
                "Giris zaman asimi suresi asildi, sunucudan atildiniz",
                KickCause::LoginTimeout,
            ),
            // projects.gg's own texts (AuthMe messages_pro.yml, LiteBans messages.yml).
            ("§cHesabınız kullanıma kapatılmıştır!", KickCause::Banned),
            ("Bölgeniz yasaklı gözüküyor: TR", KickCause::Banned),
            (
                "Sizin adınızda bir oyuncu şu an oyunda ya da ağda bir takılma mevcut.",
                KickCause::AlreadyConnected,
            ),
            (
                "Uzun süredir şifre girilmediği için bağlantı kaybedildi.",
                KickCause::LoginTimeout,
            ),
            (
                "Birçok kez hatalı şifre girdiğiniz için sunucudan atıldınız.",
                KickCause::WrongPassword,
            ),
        ];
        for (message, cause) in cases {
            let failure = kicked(message);
            assert_eq!(
                kick_cause(failure.reason.as_ref(), failure.message.as_deref()),
                Some(cause),
                "{message}"
            );
        }
        // Passing stops, plain words and messages that do not come from the server.
        for message in [
            "Server is restarting",
            "Sunucu bakımda, beyaz liste açık. Birazdan dön!",
            "Kicked for using a banned word",
            "Yasaklı kelime kullandığın için atıldın",
            "Spam yüzünden atıldınız",
            "Server is full",
            "Sunucu geliştirme modunda olarak gözüküyor!",
        ] {
            let failure = kicked(message);
            assert_eq!(
                kick_cause(failure.reason.as_ref(), failure.message.as_deref()),
                None,
                "{message}"
            );
        }
        let lost = Failure::new(
            Some(DisconnectReason::ConnectionLost),
            Some("You are banned"),
            false,
        );
        assert_eq!(
            kick_cause(lost.reason.as_ref(), lost.message.as_deref()),
            None
        );
    }

    #[test]
    fn causes_stop_the_session_at_once_or_after_kicks_in_a_row() {
        let relog = relog(-1, true);
        let ban = kicked("You are banned from this server!");
        assert_eq!(
            decide_exit(&relog, false, Some(2), Some(&ban), 0, 0),
            ExitDecision::Fail(FailureReason::Refused {
                cause: KickCause::Banned,
                message: Some("You are banned from this server!".into()),
            })
        );
        // "Already connected" retries four times, then stops on the fifth in a row.
        let twin = refused("You are already connected to this proxy!");
        for repeats in 0..REPEATED_KICKS - 1 {
            assert!(
                matches!(
                    decide_exit(&relog, false, Some(4), Some(&twin), repeats, repeats),
                    ExitDecision::Restart { .. }
                ),
                "{repeats}"
            );
        }
        assert_eq!(
            cause_of(&decide_exit(
                &relog,
                false,
                Some(4),
                Some(&twin),
                4,
                REPEATED_KICKS - 1
            )),
            Some(KickCause::AlreadyConnected)
        );
        // The account's own kick messages say "reconnect on this".
        let listed = AutoRelogOptions {
            kick_messages: vec!["whitelist".into()],
            ..relog.clone()
        };
        let whitelist = kicked("You are not whitelisted on this server!");
        assert!(matches!(
            decide_exit(&listed, false, Some(2), Some(&whitelist), 0, 0),
            ExitDecision::Restart { .. }
        ));
        // Without automatic reconnects the error says why.
        let off = AutoRelogOptions {
            enabled: false,
            ..relog.clone()
        };
        assert_eq!(
            cause_of(&decide_exit(&off, false, Some(2), Some(&twin), 0, 0)),
            Some(KickCause::AlreadyConnected)
        );
        // A version the client does not support does not fix itself either.
        let version = Failure::new(None, Some("Unsupported protocol"), true);
        assert_eq!(
            decide_exit(&relog, false, Some(1), Some(&version), 0, 0),
            ExitDecision::Fail(FailureReason::UnsupportedVersion {
                message: Some("Unsupported protocol".into())
            })
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
