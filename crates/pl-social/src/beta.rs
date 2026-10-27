//! Beta access (BETA-GATE): whether this launcher may take the beta update channel.
//!
//! The friends service decides (`GET /v1/beta`, services/social README "Beta access"): while
//! Studio's switch is on, the channel is for the Ar-Ge team, by a role on the Projects Discord that
//! the service asks ProBot about; while it is off, for everyone. This module reads the service's
//! answer ([`BetaAnswer`]), keeps when beta was last let in ([`BetaRecordFile`]) and decides what
//! the launcher does ([`decide_join`], [`decide_keep`]): switch to beta, stay on it, or go back to
//! the stable channel. It never installs anything: a launcher that leaves beta keeps its version
//! until the stable channel passes it (the update decision never goes back).
//!
//! What it cannot ask (the service or ProBot away, nobody signed in) never moves a launcher that
//! was let in during the last [`BETA_GRACE_SECS`]; a definite "not on the team" moves it at once.

use std::io;
use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use url::Url;

/// Where a player who is not on the team applies, when the service names no usable page.
pub const BETA_APPLY_URL: &str = "https://projects.gg/basvuru";

/// How long a launcher stays on beta while it cannot ask, counted from the last answer that let it
/// in: a weekend's outage of the service or ProBot never moves anyone, and a player who lost the
/// role leaves at the first answer that says so.
pub const BETA_GRACE_SECS: i64 = 7 * 86_400;

/// A "let in" time further ahead than this is not believed (a clock set back is fine; a time the
/// launcher never wrote is not).
const FUTURE_SLACK_SECS: i64 = 300;

/// What the service says about the beta channel (`GET /v1/beta`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BetaAnswer {
    /// Studio's switch is off (or the service is from before it): beta is open to everyone.
    Open,
    /// The player's Discord account holds one of the team's roles.
    Member,
    /// It holds none of them, or is not on the Discord server.
    NotMember { apply: Url },
    /// The switch is on and nobody is signed in to the friends feature here.
    SignIn { apply: Url },
    /// Nobody can tell now: the service, ProBot or the network.
    Unknown { apply: Url },
}

impl BetaAnswer {
    /// Nobody can tell, and the service named no page.
    pub fn unknown() -> Self {
        Self::Unknown {
            apply: default_apply(),
        }
    }

    /// Nobody is signed in, and the service named no page.
    pub fn sign_in() -> Self {
        Self::SignIn {
            apply: default_apply(),
        }
    }

    fn apply(&self) -> Option<&Url> {
        match self {
            Self::Open | Self::Member => None,
            Self::NotMember { apply } | Self::SignIn { apply } | Self::Unknown { apply } => {
                Some(apply)
            }
        }
    }
}

fn default_apply() -> Url {
    Url::parse(BETA_APPLY_URL).expect("the application page is a valid URL")
}

/// The service's page, if it is an `https://` address without credentials; ours otherwise.
fn apply_of(raw: Option<&str>) -> Url {
    raw.and_then(|raw| Url::parse(raw.trim()).ok())
        .filter(|url| {
            url.scheme() == "https"
                && url.host_str().is_some()
                && url.username().is_empty()
                && url.password().is_none()
        })
        .unwrap_or_else(default_apply)
}

#[derive(Debug, Deserialize)]
struct BetaWire {
    gate: bool,
    #[serde(default)]
    access: Option<String>,
    #[serde(default)]
    apply: Option<String>,
}

/// `GET /v1/beta`'s answer (a `200`): an access this launcher does not know is "cannot tell".
pub(crate) fn answer_of(body: &[u8]) -> serde_json::Result<BetaAnswer> {
    let wire: BetaWire = serde_json::from_slice(body)?;
    if !wire.gate {
        return Ok(BetaAnswer::Open);
    }
    let apply = apply_of(wire.apply.as_deref());
    Ok(match wire.access.as_deref() {
        Some("member") => BetaAnswer::Member,
        Some("none") => BetaAnswer::NotMember { apply },
        Some("signin") => BetaAnswer::SignIn { apply },
        _ => BetaAnswer::Unknown { apply },
    })
}

/// What the launcher does about the beta channel, as the UI shows it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum BetaVerdict {
    /// Beta: switch to it, or stay on it.
    Allowed,
    /// Not on the team: "Ar-Ge'ye başvur".
    NotMember,
    /// Sign in with Discord first.
    SignIn,
    /// Cannot ask now: try again later.
    Unknown,
}

/// The verdict and the page "Ar-Ge'ye başvur" opens (`https://` only).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct BetaAccess {
    pub verdict: BetaVerdict,
    pub apply: String,
}

impl BetaAccess {
    fn of(verdict: BetaVerdict, answer: &BetaAnswer) -> Self {
        Self {
            verdict,
            apply: answer
                .apply()
                .map_or_else(|| BETA_APPLY_URL.to_owned(), |url| url.as_str().to_owned()),
        }
    }

    pub fn allowed(&self) -> bool {
        self.verdict == BetaVerdict::Allowed
    }
}

/// The player picked "Beta": only a yes (or the switch off) lets this launcher in. Returns the
/// access and the "let in" time to keep (`Some(now)` when let in, else nothing changes).
pub fn decide_join(answer: &BetaAnswer, now: i64) -> (BetaAccess, Option<i64>) {
    match answer {
        BetaAnswer::Open | BetaAnswer::Member => {
            (BetaAccess::of(BetaVerdict::Allowed, answer), Some(now))
        }
        BetaAnswer::NotMember { .. } => (BetaAccess::of(BetaVerdict::NotMember, answer), None),
        BetaAnswer::SignIn { .. } => (BetaAccess::of(BetaVerdict::SignIn, answer), None),
        BetaAnswer::Unknown { .. } => (BetaAccess::of(BetaVerdict::Unknown, answer), None),
    }
}

/// On the beta channel, before an update look. `let_in`: when beta last let this launcher in
/// (from [`BetaRecordFile`]). Returns the access (`Allowed`: stay; else go back to stable, and
/// why) and the "let in" time to keep from now on (`None`: forget it).
pub fn decide_keep(
    answer: &BetaAnswer,
    let_in: Option<i64>,
    now: i64,
) -> (BetaAccess, Option<i64>) {
    match answer {
        BetaAnswer::Open | BetaAnswer::Member => {
            (BetaAccess::of(BetaVerdict::Allowed, answer), Some(now))
        }
        BetaAnswer::NotMember { .. } => (BetaAccess::of(BetaVerdict::NotMember, answer), None),
        BetaAnswer::SignIn { .. } | BetaAnswer::Unknown { .. } => {
            let believed = let_in.filter(|at| *at <= now + FUTURE_SLACK_SECS);
            if believed.is_some_and(|at| now - at <= BETA_GRACE_SECS) {
                (BetaAccess::of(BetaVerdict::Allowed, answer), believed)
            } else {
                let verdict = if matches!(answer, BetaAnswer::SignIn { .. }) {
                    BetaVerdict::SignIn
                } else {
                    BetaVerdict::Unknown
                };
                (BetaAccess::of(verdict, answer), None)
            }
        }
    }
}

/// The file that keeps when beta last let this launcher in (Unix seconds), so an outage within
/// [`BETA_GRACE_SECS`] never moves it. A missing or unreadable file is "never".
#[derive(Debug, Clone)]
pub struct BetaRecordFile {
    path: PathBuf,
}

/// The record's file name in the launcher's data folder.
pub const BETA_RECORD_FILE: &str = "beta-access.json";

#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Record {
    v: u32,
    let_in: i64,
}

impl BetaRecordFile {
    pub fn new(path: impl Into<PathBuf>) -> Self {
        Self { path: path.into() }
    }

    pub fn load(&self) -> Option<i64> {
        let bytes = std::fs::read(&self.path).ok()?;
        let record: Record = serde_json::from_slice(&bytes).ok()?;
        (record.v == 1 && record.let_in > 0).then_some(record.let_in)
    }

    /// Keeps `let_in`, or removes the file for `None`. Writes through a temporary file.
    pub fn save(&self, let_in: Option<i64>) -> io::Result<()> {
        let Some(let_in) = let_in else {
            return match std::fs::remove_file(&self.path) {
                Err(error) if error.kind() != io::ErrorKind::NotFound => Err(error),
                _ => Ok(()),
            };
        };
        if let Some(dir) = self.path.parent() {
            std::fs::create_dir_all(dir)?;
        }
        let temporary = self.path.with_extension("json.tmp");
        std::fs::write(&temporary, serde_json::to_vec(&Record { v: 1, let_in })?)?;
        std::fs::rename(&temporary, &self.path)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const NOW: i64 = 1_792_521_000;
    const DAY: i64 = 86_400;

    fn apply(text: &str) -> Url {
        Url::parse(text).unwrap()
    }

    #[test]
    fn reads_every_answer_of_the_service() {
        assert_eq!(answer_of(br#"{"gate":false}"#).unwrap(), BetaAnswer::Open);
        assert_eq!(
            answer_of(br#"{"gate":true,"access":"member","apply":"https://projects.gg/basvuru"}"#)
                .unwrap(),
            BetaAnswer::Member
        );
        assert_eq!(
            answer_of(br#"{"gate":true,"access":"none","apply":"https://projects.gg/arge"}"#)
                .unwrap(),
            BetaAnswer::NotMember {
                apply: apply("https://projects.gg/arge")
            }
        );
        assert_eq!(
            answer_of(br#"{"gate":true,"access":"signin","apply":"https://projects.gg/basvuru"}"#)
                .unwrap(),
            BetaAnswer::sign_in()
        );
        assert_eq!(
            answer_of(br#"{"gate":true,"access":"unknown","retryAfter":60}"#).unwrap(),
            BetaAnswer::unknown()
        );
        // An access from a newer service is "cannot tell", never a yes.
        assert_eq!(
            answer_of(br#"{"gate":true,"access":"later"}"#).unwrap(),
            BetaAnswer::unknown()
        );
        assert!(answer_of(b"not json").is_err());
        assert!(answer_of(br#"{"access":"member"}"#).is_err());
    }

    #[test]
    fn opens_only_an_https_page_without_credentials() {
        for odd in [
            "http://projects.gg/basvuru",
            "javascript:alert(1)",
            "https://user:pw@projects.gg/",
            "projects.gg/basvuru",
            "",
        ] {
            let body = format!(r#"{{"gate":true,"access":"none","apply":{}}}"#, json(odd));
            assert_eq!(
                answer_of(body.as_bytes()).unwrap(),
                BetaAnswer::NotMember {
                    apply: apply(BETA_APPLY_URL)
                },
                "{odd}"
            );
        }
    }

    fn json(text: &str) -> String {
        serde_json::to_string(text).unwrap()
    }

    #[test]
    fn joins_only_on_a_yes() {
        for (answer, verdict, kept) in [
            (BetaAnswer::Open, BetaVerdict::Allowed, Some(NOW)),
            (BetaAnswer::Member, BetaVerdict::Allowed, Some(NOW)),
            (
                BetaAnswer::NotMember {
                    apply: apply("https://projects.gg/arge"),
                },
                BetaVerdict::NotMember,
                None,
            ),
            (BetaAnswer::sign_in(), BetaVerdict::SignIn, None),
            (BetaAnswer::unknown(), BetaVerdict::Unknown, None),
        ] {
            let (access, let_in) = decide_join(&answer, NOW);
            assert_eq!(access.verdict, verdict, "{answer:?}");
            assert_eq!(let_in, kept, "{answer:?}");
        }
        let (refused, _) = decide_join(
            &BetaAnswer::NotMember {
                apply: apply("https://projects.gg/arge"),
            },
            NOW,
        );
        assert_eq!(refused.apply, "https://projects.gg/arge");
        assert!(!refused.allowed());
        assert_eq!(
            decide_join(&BetaAnswer::Member, NOW).0.apply,
            BETA_APPLY_URL
        );
    }

    #[test]
    fn a_member_stays_and_is_let_in_again() {
        let (access, let_in) = decide_keep(&BetaAnswer::Member, Some(NOW - 3 * DAY), NOW);
        assert!(access.allowed());
        assert_eq!(let_in, Some(NOW));
        // The switch off: everyone stays.
        let (open, _) = decide_keep(&BetaAnswer::Open, None, NOW);
        assert!(open.allowed());
    }

    #[test]
    fn not_on_the_team_leaves_at_once_whatever_was_kept() {
        let (access, let_in) = decide_keep(
            &BetaAnswer::NotMember {
                apply: apply(BETA_APPLY_URL),
            },
            Some(NOW - 60),
            NOW,
        );
        assert_eq!(access.verdict, BetaVerdict::NotMember);
        assert_eq!(let_in, None);
    }

    #[test]
    fn cannot_ask_keeps_beta_for_a_week_after_the_last_yes() {
        for answer in [BetaAnswer::unknown(), BetaAnswer::sign_in()] {
            // Let in six days ago: stays, and the time stays as it was (the grace does not grow).
            let (stays, kept) = decide_keep(&answer, Some(NOW - 6 * DAY), NOW);
            assert!(stays.allowed(), "{answer:?}");
            assert_eq!(kept, Some(NOW - 6 * DAY));
            // Exactly a week: still.
            assert!(
                decide_keep(&answer, Some(NOW - BETA_GRACE_SECS), NOW)
                    .0
                    .allowed()
            );
            // A week and a second: leaves, and says why.
            let (leaves, forgot) = decide_keep(&answer, Some(NOW - BETA_GRACE_SECS - 1), NOW);
            let why = if matches!(answer, BetaAnswer::SignIn { .. }) {
                BetaVerdict::SignIn
            } else {
                BetaVerdict::Unknown
            };
            assert_eq!(leaves.verdict, why);
            assert_eq!(forgot, None);
            // Never let in (a channel set by hand or brought by a settings backup): leaves.
            assert_eq!(decide_keep(&answer, None, NOW).0.verdict, why);
            // A time far ahead is not believed; a few minutes ahead (clocks) is.
            assert_eq!(decide_keep(&answer, Some(NOW + DAY), NOW).0.verdict, why);
            assert!(decide_keep(&answer, Some(NOW + 120), NOW).0.allowed());
        }
    }

    #[test]
    fn the_record_survives_a_restart_and_goes_with_none() {
        let dir = tempfile::tempdir().unwrap();
        let file = BetaRecordFile::new(dir.path().join("sub").join(BETA_RECORD_FILE));
        assert_eq!(file.load(), None);
        file.save(Some(NOW)).unwrap();
        assert_eq!(file.load(), Some(NOW));
        assert_eq!(
            BetaRecordFile::new(dir.path().join("sub").join(BETA_RECORD_FILE)).load(),
            Some(NOW)
        );
        file.save(None).unwrap();
        assert_eq!(file.load(), None);
        // Removing what is not there is fine; a broken file is "never".
        file.save(None).unwrap();
        std::fs::write(dir.path().join("sub").join(BETA_RECORD_FILE), b"{").unwrap();
        assert_eq!(file.load(), None);
        std::fs::write(
            dir.path().join("sub").join(BETA_RECORD_FILE),
            br#"{"v":2,"letIn":5}"#,
        )
        .unwrap();
        assert_eq!(file.load(), None);
    }
}
