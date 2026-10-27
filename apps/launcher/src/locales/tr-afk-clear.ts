// AFK-CLEAR: one card per account on the AFK page, where each one runs, what each button covers,
// "Şifreyi düzelt" after a refused password, and the one look of a secret line
// (docs/tasks/afk-clear.md). Short, sentence case, no codes.
export const trAfkClear = {
  /** The main button after the server turned the login password down: the settings open there. */
  fixPassword: "Şifreyi düzelt",
  /** The change bar's main button on an account stopped on its password. */
  saveConnect: "Kaydet ve bağla",
  saveStart: "Kaydet ve başlat",
  /** The save toast's second line after "Kaydet ve bağla" / "Kaydet ve başlat". */
  savedConnecting: "Hesap bağlanıyor.",
  savedStarting: "Hesap başlatılıyor.",
  /** The first list once the player has 7/24 Bulut accounts too. */
  here: {
    title: "Bu bilgisayarda",
    none: "Hesap yok",
  },
  /** An account in both places: its 7/24 Bulut card is its only one. */
  twin: {
    also: "Bu bilgisayarda da var",
    /** Moves it: the other place stops first. */
    runHere: "Bu bilgisayarda çalıştır",
    runInCloud: "Bulutta çalıştır",
    /** The state line, the place first ("Bu bilgisayarda oyunda, 1 sa 5 dk"). */
    hereState: "Bu bilgisayarda {{state}}",
    cloudState: "Bulutta {{state}}",
    hereSettings: "Bu bilgisayardaki ayarlar",
    cloudSettings: "Buluttaki ayarlar",
    removeHere: "Bu bilgisayardan kaldır",
    removeTitle: "{{name}} bu bilgisayardan kaldırılsın mı?",
    removeBody:
      "Bu bilgisayardaki ayarları ve kayıtlı giriş komutları silinir, 7/24 Bulut hesabı olduğu gibi kalır. Bu işlem geri alınamaz.",
  },
  /** A secret line's field (the eye inside it, what a screen reader hears for a kept one). */
  secret: {
    show: "Göster",
    hide: "Gizle",
    kept: "{{command}} şifresi kayıtlı",
    newFor: "{{command}} için yeni şifre",
  },
};
