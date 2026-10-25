// The home page's news column: its link to the news page, the patch notes box's action, the event
// card's heading while its event runs and the times on the card and in the other events list.
// Short, sentence case.
export const trNewsColumn = {
  /** The column's link to the news page. */
  allNews: "Tümü",
  /** The patch notes box's action: the box shows an excerpt, this opens the whole article. */
  readNotes: "Tamamını oku",
  /** The event card's heading while its event runs ("Sıradaki etkinlik" before it starts). */
  liveEvent: "Süren etkinlik",
  /**
   * When an upcoming event starts, a sentence like "2 saat 14 dakika kaldı": "Bugün 20:00'de
   * başlıyor", "Cumartesi 20:00'de başlıyor", "25 Ekim 20:00'de başlıyor". `ending` is the
   * locative the time takes as it is read ("20:00'de", "19:30'da", "15:00'te"; lib/event-card).
   */
  starts: {
    today: "Bugün {{time}}'{{ending}} başlıyor",
    tomorrow: "Yarın {{time}}'{{ending}} başlıyor",
    later: "{{day}} {{time}}'{{ending}} başlıyor",
  },
  /**
   * Time left, in whole words like the column's "5 saat önce": "4 gün 23 saat", "2 saat 14
   * dakika", "45 saniye" (lib/event-card `timeLeftText`).
   */
  span: {
    days: "{{count}} gün",
    hours: "{{count}} saat",
    minutes: "{{count}} dakika",
    seconds: "{{count}} saniye",
  },
};
