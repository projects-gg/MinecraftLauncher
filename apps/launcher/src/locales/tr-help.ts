// Settings → Sorun yardımcısı (HELP1): one place that checks why the game won't start or connect,
// with one-tap fixes and a support summary to copy. Short, sentence case; no codes on screen (they
// go only into the copied summary). The fixes that already have a name elsewhere keep it ("Yer aç",
// "Kur ve oyna", "Onar", "Kurulum ayarlarını aç"): the panel uses those keys.
export const trHelp = {
  /** The Settings section, the palette item and the crash dialog's button. */
  title: "Sorun yardımcısı",
  /** A launch error's action: opens the troubleshooter. */
  find: "Sorunu bul",
  verdict: {
    checking: "Denetleniyor",
    checkingLine: "Birkaç saniye sürer.",
    fine: "Her şey yolunda",
    fineLine: "Oyun yine de açılmıyorsa özeti destek kanalına gönder.",
    /** After a failed launch, when the checks found nothing else. */
    nothingElse: "Başka bir sorun bulunamadı",
    problems: "{{count}} sorun bulundu",
    warnings: "{{count}} uyarı var",
    /** Screen readers, when a run ends ("Denetim bitti. 2 sorun bulundu."). */
    done: "Denetim bitti.",
  },
  recheck: "Tekrar denetle",
  /** The launch that failed before the troubleshooter opened; its own message is the line. */
  lastLaunch: "Son açılış",
  groups: {
    connection: "Bağlantı",
    computer: "Bu bilgisayar",
    game: "Oyun",
  },
  checks: {
    internet: "İnternet",
    server: "Projects sunucusu",
    /** The signed content: the pack's updates, news, the launcher's settings from Studio. */
    content: "Paket güncellemeleri",
    friends: "Arkadaşlar",
    microsoft: "Microsoft girişi",
    discord: "Discord",
    clock: "Saat",
    /** The team's word on the servers: Studio's maintenance notice or the status page. */
    notice: "Sunucu durumu",
    disk: "Disk alanı",
    java: "Java",
    memory: "Bellek",
    mods: "Modlar",
    crash: "Son çökme",
  },
  /** Heard before each row's words, and written in the copied summary. */
  states: {
    checking: "Denetleniyor",
    ok: "Tamam",
    note: "Bilgi",
    warning: "Uyarı",
    problem: "Sorun",
    skipped: "Denetlenemedi",
  },
  reasons: {
    internetOk: "Bağlısın.",
    internetSlow: "Bağlantı yavaş.",
    internetOffline: "İnternet bağlantısı yok.",
    internetLocal: "Ağa bağlısın ama internete çıkılamıyor.",
    internetSignIn: "Bu ağ önce giriş yapmanı istiyor.",
    internetNoDns: "Adresler bulunamıyor. Modemi yeniden başlatmayı dene.",
    internetInsecure: "Güvenli bağlantı kurulamıyor.",
    /** A wrong clock is why secure connections fail. */
    internetInsecureClock: "Saat yanlış olduğu için güvenli bağlantı kurulamıyor.",
    internetDown: "İnternete ulaşılamıyor.",
    serverOk: "{{ms}} ms, {{players}} oyuncu çevrimiçi.",
    /** The server did not say how many play. */
    serverOkQuiet: "Açık, {{ms}} ms.",
    serverSlow: "Ping yüksek: {{ms}} ms.",
    serverDown: "Sunucu şu an bağlantı kabul etmiyor.",
    serverNotFound: "Sunucunun adresi bulunamıyor.",
    serverUnreachable: "Sunucuya ulaşılamıyor.",
    /** Any check that needs the internet while it is down. */
    needsInternet: "İnternet olmadan denenemedi.",
    contentOk: "Güncel.",
    contentUnreachable: "Ulaşılamıyor. Kayıtlı içerikle devam ediliyor.",
    contentMissing: "Ulaşılamıyor. Paketler şimdilik indirilemez.",
    friendsOk: "Çalışıyor.",
    friendsDown: "Şu an çalışmıyor.",
    friendsUnreachable: "Ulaşılamıyor.",
    microsoftOk: "Ulaşılıyor.",
    microsoftDown: "Microsoft şu an yanıt vermiyor.",
    microsoftUnreachable: "Ulaşılamıyor. Microsoft ile giriş çalışmayabilir.",
    discordOk: "Ulaşılıyor.",
    discordUnreachable: "Ulaşılamıyor.",
    clockAhead: "Bilgisayarının saati {{amount}} ileride.",
    clockBehind: "Bilgisayarının saati {{amount}} geride.",
    diskOk: "{{free}} boş.",
    diskLow: "{{free}} boş kaldı.",
    diskFull: "Neredeyse yer kalmadı: {{free}} boş.",
    diskUnknown: "Boş alan okunamadı.",
    javaReady: "Java {{major}} hazır.",
    javaDownloads: "İlk açılışta indirilecek.",
    javaCustomOk: "Seçtiğin Java {{major}} uygun.",
    javaX86: "32 bit Java, oyun az bellek alır.",
    javaMissing: "Seçtiğin Java bulunamadı.",
    javaBroken: "Seçtiğin Java çalışmıyor.",
    javaTooOld: "Java {{major}} eski kalıyor, Java {{need}} gerekiyor.",
    javaTooNew: "Java {{major}} çok yeni, en çok Java {{need}} çalışır.",
    memoryTooHigh: "Oyuna bilgisayarın belleğinden fazlası ayrılmış.",
    memoryHigh: "Oyuna ayrılan bellek bilgisayarı zorlayabilir.",
    modsOk: "{{count}} mod, sorun yok.",
    modsNone: "Kurulu mod yok.",
    modsWarning: "{{count}} mod uyarısı var.",
    modsBlocking: "{{count}} mod sorunu oyunun açılmasını engelliyor.",
    crashNone: "Son 7 günde çökme olmadı.",
    crashResolved: "Son çökmeden sonra oyun hatasız açıldı.",
    /** "Dün: Oyunun belleği yetmedi." (the crash dialog's title for its kind). */
    crashRecent: "{{when}}: {{title}}.",
    /** "Dün: Sodium oyunu çökertmiş olabilir." */
    crashRecentMod: "{{when}}: {{name}} oyunu çökertmiş olabilir.",
    checkFailed: "Denetlenemedi.",
  },
  /** How far the clock is off. */
  amount: {
    minutes: "{{count}} dakika",
    hours: "{{count}} saat",
    days: "{{count}} gün",
  },
  /** Fixes without a name elsewhere (the others reuse theirs). */
  fixes: {
    showMods: "Göster",
    crashDetails: "Ayrıntılar",
    memoryAuto: "Otomatik yap",
    clockSettings: "Saat ayarları",
    statusPage: "Durum sayfası",
  },
  /** A fix that did not go through. */
  fixFailed: "Bu düzeltme şu an yapılamadı.",
  /** The memory fix's confirmation. */
  memorySet: "Bellek Otomatik oldu.",
  footer: {
    stuck: "Çözülmedi mi?",
    copy: "Destek için özeti kopyala",
    copied: "Özet kopyalandı. Destek kanalına yapıştırabilirsin.",
    discord: "Discord destek kanalına git",
    /** This computer cannot reach Discord: the forum instead. */
    forum: "Forumda destek iste",
    /** The support link is a web page, not Discord. */
    support: "Destek sayfasına git",
  },
  /** The copied summary (plain text; codes in brackets for support). */
  summary: {
    title: "Projects Launcher sorun özeti",
    launcher: "Launcher",
    system: "Sistem",
    memory: "Bellek",
    instance: "Kurulum",
    official: "Projects resmî paketi",
    custom: "Kendi kurulumu",
    checked: "Denetim",
    lastLaunch: "Son açılış hatası",
  },
};
