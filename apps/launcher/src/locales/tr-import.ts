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
  /** Group headings: where the rows under them go. */
  groups: {
    projects: "Projects kurulumuna",
    instances: "Yeni kurulum olarak",
  },
  rows: {
    options: "Oyun ayarları",
    optionsMeta: "Dil, tuşlar, görüntü",
    /** The Projects instance has game settings of its own: the row cannot be ticked. */
    optionsKept: "Zaten ayarlı",
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
    leftMods: "{{count}} tanesi uymuyor",
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
    total: "Seçtiklerin {{size}}, diskte {{free}} boş",
    totalNoFree: "Seçtiklerin {{size}}",
    nothing: "Aktarılacak bir şey seçmedin",
    short: "Diskte yeterli yer yok. Bazı seçimleri kaldır.",
  },
  progress: {
    title: "Aktarılıyor",
    preparing: "Hazırlanıyor",
    label: "Aktarım ilerlemesi",
    cancel: "İptal",
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
  /** What a summary line is about, inside a sentence ("Diskte yer kalmadığı için dünyaların…"). */
  what: {
    options: "oyun ayarların",
    servers: "sunucuların",
    worlds: "dünyaların",
    resourcePacks: "kaynak paketlerin",
    shaderPacks: "shader'ların",
    screenshots: "ekran görüntülerin",
  },
  outcome: {
    options: "Oyun ayarların Projects'e geldi.",
    optionsKept: "Projects'teki oyun ayarların korundu.",
    servers: "{{count}} sunucu eklendi.",
    worlds: "{{count}} dünya geldi.",
    /** Where worlds went, when they went to more than one instance. */
    worldsIn: "{{name}} kurulumunda {{count}}",
    resourcePacks: "{{count}} kaynak paketi geldi.",
    shaderPacks: "{{count}} shader geldi.",
    screenshots: "{{count}} ekran görüntüsü geldi.",
    /** After a count line when some of them did not come over, with why. */
    leftBecause: {
      noSpace: "{{count}} tanesi için diskte yer kalmadı.",
      inUse: "{{count}} tanesi açık bir oyunda kullanıldığı için gelmedi.",
      unreadable: "{{count}} tanesi okunamadı.",
      noTarget: "{{count}} tanesi için Projects kurulumu hazırlanamadı.",
      failed: "{{count}} tanesi aktarılamadı.",
      cancelled: "{{count}} tanesi iptal edildi.",
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
    inUse: "Açık bir oyunda kullanılan dünyalar gelmedi.",
    failed: "{{What}} aktarılamadı.",
    cancelled: "Kalanlar iptal edildi.",
  },
  settings: {
    title: "Başka launcher'dan aktar",
    help: "TLauncher, CurseForge, Modrinth, Prism ve resmî launcher'daki ayarların, dünyaların ve kurulumların buraya gelir.",
    action: "İçe aktar",
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
};
