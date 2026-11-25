// INST-ADD: adding an instance of one's own, as in the official launcher's "New installation":
// findable from the home picker and the library, with the create dialog's "Daha Fazla Seçenek".
// Short, "sen"; titles and buttons in Title Case (owner, 7 Nov), lines as sentences. No ids or codes.
export const trInstAdd = {
  /** The library's card in "Kurulumlarım" while the player has only the official pack. */
  card: {
    title: "Yeni Kurulum",
    body: "İstediğin sürümle, modlu ya da modsuz.",
    action: "Oluştur",
    /** The card's button for screen readers (the palette's words). */
    actionLabel: "Yeni Kurulum Oluştur",
  },
  /** "Sıfırdan": the options most players never need, folded. */
  more: {
    title: "Daha Fazla Seçenek",
    /** Java and the window are the new instance's settings, open once it is made. */
    later: "Java ve pencere boyutunu, kurulum oluşunca açılan Ayarlar sekmesinde seçebilirsin.",
  },
};
