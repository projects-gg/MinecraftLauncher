//! Failures of the cloud AFK client. The service's envelope is `{"error": {"code", "message",
//! …extra}}` (the friends service's); codes are for programs and are never shown: the launcher
//! words them.

use serde_json::{Map, Value};

pub type Result<T, E = CloudError> = std::result::Result<T, E>;

#[derive(Debug, thiserror::Error)]
pub enum CloudError {
    /// No friends session in the launcher: the account parts need one.
    #[error("not signed in to the friends service")]
    SignedOut,
    /// The request never got an answer (no connection, a timeout).
    #[error("network: {0}")]
    Network(String),
    /// `503 unavailable`, a 5xx, or a platform page without the envelope.
    #[error("the service is unavailable")]
    Unavailable { retry_after: Option<u64> },
    /// `429`: a quota or the flood control.
    #[error("rate limited")]
    RateLimited { retry_after: Option<u64> },
    /// `401`: the friends session is gone.
    #[error("unauthorized")]
    Unauthorized,
    /// Any other refusal with the envelope: `code` and its extra fields.
    #[error("refused: HTTP {status} {code}")]
    Refused {
        status: u16,
        code: String,
        extra: Map<String, Value>,
    },
    /// The answer was not what the contract says.
    #[error("protocol: {0}")]
    Protocol(String),
    /// A secret could not be sealed (the vault key, the text) or an answer opened.
    #[error("seal: {0}")]
    Seal(String),
    /// The OS keychain did not answer (the copies of the rows this computer sealed).
    #[error("keychain: {0}")]
    Keychain(String),
    #[error("invalid input: {0}")]
    Invalid(&'static str),
}

impl CloudError {
    /// The contract's code (`rate_limited`, `host_offline`, …), or this client's own for failures
    /// without one.
    pub fn code(&self) -> &str {
        match self {
            Self::SignedOut => "signed_out",
            Self::Network(_) => "network",
            Self::Unavailable { .. } => "unavailable",
            Self::RateLimited { .. } => "rate_limited",
            Self::Unauthorized => "unauthorized",
            Self::Refused { code, .. } => code,
            Self::Protocol(_) => "protocol",
            Self::Seal(_) => "seal",
            Self::Keychain(_) => "keychain",
            Self::Invalid(_) => "invalid",
        }
    }

    /// An extra field of a refusal (`rev`, `field`, `reason`, `retryAfter`, …).
    pub fn extra(&self, name: &str) -> Option<&Value> {
        match self {
            Self::Refused { extra, .. } => extra.get(name),
            _ => None,
        }
    }

    /// An extra text field of a refusal.
    pub fn extra_str(&self, name: &str) -> Option<&str> {
        self.extra(name).and_then(Value::as_str)
    }

    /// Worth trying again later (never in a loop).
    pub fn is_transient(&self) -> bool {
        matches!(
            self,
            Self::Network(_) | Self::Unavailable { .. } | Self::RateLimited { .. }
        )
    }

    /// The seconds the service asked to wait, when it said.
    pub fn retry_after(&self) -> Option<u64> {
        match self {
            Self::Unavailable { retry_after } | Self::RateLimited { retry_after } => *retry_after,
            Self::Refused { extra, .. } => extra.get("retryAfter").and_then(Value::as_u64),
            _ => None,
        }
    }
}

/// The error of an answer that is not a success.
pub(crate) fn from_response(status: u16, retry_after: Option<&str>, body: &[u8]) -> CloudError {
    let header_wait = retry_after.and_then(|value| value.trim().parse::<u64>().ok());
    let envelope = serde_json::from_slice::<Value>(body)
        .ok()
        .and_then(|value| value.get("error").cloned())
        .and_then(|error| match error {
            Value::Object(map) => Some(map),
            _ => None,
        });
    let Some(mut extra) = envelope else {
        // A platform page (Cloudflare's own errors) or nothing: treat it like an outage.
        return match status {
            401 => CloudError::Unauthorized,
            429 => CloudError::RateLimited {
                retry_after: header_wait,
            },
            _ => CloudError::Unavailable {
                retry_after: header_wait,
            },
        };
    };
    let code = extra
        .remove("code")
        .and_then(|code| code.as_str().map(str::to_owned))
        .unwrap_or_default();
    extra.remove("message");
    let wait = header_wait.or_else(|| extra.get("retryAfter").and_then(Value::as_u64));
    match (status, code.as_str()) {
        (401, _) => CloudError::Unauthorized,
        (429, "rate_limited" | "") => CloudError::RateLimited { retry_after: wait },
        (503, "unavailable" | "") | (500, _) => CloudError::Unavailable { retry_after: wait },
        (_, "") => CloudError::Unavailable { retry_after: wait },
        _ => CloudError::Refused {
            status,
            code,
            extra,
        },
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn maps_the_envelope() {
        let error = from_response(
            412,
            None,
            br#"{"error": {"code": "rev_conflict", "message": "stale", "rev": 9}}"#,
        );
        assert_eq!(error.code(), "rev_conflict");
        assert_eq!(error.extra("rev").and_then(Value::as_u64), Some(9));
        assert!(error.extra("message").is_none());

        let error = from_response(
            429,
            Some("30"),
            br#"{"error": {"code": "rate_limited", "message": "x", "retryAfter": 30}}"#,
        );
        assert!(matches!(
            error,
            CloudError::RateLimited {
                retry_after: Some(30)
            }
        ));
        assert!(error.is_transient());

        let error = from_response(
            503,
            None,
            br#"{"error": {"code": "host_offline", "message": "x", "hostId": "kz1"}}"#,
        );
        assert_eq!(error.code(), "host_offline");
        assert!(!error.is_transient());

        let error = from_response(401, None, br#"{"error": {"code": "unauthorized"}}"#);
        assert!(matches!(error, CloudError::Unauthorized));

        let error = from_response(
            429,
            None,
            br#"{"error": {"code": "link_attempts", "retryAfter": 60}}"#,
        );
        assert_eq!(error.code(), "link_attempts");
        assert_eq!(error.retry_after(), Some(60));
    }

    #[test]
    fn a_page_without_the_envelope_is_an_outage() {
        let error = from_response(502, Some("12"), b"<html>Bad gateway</html>");
        assert!(matches!(
            error,
            CloudError::Unavailable {
                retry_after: Some(12)
            }
        ));
        assert!(matches!(
            from_response(401, None, b""),
            CloudError::Unauthorized
        ));
    }
}
