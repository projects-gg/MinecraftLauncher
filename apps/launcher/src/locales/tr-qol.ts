// QOL-L: the launcher's quality-of-life pass (docs/tasks/qol-l.md). Short, sentence case, no codes.
export const trQol = {
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
     * it). Stopping deletes the half-made instance: one sentence says so.
     */
    stopTitle: "Kurulum durdurulsun mu?",
    stopBody: "Yarım kalan kurulum silinir.",
    stop: "Durdur",
  },
  copy: {
    /** Kütüphane → "Kopyasını oluştur" while the copy is made (big worlds take a while). */
    copying: "{{name}} kopyalanıyor",
  },
};
