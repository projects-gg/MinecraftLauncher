// Event reminders and the marker on news the player has not seen yet. Short, sentence case.
export const trReminders = {
  /** The bell on the event card and in the other events list (pressed while a reminder is set). */
  bell: "Hatırlat",
  /** Shown once, the first time the player sets a reminder. */
  explained: "Başlamadan 5 dakika önce haber vereceğiz.",
  /** System notification text; its title is the event's name. */
  startsIn: "{{count}} dakika sonra başlıyor.",
  /** Toast while the launcher is in front. */
  startsInToast: "{{title}} {{count}} dakika sonra başlıyor",
  /** A reminder missed while the launcher was closed or the computer slept: the event still runs. */
  running: "Şu an sürüyor.",
  runningToast: "{{title}} sürüyor",
  /** Opens the event's article or link. */
  show: "Göster",
  /** After a news link's text, for screen readers. */
  newsNew: "yeni",
  /** The rail's news link while new news waits, for screen readers. */
  railNews: "{{label}}, yeni haber var",
  /** The navigation rail, for screen readers. */
  mainMenu: "Ana menü",
  /** Time left in the other events list ("5 gün 3 sa"); under a day it reads "2 sa 14 dk". */
  daysHours: "{{days}} gün {{hours}} sa",
  days: "{{days}} gün",
};
