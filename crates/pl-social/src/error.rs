//! Typed errors. The launcher maps [`ErrorCode`]s and variants to Turkish messages; none of the
//! codes or messages here are meant to be shown to players as they are.

use std::fmt;
use std::time::Duration;

use serde::Deserialize;

use crate::model::{ChatRejection, LimitedKey, SettingsMeta};

/// Stable error codes of the service (`{"error": {"code": …}}`).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ErrorCode {
    BadRequest,
    InvalidCode,
    Unauthorized,
    NotFound,
    MethodNotAllowed,
    AuthPendingNotFound,
    CodeNotFound,
    UserNotFound,
    RequestNotFound,
    FriendNotFound,
    BlockNotFound,
    SessionNotFound,
    SettingsNotFound,
    /// Your own code, or blocking yourself.
    SelfTarget,
    AlreadyFriends,
    AlreadyRequested,
    /// You blocked this user; unblock first.
    Blocked,
    PreconditionFailed,
    PreconditionRequired,
    PayloadTooLarge,
    UnsupportedMediaType,
    LimitFriends,
    LimitFriendsTarget,
    LimitOutgoingRequests,
    LimitIncomingRequests,
    SettingsSecretKey,
    /// The friend categories document does not add up.
    InvalidGroups,
    InvalidTeamName,
    InvalidTeamTag,
    /// The team name or tag is on the service's word list.
    TeamNameRejected,
    /// Unknown team code, or a team you are not in (also when someone in it blocked you).
    TeamNotFound,
    TeamInviteNotFound,
    /// Not a member of this team (removing, handing on the founder role).
    MemberNotFound,
    /// You are in a team already; leave it first.
    AlreadyInTeam,
    /// The friend you invited is in this team already.
    AlreadyMember,
    /// Only the founder can do this.
    TeamFounderOnly,
    /// A founder with members hands the role on before leaving.
    FounderMustTransfer,
    /// You blocked someone in this team.
    BlockedMember,
    LimitTeamMembers,
    LimitTeamInvites,
    /// The motto does not follow the rules (40 characters, the name's characters plus `,` `?`).
    InvalidTeamMotto,
    /// The motto is on the service's word list.
    TeamMottoRejected,
    /// A crew call is open already (the player's own, or one of a teammate they see).
    CallActive,
    /// The call is gone (ended, ran out, hidden), or it is the player's own when answering.
    CallNotFound,
    /// The crew's permissions do not let the player's role do this.
    TeamNotPermitted,
    /// A crew has at most 4 moderators.
    LimitTeamModerators,
    /// A crew call starts only while the player plays on a multiplayer server (CALL1).
    CallNeedsServer,
    /// On another server a crew call needs its address shared with friends (CALL1).
    CallServerHidden,
    /// The player is in as many crews as the service allows (`crewLimit`).
    LimitCrews,
    /// A new crew order did not name exactly the player's crews: read the list again.
    CrewsChanged,
    /// The account is suspended or banned from the social features (MOD1).
    AccountRestricted,
    /// A limited account ("Sınırlı hesap") may not do this, or not more today (SPAM1).
    AccountLimited,
    /// No new accounts are made right now (the daily cap, or storage protection).
    SignupsClosed,
    /// Moderators reset the crew's name, tag and motto; they stay locked for a while.
    TeamTextLocked,
    /// Nothing the player may report there (or it does not exist).
    TargetNotFound,
    /// At most 1,000 blocks.
    LimitBlocks,
    /// Chat is switched off, or not offered (CHAT1).
    ChatOff,
    /// A moderator muted the player in chat.
    ChatMuted,
    /// A link, a phone number or a listed word.
    ChatRejected,
    /// The message does not check out, or it was deleted.
    MessageNotFound,
    /// Not the player's message (and not their crew's to moderate).
    ChatNotPermitted,
    /// Too late to edit (15 minutes) or delete (7 days).
    ChatTooOld,
    UpgradeRequired,
    RateLimited,
    Internal,
    Unavailable,
    /// A code this version of the client does not know.
    Other(String),
}

const CODES: &[(&str, ErrorCode)] = &[
    ("bad_request", ErrorCode::BadRequest),
    ("invalid_code", ErrorCode::InvalidCode),
    ("unauthorized", ErrorCode::Unauthorized),
    ("not_found", ErrorCode::NotFound),
    ("method_not_allowed", ErrorCode::MethodNotAllowed),
    ("auth_pending_not_found", ErrorCode::AuthPendingNotFound),
    ("code_not_found", ErrorCode::CodeNotFound),
    ("user_not_found", ErrorCode::UserNotFound),
    ("request_not_found", ErrorCode::RequestNotFound),
    ("friend_not_found", ErrorCode::FriendNotFound),
    ("block_not_found", ErrorCode::BlockNotFound),
    ("session_not_found", ErrorCode::SessionNotFound),
    ("settings_not_found", ErrorCode::SettingsNotFound),
    ("self", ErrorCode::SelfTarget),
    ("already_friends", ErrorCode::AlreadyFriends),
    ("already_requested", ErrorCode::AlreadyRequested),
    ("blocked", ErrorCode::Blocked),
    ("precondition_failed", ErrorCode::PreconditionFailed),
    ("precondition_required", ErrorCode::PreconditionRequired),
    ("payload_too_large", ErrorCode::PayloadTooLarge),
    ("unsupported_media_type", ErrorCode::UnsupportedMediaType),
    ("limit_friends", ErrorCode::LimitFriends),
    ("limit_friends_target", ErrorCode::LimitFriendsTarget),
    ("limit_outgoing_requests", ErrorCode::LimitOutgoingRequests),
    ("limit_incoming_requests", ErrorCode::LimitIncomingRequests),
    ("settings_secret_key", ErrorCode::SettingsSecretKey),
    ("invalid_groups", ErrorCode::InvalidGroups),
    ("invalid_team_name", ErrorCode::InvalidTeamName),
    ("invalid_team_tag", ErrorCode::InvalidTeamTag),
    ("team_name_rejected", ErrorCode::TeamNameRejected),
    ("team_not_found", ErrorCode::TeamNotFound),
    ("team_invite_not_found", ErrorCode::TeamInviteNotFound),
    ("member_not_found", ErrorCode::MemberNotFound),
    ("already_in_team", ErrorCode::AlreadyInTeam),
    ("already_member", ErrorCode::AlreadyMember),
    ("team_founder_only", ErrorCode::TeamFounderOnly),
    ("founder_must_transfer", ErrorCode::FounderMustTransfer),
    ("blocked_member", ErrorCode::BlockedMember),
    ("limit_team_members", ErrorCode::LimitTeamMembers),
    ("limit_team_invites", ErrorCode::LimitTeamInvites),
    ("invalid_team_motto", ErrorCode::InvalidTeamMotto),
    ("team_motto_rejected", ErrorCode::TeamMottoRejected),
    ("call_active", ErrorCode::CallActive),
    ("call_not_found", ErrorCode::CallNotFound),
    ("team_not_permitted", ErrorCode::TeamNotPermitted),
    ("limit_team_moderators", ErrorCode::LimitTeamModerators),
    ("call_needs_server", ErrorCode::CallNeedsServer),
    ("call_server_hidden", ErrorCode::CallServerHidden),
    ("limit_crews", ErrorCode::LimitCrews),
    ("crews_changed", ErrorCode::CrewsChanged),
    ("account_restricted", ErrorCode::AccountRestricted),
    ("account_limited", ErrorCode::AccountLimited),
    ("signups_closed", ErrorCode::SignupsClosed),
    ("team_text_locked", ErrorCode::TeamTextLocked),
    ("target_not_found", ErrorCode::TargetNotFound),
    ("limit_blocks", ErrorCode::LimitBlocks),
    ("chat_off", ErrorCode::ChatOff),
    ("chat_muted", ErrorCode::ChatMuted),
    ("chat_rejected", ErrorCode::ChatRejected),
    ("message_not_found", ErrorCode::MessageNotFound),
    ("chat_not_permitted", ErrorCode::ChatNotPermitted),
    ("chat_too_old", ErrorCode::ChatTooOld),
    ("upgrade_required", ErrorCode::UpgradeRequired),
    ("rate_limited", ErrorCode::RateLimited),
    ("internal", ErrorCode::Internal),
    ("unavailable", ErrorCode::Unavailable),
];

impl ErrorCode {
    pub fn parse(code: &str) -> Self {
        CODES
            .iter()
            .find(|(name, _)| *name == code)
            .map(|(_, value)| value.clone())
            .unwrap_or_else(|| Self::Other(code.to_owned()))
    }

    pub fn as_str(&self) -> &str {
        match self {
            Self::Other(code) => code,
            known => CODES
                .iter()
                .find(|(_, value)| value == known)
                .map_or("unknown", |(name, _)| name),
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

#[derive(Debug, thiserror::Error)]
pub enum SocialError {
    /// No valid session (HTTP 401, or the presence socket closed with 4001): sign in again.
    #[error("not signed in, or the session has ended")]
    Unauthorized,
    /// Too many requests, or a daily quota is used up. Wait `retry_after` before trying again.
    #[error("rate limited")]
    RateLimited { retry_after: Option<Duration> },
    /// The service is down or a free-plan daily limit was reached. Wait `retry_after` (often
    /// until 00:00 UTC) or back off; keep the launcher working without social features.
    #[error("the friends service is unavailable")]
    Unavailable { retry_after: Option<Duration> },
    /// The settings backup changed on another device since the version you sent.
    #[error("settings changed on another device")]
    SettingsConflict { current: Option<SettingsMeta> },
    /// The service understood the request and refused it (validation, limits, not found…).
    #[error("request refused: {code} (HTTP {status})")]
    Refused {
        status: u16,
        code: ErrorCode,
        message: String,
    },
    #[error("network error: {0}")]
    Network(String),
    #[error("unexpected response: {0}")]
    Protocol(String),
    #[error("invalid input: {0}")]
    InvalidInput(&'static str),
    /// The account is suspended (until `until`, Unix seconds) or banned (`None`) from the social
    /// features: every session ended (MOD1).
    #[error("the account is restricted")]
    Restricted { until: Option<i64> },
    /// A limited account may not do this (`item`), or not more today (`retry_after` until the
    /// next day). The limits end at `until` (Unix seconds; SPAM1).
    #[error("a limited account cannot do this: {item:?}")]
    Limited {
        item: LimitedKey,
        until: Option<i64>,
        retry_after: Option<Duration>,
    },
    /// No new accounts are made right now; existing ones sign in as usual.
    #[error("no new accounts right now")]
    SignupsClosed { retry_after: Option<Duration> },
    /// Moderators reset the crew's name, tag and motto; they stay locked until `until`.
    #[error("the crew's texts are locked")]
    TextLocked { until: Option<i64> },
    /// The service turned a chat text away: a link, a phone number or a listed word.
    #[error("chat text refused: {reason:?}")]
    ChatRejected { reason: ChatRejection },
    /// A moderator muted the player in chat until `until` (Unix seconds, the hub's clock).
    #[error("muted in chat")]
    ChatMuted { until: Option<i64> },
}

impl SocialError {
    /// The service's error code, when there is one.
    pub fn code(&self) -> Option<&ErrorCode> {
        match self {
            Self::Refused { code, .. } => Some(code),
            Self::Restricted { .. } => Some(&ErrorCode::AccountRestricted),
            Self::Limited { .. } => Some(&ErrorCode::AccountLimited),
            Self::SignupsClosed { .. } => Some(&ErrorCode::SignupsClosed),
            Self::TextLocked { .. } => Some(&ErrorCode::TeamTextLocked),
            Self::ChatRejected { .. } => Some(&ErrorCode::ChatRejected),
            Self::ChatMuted { .. } => Some(&ErrorCode::ChatMuted),
            _ => None,
        }
    }

    pub fn retry_after(&self) -> Option<Duration> {
        match self {
            Self::RateLimited { retry_after }
            | Self::Unavailable { retry_after }
            | Self::SignupsClosed { retry_after }
            | Self::Limited { retry_after, .. } => *retry_after,
            _ => None,
        }
    }

    /// True when the same request can succeed later without any change (network trouble,
    /// rate limits, outages). Retry these with backoff, never in a tight loop.
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::RateLimited { .. } | Self::Unavailable { .. } | Self::Network(_)
        )
    }
}

pub type Result<T, E = SocialError> = std::result::Result<T, E>;

/// Longest wait we honour from a `Retry-After` (the daily limits reset within 24 hours).
const MAX_RETRY_AFTER: Duration = Duration::from_secs(24 * 3600);

pub(crate) fn parse_retry_after(value: Option<&str>) -> Option<Duration> {
    let seconds: u64 = value?.trim().parse().ok()?;
    Some(Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
}

#[derive(Deserialize)]
struct Envelope {
    error: ErrorBody,
}

#[derive(Deserialize)]
#[serde(rename_all = "camelCase")]
struct ErrorBody {
    code: String,
    #[serde(default)]
    message: String,
    #[serde(default)]
    retry_after: Option<u64>,
    #[serde(default)]
    current: Option<SettingsMeta>,
    /// Unix seconds, or null (a ban): `account_restricted`, `account_limited`, `team_text_locked`.
    #[serde(default)]
    until: Option<i64>,
    /// `account_limited`: what is limited.
    #[serde(default)]
    item: Option<String>,
}

/// Maps a non-success response. Bodies that are not the service's JSON envelope (for example a
/// Cloudflare error page once a daily limit is exhausted) are mapped by status alone.
pub(crate) fn from_response(status: u16, retry_after: Option<&str>, body: &[u8]) -> SocialError {
    let header = parse_retry_after(retry_after);
    let envelope = serde_json::from_slice::<Envelope>(body)
        .ok()
        .map(|e| e.error);
    let retry_after = header.or_else(|| {
        envelope
            .as_ref()
            .and_then(|e| e.retry_after)
            .map(|seconds| Duration::from_secs(seconds).min(MAX_RETRY_AFTER))
    });
    match (status, envelope) {
        (401, _) => SocialError::Unauthorized,
        (429, _) => SocialError::RateLimited { retry_after },
        (412, Some(body)) => SocialError::SettingsConflict {
            current: body.current,
        },
        (500..=599, _) => SocialError::Unavailable { retry_after },
        (403, Some(body)) if body.code == "account_restricted" => {
            SocialError::Restricted { until: body.until }
        }
        (403, Some(body)) if body.code == "account_limited" => SocialError::Limited {
            item: LimitedKey::parse(body.item.as_deref().unwrap_or_default()),
            until: body.until,
            retry_after,
        },
        (403, Some(body)) if body.code == "signups_closed" => {
            SocialError::SignupsClosed { retry_after }
        }
        (403, Some(body)) if body.code == "team_text_locked" => {
            SocialError::TextLocked { until: body.until }
        }
        (_, Some(body)) => SocialError::Refused {
            status,
            code: ErrorCode::parse(&body.code),
            message: body.message,
        },
        (_, None) => SocialError::Protocol(format!("HTTP {status} without an error body")),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn codes_round_trip() {
        for (name, code) in CODES {
            assert_eq!(&ErrorCode::parse(name), code);
            assert_eq!(code.as_str(), *name);
        }
        assert_eq!(
            ErrorCode::parse("brand_new"),
            ErrorCode::Other("brand_new".into())
        );
        assert_eq!(ErrorCode::Other("x".into()).to_string(), "x");
    }

    #[test]
    fn maps_statuses_and_envelopes() {
        let body = br#"{"error":{"code":"rate_limited","message":"Daily limit","retryAfter":120}}"#;
        let error = from_response(429, None, body);
        assert!(
            matches!(error, SocialError::RateLimited { retry_after: Some(d) } if d.as_secs() == 120)
        );
        assert!(error.is_transient());

        let error = from_response(429, Some("30"), b"<html>Cloudflare</html>");
        assert_eq!(error.retry_after(), Some(Duration::from_secs(30)));

        assert!(matches!(
            from_response(401, None, b"{}"),
            SocialError::Unauthorized
        ));
        assert!(matches!(
            from_response(503, Some("999999"), b""),
            SocialError::Unavailable { retry_after: Some(d) } if d == MAX_RETRY_AFTER
        ));

        let body = br#"{"error":{"code":"code_not_found","message":"No user has this code"}}"#;
        let error = from_response(404, None, body);
        assert_eq!(error.code(), Some(&ErrorCode::CodeNotFound));
        assert!(!error.is_transient());

        let body = br#"{"error":{"code":"precondition_failed","message":"x","current":{"version":3,"updatedAt":1,"device":"PC","size":10}}}"#;
        match from_response(412, None, body) {
            SocialError::SettingsConflict {
                current: Some(meta),
            } => assert_eq!(meta.version, 3),
            other => panic!("unexpected {other:?}"),
        }

        let body = br#"{"error":{"code":"internal","message":"Internal error"}}"#;
        assert!(matches!(
            from_response(500, None, body),
            SocialError::Unavailable { retry_after: None }
        ));
        assert!(matches!(
            from_response(418, None, b"teapot"),
            SocialError::Protocol(_)
        ));
    }

    #[test]
    fn restrictions_limits_and_closed_sign_ups_keep_their_details() {
        let banned = br#"{"error":{"code":"account_restricted","message":"x","until":null}}"#;
        assert!(matches!(
            from_response(403, None, banned),
            SocialError::Restricted { until: None }
        ));
        let suspended =
            br#"{"error":{"code":"account_restricted","message":"x","until":1792600000}}"#;
        let error = from_response(403, None, suspended);
        assert!(matches!(
            error,
            SocialError::Restricted {
                until: Some(1_792_600_000)
            }
        ));
        assert_eq!(error.code(), Some(&ErrorCode::AccountRestricted));

        let limited = br#"{"error":{"code":"account_limited","message":"x","item":"crew_invites",
            "until":1792600000,"retryAfter":3600}}"#;
        match from_response(403, None, limited) {
            SocialError::Limited {
                item,
                until,
                retry_after,
            } => {
                assert_eq!(item, LimitedKey::CrewInvites);
                assert_eq!(until, Some(1_792_600_000));
                assert_eq!(retry_after, Some(Duration::from_secs(3_600)));
            }
            other => panic!("unexpected {other:?}"),
        }
        let unknown = br#"{"error":{"code":"account_limited","message":"x","item":"rockets"}}"#;
        assert!(matches!(
            from_response(403, None, unknown),
            SocialError::Limited {
                item: LimitedKey::Other,
                ..
            }
        ));
        let closed = br#"{"error":{"code":"signups_closed","message":"x","retryAfter":7200}}"#;
        assert_eq!(
            from_response(403, None, closed).retry_after(),
            Some(Duration::from_secs(7_200))
        );
        let locked = br#"{"error":{"code":"team_text_locked","message":"x","until":1792600000}}"#;
        assert!(matches!(
            from_response(403, None, locked),
            SocialError::TextLocked {
                until: Some(1_792_600_000)
            }
        ));
        // Any other 403 stays a refusal with its code.
        let founder = br#"{"error":{"code":"team_founder_only","message":"x"}}"#;
        assert_eq!(
            from_response(403, None, founder).code(),
            Some(&ErrorCode::TeamFounderOnly)
        );
        let target = br#"{"error":{"code":"target_not_found","message":"x"}}"#;
        assert_eq!(
            from_response(404, None, target).code(),
            Some(&ErrorCode::TargetNotFound)
        );
    }
}
