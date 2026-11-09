// PACK-EDIT: the player's own versions of the official pack's mods (Modlar → Önerilenler) and what
// the launcher fixes by itself before a launch. Short, "sen"; buttons, chips and the dialog title in
// Title Case (owner, 7 Nov), notes as sentences. No file names, ids or version jargon beyond the
// number the player picks.
export const trPackEdit = {
  /** Quiet link on a recommended mod's row (shown while `features.packEdit` is on). */
  change: "Sürümü Değiştir",
  changeFor: "{{name}} sürümünü değiştir",
  /** Chip on a row that runs the player's own version. */
  yours: "Senin Sürümün",
  /** Chip on a row whose mod a jar the player added stands in for. */
  yourFile: "Senin Dosyan",
  /** Note under a row: the player picked the version. */
  chosen: "{{version}} sürümünü kullanıyorsun.",
  /** Note under a row: the launcher moved it up for the player's mods. */
  needed: "{{names}} istediği için {{version}} sürümüne geçildi.",
  /** Note under a row whose mod the player's own jar replaces. */
  standIn: "Mod klasörüne eklediğin kopya kullanılıyor.",
  /** Button beside the note: back to the pack's version. */
  back: "Paketin Sürümüne Dön",
  backDone: "{{name}} paketin sürümüne döndü.",
  dialog: {
    title: "{{name}} Sürümleri",
    /** The one sentence: what happens to the choice later (not obvious). */
    description: "Seçtiğin sürüm, paket daha yenisine geçene kadar kalır.",
    inPack: "Paketteki",
    inUse: "Kullanılan",
    beta: "Beta",
    use: "Bu Sürümü Kullan",
    useBack: "Paketin Sürümüne Dön",
    cancel: "Vazgeç",
    installing: "Kuruluyor",
    done: "{{name}} {{version}} kuruldu.",
    empty: "Bu Minecraft sürümü için başka sürüm yok.",
    failed: "Sürümler yüklenemedi. İnternet bağlantını kontrol edip tekrar dene.",
    retry: "Tekrar Dene",
    published: "{{when}} yayımlandı",
  },
  /** Launch toasts: what the launcher fixed before the game started. */
  fixed: {
    upgraded: "{{name}} güncellendi",
    upgradedMany: "{{count}} mod güncellendi",
    upgradedWhy: "{{names}} daha yeni bir sürüm istiyordu.",
    turnedOff: "{{name}} iki kez yüklüydü, fazla kopyası kapatıldı.",
    turnedOffMany: "{{count}} mod iki kez yüklüydü, fazla kopyaları kapatıldı.",
    undo: "Geri Al",
    undone: "Kopya geri açıldı. Bir dahaki açılışta sana sorulacak.",
    undoLater: "Kopya, oyunu bir dahaki açışında geri açılır ve sana sorulur.",
    details: "Ayrıntılar",
  },
};
