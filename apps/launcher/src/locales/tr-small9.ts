// SMALL9: HOST1's small launcher leftovers. Short, sentence case, no codes; the feature's name is
// "Dünyanı arkadaşlarına aç" (host1.md §1.1).
export const trSmall9 = {
  backups: {
    /** A world's backup the launcher took before the world first opened to friends. */
    beforeSharing: "Arkadaşlarına açılmadan önce",
  },
  join: {
    /**
     * A world invite's second line after its join failed, under the friend's name and beside
     * "Tekrar dene" or "Kur ve katıl": the toast's words, short, without a full stop like the
     * list's other lines (the name above is the subject where one is needed).
     */
    failed: {
      full: "Dünya dolu",
      closed: "Dünyasını kapattı",
      removed: "Dünyadan çıkarıldın",
      unreachable: "Bağlanılamadı",
      hostPackOld: "Oyunu yeniden açması gerekiyor",
      mods: "Aynı modlar gerekiyor",
    },
    /** Home's line for an invite whose join met missing mods: the goal, the friend, and why its
     * button copies the install (the name stays without a suffix: "ile" fits every name). */
    homeMissingMods: "Katılmak için {{name}} ile aynı modlar gerekiyor",
  },
  palette: {
    /** Ctrl+K: the Worlds tab of the command bar's instance, where "Arkadaşlarınla oyna" is. */
    openWorld: "Dünyanı arkadaşlarına aç",
    /** Ctrl+K while the player's world is open to friends: its sheet (never "açık dünya", the
     * game genre, and not the Worlds tab beside it). */
    manageWorld: "Paylaştığın dünyayı yönet",
  },
  settings: {
    /** Settings → Arkadaşlar: the feature by its name, with what it shares and what it keeps. */
    worlds: "Dünyanı arkadaşlarına aç",
    worldsPrivacy:
      "IP adresin arkadaşlarına görünmez. Açık dünyanı yalnız çağırdığın arkadaşların görür.",
  },
};
