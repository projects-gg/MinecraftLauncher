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
  /** In its place while the install Play opens is not Fabric (OVL-SEL1), before `toFabric`. */
  fabricOnly: "Yalnız Fabric kurulumlarında çalışır.",
  /** Selects the official pack, or a Fabric install the overlay works in. */
  toFabric: "Fabric'e geç",
  /** In its place while that install is Fabric on a version the overlay is not made for. */
  versionMissing: "Minecraft {{version}} sürümünde çalışmaz.",
  /** The same action there (the install is already Fabric). */
  toWorking: "Uygun kuruluma geç",
  /** The action's name for screen readers: what it says, then the install it selects. */
  switchTarget: "{{action}}: {{name}}",
};
