//! Sign-in with Discord: verifier and challenge, the browser URL, the one-time code of the
//! `projects://auth/done` deep link, and the session token type.
//!
//! The launcher never receives a token in a URL. It opens [`LoginAttempt::browser_url`]; the
//! service finishes the Discord step and the sign-in page opens `projects://auth/done?c=<code>`
//! ([`AUTH_DONE_LINK`]). The launcher takes the code out of that link with
//! [`auth_code_from_deep_link`] and calls [`crate::SocialClient::complete_login`] with the same
//! attempt and the code. Both are needed: the verifier (which never leaves this process until the
//! claim) proves this launcher started the sign-in, and the code reaches only the launcher on the
//! computer whose browser approved it, so a sign-in link someone else started is useless to them.
//!
//! The code is single-use and expires 5 minutes after the browser step. A claim that fails uses
//! the sign-in up: start a new attempt instead of retrying. Ignore `projects://auth/done` links
//! that arrive while no attempt is waiting.

use std::fmt;
use std::sync::Arc;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use url::Url;

use crate::model::Me;

/// Deep link the sign-in page opens when Discord is done: `projects://auth/done?c=<code>`.
pub const AUTH_DONE_LINK: &str = "projects://auth/done";

/// A session bearer token (`pls_…`). Keep it in the OS keychain; it is never logged.
#[derive(Clone, PartialEq, Eq)]
pub struct SessionToken(Arc<str>);

impl SessionToken {
    pub fn new(token: impl Into<String>) -> Self {
        Self(Arc::from(token.into()))
    }

    /// The raw token, for the keychain and the `Authorization` header only.
    pub fn expose(&self) -> &str {
        &self.0
    }

    /// Whether this looks like a token issued by the service.
    pub fn is_well_formed(&self) -> bool {
        self.0.len() == 47
            && self.0.starts_with("pls_")
            && self.0[4..]
                .bytes()
                .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(…)")
    }
}

/// A sign-in in progress. Keep it in memory until `projects://auth/done?c=…` arrives, then claim
/// it with that link's code. The browser step must finish within 10 minutes of opening the
/// browser URL, and the claim within 5 minutes after that.
pub struct LoginAttempt {
    verifier: String,
    challenge: String,
    browser_url: Url,
}

impl LoginAttempt {
    pub(crate) fn new(base: &Url) -> Self {
        let mut bytes = [0u8; 32];
        rand::fill(&mut bytes[..]);
        Self::from_verifier(base, URL_SAFE_NO_PAD.encode(bytes))
    }

    pub(crate) fn from_verifier(base: &Url, verifier: String) -> Self {
        let challenge = challenge_for(&verifier);
        let mut browser_url = base
            .join("auth/discord/start")
            .expect("a fixed relative path always joins");
        browser_url
            .query_pairs_mut()
            .append_pair("challenge", &challenge);
        Self {
            verifier,
            challenge,
            browser_url,
        }
    }

    /// Open this in the player's browser.
    pub fn browser_url(&self) -> &Url {
        &self.browser_url
    }

    pub fn challenge(&self) -> &str {
        &self.challenge
    }

    pub(crate) fn verifier(&self) -> &str {
        &self.verifier
    }
}

impl fmt::Debug for LoginAttempt {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LoginAttempt")
            .field("challenge", &self.challenge)
            .finish_non_exhaustive()
    }
}

/// `BASE64URL(SHA256(verifier))` (RFC 7636 S256).
pub(crate) fn challenge_for(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// The one-time code of `projects://auth/done?c=<code>`, or None when the link is anything else
/// (another deep link, no code, a malformed code, the code twice).
pub fn auth_code_from_deep_link(link: &str) -> Option<String> {
    let url = Url::parse(link.trim()).ok()?;
    if url.scheme() != "projects"
        || !url
            .host_str()
            .is_some_and(|host| host.eq_ignore_ascii_case("auth"))
        || url.path().trim_end_matches('/') != "/done"
    {
        return None;
    }
    let mut codes = url
        .query_pairs()
        .filter(|(key, _)| key == "c")
        .map(|(_, value)| value);
    let code = codes.next()?;
    if codes.next().is_some() || !is_auth_code(&code) {
        return None;
    }
    Some(code.into_owned())
}

/// A sign-in code as the service issues it: 32 random bytes, base64url without padding.
pub(crate) fn is_auth_code(code: &str) -> bool {
    code.len() == 43
        && code
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'_')
}

/// A completed sign-in.
#[derive(Debug, Clone)]
pub struct SignedIn {
    pub token: SessionToken,
    pub me: Me,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn rfc7636_appendix_b_vector() {
        assert_eq!(
            challenge_for("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk"),
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn attempts_are_random_and_point_at_the_service() {
        let base = Url::parse("https://social.projects.gg/").unwrap();
        let a = LoginAttempt::new(&base);
        let b = LoginAttempt::new(&base);
        assert_ne!(a.verifier(), b.verifier());
        assert_eq!(a.verifier().len(), 43);
        assert_eq!(a.challenge(), challenge_for(a.verifier()));
        assert_eq!(
            a.browser_url().as_str(),
            format!(
                "https://social.projects.gg/auth/discord/start?challenge={}",
                a.challenge()
            )
        );
        // The verifier never shows up in logs.
        assert!(!format!("{a:?}").contains(a.verifier()));
    }

    #[test]
    fn reads_the_code_of_the_sign_in_deep_link() {
        let code = format!("{}-_9", "A".repeat(40));
        for link in [
            format!("projects://auth/done?c={code}"),
            format!("projects://auth/done/?c={code}"),
            format!(" projects://AUTH/done?c={code}&later=1 "),
        ] {
            assert_eq!(
                auth_code_from_deep_link(&link).as_deref(),
                Some(code.as_str()),
                "{link}"
            );
        }
        for link in [
            "projects://auth/done".to_owned(),
            format!("projects://auth/done?c={}", "A".repeat(42)),
            format!("projects://auth/done?c={}!", "A".repeat(42)),
            format!("projects://auth/done?c={code}&c={code}"),
            format!("projects://auth/other?c={code}"),
            format!("projects://friend/done?c={code}"),
            format!("https://auth/done?c={code}"),
            "not a link".to_owned(),
        ] {
            assert_eq!(auth_code_from_deep_link(&link), None, "{link}");
        }
        assert!(is_auth_code(&code));
        assert!(!is_auth_code(&format!("{code}A")));
    }

    #[test]
    fn tokens_are_redacted() {
        let token = SessionToken::new(format!("pls_{}", "A".repeat(43)));
        assert!(token.is_well_formed());
        assert_eq!(format!("{token:?}"), "SessionToken(…)");
        assert!(!SessionToken::new("pls_short").is_well_formed());
    }
}
