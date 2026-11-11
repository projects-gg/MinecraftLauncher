// SOC-NOTE (SOC-GAP §3.5, §3.8): the status note friends see, and the pinned messages of a crew's
// chat. Short, titles in Title Case, the "sen" form; a player's name never takes a suffix.
export const trSocNote = {
  note: {
    add: "Not Ekle",
    edit: "Notu Düzenle",
    title: "Durum Notu",
    placeholder: "Ne yapıyorsun?",
    hint: "Yalnız arkadaşların görür.",
    count: "{{count}}/40",
    lengthLabel: "Ne Kadar Kalsın",
    lengths: {
      hour: "1 Saat",
      today: "Bugün",
      keep: "Silinmesin",
    },
    save: "Kaydet",
    remove: "Notu Kaldır",
    saved: "Notun kaydedildi",
    removed: "Notun kaldırıldı",
    long: "En çok 40 karakter.",
    link: "Notta bağlantı olamaz.",
    phone: "Notta telefon numarası olamaz.",
    // The player's own card and a friend's: whose and until when, for screen readers.
    mine: "Notun: {{text}}",
    theirs: "Notu: {{text}}",
    limited: "Durum notu koyamazsın",
  },
  pins: {
    pin: "Sabitle",
    unpin: "Sabitlemeyi Kaldır",
    strip: "Sabitlenen mesaj",
    position: "{{at}}/{{count}}",
    list: "Sabitlenen Mesajlar",
    listHint: "Lider ve moderatörler en çok 3 mesaj sabitleyebilir.",
    jump: "Mesaja git",
    pinned: "{{name}} bir mesajı sabitledi.",
    pinnedByYou: "Bir mesajı sabitledin.",
    fullTitle: "En çok 3 mesaj sabitlenebilir",
    fullText: "Bunu sabitlersen en eski sabitlenen mesaj kalkar.",
    replace: "Sabitle",
    done: "Mesaj sabitlendi",
    undone: "Sabitleme kaldırıldı",
    gone: "Bu mesaj bu bilgisayarda yok.",
  },
  report: {
    // "Rapor et"'s reason for a friend's note ("Uygunsuz not" is a plan's, and "Not" the details).
    reason: "Uygunsuz Durum Notu",
  },
};
