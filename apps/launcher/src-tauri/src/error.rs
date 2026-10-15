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
            _ if error.is_offline() => (
                "AG-YOK",
                "İnternete bağlanılamadı ve gereken oyun dosyaları bilgisayarında yok. Bağlantını kontrol edip tekrar dene.",
            ),
            E::VersionNotFound(_) => ("SURUM-YOK", "Bu Minecraft sürümü bulunamadı."),
            E::Java(_) => (
                "JAVA",
                "Bu sürüm için uygun Java hazırlanamadı. Kurulum ayarlarından Java'yı Otomatik'e al.",
            ),
            E::Net(_) => ("AG", "İndirme sırasında bağlantı sorunu oldu. Tekrar dene."),
            E::Io { .. } => (
                "DISK",
                "Dosyalara yazılamadı. Diskte yer olduğundan ve antivirüsün engellemediğinden emin ol.",
            ),
            E::Process(_) => ("SUREC", "Oyun başlatılamadı."),
            E::NotFound(_) => ("BULUNAMADI", "Gereken bir dosya bulunamadı."),
            _ => ("OYUN", "Oyun hazırlanırken bir sorun oluştu."),
        };
        Self::new(code, message, details)
    }
}

pub type CmdResult<T> = Result<T, CmdError>;
