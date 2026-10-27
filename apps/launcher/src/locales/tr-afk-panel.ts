// SHEET-UX: an AFK account's settings panel (desktop and 7/24 Bulut) in a few sections, its change
// bar, and the card's settings button while edits wait unsaved (docs/tasks/sheet-ux.md).
export const trAfkPanel = {
  /** The rail, by what the player asks: where it connects, what it types, what it does, ... */
  sections: {
    connection: "Bağlantı",
    commands: "Komutlar",
    /** Not "Oyunda": that is the card's word for an account in the game. */
    game: "Oyun içi",
    replies: "Otomatik yanıt",
    log: "Kayıt ve uyarılar",
  },
  /** A rail section holding edits not saved yet (read after its name). */
  unsavedSection: "kaydedilmemiş değişiklik var",
  /** The card's settings button while the panel was closed with edits in it. */
  settingsUnsaved: "Ayarlar, kaydedilmemiş değişiklik var",
  /** The change bar's region. */
  barLabel: "Kaydedilmemiş değişiklikler",
  /** Under the one address field ("mc.sunucu.net:25570"), when the port is out of range. */
  portRange: "Port 1 ile 65535 arasında olmalı.",
  /** A scheduled task's "every 300–600 sn". */
  interval: "Aralık",
  /** A running desktop account's plain save, beside "Kaydet ve yeniden bağlan". */
  saveOnly: "Yalnız kaydet",
  /** The toast after a close with edits waiting (a closed panel reads as a saved one). */
  kept: {
    title: "Değişikliklerin kaydedilmedi",
    body: "Ayarları yeniden açınca yerinde olacaklar.",
    open: "Ayarları aç",
  },
};
