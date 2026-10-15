//! PKCE (RFC 7636, S256), random state values and the Microsoft authorize URL.

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use sha2::{Digest, Sha256};
use url::Url;

/// A PKCE verifier and its S256 challenge.
pub(crate) struct Pkce {
    pub verifier: String,
    pub challenge: String,
}

impl Pkce {
    /// 32 random bytes → a 43-character verifier (the RFC minimum length, maximum entropy).
    pub fn generate() -> Self {
        Self::from_verifier(random_url_token(32))
    }

    pub fn from_verifier(verifier: String) -> Self {
        let challenge = code_challenge(&verifier);
        Self {
            verifier,
            challenge,
        }
    }
}

/// `BASE64URL-NOPAD(SHA256(ASCII(verifier)))`.
pub(crate) fn code_challenge(verifier: &str) -> String {
    URL_SAFE_NO_PAD.encode(Sha256::digest(verifier.as_bytes()))
}

/// `bytes` bytes from the thread CSPRNG, base64url without padding.
pub(crate) fn random_url_token(bytes: usize) -> String {
    let mut buffer = vec![0u8; bytes];
    rand::fill(&mut buffer[..]);
    URL_SAFE_NO_PAD.encode(buffer)
}

/// The authorize URL the browser opens.
pub(crate) fn authorize_url(
    authorize_endpoint: &str,
    client_id: &str,
    scope: &str,
    redirect_uri: &str,
    state: &str,
    challenge: &str,
) -> Result<Url, url::ParseError> {
    let mut url = Url::parse(authorize_endpoint)?;
    url.query_pairs_mut()
        .append_pair("client_id", client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", redirect_uri)
        .append_pair("response_mode", "query")
        .append_pair("scope", scope)
        .append_pair("state", state)
        .append_pair("code_challenge", challenge)
        .append_pair("code_challenge_method", "S256")
        // Let the player pick the account instead of silently reusing the browser's session.
        .append_pair("prompt", "select_account");
    Ok(url)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::config::{CLIENT_ID, Endpoints, SCOPE};

    #[test]
    fn rfc7636_appendix_b_vector() {
        let pkce = Pkce::from_verifier("dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk".to_owned());
        assert_eq!(
            pkce.challenge,
            "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM"
        );
    }

    #[test]
    fn generated_verifiers_are_valid_and_unique() {
        let a = Pkce::generate();
        let b = Pkce::generate();
        assert_eq!(a.verifier.len(), 43);
        assert!(
            a.verifier
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || c == '-' || c == '_')
        );
        assert_ne!(a.verifier, b.verifier);
        assert_ne!(a.challenge, b.challenge);
        assert_eq!(a.challenge, code_challenge(&a.verifier));
        assert_ne!(random_url_token(16), random_url_token(16));
        assert_eq!(random_url_token(16).len(), 22);
    }

    #[test]
    fn authorize_url_has_every_parameter() {
        let url = authorize_url(
            &Endpoints::production().authorize,
            CLIENT_ID,
            SCOPE,
            "http://localhost:51234",
            "state-123",
            "challenge-abc",
        )
        .unwrap();
        assert_eq!(url.scheme(), "https");
        assert_eq!(url.host_str(), Some("login.microsoftonline.com"));
        assert_eq!(url.path(), "/consumers/oauth2/v2.0/authorize");
        let query: HashMap<String, String> = url.query_pairs().into_owned().collect();
        assert_eq!(query["client_id"], CLIENT_ID);
        assert_eq!(query["response_type"], "code");
        assert_eq!(query["redirect_uri"], "http://localhost:51234");
        assert_eq!(query["response_mode"], "query");
        assert_eq!(query["scope"], "XboxLive.signin offline_access");
        assert_eq!(query["state"], "state-123");
        assert_eq!(query["code_challenge"], "challenge-abc");
        assert_eq!(query["code_challenge_method"], "S256");
        assert_eq!(query["prompt"], "select_account");
        // Reserved characters are encoded, nothing leaks into the path.
        assert!(!url.as_str().contains("localhost:51234&"));
        assert!(
            url.as_str()
                .contains("redirect_uri=http%3A%2F%2Flocalhost%3A51234")
        );
    }
}
