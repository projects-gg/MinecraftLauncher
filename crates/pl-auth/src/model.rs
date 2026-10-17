//! Account data shared with the UI, secrets kept in the keychain, and the launch identity.

use std::fmt;
use std::time::Duration;

use jiff::{SignedDuration, Timestamp};
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::config::CLIENT_ID;
use crate::error::{AuthError, Result};
use crate::offline::OFFLINE_ACCESS_TOKEN;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum AccountKind {
    Microsoft,
    Offline,
}

/// Player model of a skin: classic (Steve, 4 px arms) or slim (Alex, 3 px arms).
#[derive(
    Debug, Clone, Copy, Default, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type,
)]
#[serde(rename_all = "camelCase")]
pub enum SkinVariant {
    #[default]
    Classic,
    Slim,
}

impl SkinVariant {
    /// Value the Minecraft Services skin endpoints expect.
    pub fn api_name(self) -> &'static str {
        match self {
            Self::Classic => "classic",
            Self::Slim => "slim",
        }
    }

    /// Parses the API's `CLASSIC` / `SLIM` (any case). Unknown values fall back to classic.
    pub fn from_api(value: &str) -> Self {
        if value.eq_ignore_ascii_case("slim") {
            Self::Slim
        } else {
            Self::Classic
        }
    }
}

/// The active skin of an account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Skin {
    pub url: String,
    pub variant: SkinVariant,
    /// Set for Mojang's default skins (e.g. "STEVE"); `None` for uploaded skins.
    #[serde(default)]
    pub alias: Option<String>,
}

impl Skin {
    /// One of Mojang's default skins (Steve, Alex and the others) rather than an upload.
    pub fn is_default(&self) -> bool {
        self.alias.is_some()
    }
}

/// A cape the profile owns.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Cape {
    pub id: String,
    /// Mojang's alias, e.g. "Migrator".
    pub name: String,
    pub url: String,
    pub active: bool,
}

/// A saved account. Holds no secrets: it is persisted to `accounts.json` and sent to the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    /// Local id; also the key of the account's keychain entry.
    pub id: Uuid,
    pub kind: AccountKind,
    /// In-game name.
    pub username: String,
    /// Minecraft profile UUID (offline accounts: the `OfflinePlayer:<name>` UUID).
    pub uuid: Uuid,
    #[serde(default)]
    pub skin: Option<Skin>,
    #[serde(default)]
    pub capes: Vec<Cape>,
    pub added_at: Timestamp,
    /// Last time the game was started with this account.
    #[serde(default)]
    pub last_used_at: Option<Timestamp>,
    /// Microsoft refused the saved session (or it is gone): only a new sign-in fixes it. Set by
    /// [`crate::AuthManager::ensure_fresh`], cleared by a successful refresh or sign-in.
    #[serde(default)]
    pub needs_relogin: bool,
    /// Pinned by the player: listed before the others, in the player's order (the order of the
    /// pinned accounts in `accounts.json`). Unpinned accounts have no manual order.
    #[serde(default)]
    pub pinned: bool,
    /// Picture shown instead of the player head (offline accounts only): `preset:<slug>`, one of
    /// the pictures bundled with the launcher. `None` shows the head.
    #[serde(default)]
    pub avatar: Option<String>,
}

/// Prefix of the bundled pictures' ids (`preset:<slug>`), shared with the friends service.
pub const PRESET_AVATAR_PREFIX: &str = "preset:";

/// Whether `value` names a bundled picture: `preset:` and a slug of 1–32 characters of
/// `[a-z0-9-]`. Which slugs exist is the launcher's business (its manifest); an unknown slug
/// shows the player head.
pub fn is_preset_avatar(value: &str) -> bool {
    value
        .strip_prefix(PRESET_AVATAR_PREFIX)
        .is_some_and(|slug| {
            (1..=32).contains(&slug.len())
                && slug
                    .bytes()
                    .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'-')
        })
}

impl Account {
    pub fn new(kind: AccountKind, username: impl Into<String>, uuid: Uuid) -> Self {
        Self {
            id: Uuid::new_v4(),
            kind,
            username: username.into(),
            uuid,
            skin: None,
            capes: Vec::new(),
            added_at: Timestamp::now(),
            last_used_at: None,
            needs_relogin: false,
            pinned: false,
            avatar: None,
        }
    }

    pub fn is_microsoft(&self) -> bool {
        self.kind == AccountKind::Microsoft
    }

    /// Copies name, skin and capes from a freshly fetched profile.
    pub fn apply_profile(&mut self, profile: &MinecraftProfile) {
        self.username.clone_from(&profile.name);
        self.uuid = profile.id;
        self.skin = profile.active_skin();
        self.capes.clone_from(&profile.capes);
    }
}

/// A skin entry of a Minecraft profile.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct ProfileSkin {
    pub id: String,
    pub url: String,
    pub variant: SkinVariant,
    pub active: bool,
    /// Set for Mojang's default skins (e.g. "STEVE").
    pub alias: Option<String>,
}

/// The Minecraft: Java Edition profile of a Microsoft account.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct MinecraftProfile {
    pub id: Uuid,
    pub name: String,
    pub skins: Vec<ProfileSkin>,
    pub capes: Vec<Cape>,
}

impl MinecraftProfile {
    pub fn active_skin(&self) -> Option<Skin> {
        self.skins.iter().find(|skin| skin.active).map(|skin| Skin {
            url: skin.url.clone(),
            variant: skin.variant,
            alias: skin.alias.clone(),
        })
    }
}

/// What the entitlement check found.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Ownership {
    /// The account may play Java Edition (purchase or Game Pass).
    pub java_edition: bool,
    /// The license comes from PC Game Pass / Game Pass Ultimate.
    pub game_pass: bool,
    /// False when the entitlement service could not be asked; the other fields are then unknown.
    pub verified: bool,
}

/// What a session check of a Microsoft account found ([`crate::AuthManager::check_session`]).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum SessionCheck {
    /// The saved session works: the game starts online.
    Valid,
    /// Microsoft refused the saved session, or it is gone: sign in again.
    Expired,
    /// Could not be checked right now (no internet, service outage).
    Unknown,
}

/// Secrets of a Microsoft account. Lives only in the OS keychain (see [`crate::SecretStore`]);
/// `Debug` redacts every token.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AccountSecrets {
    /// Microsoft refresh token. Microsoft may omit it on refresh; the old one is then kept.
    pub refresh_token: Option<String>,
    /// Minecraft access token (about 24 hours).
    pub access_token: String,
    pub expires_at: Timestamp,
    /// Xbox user id, passed to the game as `--xuid`.
    pub xuid: Option<String>,
}

impl AccountSecrets {
    /// True when the access token is expired or expires within `margin`.
    pub fn expires_within(&self, margin: Duration, now: Timestamp) -> bool {
        let margin = SignedDuration::try_from(margin).unwrap_or(SignedDuration::MAX);
        now.checked_add(margin)
            .map_or(true, |deadline| self.expires_at <= deadline)
    }
}

impl fmt::Debug for AccountSecrets {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("AccountSecrets")
            .field(
                "refresh_token",
                &self.refresh_token.as_ref().map(|_| "<redacted>"),
            )
            .field("access_token", &"<redacted>")
            .field("expires_at", &self.expires_at)
            .field("xuid", &self.xuid)
            .finish()
    }
}

/// Steps of the Microsoft sign-in, for the UI's animated step list.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum LoginStep {
    OpeningBrowser,
    WaitingForBrowser,
    Microsoft,
    XboxLive,
    Xsts,
    Minecraft,
    Profile,
    Done,
}

/// What the player needs for the device-code sign-in: open `verification_uri` and type
/// `user_code`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct DeviceCodeInfo {
    pub user_code: String,
    pub verification_uri: String,
    pub expires_at: Timestamp,
    /// Microsoft's own (English) instruction text.
    pub message: Option<String>,
}

/// `--userType` of the game.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum UserType {
    Msa,
    Legacy,
}

impl UserType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Msa => "msa",
            Self::Legacy => "legacy",
        }
    }
}

impl fmt::Display for UserType {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Identity values the game's launch arguments need. The app maps it to pl-mc's type.
/// `Debug` redacts the access token.
#[derive(Clone, PartialEq, Eq)]
pub struct LaunchIdentityData {
    /// `--username`
    pub name: String,
    /// `--uuid` (format it as the target expects; the vanilla launcher uses `uuid.simple()`).
    pub uuid: Uuid,
    /// `--accessToken`
    pub access_token: String,
    /// `--userType`
    pub user_type: UserType,
    /// `--xuid` (Microsoft accounts)
    pub xuid: Option<String>,
    /// `--clientId` (Microsoft accounts)
    pub client_id: Option<String>,
}

impl fmt::Debug for LaunchIdentityData {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("LaunchIdentityData")
            .field("name", &self.name)
            .field("uuid", &self.uuid)
            .field("access_token", &"<redacted>")
            .field("user_type", &self.user_type)
            .field("xuid", &self.xuid)
            .field("client_id", &self.client_id)
            .finish()
    }
}

/// Builds the launch identity. Microsoft accounts need their secrets (expired tokens are used
/// as they are, which is what offline play wants; call [`crate::AuthManager::ensure_fresh`]
/// first for online play). Offline accounts ignore `secrets`.
pub fn launch_identity(
    account: &Account,
    secrets: Option<&AccountSecrets>,
) -> Result<LaunchIdentityData> {
    match account.kind {
        AccountKind::Microsoft => {
            let secrets = secrets.ok_or(AuthError::MissingSecrets { id: account.id })?;
            Ok(LaunchIdentityData {
                name: account.username.clone(),
                uuid: account.uuid,
                access_token: secrets.access_token.clone(),
                user_type: UserType::Msa,
                xuid: secrets.xuid.clone(),
                client_id: Some(CLIENT_ID.to_owned()),
            })
        }
        AccountKind::Offline => Ok(LaunchIdentityData {
            name: account.username.clone(),
            uuid: account.uuid,
            access_token: OFFLINE_ACCESS_TOKEN.to_owned(),
            user_type: UserType::Legacy,
            xuid: None,
            client_id: None,
        }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::offline::offline_uuid;

    fn secrets(expires_at: Timestamp) -> AccountSecrets {
        AccountSecrets {
            refresh_token: Some("dummy-refresh".into()),
            access_token: "dummy-access".into(),
            expires_at,
            xuid: Some("2535400000000000".into()),
        }
    }

    #[test]
    fn expiry_margin() {
        let now = Timestamp::from_second(1_800_000_000).unwrap();
        let margin = Duration::from_secs(600);
        let in_5_min = now.checked_add(SignedDuration::from_mins(5)).unwrap();
        let in_11_min = now.checked_add(SignedDuration::from_mins(11)).unwrap();
        assert!(secrets(now).expires_within(margin, now));
        assert!(secrets(in_5_min).expires_within(margin, now));
        assert!(!secrets(in_11_min).expires_within(margin, now));
    }

    #[test]
    fn secrets_debug_is_redacted() {
        let text = format!("{:?}", secrets(Timestamp::now()));
        assert!(!text.contains("dummy-refresh"));
        assert!(!text.contains("dummy-access"));
        assert!(text.contains("2535400000000000"));
    }

    #[test]
    fn launch_identity_for_both_kinds() {
        let mut account = Account::new(
            AccountKind::Microsoft,
            "Steve",
            Uuid::parse_str("069a79f444e94726a5befca90e38aaf5").unwrap(),
        );
        assert!(matches!(
            launch_identity(&account, None),
            Err(AuthError::MissingSecrets { .. })
        ));
        let identity = launch_identity(&account, Some(&secrets(Timestamp::now()))).unwrap();
        assert_eq!(identity.name, "Steve");
        assert_eq!(identity.user_type.as_str(), "msa");
        assert_eq!(identity.access_token, "dummy-access");
        assert_eq!(identity.xuid.as_deref(), Some("2535400000000000"));
        assert_eq!(identity.client_id.as_deref(), Some(CLIENT_ID));
        assert_eq!(
            identity.uuid.simple().to_string(),
            "069a79f444e94726a5befca90e38aaf5"
        );
        assert!(!format!("{identity:?}").contains("dummy-access"));

        account.kind = AccountKind::Offline;
        account.uuid = offline_uuid("Steve");
        let identity = launch_identity(&account, None).unwrap();
        assert_eq!(identity.user_type, UserType::Legacy);
        assert_eq!(identity.access_token, OFFLINE_ACCESS_TOKEN);
        assert_eq!(identity.xuid, None);
        assert_eq!(identity.client_id, None);
    }

    #[test]
    fn account_json_is_camel_case() {
        let mut account = Account::new(AccountKind::Offline, "Alex", offline_uuid("Alex"));
        account.skin = Some(Skin {
            url: "https://textures.minecraft.net/texture/abc".into(),
            variant: SkinVariant::Slim,
            alias: None,
        });
        let json = serde_json::to_value(&account).unwrap();
        assert_eq!(json["kind"], "offline");
        assert_eq!(json["skin"]["variant"], "slim");
        assert!(json.get("addedAt").is_some());
        assert!(json.get("lastUsedAt").is_some());
        assert_eq!(json["needsRelogin"], false);
        assert_eq!(json["pinned"], false);
        assert_eq!(json["avatar"], serde_json::Value::Null);
        let back: Account = serde_json::from_value(json).unwrap();
        assert_eq!(back, account);

        account.pinned = true;
        account.avatar = Some("preset:golden-dragon".into());
        let json = serde_json::to_value(&account).unwrap();
        assert_eq!(json["pinned"], true);
        assert_eq!(json["avatar"], "preset:golden-dragon");
        let back: Account = serde_json::from_value(json).unwrap();
        assert_eq!(back, account);
    }

    #[test]
    fn preset_avatar_ids() {
        assert!(is_preset_avatar("preset:fox"));
        assert!(is_preset_avatar("preset:golden-dragon"));
        assert!(is_preset_avatar(&format!("preset:{}", "a".repeat(32))));
        assert!(!is_preset_avatar(&format!("preset:{}", "a".repeat(33))));
        assert!(!is_preset_avatar("preset:"));
        assert!(!is_preset_avatar("preset:Fox"));
        assert!(!is_preset_avatar("preset:fox.webp"));
        assert!(!is_preset_avatar("preset:../fox"));
        assert!(!is_preset_avatar("fox"));
        assert!(!is_preset_avatar("discord"));
        assert!(!is_preset_avatar("https://example.com/fox.png"));
    }

    #[test]
    fn accounts_saved_before_the_session_flag_still_load() {
        let account: Account = serde_json::from_value(serde_json::json!({
            "id": "6c3ee5c2-5a2d-4d3e-9a52-3f2f7f0d2b11",
            "kind": "microsoft",
            "username": "Steve",
            "uuid": "069a79f4-44e9-4726-a5be-fca90e38aaf5",
            "addedAt": "2026-10-01T10:00:00Z"
        }))
        .unwrap();
        assert!(!account.needs_relogin);
        assert_eq!(account.last_used_at, None);
        // Pins and pictures came later too.
        assert!(!account.pinned);
        assert_eq!(account.avatar, None);
        assert_eq!(
            serde_json::to_value(SessionCheck::Expired).unwrap(),
            "expired"
        );
    }

    #[test]
    fn skins_saved_before_the_alias_field_still_load() {
        let skin: Skin = serde_json::from_value(serde_json::json!({
            "url": "https://textures.minecraft.net/texture/abc",
            "variant": "classic"
        }))
        .unwrap();
        assert_eq!(skin.alias, None);
        assert!(!skin.is_default());
    }

    #[test]
    fn active_skin_and_variant_parsing() {
        assert_eq!(SkinVariant::from_api("SLIM"), SkinVariant::Slim);
        assert_eq!(SkinVariant::from_api("CLASSIC"), SkinVariant::Classic);
        assert_eq!(SkinVariant::from_api("whatever"), SkinVariant::Classic);
        let profile = MinecraftProfile {
            id: Uuid::nil(),
            name: "Steve".into(),
            skins: vec![
                ProfileSkin {
                    id: "a".into(),
                    url: "https://a".into(),
                    variant: SkinVariant::Classic,
                    active: false,
                    alias: None,
                },
                ProfileSkin {
                    id: "b".into(),
                    url: "https://b".into(),
                    variant: SkinVariant::Slim,
                    active: true,
                    alias: None,
                },
            ],
            capes: Vec::new(),
        };
        assert_eq!(
            profile.active_skin(),
            Some(Skin {
                url: "https://b".into(),
                variant: SkinVariant::Slim,
                alias: None,
            })
        );
        let mut default = profile.clone();
        default.skins[1].alias = Some("ALEX".into());
        assert!(default.active_skin().unwrap().is_default());
    }
}
