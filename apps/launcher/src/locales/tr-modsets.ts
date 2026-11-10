// Ready-made mod sets (MODSET1): "Hazır set" in the new-instance dialog, "Setler" and "Set ekle"
// in the mod center, the badge on a set's files. The team writes the sets in Studio; their names
// and summaries come from the content.
export const trModSets = {
  /** What a set is for (its card's mark). */
  purpose: {
    pvp: "PvP",
    performance: "Performans",
    visual: "Görsel",
    other: "Diğer",
  },
  kind: {
    mod: "Mod",
    resourcePack: "Kaynak Paketi",
    shaderPack: "Shader",
  },
  card: {
    mods: "{{count}} mod",
    resourcePacks: "{{count}} kaynak paketi",
    shaderPacks: "{{count}} shader paketi",
    /** More item icons than the strip shows. */
    more: "+{{count}}",
    /** A set that is not made for this instance: its versions. */
    forVersions: "{{versions}} için",
    /** A set whose mods need another loader. */
    forLoader: "{{loader}} kurulumları için",
    added: "Ekli",
    /** Screen reader name of the item icons. */
    contents: "İçindekiler: {{names}}",
  },
  summary: {
    mods: "{{count}} mod",
    resourcePacks: "{{count}} kaynak paketi",
    shaderPacks: "{{count}} shader paketi",
    added: "{{items}} eklenecek",
    updated: "{{items}} güncellenecek",
    removed: "{{items}} kaldırılacak",
    download: "{{size}} indirilecek.",
    noFiles: "Dosya değişmeyecek.",
  },
  item: {
    present: "Zaten var",
    library: "Birlikte gelir",
  },
  create: {
    source: "Hazır Set",
    pick: "Bir set seç.",
    next: "Devam Et",
    back: "Setlere Dön",
    version: "Minecraft Sürümü",
    contents: "İçindekiler",
    libraries: "Birlikte gelenler",
    download: "{{size}} indirilecek",
    done: "{{name}} hazır.",
    offlineTitle: "Hazır setler yüklenemedi",
    offlineBody: "İnternet bağlantını kontrol edip tekrar dene.",
    emptyTitle: "Henüz hazır set yok",
  },
  center: {
    section: "Setler",
    sectionCount: "Setler ({{count}})",
    add: "Set Ekle",
    emptyTitle: "Henüz set eklemedin",
    update: "Güncelleme Var",
    updateNow: "Güncelle",
    /** A set a version switch carried over that is not installed for this version yet. */
    install: "{{version}} için Kur",
    notForVersion: "Bu set {{version}} için yok.",
    retired: "Artık Sunulmuyor",
    pending: "Bazı dosyaları eksik.",
    complete: "Tamamla",
    remove: "Seti Kaldır",
    actions: "{{name}} Seti",
    present: "Kurulumda zaten olanlar",
    added: "{{name}} seti eklendi.",
    updated: "{{name}} seti güncellendi.",
    removed: "{{name}} seti kaldırıldı.",
    undo: "Geri Al",
    applied: "{{name}} setindeki değişiklikler uygulandı.",
    issues: "Bazı dosyalar değiştirilemedi. Sonra yeniden denenecek.",
    unavailable: "Setlerin son hâli yüklenemedi. Kurulumdaki hâli gösteriliyor.",
  },
  add: {
    title: "Set Ekle",
    submit: "Ekle",
    adding: "Ekleniyor",
    noneFits: "{{version}} için hazır set yok.",
    newInstance: "Yeni Kurulum Olarak Aç",
  },
  /** The chip on a file a set installed. */
  badge: "{{name}} Seti",
  /** The crash dialog, when the set of the mod the crash points at has an update. */
  crash: {
    update: "{{name}} setinin güncellemesi var. Bu sorunu düzeltebilir.",
    open: "Setlerde Güncelle",
  },
};
