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

/// "28 Ekim" / "28 October" (with the year when it is not this one), on this computer's clock.
fn day_month(unix: i64, lang: Lang) -> String {
    const TR: [&str; 12] = [
        "Ocak", "Şubat", "Mart", "Nisan", "Mayıs", "Haziran", "Temmuz", "Ağustos", "Eylül", "Ekim",
        "Kasım", "Aralık",
    ];
    const EN: [&str; 12] = [
        "January",
        "February",
        "March",
        "April",
        "May",
        "June",
        "July",
        "August",
        "September",
        "October",
        "November",
        "December",
    ];
    let zone = jiff::tz::TimeZone::system();
    let Ok(at) = jiff::Timestamp::from_second(unix) else {
        return String::new();
    };
    let date = at.to_zoned(zone.clone()).date();
    let month = usize::try_from(date.month() - 1).unwrap_or(0).min(11);
    let this_year = jiff::Timestamp::now().to_zoned(zone).year();
    let name = lang.pick(TR[month], EN[month]);
    if date.year() == this_year {
        format!("{} {name}", date.day())
    } else {
        format!("{} {name} {}", date.day(), date.year())
    }
}

/// When a new account's limit lifts: "3 gün sonra açılır." / "It opens in 3 days.".
fn opens_in(until: Option<i64>, lang: Lang) -> String {
    let Some(until) = until else {
        return String::new();
    };
    let left = until - jiff::Timestamp::now().as_second();
    if left <= 3_600 {
        return lang.pick("Birazdan açılır.", "It opens soon.").to_owned();
    }
    if left < 86_400 {
        let hours = (left + 3_599) / 3_600;
        return lang.pick(
            format!("{hours} saat sonra açılır."),
            if hours == 1 {
                "It opens in an hour.".to_owned()
            } else {
                format!("It opens in {hours} hours.")
            },
        );
    }
    let days = (left + 86_399) / 86_400;
    lang.pick(
        format!("{days} gün sonra açılır."),
        if days == 1 {
            "It opens tomorrow.".to_owned()
        } else {
            format!("It opens in {days} days.")
        },
    )
}

/// Friends service failures that carry their own details (a date, a limit): the sentence is made
/// here; None for the plain ones.
fn social_special(error: &pl_social::SocialError) -> Option<(&'static str, String)> {
    use pl_social::{ChatRejection, LimitedKey, SocialError as E};
    let lang = Lang::current();
    Some(match error {
        E::Restricted { until: Some(until) } => (
            "SOSYAL-KISITLI",
            lang.pick(
                format!(
                    "Hesabın {} tarihine kadar kısıtlandı.",
                    day_month(*until, lang)
                ),
                format!(
                    "Your account is restricted until {}.",
                    day_month(*until, lang)
                ),
            ),
        ),
        E::Restricted { until: None } => (
            "SOSYAL-KISITLI",
            lang.pick(
                "Hesabın kalıcı olarak kısıtlandı.",
                "Your account is restricted for good.",
            )
            .to_owned(),
        ),
        E::SignupsClosed { .. } => (
            "SOSYAL-YENI-HESAP",
            lang.pick(
                "Şu an yeni hesap açılamıyor. Yarın yeniden dene.",
                "No new accounts can be made right now. Try again tomorrow.",
            )
            .to_owned(),
        ),
        E::TextLocked { until } => (
            "SOSYAL-EKIP-KILIT",
            match until {
                Some(until) => lang.pick(
                    format!(
                        "Ekibin adı ve sloganı {} tarihine kadar değiştirilemez.",
                        day_month(*until, lang)
                    ),
                    format!(
                        "The crew's name and motto can't change until {}.",
                        day_month(*until, lang)
                    ),
                ),
                None => lang
                    .pick(
                        "Ekibin adı ve sloganı şu an değiştirilemez.",
                        "The crew's name and motto can't change right now.",
                    )
                    .to_owned(),
            },
        ),
        E::Limited { item, until, .. } => {
            let (tr, en, daily) = match item {
                LimitedKey::CrewFound => (
                    "Discord hesabın yeni olduğu için ekip kuramazsın.",
                    "Your Discord account is new, so you can't found a crew yet.",
                    false,
                ),
                LimitedKey::CrewInvites => (
                    "Discord hesabın yeni olduğu için bugün daha fazla ekip daveti gönderemezsin.",
                    "Your Discord account is new, so you can't send more crew invites today.",
                    true,
                ),
                LimitedKey::CodeLookups => (
                    "Discord hesabın yeni olduğu için bugün daha fazla kod deneyemezsin.",
                    "Your Discord account is new, so you can't try more codes today.",
                    true,
                ),
                LimitedKey::SettingsBackup => (
                    "Discord hesabın yeni olduğu için ayar yedeği şimdilik kapalı.",
                    "Your Discord account is new, so the settings backup is off for now.",
                    false,
                ),
                LimitedKey::ChatMessages => (
                    "Discord hesabın yeni olduğu için bugünlük mesaj sınırına ulaştın.",
                    "Your Discord account is new, so you've reached today's message limit.",
                    true,
                ),
                LimitedKey::CrewLink => (
                    "Discord hesabın yeni olduğu için davet bağlantısı oluşturamazsın.",
                    "Your Discord account is new, so you can't make an invite link yet.",
                    false,
                ),
                LimitedKey::CrewPlans => (
                    "Discord hesabın yeni olduğu için plan yapamazsın.",
                    "Your Discord account is new, so you can't make plans yet.",
                    false,
                ),
                LimitedKey::ChatLinks => (
                    "Discord hesabın yeni olduğu için sohbette bağlantı gönderemezsin.",
                    "Your Discord account is new, so you can't send links in chat yet.",
                    false,
                ),
                LimitedKey::ChatStickers => (
                    "Discord hesabın yeni olduğu için bugün daha fazla çıkartma gönderemezsin.",
                    "Your Discord account is new, so you can't send more stickers today.",
                    true,
                ),
                LimitedKey::ChatReactions => (
                    "Discord hesabın yeni olduğu için bugün daha fazla tepki veremezsin.",
                    "Your Discord account is new, so you can't react more today.",
                    true,
                ),
                LimitedKey::Other => (
                    "Discord hesabın yeni olduğu için bu şimdilik yapılamıyor.",
                    "Your Discord account is new, so this can't be done yet.",
                    false,
                ),
            };
            let sentence = lang.pick(tr, en).to_owned();
            // A daily cap opens tomorrow; the rest when the account is old enough.
            let when = if daily {
                String::new()
            } else {
                opens_in(*until, lang)
            };
            (
                "SOSYAL-SINIRLI",
                if when.is_empty() {
                    sentence
                } else {
                    format!("{sentence} {when}")
                },
            )
        }
        E::ChatRejected { reason, rule } => (
            "SOSYAL-SOHBET-METIN",
            match reason {
                // CHAT2: the link rule says why, in the service's own words for it.
                ChatRejection::Link => {
                    use pl_social::LinkRule as R;
                    match rule {
                        None => lang.pick("Bağlantı gönderilemez.", "Links can't be sent."),
                        Some(R::Off) => {
                            lang.pick("Bağlantılar şu an kapalı.", "Links are off right now.")
                        }
                        Some(R::Form) => {
                            lang.pick("Bağlantı gönderilemez.", "Links can't be sent.")
                        }
                        Some(R::Count) => lang.pick(
                            "Bir mesajda en çok 3 bağlantı olabilir.",
                            "A message can have up to 3 links.",
                        ),
                        Some(R::Http) => lang.pick(
                            "Yalnız https bağlantıları gönderilebilir.",
                            "Only https links can be sent.",
                        ),
                        Some(R::Long) => {
                            lang.pick("Bu bağlantı çok uzun.", "This link is too long.")
                        }
                        Some(R::Shortener) => lang.pick(
                            "Kısaltılmış bağlantılar gönderilemez.",
                            "Shortened links can't be sent.",
                        ),
                        Some(R::Lookalike) => lang.pick(
                            "Bu bağlantı sahte bir siteye benziyor.",
                            "This link looks like a fake site.",
                        ),
                        Some(R::File) => lang.pick(
                            "Dosya bağlantıları gönderilemez.",
                            "File links can't be sent.",
                        ),
                        Some(R::Invite) => lang.pick(
                            "Discord davetleri şu an gönderilemez.",
                            "Discord invites can't be sent right now.",
                        ),
                        Some(
                            R::Credentials
                            | R::Ip
                            | R::Port
                            | R::Grabber
                            | R::Adult
                            | R::Scam
                            | R::Blocked
                            | R::Other,
                        ) => lang.pick("Bu bağlantı gönderilemez.", "This link can't be sent."),
                    }
                }
                ChatRejection::Phone => lang.pick(
                    "Telefon numarası gönderilemez.",
                    "Phone numbers can't be sent.",
                ),
                ChatRejection::Words => {
                    lang.pick("Bu mesaj gönderilemez.", "This message can't be sent.")
                }
            }
            .to_owned(),
        ),
        E::ChatMuted { .. } => (
            "SOSYAL-SOHBET-SUSTURMA",
            lang.pick(
                "Şu an mesaj gönderemezsin.",
                "You can't send messages right now.",
            )
            .to_owned(),
        ),
        // A crew plan's note (FIX2): chat's reasons, or its form.
        E::PlanNoteRejected { reason } => (
            "SOSYAL-PLAN-NOT",
            match reason {
                Some(ChatRejection::Link) => {
                    lang.pick("Notta bağlantı olamaz.", "A note can't have a link.")
                }
                Some(ChatRejection::Phone) => lang.pick(
                    "Notta telefon numarası olamaz.",
                    "A note can't have a phone number.",
                ),
                Some(ChatRejection::Words) => {
                    lang.pick("Bu not yazılamaz.", "This note can't be used.")
                }
                None => lang.pick(
                    "Not tek satır ve en çok 80 karakter olabilir.",
                    "A note is one line of up to 80 characters.",
                ),
            }
            .to_owned(),
        ),
        _ => return None,
    })
}

impl From<pl_social::SocialError> for CmdError {
    /// Friends service failures: a short sentence; the code stays in the details.
    fn from(error: pl_social::SocialError) -> Self {
        use pl_social::{ErrorCode as C, SocialError as E};
        let details = error.to_string();
        if let Some((code, message)) = social_special(&error) {
            return Self::new(code, message, details);
        }
        let unavailable = pick(
            "Arkadaş listene şu an ulaşılamıyor. Biraz sonra tekrar dene.",
            "Can't reach your friends list right now. Try again in a bit.",
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
            E::InvalidInput("not a crew link") => (
                "SOSYAL-EKIP-BAGLANTI-GECERSIZ",
                pick(
                    "Bu bir ekip davet bağlantısı değil.",
                    "That's not a crew invite link.",
                ),
            ),
            E::InvalidInput("crew links" | "join requests" | "crew plans") => (
                "SOSYAL-EKIP-YENI",
                pick(
                    "Bu şu an kullanılamıyor. Biraz sonra tekrar dene.",
                    "This isn't available right now. Try again in a bit.",
                ),
            ),
            E::InvalidInput("plan time") => (
                "SOSYAL-PLAN-SAAT",
                pick("Bu saat seçilemez.", "That time can't be picked."),
            ),
            E::InvalidInput("friend categories") => (
                "SOSYAL-KATEGORI",
                pick(
                    "Kategoriler kaydedilemedi.",
                    "Couldn't save your categories.",
                ),
            ),
            E::InvalidInput("chat off") => (
                "SOSYAL-SOHBET-KAPALI",
                pick("Sohbet şu an kapalı.", "Chat is off right now."),
            ),
            E::InvalidInput("chat not allowed") => (
                "SOSYAL-SOHBET-YOK",
                pick(
                    "Buraya artık mesaj gönderemezsin.",
                    "You can't send messages here anymore.",
                ),
            ),
            E::InvalidInput("chat text") => (
                "SOSYAL-SOHBET-METIN",
                pick(
                    "Mesaj en çok 300 karakter olabilir.",
                    "A message has up to 300 characters.",
                ),
            ),
            E::InvalidInput("chat message") => (
                "SOSYAL-SOHBET-MESAJ",
                pick("Bu mesaj artık yok.", "This message is gone."),
            ),
            E::InvalidInput("plan gone") => (
                "SOSYAL-PLAN-YOK",
                pick("Bu plan artık yok.", "This plan is gone."),
            ),
            E::InvalidInput("plan notes" | "crewmate requests") => (
                "SOSYAL-EKIP-YENI",
                pick(
                    "Bu şu an kullanılamıyor. Biraz sonra tekrar dene.",
                    "This isn't available right now. Try again in a bit.",
                ),
            ),
            E::InvalidInput("not a crewmate") => (
                "SOSYAL-EKIP-ARKADAS",
                pick(
                    "Bu oyuncuya şu an istek gönderilemiyor.",
                    "You can't send this player a request right now.",
                ),
            ),
            E::InvalidInput(_) | E::Protocol(_) => (
                "SOSYAL",
                pick(
                    "Bir sorun oluştu. Biraz sonra tekrar dene.",
                    "Something went wrong. Try again in a bit.",
                ),
            ),
            // Said by `social_special` above, with their details.
            E::Restricted { .. }
            | E::Limited { .. }
            | E::SignupsClosed { .. }
            | E::TextLocked { .. }
            | E::ChatRejected { .. }
            | E::ChatMuted { .. }
            | E::PlanNoteRejected { .. } => (
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
                C::CallNeedsServer => (
                    "SOSYAL-CAGRI-SUNUCU",
                    pick(
                        "Ekibini çağırmak için çok oyunculu bir sunucuda olmalısın.",
                        "Join a multiplayer server to call your crew.",
                    ),
                ),
                C::CallServerHidden => (
                    "SOSYAL-CAGRI-ADRES",
                    pick(
                        "Ekibini buraya çağırmak için sunucunun adresini göstermelisin.",
                        "To call your crew here, show this server's address.",
                    ),
                ),
                C::LimitCrews => (
                    "SOSYAL-EKIP-SINIR",
                    pick(
                        "Daha fazla ekipte olamazsın.",
                        "You can't be in any more crews.",
                    ),
                ),
                C::CrewsChanged => (
                    "SOSYAL-EKIP-SIRA",
                    pick(
                        "Ekiplerin bu arada değişti. Sırayı yeniden dene.",
                        "Your crews changed meanwhile. Try the order again.",
                    ),
                ),
                C::TargetNotFound => (
                    "SOSYAL-RAPOR-YOK",
                    pick(
                        "Bu artık raporlanamıyor.",
                        "This can't be reported anymore.",
                    ),
                ),
                C::LimitBlocks => (
                    "SOSYAL-ENGEL-SINIR",
                    pick(
                        "En çok 1.000 kişiyi engelleyebilirsin.",
                        "You can block up to 1,000 people.",
                    ),
                ),
                C::ChatOff => (
                    "SOSYAL-SOHBET-KAPALI",
                    pick("Sohbet şu an kapalı.", "Chat is off right now."),
                ),
                C::MessageNotFound => (
                    "SOSYAL-SOHBET-MESAJ",
                    pick("Bu mesaj artık yok.", "This message is gone."),
                ),
                C::ChatNotPermitted => (
                    "SOSYAL-SOHBET-IZIN",
                    pick(
                        "Bu mesajda bunu yapamazsın.",
                        "You can't do that with this message.",
                    ),
                ),
                C::ChatTooOld => (
                    "SOSYAL-SOHBET-SURE",
                    pick(
                        "Bu mesajın süresi doldu.",
                        "It's too late for this message.",
                    ),
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
                C::TeamApprovalRequired => (
                    "SOSYAL-EKIP-ONAY",
                    pick(
                        "Bu ekibe liderler onaylayınca katılınır.",
                        "This crew's leaders approve newcomers.",
                    ),
                ),
                C::TeamRequestNotFound => (
                    "SOSYAL-EKIP-ISTEK-YOK",
                    pick(
                        "Bu istek artık geçerli değil.",
                        "This request isn't valid anymore.",
                    ),
                ),
                C::LimitTeamRequests => (
                    "SOSYAL-EKIP-ISTEK-DOLU",
                    pick(
                        "Bu ekibin bekleyen çok isteği var. Biraz sonra tekrar dene.",
                        "This crew has too many requests waiting. Try again later.",
                    ),
                ),
                C::LimitOutgoingTeamRequests => (
                    "SOSYAL-EKIP-ISTEK-SINIR",
                    pick(
                        "Bekleyen 3 isteğin var. Önce birini geri çek.",
                        "You have 3 requests waiting. Withdraw one first.",
                    ),
                ),
                C::PlanTime => (
                    "SOSYAL-PLAN-SAAT",
                    pick(
                        "Bu saat seçilemez. Önümüzdeki 7 günden bir saat seç.",
                        "That time can't be picked. Pick one in the next 7 days.",
                    ),
                ),
                C::LimitTeamPlans => (
                    "SOSYAL-PLAN-DOLU",
                    pick(
                        "Ekipte en çok 3 plan olabilir.",
                        "A crew can have up to 3 plans.",
                    ),
                ),
                C::PlanExists => (
                    "SOSYAL-PLAN-VAR",
                    pick(
                        "Bu saatte zaten bir plan var.",
                        "There's a plan at that time already.",
                    ),
                ),
                C::PlanNotFound => (
                    "SOSYAL-PLAN-YOK",
                    pick("Bu plan artık yok.", "This plan is gone."),
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
