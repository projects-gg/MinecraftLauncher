// REPAIR-RB: "Önceki Sürüme Dön" in the crash dialog, the versions it holds (Modlar), and the file
// the crash dialog opens. Short, "sen"; buttons in Title Case (owner, 7 Nov), notes as sentences.
// Mod names take no suffix (they are not Turkish words): sentences are built around them.
export const trRepairRb = {
  crash: {
    /** Why going back is offered: the suspect changed since the last game that worked. */
    changed: "{{name}} en son çökmeyen oyundan sonra güncellendi.",
    added: "{{name}} en son çökmeyen oyundan sonra eklendi.",
    /** The crash dialog's action. */
    action: "Önceki Sürüme Dön",
    /** The same button while the earlier version is put in place. */
    working: "Kuruluyor",
    /** After it went back. */
    done: "{{name}} {{version}} sürümüne döndü.",
    held: "Sen güncelleyene kadar bu sürümde kalır.",
    undo: "Geri Al",
    /** Toast after "Geri Al": the version that crashed is back. */
    undone: "{{name}} {{version}} sürümüne geri döndü.",
    /** Going back is not offered: it went back already and crashed again. */
    repeated: "{{name}} önceki sürümde de çöktü.",
    /** Going back is not offered: nothing older runs on this Minecraft version. */
    noOlder: "{{name}} için bu Minecraft sürümünde çalışan daha eski bir sürüm yok.",
    /** The footer's file button (its tooltip): what explains this crash. */
    openReport: "Çökme Raporunu Aç",
    openLog: "Oyun Kaydını Aç",
  },
  /** Note on a mod of the player's held at its version (Modlar → Kendi modların). */
  ownHeld: "{{version}} çöktüğü için bu sürümde kalıyor.",
  /** Note on an official pack mod held at its version (Önerilenler). */
  packHeld: "Yeni sürümü çöktüğü için {{version}} sürümünde kalıyor.",
};
