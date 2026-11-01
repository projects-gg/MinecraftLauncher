//! SOC-ID: the ways into a Projects account ("Bağlı hesaplar"). An account opens with Discord or
//! with Microsoft (switched on from Studio); the player links the other one or removes one of two
//! in Arkadaşlar → Ayarlar, and an account opened with Microsoft picks its profile name.
//!
//! - `GET /v1/auth/providers` (no session) says which ways sign in now. A service from before
//!   SOC-ID answers `404`: Discord only. The launcher asks when the friends panel opens, at most
//!   once a minute ([`crate::SocialService::refresh_providers`]).
//! - Linking and removing go through the browser like the sign-in: `POST /v1/me/identities/link`
//!   or `…/unlink` with `{provider, challenge}` answer the page to open (`{url}`; a removal whose
//!   proof is fresh is done at once, `{me}`). The page ends in `projects://auth/done?c=<code>`, and
//!   while a change waits that link is claimed with `POST /v1/me/identities/claim
//!   {verifier, code}` instead of a sign-in. The service writes the change only then, so a page
//!   someone else started is useless without this launcher's verifier.
//! - `PATCH /v1/me {name}`: the profile name ([`clean_name`]; the service filters it and allows
//!   three changes a day).
//!
//! Older services: a profile without the new fields reads as an account opened with Discord (its
//! number is its Discord id, signed in and named by Discord), and so does a card without
//! `discordId`.

use serde::{Deserialize, Serialize};
use url::Url;

use crate::error::{ErrorCode, SocialError};
use crate::model::Me;

/// A way into a Projects account.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum Provider {
    Discord,
    Microsoft,
}

impl Provider {
    /// Every way this version knows, in the order the screens list them.
    pub const ALL: [Self; 2] = [Self::Discord, Self::Microsoft];

    pub fn as_wire(self) -> &'static str {
        match self {
            Self::Discord => "discord",
            Self::Microsoft => "microsoft",
        }
    }

    /// None for a way this version does not know.
    pub fn parse(value: &str) -> Option<Self> {
        Self::ALL
            .into_iter()
            .find(|provider| provider.as_wire() == value)
    }

    /// The other way (there are two): the one whose page proves a removal.
    pub fn other(self) -> Self {
        match self {
            Self::Discord => Self::Microsoft,
            Self::Microsoft => Self::Discord,
        }
    }
}

impl<'de> Deserialize<'de> for Provider {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let value = String::deserialize(deserializer)?;
        Self::parse(&value).ok_or_else(|| serde::de::Error::custom("unknown provider"))
    }
}

/// A way this version does not know (or no text at all) reads as none.
pub(crate) fn readable_provider<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Provider>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .and_then(Provider::parse))
}

/// Where the profile name comes from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum NameSource {
    /// The Discord account's name (every account opened with Discord, and an older service).
    #[default]
    Discord,
    /// The player chose it.
    Own,
    /// The placeholder of an account opened with Microsoft ("Oyuncu" and four digits): the
    /// launcher asks for a name right after the sign-in.
    Unset,
}

/// A source this version does not know reads as none (no name step).
pub(crate) fn readable_name_source<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<NameSource>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(
        match raw.as_ref().and_then(serde_json::Value::as_str) {
            Some("discord") => Some(NameSource::Discord),
            Some("own") => Some(NameSource::Own),
            Some("unset") => Some(NameSource::Unset),
            _ => None,
        },
    )
}

/// Why an account is limited ("Sınırlı hesap", `me.limited.reason`): its Discord account is new
/// (every account opened with Discord, and an older service), or the account itself is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, specta::Type)]
#[serde(rename_all = "lowercase")]
pub enum LimitedReason {
    #[default]
    Discord,
    Account,
}

impl LimitedReason {
    pub fn parse(value: &str) -> Self {
        match value {
            "account" => Self::Account,
            _ => Self::Discord,
        }
    }
}

pub(crate) fn readable_reason<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<LimitedReason, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw
        .as_ref()
        .and_then(serde_json::Value::as_str)
        .map(LimitedReason::parse)
        .unwrap_or_default())
}

/// One way into the account (`me.identities`), as "Bağlı hesaplar" lists it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct Identity {
    pub provider: Provider,
    /// `YYYY-MM-DD`
    pub linked_on: String,
    /// Linked in the last 7 days ("Yeni").
    #[serde(default)]
    pub is_new: bool,
    /// Discord: the Discord username; Microsoft: none.
    #[serde(default)]
    pub hint: Option<String>,
}

/// The list as far as this version can read it: a way it does not know is left out, and anything
/// but a list is no list (an older service's profile).
pub(crate) fn readable_identities<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Vec<Identity>>, D::Error> {
    let raw = Option::<serde_json::Value>::deserialize(deserializer)?;
    Ok(raw.and_then(|value| match value {
        serde_json::Value::Array(items) => Some(
            items
                .into_iter()
                .filter_map(|item| serde_json::from_value(item).ok())
                .collect(),
        ),
        _ => None,
    }))
}

/// A Discord account's id as the cards carry it (`discordId`): missing, `None` (the outer
/// `Option`): the account number is the Discord id; `null`: no Discord; digits: that account. A
/// value that is not a Discord id reads as no Discord, so nothing is ever built from it.
pub(crate) fn readable_discord<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<Option<String>>, D::Error> {
    let raw = serde_json::Value::deserialize(deserializer)?;
    Ok(Some(match raw {
        serde_json::Value::String(id) if is_discord_id(&id) => Some(id),
        _ => None,
    }))
}

/// 17 to 20 digits: a Discord snowflake.
pub(crate) fn is_discord_id(id: &str) -> bool {
    (17..=20).contains(&id.len()) && id.bytes().all(|b| b.is_ascii_digit())
}

/// What the service says of one way in (`GET /v1/auth/providers`).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ProviderInfo {
    pub provider: Provider,
    /// Accounts sign in with it now.
    pub sign_in: bool,
    /// A first sign-in with it opens a new account.
    pub sign_up: bool,
}

/// `GET /v1/auth/providers`' answer; a way this version does not know is left out.
pub(crate) fn providers_of(body: &[u8]) -> serde_json::Result<Vec<ProviderInfo>> {
    #[derive(Deserialize)]
    struct Wire {
        providers: Vec<serde_json::Value>,
    }
    #[derive(Deserialize)]
    #[serde(rename_all = "camelCase")]
    struct One {
        id: String,
        #[serde(default)]
        sign_in: bool,
        #[serde(default)]
        sign_up: bool,
    }
    let wire: Wire = serde_json::from_slice(body)?;
    Ok(wire
        .providers
        .into_iter()
        .filter_map(|value| serde_json::from_value::<One>(value).ok())
        .filter_map(|one| {
            Some(ProviderInfo {
                provider: Provider::parse(&one.id)?,
                sign_in: one.sign_in,
                sign_up: one.sign_up,
            })
        })
        .collect())
}

/// The ways the sign-in screen offers: Discord always (every account opened with it signs in
/// with it), then each other way whose sign-in is on.
pub fn sign_in_ways(list: &[ProviderInfo]) -> Vec<Provider> {
    Provider::ALL
        .into_iter()
        .filter(|provider| {
            *provider == Provider::Discord
                || list
                    .iter()
                    .any(|info| info.provider == *provider && info.sign_in)
        })
        .collect()
}

/// The answer to a removal: the page that proves it, or the profile when it is done at once.
#[derive(Debug, Clone)]
pub enum IdentityAnswer {
    Browser(Url),
    Done(Box<Me>),
}

/// What a link or a removal needs next ("Hesap bağla", "Kaldır").
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum IdentityStep {
    /// The browser is open on this page; the change finishes when its `projects://auth/done`
    /// comes back (the view's `identityChange` waits meanwhile).
    Browser { url: String },
    /// Done at once (a removal with a fresh proof): the profile shows it.
    Done,
    /// Refused, and why.
    Refused { reason: IdentityFailure },
}

/// Why a link or a removal did not go through; the launcher words it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum IdentityFailure {
    /// That way is switched off right now (`provider_off`).
    ProviderOff,
    /// It is linked already (`identity_exists`).
    Exists,
    /// It is not linked (`identity_not_found`).
    NotFound,
    /// The only way into the account stays (`last_identity`).
    Last,
    /// That account belongs to another Projects account (`identity_taken`).
    Taken,
    /// The proof with a linked account did not hold (`proof_failed`).
    ProofFailed,
    /// That account cannot be linked now (`identity_unavailable`).
    Unavailable,
    /// The browser step took too long, or its link was used already: start again.
    Expired,
    /// Five changes a day (`rate_limited`, item `identity_changes`).
    TooMany,
    /// The service could not be reached.
    Offline,
    Failed,
}

impl IdentityFailure {
    pub fn of(error: &SocialError) -> Self {
        match error {
            SocialError::RateLimited { .. } => Self::TooMany,
            SocialError::Unavailable { .. } | SocialError::Network(_) => Self::Offline,
            _ => match error.code() {
                Some(ErrorCode::ProviderOff) => Self::ProviderOff,
                Some(ErrorCode::IdentityExists) => Self::Exists,
                Some(ErrorCode::IdentityNotFound) => Self::NotFound,
                Some(ErrorCode::LastIdentity) => Self::Last,
                Some(ErrorCode::IdentityTaken) => Self::Taken,
                Some(ErrorCode::ProofFailed) => Self::ProofFailed,
                Some(ErrorCode::IdentityUnavailable) => Self::Unavailable,
                Some(ErrorCode::AuthPendingNotFound) => Self::Expired,
                _ => Self::Failed,
            },
        }
    }
}

/// A link or a removal waiting for its browser step, as the settings show it.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct IdentityChangeView {
    pub provider: Provider,
    /// A removal (its proof is another linked way); otherwise a link.
    pub unlink: bool,
    /// The page the browser shows ("Tarayıcıyı yeniden aç").
    pub url: String,
}

/// What became of a new profile name.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum NameResult {
    Saved,
    /// Not 3–16 letters, digits, spaces or `_` with two letters or digits (`invalid_name`).
    Invalid,
    /// The service's filter turned it away (`name_rejected`).
    Rejected,
    /// Three changes a day (`rate_limited`, item `name_changes`).
    TooMany,
    /// The service could not be reached.
    Offline,
    Failed,
}

impl NameResult {
    pub fn of(error: &SocialError) -> Self {
        match error {
            SocialError::RateLimited { .. } => Self::TooMany,
            SocialError::Unavailable { .. } | SocialError::Network(_) => Self::Offline,
            SocialError::InvalidInput("name") => Self::Invalid,
            _ => match error.code() {
                Some(ErrorCode::InvalidName | ErrorCode::BadRequest) => Self::Invalid,
                Some(ErrorCode::NameRejected) => Self::Rejected,
                _ => Self::Failed,
            },
        }
    }
}

/// Turkish letters a name may hold besides `a`–`z` (and the circumflexed ones of Turkish words).
const TURKISH_LETTERS: &str = "çğıöşüâîûÇĞİÖŞÜÂÎÛ";

fn name_letter(c: char) -> bool {
    c.is_ascii_alphanumeric() || TURKISH_LETTERS.contains(c)
}

/// The profile name as the service takes it, or None: 3–16 characters of Latin and Turkish
/// letters, digits, spaces and `_`, with at least two letters or digits. Spaces at the ends go and
/// a run of them becomes one.
pub fn clean_name(name: &str) -> Option<String> {
    let cleaned = name.split(' ').filter(|part| !part.is_empty()).collect::<Vec<_>>().join(" ");
    let length = cleaned.chars().count();
    let allowed = cleaned
        .chars()
        .all(|c| c == ' ' || c == '_' || name_letter(c));
    let letters = cleaned.chars().filter(|c| name_letter(*c)).count();
    ((3..=16).contains(&length) && allowed && letters >= 2).then_some(cleaned)
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use super::*;
    use crate::model::{Limited, ModNotice, NoticeKind, PublicProfile, SessionInfo};

    const OLD_ME: &str = r#"{"id":"123456789012345678","username":"ayse","displayName":"Ayşe",
        "avatar":"discord","avatarHash":"abc","discordAvatarHash":"abc","friendCode":"PRJ-ABCD-EFGH",
        "status":"online","privacy":{"shareServer":false,"shareLastSeen":true},
        "createdOn":"2026-10-16","settings":null}"#;

    #[test]
    fn an_older_profile_reads_as_an_account_opened_with_discord() {
        let me: Me = serde_json::from_str(OLD_ME).unwrap();
        assert_eq!(me.discord_id(), Some("123456789012345678"));
        assert!(me.discord_is_account());
        assert_eq!(me.sign_in_via(), Provider::Discord);
        assert_eq!(me.name_source(), NameSource::Discord);
        assert_eq!(
            me.identities(),
            vec![Identity {
                provider: Provider::Discord,
                linked_on: "2026-10-16".into(),
                is_new: false,
                hint: Some("ayse".into()),
            }]
        );
    }

    #[test]
    fn a_microsoft_account_without_discord() {
        let me: Me = serde_json::from_str(
            r#"{"id":"9123456789012345678","username":"oyuncu4821","displayName":"Oyuncu4821",
            "avatar":"preset:fox","avatarHash":null,"discordAvatarHash":null,"discordId":null,
            "friendCode":"PRJ-ABCD-EFGH","status":"online",
            "privacy":{"shareServer":false,"shareLastSeen":true},"createdOn":"2026-11-01",
            "settings":null,"identities":[{"provider":"microsoft","linkedOn":"2026-11-01",
            "isNew":true,"hint":null},{"provider":"steam","linkedOn":"2026-11-01","isNew":true}],
            "signInVia":"microsoft","nameSource":"unset",
            "limited":{"until":1793000000,"items":[],"reason":"account"}}"#,
        )
        .unwrap();
        assert_eq!(me.discord_id(), None);
        assert!(!me.discord_is_account());
        assert_eq!(me.sign_in_via(), Provider::Microsoft);
        assert_eq!(me.name_source(), NameSource::Unset);
        // The way this version does not know is left out.
        assert_eq!(me.identities().len(), 1);
        assert_eq!(me.identities()[0].provider, Provider::Microsoft);
        assert!(me.identities()[0].is_new);
        assert_eq!(
            me.limited.as_ref().map(|limited| limited.reason),
            Some(LimitedReason::Account)
        );
        assert_eq!(me.profile.discord_avatar_url(128), None);
        assert_eq!(me.profile.discord_profile_url(), None);
    }

    #[test]
    fn a_microsoft_account_with_a_discord_of_its_own() {
        let me: Me = serde_json::from_str(
            r#"{"id":"9123456789012345678","username":"gecekurdu","displayName":null,
            "avatar":"discord","avatarHash":"a_beef","discordAvatarHash":"a_beef",
            "discordId":"80351110224678912","friendCode":"PRJ-ABCD-EFGH","status":"online",
            "privacy":{"shareServer":false,"shareLastSeen":true},"createdOn":"2026-11-01",
            "settings":null,"identities":[{"provider":"discord","linkedOn":"2026-11-01",
            "isNew":true,"hint":"gecekurdu"},{"provider":"microsoft","linkedOn":"2026-11-01",
            "isNew":true,"hint":null}],"signInVia":"discord","nameSource":"own"}"#,
        )
        .unwrap();
        assert_eq!(me.discord_id(), Some("80351110224678912"));
        assert!(!me.discord_is_account());
        assert_eq!(
            me.profile.discord_avatar_url(128).as_deref(),
            Some("https://cdn.discordapp.com/avatars/80351110224678912/a_beef.gif?size=128")
        );
        assert_eq!(
            me.profile.discord_profile_url().as_deref(),
            Some("https://discord.com/users/80351110224678912")
        );
        // An older limited block reads as a new Discord account.
        let limited: Limited = serde_json::from_str(r#"{"until":5,"items":[]}"#).unwrap();
        assert_eq!(limited.reason, LimitedReason::Discord);
    }

    #[test]
    fn a_card_tells_a_missing_discord_from_none() {
        let card = |extra: &str| -> PublicProfile {
            serde_json::from_str(&format!(
                r#"{{"id":"223456789012345678","username":"bora","displayName":null,
                "avatar":"discord","avatarHash":null{extra}}}"#
            ))
            .unwrap()
        };
        // Missing: every account opened with Discord, and an older service.
        let old = card("");
        assert_eq!(old.discord_id(), Some("223456789012345678"));
        assert!(old.discord_avatar_url(64).is_some());
        // null: no Discord at all; no picture and no profile are made up.
        let none = card(r#","discordId":null"#);
        assert_eq!(none.discord_id(), None);
        assert_eq!(none.discord_avatar_url(64), None);
        assert_eq!(none.discord_profile_url(), None);
        // Another Discord account.
        let other = card(r#","discordId":"80351110224678912""#);
        assert_eq!(other.discord_id(), Some("80351110224678912"));
        // Not a Discord id: nothing is built from it.
        let odd = card(r#","discordId":"javascript:alert(1)""#);
        assert_eq!(odd.discord_id(), None);
        let number = card(r#","discordId":80351110224678912"#);
        assert_eq!(number.discord_id(), None);
        // Written back as it came (a kept copy reads the same).
        for profile in [old, none, other] {
            let again: PublicProfile =
                serde_json::from_str(&serde_json::to_string(&profile).unwrap()).unwrap();
            assert_eq!(again, profile);
        }
    }

    #[test]
    fn sessions_and_notices_name_their_way() {
        let session: SessionInfo = serde_json::from_str(
            r#"{"id":"0123456789abcdef","device":"PC","createdOn":"2026-11-01",
            "lastUsedOn":"2026-11-01","current":false,"via":"microsoft"}"#,
        )
        .unwrap();
        assert_eq!(session.via, Some(Provider::Microsoft));
        let old: SessionInfo = serde_json::from_str(
            r#"{"id":"0123456789abcdef","device":"PC","createdOn":"2026-11-01",
            "lastUsedOn":"2026-11-01","current":true,"via":"steam"}"#,
        )
        .unwrap();
        assert_eq!(old.via, None);

        let linked: ModNotice = serde_json::from_str(
            r#"{"id":"n1","kind":"identity_linked","at":1,"provider":"microsoft"}"#,
        )
        .unwrap();
        assert_eq!(linked.kind, NoticeKind::IdentityLinked);
        assert_eq!(linked.provider, Some(Provider::Microsoft));
        let removed: ModNotice =
            serde_json::from_str(r#"{"id":"n2","kind":"identity_removed","at":1}"#).unwrap();
        assert_eq!(removed.kind, NoticeKind::IdentityRemoved);
        assert_eq!(removed.provider, None);
    }

    #[test]
    fn the_ways_in_offer_discord_always() {
        let list = providers_of(
            br#"{"providers":[{"id":"discord","signIn":true,"signUp":true},
            {"id":"microsoft","signIn":false,"signUp":false},{"id":"steam","signIn":true},
            {"nonsense":1}]}"#,
        )
        .unwrap();
        assert_eq!(list.len(), 2);
        assert_eq!(sign_in_ways(&list), vec![Provider::Discord]);
        let on = providers_of(
            br#"{"providers":[{"id":"microsoft","signIn":true,"signUp":false}]}"#,
        )
        .unwrap();
        assert!(on[0].sign_in && !on[0].sign_up);
        assert_eq!(
            sign_in_ways(&on),
            vec![Provider::Discord, Provider::Microsoft]
        );
        assert_eq!(sign_in_ways(&[]), vec![Provider::Discord]);
        assert!(providers_of(b"[]").is_err());
        assert_eq!(Provider::Discord.other(), Provider::Microsoft);
        assert_eq!(
            serde_json::to_string(&Provider::Microsoft).unwrap(),
            r#""microsoft""#
        );
        assert!(serde_json::from_str::<Provider>(r#""steam""#).is_err());
    }

    #[test]
    fn identity_refusals_keep_their_reason() {
        let refused = |status: u16, code: &str| {
            crate::error::from_response(
                status,
                None,
                format!(r#"{{"error":{{"code":"{code}","message":"x"}}}}"#).as_bytes(),
            )
        };
        for (status, code, reason) in [
            (403, "provider_off", IdentityFailure::ProviderOff),
            (409, "identity_exists", IdentityFailure::Exists),
            (404, "identity_not_found", IdentityFailure::NotFound),
            (409, "last_identity", IdentityFailure::Last),
            (409, "identity_taken", IdentityFailure::Taken),
            (403, "proof_failed", IdentityFailure::ProofFailed),
            (403, "identity_unavailable", IdentityFailure::Unavailable),
            (404, "auth_pending_not_found", IdentityFailure::Expired),
            (429, "rate_limited", IdentityFailure::TooMany),
            (503, "unavailable", IdentityFailure::Offline),
            (400, "bad_request", IdentityFailure::Failed),
        ] {
            assert_eq!(IdentityFailure::of(&refused(status, code)), reason, "{code}");
        }
        assert_eq!(
            IdentityFailure::of(&SocialError::Network("down".into())),
            IdentityFailure::Offline
        );
        assert_eq!(
            NameResult::of(&refused(400, "invalid_name")),
            NameResult::Invalid
        );
        assert_eq!(
            NameResult::of(&refused(422, "name_rejected")),
            NameResult::Rejected
        );
        assert_eq!(
            NameResult::of(&SocialError::RateLimited {
                retry_after: Some(Duration::from_secs(60)),
                item: None,
            }),
            NameResult::TooMany
        );
        assert_eq!(
            NameResult::of(&refused(500, "internal")),
            NameResult::Offline
        );
    }

    #[test]
    fn names_follow_the_services_rules() {
        assert_eq!(clean_name("  Gece   Kurdu "), Some("Gece Kurdu".into()));
        assert_eq!(clean_name("ÇılgınÖğrenci_7"), Some("ÇılgınÖğrenci_7".into()));
        assert_eq!(clean_name("Kâmil"), Some("Kâmil".into()));
        assert_eq!(clean_name("ab"), None);
        assert_eq!(clean_name("a_ _"), None);
        assert_eq!(clean_name("_a_"), None);
        assert_eq!(clean_name("x1_"), Some("x1_".into()));
        assert_eq!(clean_name("abcdefghijklmnopq"), None);
        assert_eq!(clean_name("abcdefghijklmnop"), Some("abcdefghijklmnop".into()));
        assert_eq!(clean_name("Ayşe!"), None);
        assert_eq!(clean_name("tab\tname"), None);
        assert_eq!(clean_name("José"), None);
    }
}
