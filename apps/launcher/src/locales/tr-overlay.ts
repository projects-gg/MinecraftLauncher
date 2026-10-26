// The in-game overlay's switch (Settings → Genel, OVL2). Short, sentence case, "sen". The overlay's
// own words (its cards, its panel, its settings) live in the mod's language files. A line that
// says it will not work says where and what it needs, then its action gives the fix. Install
// names come first and take no suffix ("kurulumunda", "kurulumuna" do), as in "{{name}} hesabına
// geç". The official pack is called that everywhere ("Resmî paketi kur", "Resmî pakete geç").
export const trOverlay = {
  /** The switch: on by default. */
  setting: "Oyun içi katman",
  /**
   * What it gives and how it opens, true whether the switch is on or off (nothing in the launcher
   * shows it working). `key` is the default key, drawn as a key cap: the player may bind another
   * in the game's Controls, which the launcher cannot see, so it is named as the default.
   */
  settingHelp: "Mesajlar, arkadaşlar ve ekip çağrıları oyunda açılır; varsayılan tuşu {{key}}.",
  /**
   * CHAT-EMOJI: the chat's emoji picker, right under the overlay's switch, on by default. The same
   * mod, so the lines that say it will not work (`needs`, `packMissing`, `waits…`) are shared.
   */
  emojiSetting: "Sohbette emoji seçici",
  /** What it gives and its default key (F9; rebound in the game's Controls), drawn as a key cap. */
  emojiHelp: "Sohbet ekranında emojiler tek tıkla eklenir; varsayılan tuşu {{key}}.",
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
   * In its place while the install Play opens cannot carry it (OVL-SEL1: not Fabric, or Fabric on
   * another version), before the switch. `range` is the versions the overlay is made for, as an
   * install's line names them ("Fabric 26.1.2–26.3").
   */
  needs: "{{name}} kurulumunda çalışmaz, Fabric {{range}} gerekir.",
  /** Selects the official pack for Play. */
  toOfficial: "Resmî pakete geç",
  /** Selects the player's own install the overlay works in (the official pack is gone). */
  toInstall: "{{name}} kurulumuna geç",
  /**
   * In their place while that install's game is prepared or open: the command bar's picker is
   * locked until the game is over.
   */
  waitsOfficial: "{{name}} kurulumunda çalışmaz. Oyundan sonra resmî pakete geçebilirsin.",
  /** The same when the install it can switch to is the player's own. */
  waitsInstall: "{{name}} kurulumunda çalışmaz. Oyundan sonra {{target}} kurulumuna geçebilirsin.",
};
