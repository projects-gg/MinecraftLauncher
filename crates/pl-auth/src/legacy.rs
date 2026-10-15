//! Windows only: imports the legacy (C#) launcher's premium session so the player does not have
//! to sign in again.
//!
//! The legacy launcher stored `%APPDATA%\.projects\premium.dat`: Base64 (ASCII) of a DPAPI
//! (`CurrentUser` scope, no entropy) encrypted UTF-8 JSON document written by Newtonsoft.Json
//! with the default (PascalCase) names of `PremiumAccount`: `Name`, `Uuid` (32 hex digits),
//! `Xuid`, `AccessToken`, `ExpiresAtUtc` (ISO 8601) and `RefreshToken`. A file copied from
//! another Windows user or PC cannot be decrypted and is reported as unreadable.

use std::path::{Path, PathBuf};

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

use crate::error::{AuthError, AuthStep, Result};
use crate::manager::AuthManager;
use crate::model::{Account, AccountKind, AccountSecrets};

/// Outcome of [`AuthManager::import_legacy_premium`].
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum LegacyImport {
    /// The session is available as a Microsoft account (newly imported, or the account already
    /// existed). No sign-in is needed; the token is refreshed on first use.
    Imported { account: Account },
    /// The legacy launcher has no premium session on this machine.
    NotFound,
    /// A session file exists but cannot be used (another Windows user or PC, damaged file,
    /// missing fields). `reason` is diagnostic text, not for display.
    Unreadable { reason: String },
}

/// `%APPDATA%\.projects\premium.dat`, when `APPDATA` is set.
pub fn legacy_premium_path() -> Option<PathBuf> {
    std::env::var_os("APPDATA")
        .map(|appdata| PathBuf::from(appdata).join(".projects").join("premium.dat"))
}

#[derive(Deserialize)]
#[serde(rename_all = "PascalCase")]
struct LegacySession {
    #[serde(default)]
    name: Option<String>,
    #[serde(default)]
    uuid: Option<String>,
    #[serde(default)]
    xuid: Option<String>,
    #[serde(default)]
    access_token: Option<String>,
    #[serde(default)]
    expires_at_utc: Option<String>,
    #[serde(default)]
    refresh_token: Option<String>,
}

impl AuthManager {
    /// Imports `%APPDATA%\.projects\premium.dat` (see [`legacy_premium_path`]).
    pub async fn import_legacy_premium(&self) -> Result<LegacyImport> {
        match legacy_premium_path() {
            Some(path) => self.import_legacy_premium_from(&path).await,
            None => Ok(LegacyImport::NotFound),
        }
    }

    /// Imports a legacy `premium.dat` from `path`. An account that already exists with stored
    /// secrets is left untouched (its session is newer than the legacy one).
    pub async fn import_legacy_premium_from(&self, path: &Path) -> Result<LegacyImport> {
        let owned = path.to_owned();
        let read = tokio::task::spawn_blocking(move || read_session(&owned))
            .await
            .map_err(|error| AuthError::unexpected(AuthStep::Migration, error.to_string()))?;
        let session = match read {
            Ok(Some(session)) => session,
            Ok(None) => return Ok(LegacyImport::NotFound),
            Err(reason) => {
                tracing::warn!(%reason, "legacy premium session is unreadable");
                return Ok(LegacyImport::Unreadable { reason });
            }
        };

        let name = non_empty(session.name);
        let uuid = session
            .uuid
            .as_deref()
            .and_then(|uuid| Uuid::parse_str(uuid.trim()).ok());
        let refresh_token = non_empty(session.refresh_token);
        let (Some(name), Some(uuid), Some(refresh_token)) = (name, uuid, refresh_token) else {
            return Ok(LegacyImport::Unreadable {
                reason: "missing name, profile id or refresh token".to_owned(),
            });
        };
        let access_token = session.access_token.unwrap_or_default();
        let expires_at = if access_token.is_empty() {
            Timestamp::UNIX_EPOCH
        } else {
            // Unparseable means "expired": the first launch refreshes it.
            session
                .expires_at_utc
                .as_deref()
                .and_then(parse_dotnet_utc)
                .unwrap_or(Timestamp::UNIX_EPOCH)
        };
        let secrets = AccountSecrets {
            refresh_token: Some(refresh_token),
            access_token,
            expires_at,
            xuid: non_empty(session.xuid),
        };

        let _membership = self.membership_lock().await;
        if let Some(existing) = self.store().find_by_profile(AccountKind::Microsoft, uuid) {
            if self.load_secrets(existing.id).await?.is_none() {
                self.save_secrets(existing.id, secrets).await?;
            }
            return Ok(LegacyImport::Imported { account: existing });
        }
        let account = Account::new(AccountKind::Microsoft, name, uuid);
        self.save_secrets(account.id, secrets).await?;
        if let Err(error) = self.store().put(account.clone()) {
            let _ = self.delete_secrets(account.id).await;
            return Err(error);
        }
        if self.store().active_id().is_none() {
            self.store().set_active(Some(account.id))?;
        }
        tracing::info!(account = %account.id, "legacy premium session imported");
        Ok(LegacyImport::Imported { account })
    }
}

fn non_empty(value: Option<String>) -> Option<String> {
    value
        .map(|value| value.trim().to_owned())
        .filter(|value| !value.is_empty())
}

/// `Ok(None)` when there is no file; `Err(reason)` when it cannot be used.
fn read_session(path: &Path) -> std::result::Result<Option<LegacySession>, String> {
    let text = match std::fs::read_to_string(path) {
        Ok(text) => text,
        Err(error) if error.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(error) => return Err(format!("cannot read the file: {error}")),
    };
    let encrypted = STANDARD
        .decode(text.trim())
        .map_err(|error| format!("not Base64: {error}"))?;
    let mut plain = dpapi::unprotect(&encrypted)
        .map_err(|error| format!("DPAPI could not decrypt it (another user or PC?): {error}"))?;
    let parsed = serde_json::from_slice::<LegacySession>(&plain).map_err(|error| {
        // Position only: serde messages can quote input, and the input holds tokens.
        format!(
            "not a session document (JSON error at line {}, column {})",
            error.line(),
            error.column()
        )
    });
    plain.fill(0);
    parsed.map(Some)
}

/// Newtonsoft's ISO 8601 `DateTime`: `2026-10-15T12:34:56.1234567Z`, an offset, or no zone at
/// all (the legacy launcher always wrote UTC values).
fn parse_dotnet_utc(text: &str) -> Option<Timestamp> {
    let text = text.trim();
    if let Ok(timestamp) = text.parse::<Timestamp>() {
        return Some(timestamp);
    }
    let civil: jiff::civil::DateTime = text.parse().ok()?;
    civil
        .to_zoned(jiff::tz::TimeZone::UTC)
        .ok()
        .map(|zoned| zoned.timestamp())
}

mod dpapi {
    use windows::Win32::Foundation::{HLOCAL, LocalFree};
    use windows::Win32::Security::Cryptography::{
        CRYPT_INTEGER_BLOB, CRYPTPROTECT_UI_FORBIDDEN, CryptUnprotectData,
    };

    /// `CryptUnprotectData` for the current user, without entropy or UI (what .NET's
    /// `ProtectedData.Unprotect(data, null, DataProtectionScope.CurrentUser)` does).
    pub(super) fn unprotect(data: &[u8]) -> Result<Vec<u8>, String> {
        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(data.len()).map_err(|_| "input too large".to_owned())?,
            pbData: data.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: `input` points at `data`, which outlives the call and is only read. On
        // success DPAPI allocates `output` with LocalAlloc; it is freed below.
        unsafe {
            CryptUnprotectData(
                &input,
                None,
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
        .map_err(|error| error.message())?;
        Ok(take_blob(output))
    }

    /// Copies a DPAPI output blob, wipes it and frees it.
    fn take_blob(blob: CRYPT_INTEGER_BLOB) -> Vec<u8> {
        if blob.pbData.is_null() {
            return Vec::new();
        }
        let len = blob.cbData as usize;
        // SAFETY: DPAPI returned `cbData` initialised bytes at `pbData`, allocated with
        // LocalAlloc and owned by us from now on; nothing else references them.
        unsafe {
            let bytes = std::slice::from_raw_parts(blob.pbData, len).to_vec();
            std::ptr::write_bytes(blob.pbData, 0, len);
            let _ = LocalFree(Some(HLOCAL(blob.pbData.cast())));
            bytes
        }
    }

    /// The inverse, as the legacy launcher wrote it (tests only).
    #[cfg(test)]
    pub(super) fn protect(data: &[u8]) -> Vec<u8> {
        use windows::Win32::Security::Cryptography::CryptProtectData;
        use windows::core::PCWSTR;

        let input = CRYPT_INTEGER_BLOB {
            cbData: u32::try_from(data.len()).unwrap(),
            pbData: data.as_ptr().cast_mut(),
        };
        let mut output = CRYPT_INTEGER_BLOB::default();
        // SAFETY: as in `unprotect`.
        unsafe {
            CryptProtectData(
                &input,
                PCWSTR::null(),
                None,
                None,
                None,
                CRYPTPROTECT_UI_FORBIDDEN,
                &mut output,
            )
        }
        .unwrap();
        take_blob(output)
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use super::*;
    use crate::config::AuthConfig;
    use crate::microsoft::MicrosoftAuth;
    use crate::secrets::{MemoryStore, SecretStore};
    use crate::store::AccountStore;
    use crate::testutil;

    const SESSION_JSON: &str = r#"{"Name":"Steve","Uuid":"069a79f444e94726a5befca90e38aaf5","Xuid":"2535400000000003","AccessToken":"legacy-access-dummy","ExpiresAtUtc":"2030-01-02T03:04:05.1234567Z","RefreshToken":"legacy-refresh-dummy"}"#;

    fn manager(dir: &Path) -> (AuthManager, Arc<MemoryStore>) {
        let secrets = Arc::new(MemoryStore::new());
        let manager = AuthManager::new(
            MicrosoftAuth::with_config(testutil::http(), AuthConfig::default()),
            AccountStore::open(dir.join("accounts.json")).unwrap(),
            secrets.clone(),
        );
        (manager, secrets)
    }

    fn write_premium_dat(dir: &Path, json: &str) -> PathBuf {
        let path = dir.join("premium.dat");
        let blob = STANDARD.encode(dpapi::protect(json.as_bytes()));
        std::fs::write(&path, blob).unwrap();
        path
    }

    #[test]
    fn dpapi_round_trip() {
        let encrypted = dpapi::protect(b"dummy payload");
        assert_ne!(encrypted, b"dummy payload");
        assert_eq!(dpapi::unprotect(&encrypted).unwrap(), b"dummy payload");
        assert!(dpapi::unprotect(b"definitely not a DPAPI blob").is_err());
    }

    #[tokio::test]
    async fn imports_a_dpapi_protected_session() {
        let dir = tempfile::tempdir().unwrap();
        let path = write_premium_dat(dir.path(), SESSION_JSON);
        let (manager, secrets) = manager(dir.path());

        let LegacyImport::Imported { account } =
            manager.import_legacy_premium_from(&path).await.unwrap()
        else {
            panic!("expected an import");
        };
        assert_eq!(account.kind, AccountKind::Microsoft);
        assert_eq!(account.username, "Steve");
        assert_eq!(
            account.uuid.simple().to_string(),
            "069a79f444e94726a5befca90e38aaf5"
        );
        assert_eq!(manager.active_account().unwrap().id, account.id);

        let stored = secrets.load(account.id).unwrap().unwrap();
        assert_eq!(
            stored.refresh_token.as_deref(),
            Some("legacy-refresh-dummy")
        );
        assert_eq!(stored.access_token, "legacy-access-dummy");
        assert_eq!(stored.xuid.as_deref(), Some("2535400000000003"));
        assert_eq!(
            stored.expires_at,
            "2030-01-02T03:04:05.1234567Z".parse::<Timestamp>().unwrap()
        );

        // Importing again keeps the existing account and its (newer) secrets.
        let mut newer = stored.clone();
        newer.access_token = "newer-access".into();
        secrets.save(account.id, &newer).unwrap();
        let again = manager.import_legacy_premium_from(&path).await.unwrap();
        assert_eq!(
            again,
            LegacyImport::Imported {
                account: account.clone()
            }
        );
        assert_eq!(
            secrets.load(account.id).unwrap().unwrap().access_token,
            "newer-access"
        );
        assert_eq!(manager.accounts().len(), 1);
    }

    #[tokio::test]
    async fn missing_and_unusable_sessions() {
        let dir = tempfile::tempdir().unwrap();
        let (manager, secrets) = manager(dir.path());
        assert_eq!(
            manager
                .import_legacy_premium_from(&dir.path().join("absent.dat"))
                .await
                .unwrap(),
            LegacyImport::NotFound
        );

        let garbage = dir.path().join("garbage.dat");
        std::fs::write(&garbage, "%%% not base64 %%%").unwrap();
        assert!(matches!(
            manager.import_legacy_premium_from(&garbage).await.unwrap(),
            LegacyImport::Unreadable { .. }
        ));

        let foreign = dir.path().join("foreign.dat");
        std::fs::write(&foreign, STANDARD.encode(b"encrypted on another PC")).unwrap();
        let LegacyImport::Unreadable { reason } =
            manager.import_legacy_premium_from(&foreign).await.unwrap()
        else {
            panic!("expected unreadable");
        };
        assert!(reason.contains("DPAPI"));

        let incomplete = write_premium_dat(dir.path(), r#"{"Name":"Steve","RefreshToken":""}"#);
        assert!(matches!(
            manager
                .import_legacy_premium_from(&incomplete)
                .await
                .unwrap(),
            LegacyImport::Unreadable { .. }
        ));
        let not_json = write_premium_dat(dir.path(), "secret-looking-but-not-json");
        let LegacyImport::Unreadable { reason } =
            manager.import_legacy_premium_from(&not_json).await.unwrap()
        else {
            panic!("expected unreadable");
        };
        assert!(!reason.contains("secret-looking"));
        assert!(manager.accounts().is_empty());
        assert!(secrets.is_empty());
    }

    #[test]
    fn dotnet_dates() {
        let expected = "2030-01-02T03:04:05Z".parse::<Timestamp>().unwrap();
        assert_eq!(parse_dotnet_utc("2030-01-02T03:04:05Z"), Some(expected));
        assert_eq!(parse_dotnet_utc("2030-01-02T03:04:05"), Some(expected));
        assert_eq!(
            parse_dotnet_utc("2030-01-02T06:04:05+03:00"),
            Some(expected)
        );
        assert!(parse_dotnet_utc("2030-01-02T03:04:05.1234567Z").is_some());
        assert_eq!(parse_dotnet_utc("yesterday"), None);
    }

    #[test]
    fn legacy_path_points_into_appdata() {
        if let Some(path) = legacy_premium_path() {
            assert!(
                path.ends_with(".projects/premium.dat") || path.ends_with(r".projects\premium.dat")
            );
        }
    }
}
