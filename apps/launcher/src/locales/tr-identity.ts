// SOC-ID: the ways into the friends account (Discord and Microsoft), "Bağlı hesaplar", the profile
// name of a new account, and what works only with Discord. Short, the phone's words; a sentence of
// explanation only where a step cannot be undone (a password typed on the wrong page, what goes with
// Discord).
export const trIdentity = {
  providers: {
    discord: "Discord",
    microsoft: "Microsoft",
  },
  consent: {
    body: "Discord ya da Microsoft hesabınla giriş yaparsın. Arkadaşların adını, profil fotoğrafını ve ne oynadığını görür. E-postan ve şifren bize gelmez.",
    microsoft: "Microsoft ile giriş yap",
    /** Over Microsoft while it signs in only the accounts it is linked to. */
    linkedWay: "Bağlı hesabınla gir",
    /** Home's invitation once Microsoft opens accounts too (no brand). */
    signIn: "Giriş yap",
    waitingMicrosoft: "Tarayıcıda Microsoft'a gir",
    waitingMicrosoftNote: "Girince burada devam edersin.",
    password: "Şifreni yalnız Microsoft'un kendi sayfasında gir.",
    notLinked:
      "Bu Microsoft hesabı henüz bir Projects hesabına bağlı değil. Önce Discord ile gir, sonra Ayarlar'dan bağla.",
    providerOff: "Şu an Microsoft ile giriş yapılamıyor. Başka bir yolla dene.",
  },
  name: {
    title: "Arkadaşların seni hangi adla görsün?",
    label: "Adın",
    rules: "3–16 karakter: harf, rakam, boşluk ya da _",
    save: "Devam et",
    close: "Şimdi değil",
    invalid: "Adın 3–16 karakter olmalı: harf, rakam, boşluk ya da _.",
    rejected: "Bu ad kullanılamıyor.",
    tooMany: "Adını bugün daha fazla değiştiremezsin.",
    providerOff: "Şu an adını değiştiremezsin.",
    offline: "Şu an kaydedilemedi. Biraz sonra tekrar dene.",
    failed: "Ad kaydedilemedi. Tekrar dene.",
    saved: "Adın kaydedildi.",
  },
  settings: {
    name: "Profil adı",
    change: "Değiştir",
    save: "Kaydet",
    cancel: "Vazgeç",
    linked: "Bağlı hesaplar",
    linkedOn: "{{date}} bağlandı",
    remove: "Kaldır",
    link: {
      discord: "Discord hesabını bağla",
      microsoft: "Microsoft hesabını bağla",
    },
    /** A session's line: "Son kullanım dün, Microsoft ile girildi". */
    viaMicrosoft: "Microsoft ile girildi",
  },
  remove: {
    title: "{{provider}} bağlantısı kaldırılsın mı?",
    /** Discord is the account's number: 7/24 Bulut and the Projects skin need it. */
    discordNumber:
      "Discord olmadan 7/24 Bulut açılmaz. Projects skini eşlemen ve forumdaki ekip etiketin de kalkar.",
    discord: "Discord fotoğrafın ve forumdaki ekip etiketin de kalkar.",
    confirm: "Kaldır",
    cancel: "Vazgeç",
  },
  waiting: {
    microsoft: "Tarayıcıda Microsoft'a gir",
    microsoftNote: "Girince burada devam edersin.",
    discord: "Tarayıcıda Discord'u onayla",
    discordNote: "Onaylayınca burada devam edersin.",
    password: "Şifreni yalnız Microsoft'un kendi sayfasında gir.",
    reopen: "Tarayıcıyı yeniden aç",
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
    proofFailed: "Bağlı hesaplarından biriyle gir.",
    unavailable: "Bu {{provider}} hesabı şu an bağlanamıyor.",
    expired: "Tamamlanamadı. Tekrar dene.",
    tooMany: "Bugün daha fazla değişiklik yapamazsın.",
    offline: "Şu an bağlanılamıyor. Biraz sonra tekrar dene.",
    failed: "Bir sorun oluştu. Biraz sonra tekrar dene.",
  },
  notice: {
    linkedTitle: "Hesabına {{provider}} bağlandı",
    linked: "Sen yapmadıysan kaldır ve öteki oturumları kapat.",
    removedTitle: "Hesabından {{provider}} bağlantısı kaldırıldı",
    removed: "Sen yapmadıysan öteki oturumları kapat.",
    open: "Bağlı hesapları aç",
    ok: "Tamam",
  },
  /** SKIN1 and 7/24 Bulut, for an account not opened with Discord. */
  discordOnly: "Bu özellik şimdilik yalnız Discord ile açılmış hesaplarda var.",
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
