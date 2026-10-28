// Settings → Genel "Hızlı doku paketi yükleme" (FASTPACK). Short, sentence case, "sen". The line
// under the switch says what it does (the owner's words, 28 Oct); where it does nothing, a second
// sentence says so.
export const trFastPack = {
  /** The switch: on by default, shown while Studio's `features.fastPacks` is on. */
  setting: "Hızlı doku paketi yükleme",
  /** What it does, under the switch whenever the row shows. */
  help: "Sunucuların gönderdiği doku paketlerinde çalışır.",
  /** After `help` while the launcher's pack has no fast-pack mod for the version Play opens. */
  missing: "Bu sürümde yok.",
  /** After `help` while Play opens an install the launcher does not manage (the player's own). */
  notHere: "Bu kurulumda yok.",
};
