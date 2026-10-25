// The in-game overlay's switch (Settings → Genel, OVL2). Short, sentence case, "sen". The overlay's
// own words (its cards, its panel, its settings) live in the mod's language files. A line that
// says it will not work says where and what it needs, then its action gives the fix. Install
// names come first and take no suffix ("kurulumunda", "kurulumuna" do), as in "{{name}} hesabına
// geç".
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
  /** Sets "Oyun açılınca launcher" back to its default ("Simge durumuna küçülsün"). */
  toMinimize: "Kapanmak yerine küçülsün",
  /**
   * In its place while no install can carry it (LIB1: the official pack deleted, no Fabric install
   * of the player's on a version it is made for), before "Resmî paketi kur".
   */
  packMissing: "Kurulumlarının hiçbirinde çalışmaz.",
  /**
   * In its place while the install Play opens is not Fabric but on a version the overlay is made
   * for (OVL-SEL1), before `switchTo`.
   */
  fabricOnly: "{{name}} kurulumunda çalışmaz, Fabric gerekir.",
  /** The same when its version is not one either: Fabric alone would not do. */
  fabricAndVersion: "{{name}} kurulumunda çalışmaz, Fabric ve başka bir sürüm gerekir.",
  /** While that install is Fabric on a version the overlay is not made for ("26.1.2–26.3"). */
  versionMissing: "{{name}} kurulumunda çalışmaz, Minecraft {{range}} gerekir.",
  /** Selects the install the overlay works in (the official pack first) for Play. */
  switchTo: "{{name}} kurulumuna geç",
  /**
   * In their place while that install's game is prepared or open: the command bar's picker is
   * locked until it closes.
   */
  waitsForGame: "Bu oyunda çalışmaz. Oyun kapanınca {{name}} kurulumuna geçebilirsin.",
};
