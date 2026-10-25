// The in-game overlay's switch (Settings → Genel, OVL2). Short, sentence case, "sen". The overlay's
// own words (its cards, its panel, its settings) live in the mod's language files.
export const trOverlay = {
  /** The switch: on by default. */
  setting: "Oyun içi katman",
  /** What it gives and how it opens (nothing in the launcher shows it working). */
  settingHelp: "Oyunda F7'ye bas: mesajlar, arkadaşlar ve ekip çağrıları.",
  /** In its place while "Oyun açılınca launcher: Kapansın" is chosen. */
  closeHelp: "Oyun açılınca launcher kapandığı için çalışmaz.",
  /** In its place while no instance follows the official pack (LIB1), before "Kur". */
  packMissing: "Resmî paket kurulu değil.",
  /** In its place while the install Play opens is not Fabric (OVL-SEL1), before `switchTo`. */
  fabricOnly: "Yalnız Fabric kurulumlarında çalışır.",
  /** In its place while that install is Fabric on a version the overlay is not made for. */
  versionMissing: "Minecraft {{version}} sürümünde çalışmaz.",
  /**
   * Selects the install the overlay works in (the official pack first) for Play. The name comes
   * first and takes no suffix, as in "{{name}} hesabına geç".
   */
  switchTo: "{{name}} kurulumuna geç",
};
