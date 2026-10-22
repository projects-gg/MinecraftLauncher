// Chat extras (CHAT2): the emoji and sticker picker, stickers. Short, sentence case. Names are
// never followed by a suffix (tr-chat.ts rule). Emoji names come from CLDR, sticker names from
// the sticker manifest; neither is here.
export const trChat2 = {
  picker: {
    open: "Emoji ve çıkartmalar",
    openEmoji: "Emoji",
    label: "Emoji ve çıkartmalar",
    tabs: {
      label: "Tür",
      emoji: "Emoji",
      stickers: "Çıkartmalar",
    },
    search: "Emoji ara",
    clear: "Aramayı temizle",
    grid: "Emoji",
    recent: "Son kullanılanlar",
    empty: "Emoji bulunamadı",
    results_one: "{{count}} emoji bulundu",
    results_other: "{{count}} emoji bulundu",
    groups: "Gruplar",
  },
  groups: {
    smileys: "İfadeler ve duygular",
    people: "İnsanlar",
    nature: "Hayvanlar ve doğa",
    food: "Yiyecek ve içecek",
    travel: "Seyahat ve yerler",
    activities: "Etkinlikler",
    objects: "Nesneler",
    symbols: "Semboller",
    flags: "Bayraklar",
  },
  tone: {
    label: "Cilt tonu",
    current: "Cilt tonu: {{tone}}",
    options: {
      default: "Varsayılan",
      light: "Açık",
      mediumLight: "Orta açık",
      medium: "Orta",
      mediumDark: "Orta koyu",
      dark: "Koyu",
    },
  },
  stickers: {
    grid: "Çıkartmalar",
    recent: "Son kullanılanlar",
    label: "{{name}} çıkartması",
    unknown: "Çıkartma",
  },
};
