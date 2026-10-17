use serde::Serialize;

/// Error returned by every command. `message` is friendly Turkish text for the UI; `code` and
/// `details` are only for the "copy details" action (never shown as visible text).
#[derive(Debug, Clone, Serialize, specta::Type)]
#[serde(rename_all = "camelCase")]
pub struct CmdError {
    pub code: String,
    pub message: String,
    pub details: String,
}

impl CmdError {
    /// The disk cannot hold what is being downloaded or written: the command bar offers "Yer aç"
    /// (Settings → Veri, the cleanup) instead of copying the details.
    pub const DISK_FULL: &str = "DISK-DOLU";

    pub fn new(
        code: impl Into<String>,
        message: impl Into<String>,
        details: impl Into<String>,
    ) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
            details: details.into(),
        }
    }

    pub fn internal(details: impl std::fmt::Display) -> Self {
        Self::new(
            "GENEL",
            "Beklenmeyen bir sorun oluştu. Tekrar dene; sürerse ayrıntıları destek ekibine gönder.",
            details.to_string(),
        )
    }

    /// The disk filled up during a download or an install.
    pub fn disk_full(details: impl Into<String>) -> Self {
        Self::new(
            Self::DISK_FULL,
            "Diskte yer kalmadı. Biraz yer açıp tekrar dene.",
            details,
        )
    }

    /// Before a launch downloads anything: the disk cannot hold it (`needed` with the margin).
    pub fn disk_short(needed: u64, free: u64, details: impl Into<String>) -> Self {
        // A figure and its unit stay on one line of the toast ("1,2 GB").
        let size = |bytes| {
            pl_mc::install::space::format_size(bytes).replace(' ', &char::from(0xA0_u8).to_string())
        };
        Self::new(
            Self::DISK_FULL,
            format!(
                "Diskte yeterli yer yok. Oyun için {} gerekiyor, {} boş.",
                size(needed),
                size(free)
            ),
            details,
        )
    }
}

impl From<pl_auth::AuthError> for CmdError {
    fn from(error: pl_auth::AuthError) -> Self {
        Self::new(error.code(), error.user_message_tr(), error.details())
    }
}

impl From<pl_content::Error> for CmdError {
    fn from(error: pl_content::Error) -> Self {
        if error.is_offline() {
            Self::new(
                "ICERIK-AG",
                "Haberlere ulaşılamadı. İnternet bağlantını kontrol et.",
                error.to_string(),
            )
        } else {
            Self::new("ICERIK", "Haberler yüklenemedi.", error.to_string())
        }
    }
}

impl From<pl_mc::Error> for CmdError {
    fn from(error: pl_mc::Error) -> Self {
        use pl_mc::Error as E;
        let details = error.to_string();
        let (code, message) = match &error {
            _ if error.is_cancelled() => ("IPTAL", "İşlem iptal edildi."),
            _ if error.is_disk_full() => return Self::disk_full(details),
            _ if error.is_offline() => (
                "AG-YOK",
                "İnternete bağlanılamadı ve gereken oyun dosyaları bilgisayarında yok. Bağlantını kontrol edip tekrar dene.",
            ),
            E::VersionNotFound(_) => (
                "SURUM-YOK",
                "Bu Minecraft sürümü bulunamadı. Kurulumun sürümünü değiştir.",
            ),
            E::Java(_) => (
                "JAVA",
                "Bu sürüm için uygun Java hazırlanamadı. Kurulum ayarlarından Java'yı Otomatik'e al.",
            ),
            E::Net(_) => ("AG", "İndirme sırasında bağlantı sorunu oldu. Tekrar dene."),
            E::Io { .. } => (
                "DISK",
                "Dosyalara yazılamadı. Diskte yer olduğundan ve antivirüsün engellemediğinden emin ol.",
            ),
            E::Process(_) => ("SUREC", "Oyun başlatılamadı. Tekrar dene."),
            E::NotFound(_) => ("BULUNAMADI", "Gereken bir dosya bulunamadı. Tekrar dene."),
            _ => ("OYUN", "Oyun hazırlanırken bir sorun oluştu. Tekrar dene."),
        };
        Self::new(code, message, details)
    }
}

impl From<pl_social::SocialError> for CmdError {
    /// Friends service failures: a short Turkish sentence; the code stays in the details.
    fn from(error: pl_social::SocialError) -> Self {
        use pl_social::{ErrorCode as C, SocialError as E};
        let details = error.to_string();
        let unavailable = "Arkadaşlar şu an kullanılamıyor. Biraz sonra tekrar dene.";
        let (code, message) = match &error {
            E::Unauthorized => ("SOSYAL-OTURUM", "Oturumun sona erdi. Yeniden giriş yap."),
            E::RateLimited { .. } => (
                "SOSYAL-SINIR",
                "Bunu çok sık denedin. Biraz sonra tekrar dene.",
            ),
            E::Unavailable { .. } | E::Network(_) => ("SOSYAL-YOK", unavailable),
            E::SettingsConflict { .. } => (
                "SOSYAL-YEDEK",
                "Ayarların başka bir bilgisayarda değişti. Tekrar dene.",
            ),
            E::InvalidInput("not a friend code") => ("SOSYAL-KOD", "Bu bir arkadaşlık kodu değil."),
            E::InvalidInput(_) | E::Protocol(_) => {
                ("SOSYAL", "Bir sorun oluştu. Biraz sonra tekrar dene.")
            }
            E::Refused { code, .. } => match code {
                C::InvalidCode | C::BadRequest => ("SOSYAL-KOD", "Bu bir arkadaşlık kodu değil."),
                C::CodeNotFound | C::UserNotFound => {
                    ("SOSYAL-KOD-YOK", "Bu koda ait kimse bulunamadı.")
                }
                C::SelfTarget => ("SOSYAL-KENDIN", "Bu senin kodun."),
                C::AlreadyFriends => ("SOSYAL-ZATEN", "Zaten arkadaşsınız."),
                C::AlreadyRequested => ("SOSYAL-ISTEK-VAR", "İsteğin zaten gönderildi."),
                C::Blocked => (
                    "SOSYAL-ENGEL",
                    "Bu kişiyi engellemişsin. Önce engeli kaldır.",
                ),
                C::LimitFriends => ("SOSYAL-LISTE-DOLU", "Arkadaş listen dolu."),
                C::LimitFriendsTarget => ("SOSYAL-LISTE-DOLU", "Bu kişinin arkadaş listesi dolu."),
                C::LimitOutgoingRequests => (
                    "SOSYAL-ISTEK-SINIR",
                    "Bekleyen çok isteğin var. Bazıları yanıtlanınca tekrar dene.",
                ),
                C::LimitIncomingRequests => (
                    "SOSYAL-ISTEK-SINIR",
                    "Bu kişinin bekleyen çok isteği var. Biraz sonra tekrar dene.",
                ),
                C::RequestNotFound => ("SOSYAL-ISTEK-YOK", "Bu istek artık geçerli değil."),
                C::FriendNotFound => (
                    "SOSYAL-ARKADAS-YOK",
                    "Bu kişi artık arkadaş listende değil.",
                ),
                C::SessionNotFound => ("SOSYAL-OTURUM-YOK", "Bu oturum zaten kapanmış."),
                C::Unauthorized => ("SOSYAL-OTURUM", "Oturumun sona erdi. Yeniden giriş yap."),
                C::RateLimited => (
                    "SOSYAL-SINIR",
                    "Bunu çok sık denedin. Biraz sonra tekrar dene.",
                ),
                C::Unavailable => (
                    "SOSYAL-YOK",
                    "Bu şu an yapılamıyor. Biraz sonra tekrar dene.",
                ),
                _ => ("SOSYAL", "Bir sorun oluştu. Biraz sonra tekrar dene."),
            },
        };
        Self::new(code, message, details)
    }
}

pub type CmdResult<T> = Result<T, CmdError>;
