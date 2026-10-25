// The in-game overlay's switch (Settings → Genel, OVL2). Short, sentence case, "sen". The overlay's
// own words (its cards, its panel, its settings) live in the mod's language files. A line that
// says it will not work says where or why, then its action gives the fix.
export const trOverlay = {
  /** The switch: on by default. */
  setting: "Oyun içi katman",
  /**
   * What it gives and how it opens, true whether the switch is on or off (nothing in the launcher
   * shows it working). `key` is the default shortcut, drawn as a key cap.
   */
  settingHelp: "Mesajlar, arkadaşlar ve ekip çağrıları oyunda {{key}} ile açılır.",
  /** In its place while "Oyun açılınca launcher: Kapansın" is chosen, before `toMinimize`. */
  closeHelp: "Oyun açılınca launcher kapandığı için çalışmaz.",
  /** Sets "Oyun açılınca launcher" to its default, in that select's own words. */
  toMinimize: "Simge durumuna küçülsün",
  /**
   * In its place while no install can carry it (LIB1: the official pack deleted, no Fabric install
   * of the player's on a version it is made for), before "Resmî paketi kur".
   */
  packMissing: "Kurulumlarının hiçbirinde çalışmaz.",
  /**
   * In its place while the install Play opens is not Fabric but on a version the overlay is made
   * for (OVL-SEL1), before `switchTo`. The name takes no suffix ("kurulumunda" does), as in
   * "{{name}} hesabına geç".
   */
  fabricOnly: "{{name}} kurulumunda çalışmaz, Fabric gerekir.",
  /** The same when its version is not one either: Fabric alone would not do. */
  notHere: "{{name}} kurulumunda çalışmaz.",
  /** In its place while that install is Fabric on a version the overlay is not made for. */
  versionMissing: "Minecraft {{version}} sürümünde çalışmaz.",
  /** Selects the install the overlay works in (the official pack first) for Play. */
  switchTo: "{{name}} kurulumuna geç",
  /** In the switch's place while a game is prepared or open (the command bar's picker is locked). */
  waitsForGame: "Oyun kapanınca geçebilirsin.",
};
