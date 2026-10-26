//! Who the game runs as: Microsoft accounts (from `pl-auth`) or offline players with the same
//! `OfflinePlayer:<name>` UUID that offline-mode servers compute.

use std::fmt;

use md5::{Digest, Md5};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum UserType {
    /// Microsoft account.
    Msa,
    /// Offline player.
    Legacy,
}

impl UserType {
    /// Value of `${user_type}`.
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Msa => "msa",
            Self::Legacy => "legacy",
        }
    }
}

/// The launcher account a game was started with: its id in the account list and the name it plays
/// as (a username account renamed meanwhile still plays with the old name). The launcher says
/// "Oyunda" for this account alone (PLAY-ACCT); the game knows it from its start to its end, also
/// across a launcher restart ([`crate::launch::DetachedGame`]).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct GameAccount {
    pub id: String,
    pub name: String,
}

/// Identity passed to the game. `Debug` never prints the access token.
#[derive(Clone, PartialEq, Eq)]
pub struct LaunchIdentity {
    pub name: String,
    pub uuid: Uuid,
    /// Minecraft access token (`"0"` for offline players). Secret: never log it.
    pub access_token: String,
    pub user_type: UserType,
    pub xuid: Option<String>,
    pub client_id: Option<String>,
}

impl LaunchIdentity {
    /// Offline player with the server-compatible offline UUID.
    pub fn offline(name: impl Into<String>) -> Self {
        let name = name.into();
        Self {
            uuid: offline_uuid(&name),
            name,
            access_token: "0".to_owned(),
            user_type: UserType::Legacy,
            xuid: None,
            client_id: None,
        }
    }

    pub fn is_offline(&self) -> bool {
        self.user_type == UserType::Legacy
    }
}

impl fmt::Debug for LaunchIdentity {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchIdentity")
            .field("name", &self.name)
            .field("uuid", &self.uuid)
            .field("access_token", &"<redacted>")
            .field("user_type", &self.user_type)
            .field("has_xuid", &self.xuid.is_some())
            .field("has_client_id", &self.client_id.is_some())
            .finish()
    }
}

/// Java's `UUID.nameUUIDFromBytes(("OfflinePlayer:" + name).getBytes(UTF_8))`: an MD5-based
/// version 3 UUID, the id offline-mode servers assign to a player name.
pub fn offline_uuid(name: &str) -> Uuid {
    let digest = Md5::digest(format!("OfflinePlayer:{name}").as_bytes());
    let mut bytes = [0u8; 16];
    bytes.copy_from_slice(&digest);
    uuid::Builder::from_md5_bytes(bytes).into_uuid()
}

/// Offline names the game and servers accept: 3–16 characters of `A-Z a-z 0-9 _`.
pub fn is_valid_offline_name(name: &str) -> bool {
    (3..=16).contains(&name.len()) && name.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn notch_offline_uuid() {
        assert_eq!(
            offline_uuid("Notch").to_string(),
            "b50ad385-829d-3141-a216-7e7d7539ba7f"
        );
        let id = LaunchIdentity::offline("Notch");
        assert_eq!(
            id.uuid.simple().to_string(),
            "b50ad385829d3141a2167e7d7539ba7f"
        );
        assert!(id.is_offline());
        assert_eq!(id.user_type.as_str(), "legacy");
    }

    #[test]
    fn debug_hides_token() {
        let mut id = LaunchIdentity::offline("Player");
        id.access_token = "super-secret-token".into();
        let printed = format!("{id:?}");
        assert!(!printed.contains("super-secret-token"));
        assert!(printed.contains("Player"));
    }

    #[test]
    fn offline_names() {
        assert!(is_valid_offline_name("Steve_123"));
        assert!(!is_valid_offline_name("ab"));
        assert!(!is_valid_offline_name("this_name_is_too_long"));
        assert!(!is_valid_offline_name("bad name"));
        assert!(!is_valid_offline_name("Çağrı"));
    }
}
