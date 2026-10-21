// LIB1: every instance can be deleted, the official pack too, and the library offers the pack again
// when it is gone. Short, sentence case; the toast after installing is `official.added`.
export const trLib1 = {
  /** The library with no instance at all, and the offer when OYNA has nothing to play. */
  empty: {
    title: "Hiç kurulum yok",
    /** The owner's own words: an invitation; the buttons say the rest. */
    question: "Resmî paketi kuralım mı?",
  },
  /** Installs the official pack again (made at once; its files download at the first play). */
  install: "Resmî paketi kur",
  /** The same on the library's "not installed" card, under the pack's name. */
  installShort: "Kur",
  /** While it is being made. */
  installing: "Kuruluyor",
  /** The "not installed" card's line, where an instance card says its version. */
  notInstalled: "Kurulu değil",
  /** The confirmation's one sentence when the last official instance goes. */
  deleteOfficial:
    "Dünyaları ve ayarlarıyla çöp kutusuna taşınır, resmî paketi istediğin zaman yeniden kurabilirsin.",
  /** When an action needed the official instance (OYNA with nothing to play, "Katıl"). */
  offer: {
    /** Other instances are there; only the official pack is missing. */
    missingTitle: "Resmî paket kurulu değil",
    play: "Kur ve oyna",
    join: "Kur ve katıl",
    /** Joins with the instance OYNA would start instead. */
    joinWith: "{{name}} ile katıl",
    /** Opens "Yeni kurulum". */
    create: "Yeni kurulum",
  },
};
