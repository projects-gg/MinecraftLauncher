// Graphics presets (Akıcı, Dengeli, Güzel) and the instance settings they live in. Short, sentence case.
export const trGraphics = {
  /** Instance settings group. */
  title: "Grafik",
  preset: {
    smooth: "Akıcı",
    balanced: "Dengeli",
    quality: "Güzel",
  },
  /** options.txt has values of its own (changed in the game, a modpack's). */
  custom: "Özel",
  /** A preset picked over those values: saving replaces them. */
  replacesCustom: "Oyundaki görüntü ayarların değişir.",
  /** The save bar while the game is open and a picked preset waits. */
  waitsForGame: "Grafik ayarını oyun kapanınca kaydedebilirsin.",
  /** Command palette item: the selected instance's graphics. */
  palette: "Grafik ayarı",
  /** Collapsible part of the instance settings: JVM arguments, commands, wrapper. */
  advanced: "İleri düzey",
  /** Under the JVM arguments, inside "İleri düzey" (the title says the rest). */
  jvmHelp: "Boş bırakmak çoğu zaman en iyisidir.",
};
