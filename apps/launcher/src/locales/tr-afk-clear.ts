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
  /** A desktop account the server turned away for good, as the cloud cards say it. */
  stoppedBecause: "Durdu: {{reason}}",
  /** This computer's list (over it the page title, under it 7/24 Bulut). */
  here: {
    title: "Bu bilgisayarda",
    none: "Hesap yok",
    /** Only accounts in both places, none running here (their cards are in 7/24 Bulut). */
    noneRunning: "Şu an burada çalışan hesap yok",
  },
  /** The buttons in that list's heading: the words say they take this list only. */
  bulk: {
    stopHere: "Buradakileri durdur",
    connectHere: "Buradakileri bağla",
    /** On "Buradakileri bağla" while the list has an account the server stopped (a ban, a password). */
    skipped: "Sunucunun durdurduğu hesap bağlanmaz.",
  },
  /** The 7/24 Bulut list's count while some of its accounts run on this computer (listed there). */
  cloud: {
    countHere: "{{count}} bulut hesabı, {{here}} tanesi bu bilgisayarda",
    countOnlineHere:
      "{{count}} bulut hesabı, {{online}} tanesi oyunda, {{here}} tanesi bu bilgisayarda",
  },
  /** An account in both places: its 7/24 Bulut card is its only one. */
  twin: {
    /** Moves it: the other place stops first. */
    runHere: "Bu bilgisayarda çalıştır",
    runInCloud: "Bulutta çalıştır",
    /** Asked while the account runs in the cloud (what the move leaves behind). */
    moveTitle: "{{name}} bu bilgisayarda çalışsın mı?",
    moveBody:
      "Bulutta durur, süresi işlemeye devam eder. Bu bilgisayar ya da launcher kapanınca oyundan çıkar.",
    /** Its paid time, on its card while it runs on this computer ("18 gün kaldı"). */
    timeLeft: "Bulutta {{time}}",
    hereSettings: "Bu bilgisayardaki ayarlar",
    cloudSettings: "Buluttaki ayarlar",
    /** The settings panel's title: which of the two it is. */
    hereTitle: "Bu bilgisayardaki {{name}} ayarları",
    cloudTitle: "Buluttaki {{name}} ayarları",
    removeHere: "Bu bilgisayardan kaldır",
    removeTitle: "{{name}} bu bilgisayardan kaldırılsın mı?",
    removeBody:
      "Bu bilgisayardaki ayarları ve kayıtlı giriş komutları silinir, 7/24 Bulut hesabı olduğu gibi kalır. Bu işlem geri alınamaz.",
  },
  /** "Ayarlarını kopyala" from an account whose password the server turned down. */
  copy: {
    refused:
      "Sunucu {{name}} hesabının şifresini kabul etmedi. Önce şifresini düzelt, sonra kopyala.",
  },
  /** A secret line's field (the eye inside it, what a screen reader hears for a kept one). */
  secret: {
    show: "Göster",
    hide: "Gizle",
    kept: "{{command}} şifresi kayıtlı",
    newFor: "{{command}} için yeni şifre",
    /** Under a console's field while a password line is typed (both consoles). */
    consoleNote: "Şifre konsolda gizli kalır.",
  },
};
