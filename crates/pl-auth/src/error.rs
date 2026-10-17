//! Errors of the account flows.
//!
//! Every [`AuthError`] carries three faces:
//! - [`AuthError::code`]: a short, stable code in the legacy launcher's style (`XSTS-2148916233`,
//!   `MC-APPREG`, `AG-BAGLANTI`) for support conversations;
//! - [`AuthError::user_message_tr`]: friendly Turkish text telling the player what to do, without
//!   codes or other technical identifiers;
//! - [`AuthError::details`]: multi-line diagnostics for the "Ayrıntıları kopyala" button (code,
//!   step, HTTP status, time). Server snippets are sanitized and token-like strings are redacted.

use std::fmt::{self, Write as _};
use std::path::PathBuf;

use jiff::Timestamp;
use serde::{Deserialize, Serialize};
use uuid::Uuid;

pub type Result<T, E = AuthError> = std::result::Result<T, E>;

/// Where in the account flows a failure happened (diagnostics only, never shown as UI text).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum AuthStep {
    Browser,
    /// Authorization-code exchange at the Microsoft token endpoint.
    Microsoft,
    /// Refresh-token grant at the Microsoft token endpoint.
    Refresh,
    DeviceCode,
    XboxLive,
    Xsts,
    Minecraft,
    Entitlements,
    Profile,
    Skin,
    Cape,
    Texture,
    Keychain,
    Storage,
    Migration,
    Offline,
}

impl AuthStep {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Browser => "browser",
            Self::Microsoft => "microsoft-token",
            Self::Refresh => "microsoft-refresh",
            Self::DeviceCode => "device-code",
            Self::XboxLive => "xbox-live",
            Self::Xsts => "xsts",
            Self::Minecraft => "minecraft-login",
            Self::Entitlements => "entitlements",
            Self::Profile => "profile",
            Self::Skin => "skin",
            Self::Cape => "cape",
            Self::Texture => "texture",
            Self::Keychain => "keychain",
            Self::Storage => "storage",
            Self::Migration => "migration",
            Self::Offline => "offline",
        }
    }
}

impl fmt::Display for AuthStep {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// How the caller should react to a failure, mainly of [`crate::AuthManager::ensure_fresh`].
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum FailureClass {
    /// The stored session is unusable (revoked or expired refresh token, missing secrets):
    /// ask the player to sign in again.
    NeedsRelogin,
    /// Network or service outage. The caller may still launch with the cached token for offline
    /// play; online-mode servers will reject it.
    Offline,
    /// Anything else: show the message, allow a retry.
    Other,
}

/// Why an offline player name was rejected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub enum OfflineNameProblem {
    Empty,
    TooShort,
    TooLong,
    InvalidCharacters,
    /// Another offline account already uses this name (ignoring case).
    Taken,
}

impl OfflineNameProblem {
    pub fn user_message_tr(self) -> &'static str {
        match self {
            Self::Empty => "Bir oyuncu adı yaz.",
            Self::TooShort => "Oyuncu adı en az 3 karakter olmalı.",
            Self::TooLong => "Oyuncu adı en fazla 16 karakter olabilir.",
            Self::InvalidCharacters => {
                "Oyuncu adında yalnızca İngilizce harf, rakam ve alt çizgi (_) kullanılabilir. \
                 Boşluk ve Türkçe harf kullanılamaz."
            }
            Self::Taken => "Bu adla bir hesabın zaten var.",
        }
    }
}

/// Why a skin was rejected before it was sent to Mojang.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SkinFileProblem {
    NotPng,
    BadDimensions {
        width: u32,
        height: u32,
    },
    TooLarge,
    /// "Set skin by URL" with something that is not an http(s) URL.
    InvalidUrl,
    /// The chosen file could not be read (missing, a folder, no permission).
    Unreadable {
        kind: std::io::ErrorKind,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum AuthError {
    #[error("operation cancelled")]
    Cancelled,

    #[error("timed out waiting for the browser sign-in")]
    BrowserTimeout { at: Timestamp },

    #[error("could not start the local sign-in listener: {message}")]
    Loopback { message: String, at: Timestamp },

    #[error("sign-in response state did not match the request")]
    StateMismatch { at: Timestamp },

    #[error("sign-in response carried no authorization code")]
    MissingCode { at: Timestamp },

    #[error("Microsoft authorization failed: {error}")]
    Authorize {
        error: String,
        description: Option<String>,
        at: Timestamp,
    },

    #[error("Microsoft token request failed with HTTP {status} ({})", error.as_deref().unwrap_or("no error code"))]
    MicrosoftToken {
        step: AuthStep,
        status: u16,
        error: Option<String>,
        detail: String,
        at: Timestamp,
    },

    #[error("no Microsoft refresh token is stored")]
    NoRefreshToken { at: Timestamp },

    #[error("the device code expired before sign-in completed")]
    DeviceCodeExpired { at: Timestamp },

    #[error("the device code sign-in was declined")]
    DeviceCodeDeclined { at: Timestamp },

    #[error("Xbox Live authentication failed with HTTP {status}")]
    XboxLive {
        status: u16,
        detail: String,
        at: Timestamp,
    },

    #[error("XSTS authorization failed with HTTP {status} (XErr {xerr:?})")]
    Xsts {
        status: u16,
        xerr: Option<u64>,
        detail: String,
        at: Timestamp,
    },

    #[error("Minecraft login failed with HTTP {status}")]
    MinecraftLogin {
        status: u16,
        /// Mojang rejected the launcher's Azure app id ("Invalid app registration").
        app_registration: bool,
        detail: String,
        at: Timestamp,
    },

    #[error("this Microsoft account does not own Minecraft: Java Edition")]
    NoLicense { at: Timestamp },

    #[error("this account owns Minecraft but has no Java Edition profile yet")]
    NoProfile { at: Timestamp },

    #[error("Minecraft Services request failed at {step} with HTTP {status}")]
    MinecraftApi {
        step: AuthStep,
        status: u16,
        detail: String,
        at: Timestamp,
    },

    #[error("invalid skin file: {problem:?}")]
    InvalidSkinFile { problem: SkinFileProblem },

    #[error("the skin is not in the account's skin history")]
    NotInSkinHistory,

    #[error("network error at {step}: {message}")]
    Network {
        step: AuthStep,
        message: String,
        at: Timestamp,
    },

    #[error("unexpected response at {step}: {detail}")]
    UnexpectedResponse {
        step: AuthStep,
        detail: String,
        at: Timestamp,
    },

    #[error("invalid offline player name: {problem:?}")]
    InvalidOfflineName { problem: OfflineNameProblem },

    #[error("account {id} not found")]
    AccountNotFound { id: Uuid },

    #[error("account {id} is not a Microsoft account")]
    NotMicrosoftAccount { id: Uuid },

    #[error("no stored session for account {id}")]
    MissingSecrets { id: Uuid },

    #[error("keychain {op} failed: {message}")]
    Keychain {
        op: &'static str,
        /// The keychain itself is missing or locked (no Secret Service, locked keychain).
        unavailable: bool,
        message: String,
        at: Timestamp,
    },

    #[error("could not {op} {}: {source}", path.display())]
    Storage {
        op: &'static str,
        path: PathBuf,
        #[source]
        source: std::io::Error,
        at: Timestamp,
    },
}

impl AuthError {
    // --- constructors used across the crate ---

    pub(crate) fn network(step: AuthStep, message: impl Into<String>) -> Self {
        Self::Network {
            step,
            message: sanitize(&message.into()),
            at: Timestamp::now(),
        }
    }

    pub(crate) fn unexpected(step: AuthStep, detail: impl Into<String>) -> Self {
        Self::UnexpectedResponse {
            step,
            detail: sanitize(&detail.into()),
            at: Timestamp::now(),
        }
    }

    pub(crate) fn storage(
        op: &'static str,
        path: impl Into<PathBuf>,
        source: std::io::Error,
    ) -> Self {
        Self::Storage {
            op,
            path: path.into(),
            source,
            at: Timestamp::now(),
        }
    }

    pub(crate) fn keychain(
        op: &'static str,
        unavailable: bool,
        message: impl Into<String>,
    ) -> Self {
        Self::Keychain {
            op,
            unavailable,
            message: sanitize(&message.into()),
            at: Timestamp::now(),
        }
    }

    pub fn is_cancelled(&self) -> bool {
        matches!(self, Self::Cancelled)
    }

    /// Short code for support, in the legacy launcher's style. Never shown as UI text; it goes
    /// into [`AuthError::details`].
    pub fn code(&self) -> String {
        match self {
            Self::Cancelled => "IPTAL".to_owned(),
            Self::BrowserTimeout { .. } => "MSA-ZAMANASIMI".to_owned(),
            Self::Loopback { .. } => "AG-YEREL".to_owned(),
            Self::StateMismatch { .. } => "MSA-STATE".to_owned(),
            Self::MissingCode { .. } => "MSA-NOCODE".to_owned(),
            Self::Authorize { error, .. } => format!("MSA-{}", code_tag(error)),
            Self::MicrosoftToken { error, status, .. } => match error {
                Some(error) => format!("MSA-{}", code_tag(error)),
                None => format!("MSA-{status}"),
            },
            Self::NoRefreshToken { .. } => "MSA-NOREFRESH".to_owned(),
            Self::DeviceCodeExpired { .. } => "MSA-expired_token".to_owned(),
            Self::DeviceCodeDeclined { .. } => "MSA-authorization_declined".to_owned(),
            Self::XboxLive { status, .. } => format!("XBL-{status}"),
            Self::Xsts { xerr, status, .. } => match xerr {
                Some(xerr) => format!("XSTS-{xerr}"),
                None => format!("XSTS-{status}"),
            },
            Self::MinecraftLogin {
                app_registration: true,
                ..
            } => "MC-APPREG".to_owned(),
            Self::MinecraftLogin { status, .. } => format!("MC-{status}"),
            Self::NoLicense { .. } => "MC-LISANSYOK".to_owned(),
            Self::NoProfile { .. } => "MC-PROFILYOK".to_owned(),
            Self::MinecraftApi { step, status, .. } => match step {
                AuthStep::Profile => format!("MC-PROFIL-{status}"),
                AuthStep::Skin => format!("MC-SKIN-{status}"),
                AuthStep::Cape => format!("MC-PELERIN-{status}"),
                AuthStep::Texture => format!("MC-DOKU-{status}"),
                AuthStep::Entitlements => format!("MC-LISANS-{status}"),
                _ => format!("MC-{status}"),
            },
            Self::InvalidSkinFile { .. } => "SKIN-DOSYA".to_owned(),
            Self::NotInSkinHistory => "SKIN-GECMIS".to_owned(),
            Self::Network { .. } => "AG-BAGLANTI".to_owned(),
            Self::UnexpectedResponse { step, .. } => match step {
                AuthStep::XboxLive | AuthStep::Xsts => "XBL-VERI".to_owned(),
                _ => "VERI-BICIM".to_owned(),
            },
            Self::InvalidOfflineName {
                problem: OfflineNameProblem::Taken,
            } => "AD-VAR".to_owned(),
            Self::InvalidOfflineName { .. } => "AD-GECERSIZ".to_owned(),
            Self::AccountNotFound { .. } => "HESAP-YOK".to_owned(),
            Self::NotMicrosoftAccount { .. } => "HESAP-TUR".to_owned(),
            Self::MissingSecrets { .. } => "KC-YOK".to_owned(),
            Self::Keychain {
                unavailable: true, ..
            } => "KC-ERISIM".to_owned(),
            Self::Keychain { .. } => "KC-HATA".to_owned(),
            Self::Storage { .. } => "DOSYA-HATA".to_owned(),
        }
    }

    /// Friendly Turkish explanation that tells the player what to do. Contains no error codes.
    pub fn user_message_tr(&self) -> String {
        const RELOGIN: &str = "Oturumunun süresi doldu. Microsoft hesabınla yeniden giriş yap.";
        const RATE_LIMITED: &str =
            "Kısa sürede çok fazla deneme yapıldı. Birkaç dakika bekleyip tekrar dene.";
        const MC_DOWN: &str = "Minecraft sunucuları şu an yanıt vermiyor. Biraz sonra tekrar dene.";
        const XBOX_DOWN: &str = "Xbox Live şu an yanıt vermiyor. Biraz sonra tekrar dene.";

        let text = match self {
            Self::Cancelled => "Giriş iptal edildi.",
            Self::BrowserTimeout { .. } => {
                "Giriş süresi doldu. Tarayıcıdaki adımları beş dakika içinde tamamlayıp tekrar \
                 dene."
            }
            Self::Loopback { .. } => {
                "Giriş yanıtı tarayıcıdan alınamadı. Güvenlik yazılımın launcher'ı engelliyor \
                 olabilir; izin verip tekrar dene."
            }
            Self::StateMismatch { .. } => {
                "Giriş yanıtı doğrulanamadı, güvenliğin için giriş durduruldu. Tekrar dene."
            }
            Self::MissingCode { .. } => "Microsoft'tan giriş onayı alınamadı. Tekrar dene.",
            Self::Authorize { error, .. } => {
                if error.eq_ignore_ascii_case("access_denied") {
                    "Giriş izni verilmedi. Devam etmek için Microsoft sayfasında istenen izni \
                     onayla."
                } else {
                    "Microsoft giriş sayfası isteği reddetti. Tekrar dene."
                }
            }
            Self::MicrosoftToken {
                step,
                error,
                status,
                ..
            } => {
                let error = error.as_deref().unwrap_or_default();
                if is_relogin_error(error) {
                    if *step == AuthStep::Refresh {
                        RELOGIN
                    } else {
                        "Giriş onayı geçersiz ya da süresi dolmuş. Yeniden giriş yap."
                    }
                } else if matches!(error, "unauthorized_client" | "invalid_client") {
                    "Microsoft ile giriş şu an kullanılamıyor. Launcher'ı güncelle; sorun sürerse \
                     destek ekibimize yaz."
                } else if is_transient_status(*status) {
                    "Microsoft giriş hizmetine şu an ulaşılamıyor. Biraz sonra tekrar dene."
                } else {
                    "Microsoft girişi tamamlanamadı. Tekrar dene."
                }
            }
            Self::NoRefreshToken { .. } => RELOGIN,
            Self::DeviceCodeExpired { .. } => {
                "Giriş kodunun süresi doldu. Yeni bir kod alıp tekrar dene."
            }
            Self::DeviceCodeDeclined { .. } => {
                "Giriş isteği reddedildi. Devam etmek için isteği onayla."
            }
            Self::XboxLive { status, .. } => {
                if is_transient_status(*status) {
                    XBOX_DOWN
                } else {
                    "Xbox Live oturumu açılamadı. Microsoft hesabının bir Xbox profili olduğundan \
                     emin olup tekrar dene."
                }
            }
            Self::Xsts { xerr, status, .. } => {
                if xerr.is_none() && is_transient_status(*status) {
                    XBOX_DOWN
                } else {
                    xsts_message(*xerr)
                }
            }
            Self::MinecraftLogin {
                app_registration: true,
                ..
            } => {
                "Microsoft ile giriş şu an kullanılamıyor. Sorun hesabında değil, launcher'da. Bu \
                 sürede çevrimdışı hesapla oynayabilirsin."
            }
            Self::MinecraftLogin { status, .. } => match status {
                401 | 403 => {
                    "Minecraft hesabın doğrulanamadı. Oyunu bu Microsoft hesabıyla satın \
                     aldığından emin ol."
                }
                429 => RATE_LIMITED,
                _ => MC_DOWN,
            },
            Self::NoLicense { .. } => {
                "Bu Microsoft hesabında Minecraft: Java Edition yok. Oyunu satın aldığın hesapla \
                 giriş yap."
            }
            Self::NoProfile { .. } => {
                "Bu hesapta Minecraft: Java Edition var ama henüz oyuncu profili yok. \
                 minecraft.net'te giriş yapıp oyuncu adını seç, sonra tekrar dene."
            }
            Self::MinecraftApi { step, status, .. } => {
                if *status == 429 {
                    RATE_LIMITED
                } else if is_transient_status(*status) {
                    MC_DOWN
                } else {
                    match (step, status) {
                        (AuthStep::Skin, 400) => {
                            "Minecraft bu skini kabul etmedi. 64×64 ya da 64×32 boyutunda bir \
                             PNG dosyası seç."
                        }
                        (AuthStep::Skin, _) => "Skin güncellenemedi. Tekrar dene.",
                        (AuthStep::Cape, _) => "Pelerin değiştirilemedi. Tekrar dene.",
                        (AuthStep::Texture, _) => "Skin görüntüsü indirilemedi. Tekrar dene.",
                        (AuthStep::Entitlements, _) => "Oyun lisansı denetlenemedi. Tekrar dene.",
                        _ => "Minecraft profili alınamadı. Tekrar dene.",
                    }
                }
            }
            Self::InvalidSkinFile { problem } => match problem {
                SkinFileProblem::NotPng => {
                    "Bu dosya PNG değil. Skin olarak 64×64 ya da 64×32 boyutunda bir PNG dosyası \
                     seç."
                }
                SkinFileProblem::BadDimensions { .. } => "Skin 64×64 ya da 64×32 piksel olmalı.",
                SkinFileProblem::TooLarge => {
                    "Skin dosyası çok büyük. Daha küçük bir PNG dosyası seç."
                }
                SkinFileProblem::InvalidUrl => {
                    "Bu skin bağlantısı geçerli değil. Bir görsel bağlantısı yaz."
                }
                SkinFileProblem::Unreadable { .. } => {
                    "Skin dosyası okunamadı. Dosyanın yerinde olduğundan emin olup tekrar dene."
                }
            },
            Self::NotInSkinHistory => "Bu skin artık geçmişte yok.",
            Self::Network { .. } => {
                "Sunuculara ulaşılamadı. İnternet bağlantını kontrol edip tekrar dene."
            }
            Self::UnexpectedResponse { .. } => {
                "Sunucudan beklenmedik bir yanıt geldi. Tekrar dene."
            }
            Self::InvalidOfflineName { problem } => problem.user_message_tr(),
            Self::AccountNotFound { .. } => {
                "Hesap bulunamadı. Hesap listesi değişmiş olabilir, tekrar dene."
            }
            Self::NotMicrosoftAccount { .. } => {
                "Bu işlem yalnızca Microsoft hesaplarında kullanılabilir."
            }
            Self::MissingSecrets { .. } => {
                "Bu hesabın oturum bilgileri bulunamadı. Microsoft hesabınla yeniden giriş yap."
            }
            Self::Keychain {
                unavailable: true, ..
            } => {
                "Sistemin güvenli parola deposuna erişilemedi. Kilitliyse kilidini açıp tekrar \
                 dene. Linux'ta GNOME Keyring ya da KWallet gibi bir parola deposunun \
                 çalıştığından emin ol."
            }
            Self::Keychain { .. } => {
                "Sistemin güvenli parola deposunda bir sorun oluştu. Tekrar dene."
            }
            Self::Storage { op, .. } => {
                if *op == "read" {
                    "Hesap bilgileri okunamadı. Veri klasörüne erişim izni olduğundan emin olup \
                     tekrar dene."
                } else {
                    "Hesap bilgileri kaydedilemedi. Diskte yer olduğundan ve veri klasörüne yazma \
                     izni bulunduğundan emin ol."
                }
            }
        };
        text.to_owned()
    }

    /// The step the failure belongs to, when there is one.
    pub fn step(&self) -> Option<AuthStep> {
        Some(match self {
            Self::Cancelled | Self::AccountNotFound { .. } | Self::NotMicrosoftAccount { .. } => {
                return None;
            }
            Self::BrowserTimeout { .. }
            | Self::Loopback { .. }
            | Self::StateMismatch { .. }
            | Self::MissingCode { .. }
            | Self::Authorize { .. } => AuthStep::Browser,
            Self::MicrosoftToken { step, .. } => *step,
            Self::NoRefreshToken { .. } => AuthStep::Refresh,
            Self::DeviceCodeExpired { .. } | Self::DeviceCodeDeclined { .. } => {
                AuthStep::DeviceCode
            }
            Self::XboxLive { .. } => AuthStep::XboxLive,
            Self::Xsts { .. } => AuthStep::Xsts,
            Self::MinecraftLogin { .. } => AuthStep::Minecraft,
            Self::NoLicense { .. } | Self::NoProfile { .. } => AuthStep::Profile,
            Self::MinecraftApi { step, .. }
            | Self::Network { step, .. }
            | Self::UnexpectedResponse { step, .. } => *step,
            Self::InvalidSkinFile { .. } | Self::NotInSkinHistory => AuthStep::Skin,
            Self::InvalidOfflineName { .. } => AuthStep::Offline,
            Self::MissingSecrets { .. } | Self::Keychain { .. } => AuthStep::Keychain,
            Self::Storage { .. } => AuthStep::Storage,
        })
    }

    /// HTTP status of the failed response, when the failure came from one.
    pub fn http_status(&self) -> Option<u16> {
        match self {
            Self::MicrosoftToken { status, .. }
            | Self::XboxLive { status, .. }
            | Self::Xsts { status, .. }
            | Self::MinecraftLogin { status, .. }
            | Self::MinecraftApi { status, .. } => Some(*status),
            _ => None,
        }
    }

    /// When the failure happened (remote and storage failures record it).
    pub fn occurred_at(&self) -> Option<Timestamp> {
        match self {
            Self::BrowserTimeout { at }
            | Self::Loopback { at, .. }
            | Self::StateMismatch { at }
            | Self::MissingCode { at }
            | Self::Authorize { at, .. }
            | Self::MicrosoftToken { at, .. }
            | Self::NoRefreshToken { at }
            | Self::DeviceCodeExpired { at }
            | Self::DeviceCodeDeclined { at }
            | Self::XboxLive { at, .. }
            | Self::Xsts { at, .. }
            | Self::MinecraftLogin { at, .. }
            | Self::NoLicense { at }
            | Self::NoProfile { at }
            | Self::MinecraftApi { at, .. }
            | Self::Network { at, .. }
            | Self::UnexpectedResponse { at, .. }
            | Self::Keychain { at, .. }
            | Self::Storage { at, .. } => Some(*at),
            _ => None,
        }
    }

    fn extra_detail(&self) -> Option<String> {
        match self {
            Self::Authorize { description, .. } => description.clone(),
            Self::MicrosoftToken { detail, .. }
            | Self::XboxLive { detail, .. }
            | Self::Xsts { detail, .. }
            | Self::MinecraftLogin { detail, .. }
            | Self::MinecraftApi { detail, .. }
            | Self::UnexpectedResponse { detail, .. } => Some(detail.clone()),
            Self::Network { message, .. }
            | Self::Loopback { message, .. }
            | Self::Keychain { message, .. } => Some(message.clone()),
            Self::Storage { path, source, .. } => Some(format!("{}: {source}", path.display())),
            Self::InvalidSkinFile {
                problem: SkinFileProblem::BadDimensions { width, height },
            } => Some(format!("{width}x{height}")),
            Self::InvalidSkinFile {
                problem: SkinFileProblem::Unreadable { kind },
            } => Some(kind.to_string()),
            _ => None,
        }
        .filter(|detail| !detail.trim().is_empty())
    }

    /// Multi-line diagnostics for the "copy details" button: code, step, HTTP status, time and a
    /// sanitized server detail. Never contains tokens.
    pub fn details(&self) -> String {
        let mut out = String::new();
        let at = self.occurred_at().unwrap_or_else(Timestamp::now);
        let _ = writeln!(out, "Projects Launcher hesap hatası");
        let _ = writeln!(out, "Kod: {}", self.code());
        if let Some(step) = self.step() {
            let _ = writeln!(out, "Adım: {step}");
        }
        if let Some(status) = self.http_status() {
            let _ = writeln!(out, "HTTP: {status}");
        }
        let _ = writeln!(out, "Zaman: {}", at.strftime("%Y-%m-%d %H:%M:%S UTC"));
        let _ = writeln!(
            out,
            "Sürüm: {} ({} {})",
            env!("CARGO_PKG_VERSION"),
            std::env::consts::OS,
            std::env::consts::ARCH
        );
        let _ = writeln!(out, "Hata: {}", sanitize(&self.to_string()));
        if let Some(detail) = self.extra_detail() {
            let _ = writeln!(out, "Ayrıntı: {}", sanitize(&detail));
        }
        out.trim_end().to_owned()
    }

    /// How a caller should react (see [`FailureClass`]).
    pub fn class(&self) -> FailureClass {
        match self {
            Self::MicrosoftToken { error, status, .. } => {
                if error.as_deref().is_some_and(is_relogin_error) {
                    FailureClass::NeedsRelogin
                } else if is_transient_status(*status) {
                    FailureClass::Offline
                } else {
                    FailureClass::Other
                }
            }
            Self::NoRefreshToken { .. } | Self::MissingSecrets { .. } => FailureClass::NeedsRelogin,
            Self::Network { .. } => FailureClass::Offline,
            Self::XboxLive { status, .. }
            | Self::Xsts {
                xerr: None, status, ..
            }
            | Self::MinecraftLogin {
                app_registration: false,
                status,
                ..
            }
            | Self::MinecraftApi { status, .. }
                if is_transient_status(*status) =>
            {
                FailureClass::Offline
            }
            _ => FailureClass::Other,
        }
    }

    /// Everything the UI needs to show this error, in one serializable value.
    pub fn info(&self) -> AuthErrorInfo {
        AuthErrorInfo {
            code: self.code(),
            message: self.user_message_tr(),
            details: self.details(),
            class: self.class(),
            cancelled: self.is_cancelled(),
        }
    }
}

/// Serializable view of an [`AuthError`] for the UI.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct AuthErrorInfo {
    /// Short code; goes only into "copy details", never into visible text.
    pub code: String,
    /// Friendly Turkish message without codes.
    pub message: String,
    /// Multi-line diagnostics for the "copy details" button.
    pub details: String,
    pub class: FailureClass,
    pub cancelled: bool,
}

impl From<&AuthError> for AuthErrorInfo {
    fn from(error: &AuthError) -> Self {
        error.info()
    }
}

/// OAuth error codes after which the stored refresh token can never work again.
pub(crate) fn is_relogin_error(error: &str) -> bool {
    matches!(
        error,
        "invalid_grant" | "interaction_required" | "consent_required" | "login_required"
    )
}

/// Statuses that mean "the service is temporarily unavailable", not "your request is wrong".
pub(crate) fn is_transient_status(status: u16) -> bool {
    status >= 500 || status == 429 || status == 408
}

/// XSTS rejections describe the state of the player's account; each needs its own fix.
pub(crate) fn xsts_message(xerr: Option<u64>) -> &'static str {
    match xerr {
        Some(2_148_916_233) => {
            "Bu Microsoft hesabına bağlı bir Xbox profili yok. xbox.com'dan ücretsiz bir profil \
             oluşturup tekrar dene."
        }
        Some(2_148_916_235) => {
            "Xbox Live, hesabının kayıtlı olduğu ülkede veya bölgede kullanılamıyor."
        }
        Some(2_148_916_236 | 2_148_916_237) => {
            "Hesabın için yetişkin doğrulaması gerekiyor. xbox.com'da doğrulamayı tamamlayıp \
             tekrar dene."
        }
        Some(2_148_916_238) => {
            "Bu hesap bir çocuk hesabı. Xbox Live'a girebilmesi için bir yetişkinin bu hesabı \
             Microsoft aile grubuna eklemesi gerekiyor."
        }
        Some(2_148_916_227) => {
            "Bu Microsoft hesabı Xbox tarafından engellendiği için Minecraft'a giriş yapılamıyor."
        }
        Some(2_148_916_229) => {
            "Hesabında çevrimiçi oyun izni kısıtlanmış. Ailen bu izni Microsoft aile \
             ayarlarından açabilir."
        }
        Some(2_148_916_234) => {
            "Xbox kullanım koşulları henüz kabul edilmemiş. xbox.com'da giriş yapıp koşulları \
             kabul et, sonra tekrar dene."
        }
        _ => "Xbox Live doğrulaması tamamlanamadı. Tekrar dene.",
    }
}

/// A server-provided error code reduced to a safe, short tag for [`AuthError::code`].
fn code_tag(raw: &str) -> String {
    let tag: String = raw
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '_')
        .take(40)
        .collect();
    if tag.is_empty() {
        "HATA".to_owned()
    } else {
        tag
    }
}

const MAX_DETAIL_CHARS: usize = 400;
const MIN_SECRET_LEN: usize = 40;
const REDACTED: &str = "[gizlendi]";

/// Makes server text safe for logs and "copy details": one line, at most 400 characters, and
/// every long token-like run (JWTs, refresh tokens, signatures) replaced by a marker. URLs and
/// GUIDs survive because they help support.
pub(crate) fn sanitize(text: &str) -> String {
    let is_delimiter = |c: char| c.is_whitespace() || "\"'{}[](),;<>".contains(c);
    let mut out = String::with_capacity(text.len().min(MAX_DETAIL_CHARS + 16));
    let mut word = String::new();
    let flush = |word: &mut String, out: &mut String| {
        if word.chars().count() >= MIN_SECRET_LEN && !word.starts_with("http") {
            out.push_str(REDACTED);
        } else {
            out.push_str(word);
        }
        word.clear();
    };
    for c in text.chars() {
        if is_delimiter(c) {
            flush(&mut word, &mut out);
            if c.is_whitespace() {
                if !out.ends_with(' ') {
                    out.push(' ');
                }
            } else {
                out.push(c);
            }
        } else {
            word.push(c);
        }
    }
    flush(&mut word, &mut out);
    let out = out.trim();
    if out.chars().count() > MAX_DETAIL_CHARS {
        let cut: String = out.chars().take(MAX_DETAIL_CHARS).collect();
        format!("{cut}…")
    } else {
        out.to_owned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn now() -> Timestamp {
        Timestamp::now()
    }

    fn xsts(xerr: u64) -> AuthError {
        AuthError::Xsts {
            status: 401,
            xerr: Some(xerr),
            detail: String::new(),
            at: now(),
        }
    }

    #[test]
    fn xerr_codes_map_to_specific_messages() {
        let cases = [
            (2_148_916_233, "Xbox profili yok"),
            (2_148_916_235, "ülkede veya bölgede"),
            (2_148_916_236, "yetişkin doğrulaması"),
            (2_148_916_237, "yetişkin doğrulaması"),
            (2_148_916_238, "çocuk hesabı"),
            (2_148_916_227, "engellendiği"),
            (2_148_916_229, "aile ayarlarından"),
            (2_148_916_234, "kullanım koşulları"),
        ];
        for (xerr, needle) in cases {
            let error = xsts(xerr);
            assert_eq!(error.code(), format!("XSTS-{xerr}"));
            let message = error.user_message_tr();
            assert!(message.contains(needle), "{xerr}: {message}");
            assert!(!message.contains(&xerr.to_string()));
            assert_eq!(error.class(), FailureClass::Other);
        }
        let unknown = xsts(1);
        assert!(unknown.user_message_tr().contains("Xbox Live doğrulaması"));
        let no_xerr = AuthError::Xsts {
            status: 503,
            xerr: None,
            detail: String::new(),
            at: now(),
        };
        assert_eq!(no_xerr.code(), "XSTS-503");
        assert_eq!(no_xerr.class(), FailureClass::Offline);
    }

    #[test]
    fn codes_follow_the_legacy_style() {
        let token = |error: &str, status| AuthError::MicrosoftToken {
            step: AuthStep::Microsoft,
            status,
            error: Some(error.to_owned()),
            detail: String::new(),
            at: now(),
        };
        assert_eq!(token("invalid_grant", 400).code(), "MSA-invalid_grant");
        assert_eq!(token("weird code!<>", 400).code(), "MSA-weirdcode");
        let login = |status, app_registration| AuthError::MinecraftLogin {
            status,
            app_registration,
            detail: String::new(),
            at: now(),
        };
        assert_eq!(login(403, true).code(), "MC-APPREG");
        assert_eq!(login(401, false).code(), "MC-401");
        assert_eq!(login(429, false).code(), "MC-429");
        assert_eq!(
            AuthError::network(AuthStep::Xsts, "dns").code(),
            "AG-BAGLANTI"
        );
        assert_eq!(AuthError::NoProfile { at: now() }.code(), "MC-PROFILYOK");
        assert_eq!(AuthError::NoLicense { at: now() }.code(), "MC-LISANSYOK");
        assert_eq!(
            AuthError::unexpected(AuthStep::XboxLive, "no uhs").code(),
            "XBL-VERI"
        );
        assert_eq!(
            AuthError::keychain("save", true, "locked").code(),
            "KC-ERISIM"
        );
    }

    #[test]
    fn classes() {
        let token = |error: Option<&str>, status| AuthError::MicrosoftToken {
            step: AuthStep::Microsoft,
            status,
            error: error.map(str::to_owned),
            detail: String::new(),
            at: now(),
        };
        for relogin in ["invalid_grant", "interaction_required", "consent_required"] {
            assert_eq!(
                token(Some(relogin), 400).class(),
                FailureClass::NeedsRelogin
            );
        }
        assert_eq!(token(None, 503).class(), FailureClass::Offline);
        assert_eq!(
            token(Some("invalid_request"), 400).class(),
            FailureClass::Other
        );
        assert_eq!(
            AuthError::network(AuthStep::Microsoft, "connect").class(),
            FailureClass::Offline
        );
        assert_eq!(
            AuthError::MissingSecrets { id: Uuid::nil() }.class(),
            FailureClass::NeedsRelogin
        );
        let app_reg = AuthError::MinecraftLogin {
            status: 503,
            app_registration: true,
            detail: String::new(),
            at: now(),
        };
        assert_eq!(app_reg.class(), FailureClass::Other);
        assert_eq!(AuthError::Cancelled.class(), FailureClass::Other);
    }

    #[test]
    fn invalid_grant_wording_depends_on_the_grant() {
        let token = |step| AuthError::MicrosoftToken {
            step,
            status: 400,
            error: Some("invalid_grant".into()),
            detail: String::new(),
            at: now(),
        };
        let refresh = token(AuthStep::Refresh);
        assert!(
            refresh
                .user_message_tr()
                .contains("Oturumunun süresi doldu")
        );
        assert!(refresh.details().contains("Adım: microsoft-refresh"));
        let sign_in = token(AuthStep::Microsoft);
        assert!(!sign_in.user_message_tr().contains("Oturumunun"));
        assert_eq!(sign_in.class(), FailureClass::NeedsRelogin);
    }

    #[test]
    fn messages_never_contain_codes() {
        let errors = vec![
            AuthError::Cancelled,
            AuthError::BrowserTimeout { at: now() },
            AuthError::StateMismatch { at: now() },
            AuthError::MissingCode { at: now() },
            AuthError::Authorize {
                error: "access_denied".into(),
                description: None,
                at: now(),
            },
            AuthError::MicrosoftToken {
                step: AuthStep::Microsoft,
                status: 400,
                error: Some("invalid_grant".into()),
                detail: String::new(),
                at: now(),
            },
            AuthError::XboxLive {
                status: 401,
                detail: String::new(),
                at: now(),
            },
            xsts(2_148_916_233),
            AuthError::MinecraftLogin {
                status: 403,
                app_registration: true,
                detail: String::new(),
                at: now(),
            },
            AuthError::NoLicense { at: now() },
            AuthError::NoProfile { at: now() },
            AuthError::MinecraftApi {
                step: AuthStep::Skin,
                status: 400,
                detail: String::new(),
                at: now(),
            },
            AuthError::InvalidSkinFile {
                problem: SkinFileProblem::BadDimensions {
                    width: 10,
                    height: 10,
                },
            },
            AuthError::InvalidSkinFile {
                problem: SkinFileProblem::Unreadable {
                    kind: std::io::ErrorKind::NotFound,
                },
            },
            AuthError::network(AuthStep::Profile, "timeout"),
            AuthError::unexpected(AuthStep::Profile, "garbage"),
            AuthError::InvalidOfflineName {
                problem: OfflineNameProblem::InvalidCharacters,
            },
            AuthError::InvalidOfflineName {
                problem: OfflineNameProblem::Taken,
            },
            AuthError::MissingSecrets { id: Uuid::nil() },
            AuthError::keychain("load", true, "no secret service"),
            AuthError::storage("write", "accounts.json", std::io::Error::other("disk full")),
        ];
        for error in errors {
            let code = error.code();
            let message = error.user_message_tr();
            assert!(!message.is_empty());
            assert!(!message.contains(&code), "{code} leaked into: {message}");
            assert!(!message.contains("HTTP"), "{message}");
            assert!(message.ends_with('.'), "{message}");
            let details = error.details();
            assert!(details.contains(&format!("Kod: {code}")), "{details}");
            assert!(details.contains("Zaman: "), "{details}");
        }
    }

    #[test]
    fn details_have_step_status_and_no_tokens() {
        let jwt =
            "eyJhbGciOiJIUzI1NiJ9.eyJ4dWlkIjoiMjUzNTQwMDAwMDAwMDAwMCJ9.c2lnbmF0dXJlLXNpZ25hdHVyZQ";
        let error = AuthError::MicrosoftToken {
            step: AuthStep::Microsoft,
            status: 400,
            error: Some("invalid_grant".into()),
            detail: sanitize(&format!(
                "{{\"error\":\"invalid_grant\",\"refresh_token\":\"{jwt}\"}}\nsecond line"
            )),
            at: now(),
        };
        let details = error.details();
        assert!(details.contains("Kod: MSA-invalid_grant"));
        assert!(details.contains("Adım: microsoft-token"));
        assert!(details.contains("HTTP: 400"));
        assert!(!details.contains(jwt), "{details}");
        assert!(details.contains(REDACTED));
        assert!(details.lines().count() >= 6);
    }

    #[test]
    fn sanitize_keeps_short_words_urls_and_guids() {
        let text = "AADSTS70000: grant expired. Trace ID: 0f3e6f43-56d1-4c1d-9a6b-8b1a7c11e900 \
                    see https://login.microsoftonline.com/error?code=70000&something=long-value";
        let clean = sanitize(text);
        assert!(clean.contains("AADSTS70000"));
        assert!(clean.contains("0f3e6f43-56d1-4c1d-9a6b-8b1a7c11e900"));
        assert!(clean.contains("https://login.microsoftonline.com/error?code=70000"));
        let long = "x".repeat(2000);
        let clean = sanitize(&format!("a {long} b\n\nc"));
        assert_eq!(clean, format!("a {REDACTED} b c"));
        let many = "word ".repeat(200);
        assert!(sanitize(&many).chars().count() <= MAX_DETAIL_CHARS + 1);
    }
}
