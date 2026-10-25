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
  /** In its place while "Oyun açılınca launcher: Kapansın" is chosen. */
  closeHelp: "Oyun açılınca launcher kapandığı için çalışmaz.",
  /**
   * In its place while no install can carry it (LIB1: the official pack deleted, no Fabric install
   * of the player's on a version it is made for), before "Resmî paketi kur".
   */
  packMissing: "Çalışabileceği bir kurulum yok.",
  /**
   * In its place while the install Play opens is not Fabric (OVL-SEL1), before `switchTo`. The
   * name takes no suffix ("kurulumunda" does), as in "{{name}} hesabına geç".
   */
  fabricOnly: "{{name}} kurulumunda çalışmaz, Fabric gerekir.",
  /** In its place while that install is Fabric on a version the overlay is not made for. */
  versionMissing: "Minecraft {{version}} sürümünde çalışmaz.",
  /** Selects the install the overlay works in (the official pack first) for Play. */
  switchTo: "{{name}} kurulumuna geç",
};
