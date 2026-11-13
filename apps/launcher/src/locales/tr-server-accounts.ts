// "Sunucu Hesapların" in the account center (SRV-ACC): the player's accounts on the Projects
// server's site, added as username accounts in one press. One calm row, no explanation beyond the
// names (the owner: "over explanation olmamalı"); Title Case on the title and the buttons (the
// owner, 7 Nov 2026), sentences stay sentence case. Player names never take a suffix.
export const trServerAccounts = {
  /** The row's title. */
  title: "Sunucu Hesapların",
  /** After the names that fit: how many more there are. */
  more: "+{{count}}",
  /** Adds every name of the row (more than one). */
  addAll: "Hepsini Ekle",
  /** Adds the row's one name. */
  addOne: "Ekle",
  /** Keeps quiet until the list changes. */
  notNow: "Şimdi Değil",
  /** In place of the names once they were added. */
  added: "Eklendi",
  /** Takes the accounts just added out again. */
  undo: "Geri Al",
  /** Screen readers, after the press. */
  addedAnnounce: "{{count}} hesap eklendi.",
  /** Screen readers, after "Geri Al". */
  undoneAnnounce: "Eklenen hesaplar kaldırıldı.",
  addFailed: "Hesaplar eklenemedi. Tekrar dene.",
  undoFailed: "Hesaplar kaldırılamadı. Tekrar dene.",
};
