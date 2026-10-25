// The home page's news column: the patch notes box's action, the event card's heading while its
// event runs and the time left on the card and in the other events list. Short, sentence case.
export const trNewsColumn = {
  /** The patch notes box's action (the box opens the notes' article; its time is plain meta). */
  readNotes: "Notları oku",
  /** The event card's heading while its event runs ("Sıradaki etkinlik" before it starts). */
  liveEvent: "Süren etkinlik",
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
