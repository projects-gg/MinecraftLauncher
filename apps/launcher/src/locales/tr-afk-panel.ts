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
    /** Not "Kayıt": the servers' /kayıt is signing up. */
    log: "Uyarılar ve günlük",
  },
  /** A rail section holding edits not saved yet (read after its name). */
  unsavedSection: "kaydedilmemiş değişiklik var",
  /** The card's settings button while the panel was closed with edits in it. */
  settingsUnsaved: "Ayarlar, kaydedilmemiş değişiklik var",
  /** A list row's switch, right beside it ("Komutu kullan" is its name). */
  use: "Kullan",
  /** A line's secret switch while it is not secret yet (pressed, it says "Gizli"). */
  makeSecret: "Gizle",
  ruleOrdinal: "{{n}}. kural",
  commandOrdinal: "{{n}}. komut",
  /** The change bar's region. */
  barLabel: "Kaydedilmemiş değişiklikler",
  /**
   * Under the one address field ("mc.sunucu.net:25570"), when the port is out of range. The colon
   * by its name and its sign: "iki nokta" alone read as the address's dots.
   */
  portRange: "İki nokta üst üsteden (:) sonraki sayı 1 ile 65535 arasında olmalı.",
  /** Two numbers of one wait: "5 sn ile 10 sn arası". */
  rangeJoin: "ile",
  rangeAfter: "arası",
  /** A scheduled task's "every 300–600 sn". */
  interval: "Aralık",
  /** A running desktop account's plain save, beside "Kaydet ve yeniden bağlan". */
  saveOnly: "Yalnız kaydet",
  /**
   * The toast after a close with edits waiting (a closed panel reads as a saved one); waiting, not
   * failed ("kaydedilmedi" read as an error).
   */
  kept: {
    title: "{{name}} ayarlarındaki değişiklikler bekliyor",
    body: "Kaydetmek için ayarları yeniden aç.",
  },
  /** A desktop scheduler task (the client's "send" stays out of sight). */
  task: {
    line: "Komut ya da mesaj",
    linePlaceholder: "/kit günlük",
    /** An action of the client's own kind ("log …", "script …"), kept as written. */
    raw: "İstemci komutu",
    onJoin: "Ne zaman",
    every: "Her girişte",
    first: "Yalnız ilk girişte",
    never: "Girişte değil",
    neverRuns: "Bu görev hiç çalışmaz: girişte ya da aralıkla çalışmasını seç.",
  },
  /** Under a cloud account's login line after the machine stopped it for a wrong password. */
  wrongPassword: "Sunucu bu şifreyi kabul etmedi. Doğrusunu yaz.",
  /** The same while the line is kept in the cloud: its field opens with "Değiştir". */
  wrongPasswordSealed: 'Sunucu bu şifreyi kabul etmedi. "Değiştir"e bas, doğrusunu yaz.',
  /**
   * An auto-reply rule's card, desktop and 7/24 Bulut, in the site's words (afk.projects.gg): a
   * plain phrase by default, a pattern only on the way for those who know it.
   */
  rule: {
    match: "Mesajda geçen söz",
    pattern: "Özel kalıp",
    patternPlaceholder: "(?i)^!davet \\w+$",
    patternHelp: "Düzenli ifade. Büyük küçük harf fark etmesin diye başına (?i) yaz.",
    writeMyself: "Kalıbı kendim yazacağım",
    toPlain: "Düz söze dön",
    cooldown: "İki yanıt arası",
    ownerOnly: "Yalnız bana yanıt versin",
    /** The cloud: the player's own name from above the rules. */
    ownerOnlyHelp: "Yalnız {{name}} yazınca çalışır.",
    ownerFirst: "Önce yukarıda oynadığın hesabın adını yaz.",
    /** The desktop: the client takes the launcher's account as its owner. */
    ownerDesktop: "Yalnız launcher'da oynadığın hesap yazınca çalışır.",
    /** The desktop's three replies, each to its own kind of message. */
    emptyReply: "Boş bıraktığın yerde yanıt verilmez.",
    insert: "Yanıta ekle",
    sender: "Yazanın adı",
    insertSender: "Yanıta yazanın adını ekle",
  },
};
