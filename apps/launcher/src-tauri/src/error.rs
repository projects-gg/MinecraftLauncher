use pl_lang::{Lang, pick};
use serde::Serialize;

/// Error returned by every command. `message` is friendly text for the UI in the interface's
/// language (`pl_lang`: the backend's own texts are written as Turkish/English pairs); `code` and
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
            pick(
                "Beklenmeyen bir sorun oluştu. Tekrar dene; sürerse ayrıntıları destek ekibine gönder.",
                "Something unexpected went wrong. Try again; if it keeps happening, send the details to support.",
            ),
            details.to_string(),
        )
    }

    /// The disk filled up during a download or an install.
    pub fn disk_full(details: impl Into<String>) -> Self {
        Self::new(
            Self::DISK_FULL,
            pick(
                "Diskte yer kalmadı. Biraz yer açıp tekrar dene.",
                "The disk is full. Free up some space and try again.",
            ),
            details,
        )
    }

    /// Before a launch downloads anything: the disk cannot hold it (`needed` with the margin).
    pub fn disk_short(needed: u64, free: u64, details: impl Into<String>) -> Self {
        let lang = Lang::current();
        let (needed, free) = (toast_size(needed, lang), toast_size(free, lang));
        Self::new(
            Self::DISK_FULL,
            lang.pick(
                format!("Diskte yeterli yer yok. Oyun için {needed} gerekiyor, {free} boş."),
                format!(
                    "There isn't enough space on the disk. The game needs {needed}, but only {free} is free."
                ),
            ),
            details,
        )
    }

    /// Before a mod, a modpack or a mod center change downloads or unpacks anything: the disk
    /// cannot hold it (`needed` with the launch's margin; `pl_mc::Error::NoRoom`).
    pub fn no_room(needed: u64, free: u64, details: impl Into<String>) -> Self {
        let lang = Lang::current();
        let (needed, free) = (toast_size(needed, lang), toast_size(free, lang));
        Self::new(
            Self::DISK_FULL,
            lang.pick(
                format!("Diskte yeterli yer yok. Bunun için {needed} gerekiyor, {free} boş."),
                format!(
                    "There isn't enough space on the disk. This needs {needed}, but only {free} is free."
                ),
            ),
            details,
        )
    }
}

/// "1,2 GB" with a no-break space: a figure and its unit stay on one line of the toast.
fn toast_size(bytes: u64, lang: Lang) -> String {
    pl_mc::install::space::format_size(bytes, lang).replace(' ', &char::from(0xA0_u8).to_string())
}

impl From<pl_auth::AuthError> for CmdError {
    fn from(error: pl_auth::AuthError) -> Self {
        Self::new(
            error.code(),
            error.user_message(Lang::current()),
            error.details(),
        )
    }
}

impl From<pl_content::Error> for CmdError {
    fn from(error: pl_content::Error) -> Self {
        if error.is_offline() {
            Self::new(
                "ICERIK-AG",
                pick(
                    "Haberlere ulaşılamadı. İnternet bağlantını kontrol et.",
                    "Couldn't reach the news. Check your internet connection.",
                ),
                error.to_string(),
            )
        } else {
            Self::new(
                "ICERIK",
                pick("Haberler yüklenemedi.", "The news couldn't be loaded."),
                error.to_string(),
            )
        }
    }
}

impl From<pl_mc::Error> for CmdError {
    fn from(error: pl_mc::Error) -> Self {
        use pl_mc::Error as E;
        let details = error.to_string();
        let (code, message) = match &error {
            _ if error.is_cancelled() => ("IPTAL", pick("İşlem iptal edildi.", "Cancelled.")),
            E::NoRoom(short) => return Self::no_room(short.needed, short.free, details),
            _ if error.is_disk_full() => return Self::disk_full(details),
            _ if error.is_offline() => (
                "AG-YOK",
                pick(
                    "İnternete bağlanılamadı ve gereken oyun dosyaları bilgisayarında yok. Bağlantını kontrol edip tekrar dene.",
                    "Couldn't connect to the internet, and the game files it needs aren't on your computer. Check your connection and try again.",
                ),
            ),
            E::VersionNotFound(_) => (
                "SURUM-YOK",
                pick(
                    "Bu Minecraft sürümü bulunamadı. Kurulumun sürümünü değiştir.",
                    "This Minecraft version wasn't found. Pick another version for the instance.",
                ),
            ),
            E::Java(_) => (
                "JAVA",
                pick(
                    "Bu sürüm için uygun Java hazırlanamadı. Kurulum ayarlarından Java'yı Otomatik'e al.",
                    "Couldn't get the right Java ready for this version. Set Java to Automatic in the instance settings.",
                ),
            ),
            E::Net(_) => (
                "AG",
                pick(
                    "İndirme sırasında bağlantı sorunu oldu. Tekrar dene.",
                    "The connection dropped during the download. Try again.",
                ),
            ),
            E::Io { .. } => (
                "DISK",
                pick(
                    "Dosyalara yazılamadı. Diskte yer olduğundan ve antivirüsün engellemediğinden emin ol.",
                    "Couldn't write the files. Make sure there's space on the disk and your antivirus isn't blocking it.",
                ),
            ),
            E::Process(_) => (
                "SUREC",
                pick(
                    "Oyun başlatılamadı. Tekrar dene.",
                    "The game couldn't start. Try again.",
                ),
            ),
            E::NotFound(_) => (
                "BULUNAMADI",
                pick(
                    "Gereken bir dosya bulunamadı. Tekrar dene.",
                    "A file it needs is missing. Try again.",
                ),
            ),
            _ => (
                "OYUN",
                pick(
                    "Oyun hazırlanırken bir sorun oluştu. Tekrar dene.",
                    "Something went wrong while getting the game ready. Try again.",
                ),
            ),
        };
        Self::new(code, message, details)
    }
}

impl From<pl_social::SocialError> for CmdError {
    /// Friends service failures: a short sentence; the code stays in the details.
    fn from(error: pl_social::SocialError) -> Self {
        use pl_social::{ErrorCode as C, SocialError as E};
        let details = error.to_string();
        let unavailable = pick(
            "Arkadaşlar şu an kullanılamıyor. Biraz sonra tekrar dene.",
            "Friends isn't available right now. Try again in a bit.",
        );
        let (code, message) = match &error {
            E::Unauthorized => (
                "SOSYAL-OTURUM",
                pick(
                    "Oturumun sona erdi. Yeniden giriş yap.",
                    "Your session ended. Sign in again.",
                ),
            ),
            E::RateLimited { .. } => (
                "SOSYAL-SINIR",
                pick(
                    "Bunu çok sık denedin. Biraz sonra tekrar dene.",
                    "You tried that too often. Try again in a bit.",
                ),
            ),
            E::Unavailable { .. } | E::Network(_) => ("SOSYAL-YOK", unavailable),
            E::SettingsConflict { .. } => (
                "SOSYAL-YEDEK",
                pick(
                    "Ayarların başka bir bilgisayarda değişti. Tekrar dene.",
                    "Your settings changed on another computer. Try again.",
                ),
            ),
            E::InvalidInput("not a friend code") => (
                "SOSYAL-KOD",
                pick("Bu bir arkadaşlık kodu değil.", "That's not a friend code."),
            ),
            E::InvalidInput("not a team code") => (
                "SOSYAL-TAKIM-KOD",
                pick("Bu bir ekip kodu değil.", "That's not a crew code."),
            ),
            E::InvalidInput("not in a team") => (
                "SOSYAL-TAKIM-YOK",
                pick(
                    "Artık bir ekipte değilsin.",
                    "You're not in a crew anymore.",
                ),
            ),
            E::InvalidInput("friend categories") => (
                "SOSYAL-KATEGORI",
                pick(
                    "Kategoriler kaydedilemedi.",
                    "Couldn't save your categories.",
                ),
            ),
            E::InvalidInput(_) | E::Protocol(_) => (
                "SOSYAL",
                pick(
                    "Bir sorun oluştu. Biraz sonra tekrar dene.",
                    "Something went wrong. Try again in a bit.",
                ),
            ),
            E::Refused { code, .. } => match code {
                C::InvalidCode | C::BadRequest => (
                    "SOSYAL-KOD",
                    pick("Bu bir arkadaşlık kodu değil.", "That's not a friend code."),
                ),
                C::CodeNotFound | C::UserNotFound => (
                    "SOSYAL-KOD-YOK",
                    pick("Bu koda ait kimse bulunamadı.", "Nobody has this code."),
                ),
                C::SelfTarget => (
                    "SOSYAL-KENDIN",
                    pick("Bu senin kodun.", "That's your own code."),
                ),
                C::AlreadyFriends => (
                    "SOSYAL-ZATEN",
                    pick("Zaten arkadaşsınız.", "You're already friends."),
                ),
                C::AlreadyRequested => (
                    "SOSYAL-ISTEK-VAR",
                    pick("İsteğin zaten gönderildi.", "Your request is already sent."),
                ),
                C::Blocked => (
                    "SOSYAL-ENGEL",
                    pick(
                        "Bu kişiyi engellemişsin. Önce engeli kaldır.",
                        "You've blocked this person. Unblock them first.",
                    ),
                ),
                C::LimitFriends => (
                    "SOSYAL-LISTE-DOLU",
                    pick("Arkadaş listen dolu.", "Your friends list is full."),
                ),
                C::LimitFriendsTarget => (
                    "SOSYAL-LISTE-DOLU",
                    pick(
                        "Bu kişinin arkadaş listesi dolu.",
                        "This person's friends list is full.",
                    ),
                ),
                C::LimitOutgoingRequests => (
                    "SOSYAL-ISTEK-SINIR",
                    pick(
                        "Bekleyen çok isteğin var. Bazıları yanıtlanınca tekrar dene.",
                        "You have too many pending requests. Try again once some are answered.",
                    ),
                ),
                C::LimitIncomingRequests => (
                    "SOSYAL-ISTEK-SINIR",
                    pick(
                        "Bu kişinin bekleyen çok isteği var. Biraz sonra tekrar dene.",
                        "This person has too many pending requests. Try again in a bit.",
                    ),
                ),
                C::RequestNotFound => (
                    "SOSYAL-ISTEK-YOK",
                    pick(
                        "Bu istek artık geçerli değil.",
                        "This request isn't valid anymore.",
                    ),
                ),
                C::FriendNotFound => (
                    "SOSYAL-ARKADAS-YOK",
                    pick(
                        "Bu kişi artık arkadaş listende değil.",
                        "This person isn't on your friends list anymore.",
                    ),
                ),
                C::SessionNotFound => (
                    "SOSYAL-OTURUM-YOK",
                    pick(
                        "Bu oturum zaten kapanmış.",
                        "This session is already closed.",
                    ),
                ),
                C::InvalidGroups => (
                    "SOSYAL-KATEGORI",
                    pick(
                        "Kategoriler kaydedilemedi.",
                        "Couldn't save your categories.",
                    ),
                ),
                C::InvalidTeamName => (
                    "SOSYAL-TAKIM-AD",
                    pick(
                        "Ekip adı 3 ile 24 karakter arasında olmalı: harf, rakam ve boşluk.",
                        "A crew name has 3 to 24 letters, digits or spaces.",
                    ),
                ),
                C::InvalidTeamMotto => (
                    "SOSYAL-TAKIM-SLOGAN",
                    pick(
                        "Slogan en fazla 40 karakter olabilir: harf, rakam, boşluk ve ' & ! _ - , ?",
                        "A motto has up to 40 letters, digits, spaces and ' & ! _ - , ?",
                    ),
                ),
                C::TeamMottoRejected => (
                    "SOSYAL-TAKIM-SLOGAN",
                    pick(
                        "Bu slogan kullanılamaz. Başka bir tane yaz.",
                        "That motto can't be used. Try another one.",
                    ),
                ),
                C::TeamNotPermitted => (
                    "SOSYAL-TAKIM-IZIN",
                    pick(
                        "Bunu yapma iznin yok.",
                        "You don't have permission to do that.",
                    ),
                ),
                C::LimitTeamModerators => (
                    "SOSYAL-TAKIM-MODERATOR",
                    pick(
                        "Ekipte en çok 4 moderatör olabilir.",
                        "A crew can have up to 4 moderators.",
                    ),
                ),
                C::CallActive => (
                    "SOSYAL-CAGRI-VAR",
                    pick(
                        "Ekibinde açık bir çağrı var.",
                        "Your crew already has a call on.",
                    ),
                ),
                C::CallNotFound => (
                    "SOSYAL-CAGRI-YOK",
                    pick("Bu çağrı bitti.", "This call has ended."),
                ),
                C::InvalidTeamTag => (
                    "SOSYAL-TAKIM-ETIKET",
                    pick(
                        "Etiket 2 ile 4 harf ya da rakam olmalı.",
                        "A tag has 2 to 4 letters or digits.",
                    ),
                ),
                C::TeamNameRejected => (
                    "SOSYAL-TAKIM-AD",
                    pick(
                        "Bu ad kullanılamaz. Başka bir tane seç.",
                        "That name can't be used. Pick another one.",
                    ),
                ),
                C::TeamNotFound => (
                    "SOSYAL-TAKIM-YOK",
                    pick("Bu koda ait bir ekip yok.", "No crew has this code."),
                ),
                C::TeamInviteNotFound => (
                    "SOSYAL-TAKIM-DAVET-YOK",
                    pick(
                        "Bu davet artık geçerli değil.",
                        "This invite isn't valid anymore.",
                    ),
                ),
                C::MemberNotFound => (
                    "SOSYAL-TAKIM-UYE-YOK",
                    pick(
                        "Bu oyuncu artık ekipte değil.",
                        "This player isn't in the crew anymore.",
                    ),
                ),
                C::AlreadyInTeam => (
                    "SOSYAL-TAKIM-VAR",
                    pick(
                        "Zaten bir ekiptesin. Önce ondan ayrıl.",
                        "You're already in a crew. Leave it first.",
                    ),
                ),
                C::AlreadyMember => (
                    "SOSYAL-TAKIM-UYE",
                    pick(
                        "Bu oyuncu zaten ekipte.",
                        "This player is already in the crew.",
                    ),
                ),
                C::TeamFounderOnly => (
                    "SOSYAL-TAKIM-KURUCU",
                    pick(
                        "Bunu yalnız ekibin lideri yapabilir.",
                        "Only the crew's leader can do that.",
                    ),
                ),
                C::FounderMustTransfer => (
                    "SOSYAL-TAKIM-DEVRET",
                    pick(
                        "Ayrılmadan önce liderliği başka bir üyeye devret.",
                        "Hand the leader role to another member before you leave.",
                    ),
                ),
                C::BlockedMember => (
                    "SOSYAL-TAKIM-ENGEL",
                    pick(
                        "Bu ekipte engellediğin biri var.",
                        "You've blocked someone in this crew.",
                    ),
                ),
                C::LimitTeamMembers => {
                    ("SOSYAL-TAKIM-DOLU", pick("Ekip dolu.", "The crew is full."))
                }
                C::LimitTeamInvites => (
                    "SOSYAL-TAKIM-DAVET-SINIR",
                    pick(
                        "Ekibin bekleyen çok daveti var. Biraz sonra tekrar dene.",
                        "The crew has too many invites waiting. Try again later.",
                    ),
                ),
                C::Unauthorized => (
                    "SOSYAL-OTURUM",
                    pick(
                        "Oturumun sona erdi. Yeniden giriş yap.",
                        "Your session ended. Sign in again.",
                    ),
                ),
                C::RateLimited => (
                    "SOSYAL-SINIR",
                    pick(
                        "Bunu çok sık denedin. Biraz sonra tekrar dene.",
                        "You tried that too often. Try again in a bit.",
                    ),
                ),
                C::Unavailable => (
                    "SOSYAL-YOK",
                    pick(
                        "Bu şu an yapılamıyor. Biraz sonra tekrar dene.",
                        "That can't be done right now. Try again in a bit.",
                    ),
                ),
                _ => (
                    "SOSYAL",
                    pick(
                        "Bir sorun oluştu. Biraz sonra tekrar dene.",
                        "Something went wrong. Try again in a bit.",
                    ),
                ),
            },
        };
        Self::new(code, message, details)
    }
}

pub type CmdResult<T> = Result<T, CmdError>;
