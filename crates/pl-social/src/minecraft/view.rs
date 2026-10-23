//! What SKIN1's interface sees (`apps/launcher/src/lib/api/projects-skin-types.ts`, field for
//! field) and the pure rules that make it: a service answer as the panel shows it, every refusal
//! as `skin1-l-r.md` maps it, what an `mc` message means to the player.

use base64::Engine as _;
use base64::engine::general_purpose::STANDARD;
use pl_auth::SkinVariant;
use serde::{Deserialize, Serialize};

use super::wire::{
    FEATURE_SKINS, McFailure, McJob, McJobKind, McJobState, McLink, McLinkState, McMessage, McSkin,
    McSkinBy, McSkinKind, McState,
};
use crate::view::SkinBlockView;

/// A PNG as a `data:` address for the preview.
pub(crate) fn data_url(png: &[u8]) -> String {
    format!("data:image/png;base64,{}", STANDARD.encode(png))
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectsLinkState {
    Linked,
    None,
    Unknown,
}

/// The Discord account's link to a game account on Projects.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsLink {
    pub state: ProjectsLinkState,
    /// The linked game account (the last known one while `unknown`; null for `none`).
    pub name: Option<String>,
    pub at: Option<i64>,
    /// When the service last learned it from ProBot; null: never.
    pub checked_at: Option<i64>,
}

pub(crate) fn link_view(link: &McLink) -> ProjectsLink {
    ProjectsLink {
        state: match link.state {
            McLinkState::Linked => ProjectsLinkState::Linked,
            McLinkState::None => ProjectsLinkState::None,
            McLinkState::Unknown => ProjectsLinkState::Unknown,
        },
        name: if link.state == McLinkState::None {
            None
        } else {
            link.name.clone()
        },
        at: link.at,
        checked_at: link.checked_at,
    }
}

/// The service's switches as its last answer listed them.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkinService {
    /// `mcLink`: linking from the launcher.
    pub link: bool,
    /// `skins`: Projects skins.
    pub skins: bool,
}

/// What the launcher offers at all. Answered from memory, never waiting for the network.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkinOverview {
    /// The signed content has `features.friends` and `features.skins`: the entry points show.
    pub shown: bool,
    /// Null while unknown (signed out, or nothing heard yet).
    pub service: Option<ProjectsSkinService>,
    /// Null while signed out or never learned.
    pub link: Option<ProjectsLink>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectsSkinKind {
    Png,
    Premium,
    Default,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectsSkinBy {
    Launcher,
    Staff,
}

/// The Projects skin as last changed from a launcher (or reset by staff).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkin {
    pub kind: ProjectsSkinKind,
    pub variant: SkinVariant,
    /// `premium`: the premium player's name as Mojang writes it.
    pub premium: Option<String>,
    pub at: i64,
    pub by: ProjectsSkinBy,
    /// 64×64 PNG data URL; null for `default` and when neither a copy nor Mojang has it.
    pub picture: Option<String>,
}

pub(crate) fn skin_view(skin: &McSkin, picture: Option<String>) -> ProjectsSkin {
    ProjectsSkin {
        kind: match skin.kind {
            McSkinKind::Png => ProjectsSkinKind::Png,
            McSkinKind::Premium => ProjectsSkinKind::Premium,
            McSkinKind::Default => ProjectsSkinKind::Default,
        },
        variant: skin.variant,
        premium: skin.premium.clone(),
        at: skin.at,
        by: match skin.by {
            McSkinBy::Launcher => ProjectsSkinBy::Launcher,
            McSkinBy::Staff => ProjectsSkinBy::Staff,
        },
        picture: picture.filter(|_| skin.kind != McSkinKind::Default),
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectsSkinJobKind {
    Link,
    Unlink,
    Check,
    Skin,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectsSkinJobState {
    Pending,
    Working,
    Queued,
    Done,
    Failed,
    Cancelled,
}

/// How a job ended: the service's codes in camelCase; one this version does not know is `error`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectsSkinResultCode {
    Linked,
    Unlinked,
    Applied,
    NotLinked,
    OtherAccount,
    MinecraftTaken,
    PinInvalid,
    PinExpired,
    Paused,
    SkinBlocked,
    PremiumInvalid,
    PremiumDisabled,
    InvalidImage,
    MineskinBusy,
    Superseded,
    Timeout,
    Cancelled,
    Error,
}

pub(crate) fn result_code(code: &str) -> ProjectsSkinResultCode {
    use ProjectsSkinResultCode as C;
    match code {
        "linked" => C::Linked,
        "unlinked" => C::Unlinked,
        "applied" => C::Applied,
        "not_linked" => C::NotLinked,
        "other_account" => C::OtherAccount,
        "minecraft_taken" => C::MinecraftTaken,
        "pin_invalid" => C::PinInvalid,
        "pin_expired" => C::PinExpired,
        "paused" => C::Paused,
        "skin_blocked" => C::SkinBlocked,
        "premium_invalid" => C::PremiumInvalid,
        "premium_disabled" => C::PremiumDisabled,
        "invalid_image" => C::InvalidImage,
        "mineskin_busy" => C::MineskinBusy,
        "superseded" => C::Superseded,
        "timeout" => C::Timeout,
        "cancelled" => C::Cancelled,
        _ => C::Error,
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkinResult {
    pub code: ProjectsSkinResultCode,
    /// `otherAccount`: the game account the Discord account is linked to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `skinBlocked`: until when (null: for good).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<Option<i64>>,
}

/// One job of the player's.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkinJob {
    /// Never shown.
    pub id: String,
    pub kind: ProjectsSkinJobKind,
    pub state: ProjectsSkinJobState,
    /// The game account it is for (`link` and `skin`).
    pub name: Option<String>,
    /// `skin`: what it saves.
    pub source: Option<ProjectsSkinKind>,
    /// Once closed.
    pub result: Option<ProjectsSkinResult>,
    /// `skin` sent from this computer: the skin it saves (64×64 PNG data URL; null for
    /// `default`).
    pub picture: Option<String>,
    /// `skin` sent from this computer: its arm model.
    pub variant: Option<SkinVariant>,
    pub created_at_ms: i64,
    pub updated_at_ms: i64,
}

/// The job as the interface sees it; None for a kind this version does not know.
pub(crate) fn job_view(
    job: &McJob,
    picture: Option<String>,
    variant: Option<SkinVariant>,
) -> Option<ProjectsSkinJob> {
    let kind = match job.kind {
        McJobKind::Link => ProjectsSkinJobKind::Link,
        McJobKind::Unlink => ProjectsSkinJobKind::Unlink,
        McJobKind::Check => ProjectsSkinJobKind::Check,
        McJobKind::Skin => ProjectsSkinJobKind::Skin,
        McJobKind::Other => return None,
    };
    let closed = !job.state.is_open();
    Some(ProjectsSkinJob {
        id: job.id.clone(),
        kind,
        state: match job.state {
            McJobState::Pending => ProjectsSkinJobState::Pending,
            McJobState::Working => ProjectsSkinJobState::Working,
            McJobState::Queued => ProjectsSkinJobState::Queued,
            McJobState::Done => ProjectsSkinJobState::Done,
            McJobState::Failed => ProjectsSkinJobState::Failed,
            McJobState::Cancelled => ProjectsSkinJobState::Cancelled,
        },
        name: job.name.clone(),
        source: match job.source.as_deref() {
            Some("png") => Some(ProjectsSkinKind::Png),
            Some("premium") => Some(ProjectsSkinKind::Premium),
            Some("default") => Some(ProjectsSkinKind::Default),
            _ => None,
        },
        result: closed.then(|| {
            let code = result_code(job.code());
            let result = job.result.as_ref();
            ProjectsSkinResult {
                code,
                name: result
                    .and_then(|result| result.name.clone())
                    .filter(|_| code == ProjectsSkinResultCode::OtherAccount),
                until: (code == ProjectsSkinResultCode::SkinBlocked)
                    .then(|| result.and_then(|result| result.until).unwrap_or(None)),
            }
        }),
        picture: if job.kind == McJobKind::Skin {
            picture
        } else {
            None
        },
        variant: if job.kind == McJobKind::Skin {
            variant.or_else(|| job.result.as_ref().and_then(|result| result.variant))
        } else {
            None
        },
        created_at_ms: job.created_at,
        updated_at_ms: job.updated_at,
    })
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkinLimits {
    /// Skin changes a day (Studio, 1–10).
    pub per_day: u32,
    /// Changes left today (Turkey days; only `applied` counts).
    pub left: u32,
    /// When today's changes come back.
    pub reset_at: i64,
    /// Link tries left (the lower of this hour's and today's).
    pub link_tries_left: u32,
    /// After the service turned a link try away as too many: when tries open again.
    pub link_retry_at: Option<i64>,
}

/// Why the panel can do nothing with the service right now.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectsSkinAccess {
    /// Not signed in to friends (Discord).
    SignedOut,
    /// The service's skin switch is off (or it has no SKIN1).
    Off,
    /// The service did not answer, or ProBot is away.
    Unreachable,
    /// The Discord session ended.
    Session,
    Ready,
}

/// The access an answer of `GET /v1/mc` gives.
pub(crate) fn access_of_state(state: &McState) -> ProjectsSkinAccess {
    if !state.offers(FEATURE_SKINS) {
        ProjectsSkinAccess::Off
    } else if !state.health.bot {
        ProjectsSkinAccess::Unreachable
    } else {
        ProjectsSkinAccess::Ready
    }
}

/// The access a failed `GET /v1/mc` leaves.
pub(crate) fn access_of(failure: &McFailure) -> ProjectsSkinAccess {
    match (failure.status, failure.code.as_str()) {
        // Suspended or banned accounts lose their sessions too.
        (401, _) | (403, "account_restricted") => ProjectsSkinAccess::Session,
        // Every SKIN1 switch is off, or the service is older than SKIN1.
        (404, _) => ProjectsSkinAccess::Off,
        (503, _) if failure.reason.as_deref() == Some("off") => ProjectsSkinAccess::Off,
        _ => ProjectsSkinAccess::Unreachable,
    }
}

/// A new Discord account may not change skins until then.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkinLimited {
    pub until: i64,
}

/// The Görünüm panel's view of one username account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkinView {
    pub account_id: String,
    /// The game account's name.
    pub name: String,
    pub access: ProjectsSkinAccess,
    /// Lobi did not take ProBot's queue lately: a saved skin waits in line meanwhile.
    pub server_away: bool,
    pub link: ProjectsLink,
    /// Null: no Projects skin set from a launcher for this account (or linked to another).
    pub skin: Option<ProjectsSkin>,
    /// This account's skin job still open.
    pub job: Option<ProjectsSkinJob>,
    /// This account's skin job that closed last (the last day).
    pub last_job: Option<ProjectsSkinJob>,
    /// The link family's open job.
    pub link_job: Option<ProjectsSkinJob>,
    pub limits: ProjectsSkinLimits,
    /// Staff keep the player from changing skins (`until` null: for good).
    pub blocked: Option<SkinBlockView>,
    /// A new Discord account may not change skins until then.
    pub limited: Option<ProjectsSkinLimited>,
    /// "Geri al" can put back the skin before the last change (kept on this computer).
    pub can_undo: bool,
    /// The game this launcher started is on projects.gg right now.
    pub on_projects: bool,
}

/// What an account looks like on Projects without a Projects skin.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkinLook {
    pub picture: String,
    pub variant: SkinVariant,
    /// A premium account of the same name lends its skin (else Minecraft's default skin).
    pub premium: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectsSkinChoiceKind {
    Png,
    Premium,
}

/// A skin the player picked, checked and ready to save.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkinChoice {
    /// Handed back to `save`; lives in the backend for this run. Never shown.
    pub id: String,
    pub kind: ProjectsSkinChoiceKind,
    /// 64×64 PNG data URL (a 64×32 file is converted already), the base layer opaque.
    pub picture: String,
    /// `png`: the file's name; `premium`: the player's name as Mojang writes it.
    pub name: String,
    /// `premium`: from the texture. `png`: classic (the interface guesses from the picture).
    pub variant: SkinVariant,
    /// `png`: an old 64×32 skin (no slim arms).
    pub legacy: bool,
    /// `png`: more than 5% of the base layer is see-through.
    pub black_parts: bool,
    /// `premium`: the account's own name.
    pub own: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectsSkinPickRefusal {
    NotPng,
    Dimensions,
    TooBig,
    Empty,
    Unreadable,
}

/// A skin file as a skin, or why not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ProjectsSkinPick {
    Ready { choice: ProjectsSkinChoice },
    Refused { reason: ProjectsSkinPickRefusal },
}

pub(crate) fn pick_refusal(problem: pl_auth::SkinPngProblem) -> ProjectsSkinPickRefusal {
    use pl_auth::SkinPngProblem as P;
    match problem {
        P::NotPng => ProjectsSkinPickRefusal::NotPng,
        P::Dimensions { .. } => ProjectsSkinPickRefusal::Dimensions,
        P::TooBig => ProjectsSkinPickRefusal::TooBig,
        P::Empty => ProjectsSkinPickRefusal::Empty,
        P::Unreadable(_) => ProjectsSkinPickRefusal::Unreadable,
    }
}

/// A premium player's skin, or why not.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ProjectsSkinLookup {
    Found {
        choice: ProjectsSkinChoice,
    },
    /// "Bu adda bir Minecraft hesabı yok."
    NoAccount,
    /// The player wears Minecraft's default skin: nothing to keep.
    NoSkin,
    /// Mojang did not answer (asked again after 10 minutes at the earliest).
    Unreachable,
}

/// What `save` sends.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProjectsSkinSave {
    /// A picked skin; `variant` counts for `png` only (a premium skin keeps its own).
    Choice { id: String, variant: SkinVariant },
    /// "Varsayılana dön".
    Default,
    /// "Geri al": the skin before the last change.
    Undo,
}

/// Why the service (or this launcher) turned a request away before any job.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum ProjectsSkinRefusalCode {
    Off,
    Unreachable,
    Session,
    Limited,
    Blocked,
    Restricted,
    Pending,
    NotLinked,
    OtherAccount,
    AlreadyLinked,
    Quota,
    Tries,
    InvalidPin,
    InvalidSkin,
    ChoiceGone,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProjectsSkinRefusal {
    pub code: ProjectsSkinRefusalCode,
    /// `limited`, `blocked`: until when (null: for good).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub until: Option<Option<i64>>,
    /// `otherAccount`, `alreadyLinked`: the linked game account.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// `tries`, `quota`, `unreachable`: seconds until trying again may work.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub retry_after: Option<u64>,
}

impl ProjectsSkinRefusal {
    pub(crate) fn of(code: ProjectsSkinRefusalCode) -> Self {
        Self {
            code,
            until: None,
            name: None,
            retry_after: None,
        }
    }
}

/// A failed request as the player is told (`skin1-l-r.md`, "HTTP → refusal.code").
pub(crate) fn refusal_of(failure: &McFailure) -> ProjectsSkinRefusal {
    use ProjectsSkinRefusalCode as C;
    let item = failure.item.as_deref();
    let mut refusal = ProjectsSkinRefusal::of(match (failure.status, failure.code.as_str()) {
        (401, _) => C::Session,
        (503, _) if failure.reason.as_deref() == Some("off") => C::Off,
        // Every SKIN1 switch is off, or the service is older than SKIN1.
        (404, "not_found") => C::Off,
        (403, "account_limited") => C::Limited,
        (403, "skin_blocked") => C::Blocked,
        (403, "account_restricted") => C::Restricted,
        (409, "job_pending") => C::Pending,
        (409 | 404, "not_linked") => C::NotLinked,
        (409, "other_account") => C::OtherAccount,
        (409, "already_linked") => C::AlreadyLinked,
        (429, _) if item == Some("skin_changes") => C::Quota,
        (429, _) if item == Some("link_tries") => C::Tries,
        (400, "invalid_pin") => C::InvalidPin,
        (400, "invalid_skin") | (413, _) => C::InvalidSkin,
        // ProBot away, a flood limit, storage, a network failure, anything unreadable.
        _ => C::Unreachable,
    });
    match refusal.code {
        C::Limited => refusal.until = failure.until.flatten().map(Some),
        // Always said: null is "for good".
        C::Blocked => refusal.until = Some(failure.until.flatten()),
        C::OtherAccount | C::AlreadyLinked => refusal.name = failure.name.clone(),
        C::Quota | C::Tries | C::Unreachable => refusal.retry_after = failure.retry_after,
        _ => {}
    }
    refusal
}

/// `save`, `link`, `unlink`: a job on its way, or a refusal.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum ProjectsSkinOutcome {
    Sent { job: Box<ProjectsSkinJob> },
    Refused { refusal: ProjectsSkinRefusal },
}

impl ProjectsSkinOutcome {
    pub(crate) fn refused(code: ProjectsSkinRefusalCode) -> Self {
        Self::Refused {
            refusal: ProjectsSkinRefusal::of(code),
        }
    }
}

/// `projects-skin-changed`: something the panel, the link row or a toast cares about.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum ProjectsSkinChange {
    /// A job of the player's closed.
    #[serde(rename_all = "camelCase")]
    Job {
        job: Box<ProjectsSkinJob>,
        /// The launcher account the job was for (by name), if it is still here.
        account_id: Option<String>,
        /// "Geri al" can follow an applied skin: the one before is kept and a change is left.
        undo: bool,
        /// The game this launcher started is on projects.gg right now.
        on_projects: bool,
    },
    /// The Discord account was linked outside a code typed here (in game, codeless, Discord).
    Linked { name: String },
    /// Anything else (a staff reset, a link removed elsewhere, a refresh): read the state again.
    State,
}

/// What an `mc` message means to the player.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) enum McNews {
    /// A job closed.
    Job(McJob),
    /// Linked (to this name) outside a job: in game, codeless or on Discord.
    Linked(String),
    State,
}

/// `before`: the link this launcher knew (None: nothing known in this session, so a link that
/// came with a staff reset cannot be told from a new one: only "read again").
pub(crate) fn news_of(before: Option<&McLink>, message: &McMessage) -> McNews {
    if let Some(job) = &message.job {
        return McNews::Job(job.clone());
    }
    let link = &message.link;
    if link.state == McLinkState::Linked
        && let Some(name) = &link.name
        && let Some(before) = before
        && !before.linked_to(name)
    {
        return McNews::Linked(name.clone());
    }
    McNews::State
}

#[cfg(test)]
mod tests {
    use super::*;

    fn failure(status: u16, code: &str) -> McFailure {
        McFailure {
            status,
            code: code.to_owned(),
            ..McFailure::default()
        }
    }

    #[test]
    fn every_refusal_maps_as_the_interface_expects() {
        use ProjectsSkinRefusalCode as C;
        let cases = [
            (
                McFailure {
                    reason: Some("off".into()),
                    retry_after: Some(3600),
                    ..failure(503, "unavailable")
                },
                C::Off,
            ),
            (
                McFailure {
                    reason: Some("bot".into()),
                    ..failure(503, "unavailable")
                },
                C::Unreachable,
            ),
            (McFailure::network(), C::Unreachable),
            (failure(401, "unauthorized"), C::Session),
            (McFailure::signed_out(), C::Session),
            (failure(403, "account_restricted"), C::Restricted),
            (failure(409, "job_pending"), C::Pending),
            (failure(409, "not_linked"), C::NotLinked),
            (failure(404, "not_linked"), C::NotLinked),
            (failure(404, "not_found"), C::Off),
            (failure(400, "invalid_pin"), C::InvalidPin),
            (failure(400, "invalid_skin"), C::InvalidSkin),
            (failure(413, "payload_too_large"), C::InvalidSkin),
            (failure(500, "internal"), C::Unreachable),
            (failure(0, "protocol"), C::Unreachable),
            (failure(429, "rate_limited"), C::Unreachable),
        ];
        for (failure, code) in cases {
            assert_eq!(refusal_of(&failure).code, code, "{failure:?}");
        }
    }

    #[test]
    fn refusals_carry_their_details() {
        use ProjectsSkinRefusalCode as C;
        let limited = refusal_of(&McFailure {
            until: Some(Some(1_800_000_000)),
            item: Some("skin_change".into()),
            ..failure(403, "account_limited")
        });
        assert_eq!(
            (limited.code, limited.until),
            (C::Limited, Some(Some(1_800_000_000)))
        );
        let forever = refusal_of(&McFailure {
            until: Some(None),
            ..failure(403, "skin_blocked")
        });
        assert_eq!((forever.code, forever.until), (C::Blocked, Some(None)));
        assert_eq!(
            serde_json::to_value(&forever).unwrap(),
            serde_json::json!({"code": "blocked", "until": null})
        );
        let other = refusal_of(&McFailure {
            name: Some("Mehmet".into()),
            ..failure(409, "other_account")
        });
        assert_eq!(other.name.as_deref(), Some("Mehmet"));
        let already = refusal_of(&McFailure {
            name: Some("Ahmet".into()),
            ..failure(409, "already_linked")
        });
        assert_eq!(
            (already.code, already.name.as_deref()),
            (C::AlreadyLinked, Some("Ahmet"))
        );
        let quota = refusal_of(&McFailure {
            item: Some("skin_changes".into()),
            retry_after: Some(7200),
            ..failure(429, "rate_limited")
        });
        assert_eq!(
            serde_json::to_value(&quota).unwrap(),
            serde_json::json!({"code": "quota", "retryAfter": 7200})
        );
        let tries = refusal_of(&McFailure {
            item: Some("link_tries".into()),
            retry_after: Some(1800),
            ..failure(429, "rate_limited")
        });
        assert_eq!((tries.code, tries.retry_after), (C::Tries, Some(1800)));
        // Nothing beside the code where none belongs.
        assert_eq!(
            serde_json::to_value(refusal_of(&failure(409, "job_pending"))).unwrap(),
            serde_json::json!({"code": "pending"})
        );
    }

    #[test]
    fn the_panels_access() {
        assert_eq!(
            access_of(&failure(401, "unauthorized")),
            ProjectsSkinAccess::Session
        );
        assert_eq!(
            access_of(&failure(404, "not_found")),
            ProjectsSkinAccess::Off
        );
        assert_eq!(
            access_of(&McFailure::network()),
            ProjectsSkinAccess::Unreachable
        );
        let mut state = McState {
            features: vec!["mcLink".into()],
            ..McState::default()
        };
        assert_eq!(access_of_state(&state), ProjectsSkinAccess::Off);
        state.features.push("skins".into());
        assert_eq!(access_of_state(&state), ProjectsSkinAccess::Ready);
        state.health.bot = false;
        assert_eq!(access_of_state(&state), ProjectsSkinAccess::Unreachable);
    }

    fn job(value: serde_json::Value) -> McJob {
        serde_json::from_value(value).unwrap()
    }

    #[test]
    fn jobs_and_results_as_the_interface_sees_them() {
        let blocked = job_view(
            &job(serde_json::json!({
                "id": "j1", "kind": "skin", "state": "failed", "name": "Ahmet", "source": "png",
                "result": {"code": "skin_blocked", "until": null}, "createdAt": 1, "updatedAt": 2
            })),
            Some("data:x".into()),
            Some(SkinVariant::Slim),
        )
        .unwrap();
        assert_eq!(
            serde_json::to_value(&blocked).unwrap(),
            serde_json::json!({
                "id": "j1", "kind": "skin", "state": "failed", "name": "Ahmet", "source": "png",
                "result": {"code": "skinBlocked", "until": null}, "picture": "data:x",
                "variant": "slim", "createdAtMs": 1, "updatedAtMs": 2
            })
        );
        let other = job_view(
            &job(serde_json::json!({
                "id": "j2", "kind": "link", "state": "failed", "name": "Ahmet",
                "result": {"code": "other_account", "name": "Mehmet"}
            })),
            Some("ignored".into()),
            None,
        )
        .unwrap();
        let result = other.result.unwrap();
        assert_eq!(
            (result.code, result.name.as_deref(), result.until),
            (ProjectsSkinResultCode::OtherAccount, Some("Mehmet"), None)
        );
        assert_eq!(other.picture, None, "only skin jobs have pictures");
        let unknown = job_view(
            &job(
                serde_json::json!({"id": "j3", "kind": "check", "state": "done",
                                    "result": {"code": "forgotten_somehow"}}),
            ),
            None,
            None,
        )
        .unwrap();
        assert_eq!(unknown.result.unwrap().code, ProjectsSkinResultCode::Error);
        let open = job_view(
            &job(serde_json::json!({"id": "j4", "kind": "skin", "state": "queued"})),
            None,
            None,
        )
        .unwrap();
        assert_eq!(open.result, None);
        assert!(
            job_view(
                &job(serde_json::json!({"id": "j5", "kind": "teleport", "state": "done"})),
                None,
                None
            )
            .is_none()
        );
    }

    #[test]
    fn what_a_message_means() {
        let linked = |name: &str| McLink {
            state: McLinkState::Linked,
            name: Some(name.into()),
            at: Some(1),
            checked_at: Some(2),
        };
        let message = |link: McLink| McMessage {
            job: None,
            link,
            skin: None,
            blocked: None,
        };
        // Nothing known in this session: only "read again".
        assert_eq!(news_of(None, &message(linked("Ahmet"))), McNews::State);
        // Linked in game or on Discord while this launcher knew no link, or another one.
        assert_eq!(
            news_of(Some(&McLink::default()), &message(linked("Ahmet"))),
            McNews::Linked("Ahmet".into())
        );
        assert_eq!(
            news_of(Some(&linked("Mehmet")), &message(linked("Ahmet"))),
            McNews::Linked("Ahmet".into())
        );
        // The same link again (a staff reset carries it), or a link removed elsewhere.
        assert_eq!(
            news_of(Some(&linked("ahmet")), &message(linked("Ahmet"))),
            McNews::State
        );
        assert_eq!(
            news_of(Some(&linked("Ahmet")), &message(McLink::default())),
            McNews::State
        );
        // A closed job is the job's news, whatever the link says.
        let mut with_job = message(linked("Ahmet"));
        with_job.job = Some(job(
            serde_json::json!({"id": "j1", "kind": "link", "state": "done",
                               "result": {"code": "linked"}}),
        ));
        assert!(matches!(news_of(None, &with_job), McNews::Job(_)));
        assert!(matches!(
            news_of(Some(&McLink::default()), &with_job),
            McNews::Job(_)
        ));
    }

    #[test]
    fn changes_and_choices_as_the_interface_reads_them() {
        assert_eq!(
            serde_json::to_value(ProjectsSkinChange::Linked {
                name: "Ahmet".into()
            })
            .unwrap(),
            serde_json::json!({"kind": "linked", "name": "Ahmet"})
        );
        assert_eq!(
            serde_json::to_value(ProjectsSkinChange::State).unwrap(),
            serde_json::json!({"kind": "state"})
        );
        let saves: Vec<ProjectsSkinSave> = serde_json::from_value(serde_json::json!([
            {"kind": "choice", "id": "c1", "variant": "slim"},
            {"kind": "default"},
            {"kind": "undo"}
        ]))
        .unwrap();
        assert_eq!(
            saves[0],
            ProjectsSkinSave::Choice {
                id: "c1".into(),
                variant: SkinVariant::Slim
            }
        );
        assert_eq!(
            serde_json::to_value(ProjectsSkinLookup::NoAccount).unwrap(),
            serde_json::json!({"status": "noAccount"})
        );
        assert_eq!(
            serde_json::to_value(ProjectsSkinPick::Refused {
                reason: ProjectsSkinPickRefusal::TooBig
            })
            .unwrap(),
            serde_json::json!({"status": "refused", "reason": "tooBig"})
        );
    }
}
