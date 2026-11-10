// QOL-L: the launcher's quality-of-life pass (docs/tasks/qol-l.md). Short, titles in Title Case, no codes.
export const trQol = {
  launch: {
    /**
     * OYNA stopped on an expired Microsoft session or its lost secrets: the toast's line, its
     * button "Yeniden giriş yap" (the backend's words end with that same advice).
     */
    signedOut: "Hesabının oturumu kapandı.",
    /** OYNA without any account: the toast's line, its button "Hesap ekle". */
    noAccount: "Oynamak için bir hesap gerekiyor.",
  },
  login: {
    /**
     * The Microsoft sign-in's main button after "Bu hesapta Minecraft yok": the sign-in lets the
     * player pick the account, so it signs in with another one.
     */
    otherAccount: "Başka Hesapla Giriş Yap",
  },
  welcome: {
    /**
     * The welcome's last step when the Projects instance could not be prepared: "Hazırım" goes on
     * and it is made again in the background (the line under says why, beside "Tekrar dene").
     */
    later: "Kurulum sonra hazırlanacak",
  },
  picker: {
    /**
     * The command bar's installation menu while a launch prepares its files: the other rows are
     * off, and this says how to free them (the bar's "İptal et").
     */
    lockedPreparing: "Başka kurulum seçmek için önce hazırlığı iptal et.",
    /** The same menu while the game opens or runs. */
    lockedPlaying: "Oyun kapanınca başka kurulum seçebilirsin.",
  },
  mods: {
    /** Modlar → Önerilenler while a game of the instance prepares or runs: every switch is off. */
    locked: "Oyun açıkken modlar değiştirilemez.",
    /** Modlar → Setler, the same. */
    setsLocked: "Oyun açıkken setler değiştirilemez.",
  },
  create: {
    /**
     * "Yeni kurulum": the picked mod loader has no build for the picked Minecraft version (under
     * the loader box; "Kurulum oluştur" stays off). `loader` is its name ("NeoForge").
     */
    noLoaderBuild: "{{loader}} bu sürümü desteklemiyor. Başka bir sürüm ya da yükleyici seç.",
    /** The loader's versions could not be fetched (no connection). */
    loaderFailed: "Yükleyici sürümleri alınamadı. Bağlantını kontrol et.",
  },
  install: {
    /**
     * "Yeni kurulum": the footer's stop while a modpack or a set installs (Esc no longer stops
     * it). Stopping deletes the half-made instance: one sentence says so. "Set ekle" asks the
     * same question without the sentence (stopping there deletes no instance).
     */
    stopTitle: "Kurulum durdurulsun mu?",
    stopBody: "Yarım kalan kurulum silinir.",
    stop: "Durdur",
  },
  copy: {
    /** Kütüphane → "Kopyasını oluştur" while the copy is made (big worlds take a while). */
    copying: "{{name}} kopyalanıyor",
  },
  afk: {
    /**
     * A desktop AFK account the login plugin kept turning away (its time ran out: no or wrong
     * login commands): the card's button opens its settings on Komutlar ("Bağla" ran the loop).
     */
    fixLogin: "Girişi Düzelt",
  },
};
