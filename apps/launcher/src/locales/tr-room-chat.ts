// The voice room panel's chat (SQOL-U3, sqol.md §1.3): the room's own chat, the call's friend's
// messages over it, the people folded under one line, the previews over the stream. Titles in Title
// Case; names never take a suffix. Everything else is chat's own words (`chat.*`, `chat2.*`).
export const trRoomChat = {
  panel: {
    /** The docked panel's landmark name. */
    label: "Ses Odası",
    /** The header's menu: the voice settings, the room's own actions. */
    menu: "Oda Seçenekleri",
  },
  /** The separator on the panel's left edge that widens it. */
  resize: {
    label: "Panel Genişliği",
    tooltip: "Sürükleyerek Genişlet",
    /** Its value for screen readers: how much of the window, and the layout that gives. */
    value: {
      one: "{{percent}}, tek sütun",
      two: "{{percent}}, iki sütun",
    },
  },
  people: {
    title: "Odadakiler",
    label_one: "Odadakiler, {{count}} kişi",
    label_other: "Odadakiler, {{count}} kişi",
  },
  title: "Oda Sohbeti",
  dm: "Mesajlar",
  crewChat: "Ekip Sohbeti",
  empty: "Burada yazdıkların yalnız bu odadakilere görünür ve oda kapanınca silinir.",
  composer: {
    placeholder: "Odaya yaz",
    label: "Odaya yaz",
  },
  tombstone: {
    /** The room's leader or a moderator took it away. */
    mod: "Bu mesaj kaldırıldı",
  },
  refused: {
    /** Deleting someone else's message without the right to (SQOL-R `notAllowed`). */
    notAllowed: "Bu mesajı silemezsin.",
  },
  /** In a person's name for screen readers, while they speak. */
  speaking: "konuşuyor",
  /** A connection's bars, in words (their tooltip, screen readers); bars show only when not good. */
  connection: {
    good: "Bağlantı: İyi",
    weak: "Bağlantı: Orta",
    bad: "Bağlantı: Zayıf",
  },
  announce: {
    one: "Oda sohbeti, {{name}}: {{text}}",
    many_one: "Oda sohbetinde {{count}} yeni mesaj",
    many_other: "Oda sohbetinde {{count}} yeni mesaj",
  },
  settings: {
    peek: "Mesaj Önizlemelerini Göster",
  },
};
