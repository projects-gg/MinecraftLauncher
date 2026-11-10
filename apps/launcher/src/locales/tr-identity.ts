// SOC-ID: the ways into the friends account (Discord and Microsoft), "Bağlı hesaplar" and the
// profile name of a new account. Short, the phone's words; a sentence of explanation only where a step
// cannot be undone (a password typed on the wrong page, what goes with Discord).
export const trIdentity = {
  providers: {
    discord: "Discord",
    microsoft: "Microsoft",
  },
  consent: {
    body: "Discord ya da Microsoft hesabınla giriş yaparsın. Arkadaşların senin adını, profil fotoğrafını ve ne oynadığını görür. E-postan ve şifren bize gelmez.",
    microsoft: "Microsoft ile Giriş Yap",
    /** Over Microsoft while it signs in only the accounts it is linked to. */
    linkedWay: "Microsoft hesabını bağladıysan",
    /** Home's invitation once Microsoft opens accounts too (no brand). */
    signIn: "Giriş Yap",
    waitingMicrosoft: "Tarayıcıda Microsoft'a giriş yap",
    waitingMicrosoftNote: "Giriş yapınca burada devam edersin.",
    password: "Şifreni yalnız Microsoft'un kendi sayfasında gir.",
    notLinked:
      "Bu Microsoft hesabı henüz bağlı değil. Önce Discord ile giriş yap, sonra Arkadaş ayarları'ndan bağla.",
    providerOff: "Şu an Microsoft ile giriş yapılamıyor. Başka bir yolla dene.",
  },
  name: {
    title: "Arkadaşların seni hangi adla görsün?",
    label: "Adın",
    rules: "3–16 karakter: harf, rakam, boşluk ya da alt çizgi (_)",
    save: "Devam Et",
    close: "Şimdi Değil",
    invalid: "Adın 3–16 karakter olmalı: harf, rakam, boşluk ya da alt çizgi (_).",
    rejected: "Bu ad kullanılamıyor.",
    tooMany: "Adını bugün daha fazla değiştiremezsin.",
    providerOff: "Şu an adını değiştiremezsin.",
    offline: "Şu an kaydedilemedi. Biraz sonra tekrar dene.",
    failed: "Ad kaydedilemedi. Tekrar dene.",
    saved: "Adın kaydedildi.",
  },
  settings: {
    name: "Profil Adı",
    change: "Değiştir",
    save: "Kaydet",
    cancel: "Vazgeç",
    linked: "Bağlı Hesaplar",
    linkedOn: "{{date}} bağlandı",
    remove: "Kaldır",
    link: {
      discord: "Discord Hesabını Bağla",
      microsoft: "Microsoft Hesabını Bağla",
    },
    /** A session's line: "Son kullanım dün, Microsoft ile girildi". */
    viaMicrosoft: "Microsoft ile girildi",
  },
  remove: {
    title: "{{provider}} bağlantısı kaldırılsın mı?",
    /**
     * What goes with Discord, however the account was opened (MS-REST: the skin needs a linked
     * Discord; AFK-MS: 7/24 Bulut needs none).
     */
    discord: "Discord fotoğrafın, Projects skinin ve forumdaki ekip etiketin de kalkar.",
    confirm: "Kaldır",
    cancel: "Vazgeç",
  },
  waiting: {
    microsoft: "Tarayıcıda Microsoft'a giriş yap",
    microsoftNote: "Giriş yapınca burada devam edersin.",
    discord: "Tarayıcıda Discord'u onayla",
    discordNote: "Onaylayınca burada devam edersin.",
    password: "Şifreni yalnız Microsoft'un kendi sayfasında gir.",
    reopen: "Tarayıcıyı Yeniden Aç",
    cancel: "Vazgeç",
  },
  done: {
    linked: "{{provider}} bağlandı.",
    removed: "{{provider}} bağlantısı kaldırıldı.",
  },
  errors: {
    providerOff: "Şu an {{provider}} hesabı bağlanamıyor.",
    exists: "Hesabına zaten bir {{provider}} hesabı bağlı.",
    notFound: "Bu hesap artık bağlı değil.",
    last: "Son bağlı hesabını kaldıramazsın.",
    taken: "Bu {{provider}} hesabı başka bir Projects hesabına bağlı.",
    proofFailed: "Bağlı hesaplarından biriyle giriş yap.",
    unavailable: "Bu {{provider}} hesabı şu an bağlanamıyor.",
    expired: "Tamamlanamadı. Tekrar dene.",
    tooMany: "Bugün daha fazla değişiklik yapamazsın.",
    offline: "Şu an bağlanılamıyor. Biraz sonra tekrar dene.",
    failed: "Bir sorun oluştu. Biraz sonra tekrar dene.",
  },
  notice: {
    linkedTitle: "Hesabına {{provider}} bağlandı",
    /** "Diğer cihazlardan çıkış yap" is the sessions' own button (one name per action). */
    linked: "Sen yapmadıysan kaldır ve diğer cihazlardan çıkış yap.",
    removedTitle: "Hesabından {{provider}} bağlantısı kaldırıldı",
    removed: "Sen yapmadıysan diğer cihazlardan çıkış yap.",
    open: "Bağlı Hesaplara Bak",
    ok: "Tamam",
  },
  /** A limited account that is new itself (opened with Microsoft): its own words. */
  limited: {
    title: "Hesabın yeni olduğu için {{date}} tarihine kadar:",
    crewFound: "Hesabın yeni olduğu için ekip kuramazsın. {{when}}",
    backup: "Hesabın yeni olduğu için ayar yedeği {{date}} tarihine kadar kapalı.",
    link: "Hesabın yeni olduğu için davet bağlantısı oluşturamazsın. {{when}}",
    plans: "Hesabın yeni olduğu için plan yapamazsın. {{when}}",
    links: "Hesabın yeni olduğu için bağlantı gönderemezsin. {{when}}",
  },
};
