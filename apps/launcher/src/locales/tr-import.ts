// Bringing a player over from another launcher (IMPORT1): the first-run step, Settings → Veri
// and what came over. Launcher names (TLauncher, CurseForge, ...) are brand names and stay as they
// are; "Projects" is the Projects instance.
export const trImport = {
  step: {
    title: "Başka bir launcher'dan geldiğini gördük",
    body: "Alıştığın ayarları, sunucuları ve kurulumları buraya aktarabilirsin. Oradaki dosyaların olduğu gibi kalır.",
    import: "Aktar",
    skip: "Aktarmadan devam et",
  },
  /**
   * Group headings: where the rows under them go, as a sentence (LANDING-L: "Oynandıkları sürümün
   * kurulumuna" was hard to read as a heading).
   */
  groups: {
    projects: "Projects kurulumuna aktarılır",
    /** Each world goes into the instance of the version it was played on; one no instance fits stays. */
    worlds: "Her dünya kendi sürümünün kurulumuna aktarılır",
    instances: "Yeni kurulum olarak aktarılır",
  },
  rows: {
    /** One row above the launchers: the game settings come from one place (IMPORT-SET). */
    options: "Oyun ayarları",
    optionsMeta: "Tuşlar, görüntü ve ses",
    /** An earlier import brought them from the place shown: unticked, so later changes stay. */
    optionsBefore: "Daha önce aktarıldı",
    /** The select that picks where they come from (screen readers). */
    optionsFrom: "Ayarlar nereden gelsin",
    /** Before that select, so it reads as the settings' place, not the import's. */
    optionsFromLabel: "Nereden",
    /** The only place, an instance of another launcher ("TLauncher'dan" names a launcher). */
    optionsFromInstance: "{{name}} kurulumundan",
    servers: "Sunucu listesi",
    serversMeta: "{{count}} sunucu",
    worlds: "Dünyalar",
    worldsMeta: "{{count}} dünya",
    resourcePacks: "Kaynak paketleri",
    resourcePacksMeta: "{{count}} paket",
    shaderPacks: "Shader'lar",
    shaderPacksMeta: "{{count}} paket",
    screenshots: "Ekran görüntüleri",
    screenshotsMeta: "{{count}} görüntü",
  },
  profile: {
    /** What it runs, when its name does not say it ("Forge 1.20.1"). */
    version: "{{loader}} {{version}}",
    mods: "{{count}} mod",
    /** After the mod count ("21 mod, 3 tanesi uymuyor"). */
    leftMods: "{{count}} tanesi bu sürüme uymuyor",
    /** None of its mods fits the version it runs. */
    noModsFit: "Modları bu sürüme uymuyor",
    worlds: "{{count}} dünya",
    noOptifine: "OptiFine olmadan",
    /** An earlier import made it into an instance that still exists. */
    imported: "Kütüphanende",
    loader: "{{name}} desteklenmiyor",
    unknownLoader: "Bu mod yükleyici desteklenmiyor",
    unknownVersion: "Sürümü okunamadı",
  },
  space: {
    /** A verb after the size: "3,7 GB, diskte" put a sentence comma beside the decimal one. */
    total: "Seçtiklerin {{size}} tutuyor, diskte {{free}} boş",
    totalNoFree: "Seçtiklerin {{size}} tutuyor",
    nothing: "Aktarılacak bir şey seçmedin",
    short: "Diskte yeterli yer yok. Bazı seçimleri kaldır.",
  },
  progress: {
    title: "Aktarılıyor",
    preparing: "Hazırlanıyor",
    label: "Aktarım ilerlemesi",
    cancel: "İptal et",
    stopping: "İptal ediliyor",
  },
  done: {
    title: "Aktarım tamamlandı",
    /** Some items did not come over; the summary says which and why. */
    titleSome: "Aktarım tamamlandı, bazıları gelmedi",
    titleCancelled: "Aktarım iptal edildi",
    titleFailed: "Aktarım yapılamadı",
    summary: "Özet",
    summaryTitle: "Aktarım özeti",
    close: "Tamam",
  },
  /**
   * What a summary line is about, inside a sentence ("Diskte yer kalmadığı için TLauncher'daki
   * dünyaların…"); `at` is the launcher it is in.
   */
  what: {
    options: "{{at}} oyun ayarların",
    servers: "{{at}} sunucuların",
    worlds: "{{at}} dünyaların",
    resourcePacks: "{{at}} kaynak paketlerin",
    shaderPacks: "{{at}} shader paketlerin",
    screenshots: "{{at}} ekran görüntülerin",
  },
  /**
   * Where a summary line's things came from ("TLauncher'dan 3 dünya geldi."). The suffixes
   * follow how each name is said: "Modrinth App'ten", "PolyMC'den".
   */
  from: {
    minecraft: "Minecraft Launcher'dan",
    tLauncher: "TLauncher'dan",
    skLauncher: "SKlauncher'dan",
    curseForge: "CurseForge'dan",
    modrinth: "Modrinth App'ten",
    prism: "Prism Launcher'dan",
    polyMc: "PolyMC'den",
    multiMc: "MultiMC'den",
  },
  /** The same launchers as where things are ("TLauncher'daki dünyaların"). */
  at: {
    minecraft: "Minecraft Launcher'daki",
    tLauncher: "TLauncher'daki",
    skLauncher: "SKlauncher'daki",
    curseForge: "CurseForge'daki",
    modrinth: "Modrinth App'teki",
    prism: "Prism Launcher'daki",
    polyMc: "PolyMC'deki",
    multiMc: "MultiMC'deki",
  },
  outcome: {
    /** "Projects kurulumu" everywhere, as the picker's heading says (IMPORT-POLISH). */
    options: "Oyun ayarların {{from}} Projects kurulumuna geldi.",
    /** From one of the other launcher's instances ("Fabulously Optimized kurulumundan"). */
    optionsFromInstance: "Oyun ayarların {{name}} kurulumundan Projects kurulumuna geldi.",
    /** Projects already had exactly these settings: nothing changed. */
    optionsSame: "{{What}} zaten aynı.",
    servers: "{{from}} {{count}} sunucu eklendi.",
    worlds: "{{from}} {{count}} dünya geldi.",
    /**
     * Where worlds went, when they went to more than one instance. The count comes first: after
     * a name that ends in a version ("Forge 1.20.1") a number read as part of it.
     */
    worldsIn: "{{count}} tanesi {{name}} kurulumunda",
    /** Where worlds went, when they all went to one instance (a sentence, never a fragment). */
    worldsInOne: "{{name}} kurulumunda duruyor",
    resourcePacks: "{{from}} {{count}} kaynak paketi geldi.",
    /** "paket", as the picker counts them ("2 paket"). */
    shaderPacks: "{{from}} {{count}} shader paketi geldi.",
    screenshots: "{{from}} {{count}} ekran görüntüsü geldi.",
    /**
     * When some of them stayed behind the line says so itself, with the total, so the mark beside
     * it and the follow-up's "Kalan 1 tanesi" match it.
     */
    serversSome: "{{from}} {{total}} sunucunun {{count}} tanesi eklendi.",
    worldsSome: "{{from}} {{total}} dünyanın {{count}} tanesi geldi.",
    resourcePacksSome: "{{from}} {{total}} kaynak paketinin {{count}} tanesi geldi.",
    shaderPacksSome: "{{from}} {{total}} shader paketinin {{count}} tanesi geldi.",
    screenshotsSome: "{{from}} {{total}} ekran görüntüsünün {{count}} tanesi geldi.",
    /**
     * After a count line when some of them did not come over, with why. Never a number first: a
     * sentence after "Forge 1.20.1 kurulumunda" read "1.20.1. 1".
     */
    leftBecause: {
      noSpace: "Kalan {{count}} tanesine diskte yer yetmedi.",
      inUse: "Kalan {{count}} tanesi açık bir oyunda kullanıldığı için gelmedi.",
      unreadable: "Kalan {{count}} tanesi okunamadı.",
      noTarget: "Kalan {{count}} tanesi için Projects kurulumu hazırlanamadı.",
      failed: "Kalan {{count}} tanesi aktarılamadı.",
      cancelled: "Kalan {{count}} tanesi iptal edildi.",
      /**
       * Worlds whose version and loader no instance here plays (their own was not picked): the
       * sentence says what brings them.
       */
      needsInstance: "Kalan {{count}} tanesi, oynandığı kurulumu da aktarırsan gelir.",
    },
    already: "{{What}} zaten burada.",
    profile: "{{name}} kütüphanene eklendi.",
    profileMods: "{{name}} kütüphanene eklendi, içinde {{count}} mod var.",
    profileLeftMods: "{{count}} mod bu sürüme uymadığı için gelmedi.",
    profileLeftFiles: "Bazı dosyaları gelmedi.",
    profileAlready: "{{name}} zaten kütüphanende.",
    profileUnsupported: "{{name}} gelmedi, bu sürüm desteklenmiyor.",
    profileNoSpace: "{{name}} için diskte yer kalmadı.",
    profileFailed: "{{name}} aktarılamadı.",
    noSpace: "Diskte yer kalmadığı için {{what}} gelmedi.",
    noTarget: "Projects kurulumu hazırlanamadığı için {{what}} gelmedi.",
    unreadable: "{{What}} okunamadı.",
    inUse: "{{What}} açık bir oyunda kullanıldığı için gelmedi.",
    needsInstance: "{{What}}, oynandıkları kurulumları da aktarırsan gelir.",
    failed: "{{What}} aktarılamadı.",
    cancelled: "Kalanlar iptal edildi.",
  },
  settings: {
    title: "Başka launcher'dan aktar",
    help: "TLauncher, CurseForge, Modrinth, Prism ve resmî launcher'daki ayarların, dünyaların ve kurulumların buraya gelir.",
    /** The dialog it opens says "Aktar" too (IMPORT-POLISH). */
    action: "Aktar",
    dialogBody: "Seçtiklerin buraya kopyalanır. Oradaki dosyaların olduğu gibi kalır.",
    notFound: "Bu bilgisayarda başka bir launcher'ın verisi bulunamadı.",
    later: "Vazgeç",
    background: "Arka planda sürsün",
  },
  /** The notification when a long import ends while the launcher is out of sight. */
  attention: {
    title: "Aktarım tamamlandı",
    body: "Diğer launcher'dan getirdiklerin hazır.",
  },
  /** The game settings' "Geri al" in a summary: every file back as it was before the import. */
  undo: {
    action: "Geri al",
    done: "Oyun ayarların aktarımdan önceki hâline döndü.",
    /** A newer import replaced it, or its copies are gone. */
    gone: "Artık geri alamazsın.",
    /** The v1 launcher's step says what came with its own words. */
    v1: "Oyun ayarların eski launcher'dan Projects kurulumuna geldi.",
  },
  /**
   * The v1 launcher's summary (`features/migrate`): what its game data brought, by kind
   * ("Dünyaların, ekran görüntülerin ve paketlerin …"), never as a count of files.
   */
  v1: {
    files: "{{things}} Projects kurulumuna geldi.",
    /** A note says something stayed behind (or no kind is known): no kind is named. */
    filesSome: "Oyun dosyaların Projects kurulumuna geldi.",
    things: {
      worlds: "dünyaların",
      screenshots: "ekran görüntülerin",
      packs: "paketlerin",
    },
    /** Its choices, as the picker's rows are written: a title and a quiet second line. */
    choices: {
      premium: "Microsoft hesabın",
      nickname: "Oyuncu adı",
      afk: "AFK hesapları",
      afkMeta: "{{count}} hesap",
      gameData: "Oyun ayarları ve dosyaları",
      gameDataMeta: "Sunucu listesi, ekran görüntüleri, kaynak ve shader paketleri",
      worlds: "Dünyalar",
      worldsMeta: "{{count}} dünya, {{size}}",
    },
  },
};
