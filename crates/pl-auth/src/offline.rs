//! Offline (cracked-server style) accounts: name rules and the server-compatible UUID.

use md5::{Digest, Md5};
use uuid::Uuid;

use crate::error::OfflineNameProblem;

/// Access token passed to the game for offline accounts. Any non-empty value works; the game
/// only uses it against online services, which reject it.
pub const OFFLINE_ACCESS_TOKEN: &str = "0";

pub const OFFLINE_NAME_MIN: usize = 3;
pub const OFFLINE_NAME_MAX: usize = 16;

/// Checks an offline player name: 3–16 characters of `[A-Za-z0-9_]` (what vanilla servers
/// accept). The caller trims surrounding whitespace first if it wants to.
pub fn validate_offline_name(name: &str) -> Result<(), OfflineNameProblem> {
    if name.is_empty() {
        return Err(OfflineNameProblem::Empty);
    }
    if !name.chars().all(|c| c.is_ascii_alphanumeric() || c == '_') {
        return Err(OfflineNameProblem::InvalidCharacters);
    }
    // Only ASCII is left here, so the byte length is the character count.
    if name.len() < OFFLINE_NAME_MIN {
        return Err(OfflineNameProblem::TooShort);
    }
    if name.len() > OFFLINE_NAME_MAX {
        return Err(OfflineNameProblem::TooLong);
    }
    Ok(())
}

/// The UUID offline-mode servers give this name: Java's
/// `UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes(UTF_8))`, i.e. an MD5 hash with
/// the version 3 and IETF variant bits set (no namespace, unlike RFC 4122 v3).
pub fn offline_uuid(name: &str) -> Uuid {
    let digest: [u8; 16] = Md5::digest(format!("OfflinePlayer:{name}").as_bytes()).into();
    uuid::Builder::from_md5_bytes(digest).into_uuid()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notch_matches_java() {
        assert_eq!(
            offline_uuid("Notch").to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        assert_eq!(offline_uuid("Notch").get_version_num(), 3);
        // Case matters, as on servers.
        assert_ne!(offline_uuid("notch"), offline_uuid("Notch"));
    }

    #[test]
    fn name_rules() {
        assert_eq!(validate_offline_name("Notch"), Ok(()));
        assert_eq!(validate_offline_name("abc"), Ok(()));
        assert_eq!(validate_offline_name("A_1234567890_bcd"), Ok(()));
        assert_eq!(validate_offline_name(""), Err(OfflineNameProblem::Empty));
        assert_eq!(
            validate_offline_name("ab"),
            Err(OfflineNameProblem::TooShort)
        );
        assert_eq!(
            validate_offline_name("abcdefghijklmnopq"),
            Err(OfflineNameProblem::TooLong)
        );
        for bad in ["with space", "Çağrı", "name!", "a-b-c", "nick?", " pad"] {
            assert_eq!(
                validate_offline_name(bad),
                Err(OfflineNameProblem::InvalidCharacters),
                "{bad}"
            );
        }
    }
}
