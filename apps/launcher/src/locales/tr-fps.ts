// Settings → Grafik ve FPS: the graphics preset of the instance Play launches, applied at once.
// Short, sentence case; the presets' names are in tr-graphics.ts.
export const trFps = {
  /** The Settings section: its name says "FPS", what players look for. */
  title: "Grafik ve FPS",
  /** The instance the section changes: the one Play launches. */
  instance: "Kurulum",
  /** The preset row. */
  preset: "Grafik ayarı",
  /** One line under the preset row: what the chosen one gives. */
  effect: {
    smooth: "En çok FPS.",
    balanced: "FPS ve görüntü dengede.",
    quality: "En güzel görüntü, daha az FPS.",
    custom: "Oyunda kendi ayarların var.",
  },
  /** While the instance's game is prepared or open (the game writes its options back). */
  waitsForGame: "Oyun kapanınca değiştirebilirsin.",
  /** Minecraft before 1.7.2. */
  unsupported: "Bu Minecraft sürümünde grafik ayarı yok.",
  /** No instance yet. */
  noInstance: "Henüz kurulum yok.",
  /** The toast after a pick; "Geri al" brings back what was there. */
  switched: "Grafik ayarı {{preset}} oldu.",
  undo: "Geri al",
};
