// The official pack's mod keys that still clash in an older options.txt (KEYS2): the mod check's
// finding, the question before a launch, the fix and its undo.
export const trKeyClash = {
  /** The mod check's line; `keys` are the shared keys ("V, G ve H"). */
  issue: "Mod tuşları çakışıyor: {{keys}}.",
  /** The question before a launch. */
  title: "Mod tuşları çakışıyor",
  /** Its one line above the bindings that change. */
  lead: "Düzeltince yalnız bu tuşlar değişir.",
  fixedTitle: "Mod tuşları düzeltildi",
  fixedLead: "Diğer tuşların olduğu gibi kaldı.",
  fix: "Düzelt",
  anyway: "Yine de başlat",
  undo: "Geri al",
  play: "Oyna",
  /** A binding left without a key (the game's own word). */
  unbound: "Atanmamış",
  /** The toast after "Düzelt" in the mod center; `keys`: "Yakınlaştır C ve Tam parlaklık X". */
  fixed: "Mod tuşları düzeltildi.",
  newKeys: "Yeni tuşlar: {{keys}}.",
  /** Announced when "Geri al" put the old keys back. */
  undone: "Eski tuşların geri geldi.",
  /** The bindings the pack's table moves, by what they do. */
  actions: {
    zoom: "Yakınlaştır",
    gamma: "Tam parlaklık",
    hideIcons: "Simgeleri gizle",
    reloadShaders: "Gölgelendiricileri yenile",
  },
  /** Keys that are not a single letter or digit. */
  keyNames: { capsLock: "Caps Lock", mouse: "Fare {{button}}" },
  /** Screen readers: one row's change, before and after "Düzelt". */
  sr: {
    change: "{{from}} yerine {{to}}",
    changeOff: "{{from}} tuşu kaldırılır",
    changed: "Artık {{to}}",
    changedOff: "Artık atanmamış",
  },
};
