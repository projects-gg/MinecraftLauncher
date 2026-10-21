//! The per-launch secret the mod must present.

use std::fmt;

use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;

/// 32 random bytes as base64url (43 characters). Made fresh for every game launch and useless once that
/// game exits, because the listener goes away with it.
#[derive(Clone)]
pub struct SessionToken(String);

impl SessionToken {
    pub fn generate() -> Self {
        let mut bytes = [0u8; 32];
        rand::fill(&mut bytes[..]);
        Self(URL_SAFE_NO_PAD.encode(bytes))
    }

    /// The value for the game's command line. Keep it out of logs.
    pub(crate) fn expose(&self) -> &str {
        &self.0
    }

    /// Constant-time comparison (the length is public: always 43).
    pub fn matches(&self, offered: &str) -> bool {
        let (a, b) = (self.0.as_bytes(), offered.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
    }
}

impl fmt::Debug for SessionToken {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("SessionToken(<redacted>)")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tokens_are_long_random_and_url_safe() {
        let a = SessionToken::generate();
        let b = SessionToken::generate();
        assert_eq!(a.expose().len(), 43);
        assert_ne!(a.expose(), b.expose());
        assert!(
            a.expose()
                .bytes()
                .all(|c| c.is_ascii_alphanumeric() || c == b'-' || c == b'_')
        );
    }

    #[test]
    fn matching_needs_the_exact_value() {
        let t = SessionToken::generate();
        assert!(t.matches(t.expose()));
        assert!(!t.matches(""));
        assert!(!t.matches(&t.expose()[1..]));
        let mut wrong = t.expose().to_owned().into_bytes();
        wrong[0] ^= 1;
        assert!(!t.matches(std::str::from_utf8(&wrong).unwrap()));
        assert!(!format!("{t:?}").contains(t.expose()));
    }
}
