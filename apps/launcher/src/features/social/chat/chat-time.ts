// Chat times (CHAT-UX; the overlay's ChatTime.java and the phone keep the same rules): the day line
// over the first message of each local day, a group's clock time, and the full time a pointer
// resting on a message shows; the conversation list's stamp (CHAT-UX2). Days are calendar days in
// this computer's zone, never 24-hour spans, so summer time and a message just before midnight read
// right; a time a little ahead of this computer's clock reads as today. Pure (chat-time.test.ts):
// the words are worked out when the list draws, nothing wakes up for them.
//
// Dates are written from the local wall time read as UTC, so the `Intl` formatters (built once per
// language and options, `lib/format`) carry no zone of their own: they write this computer's wall
// time whatever its zone is, and follow a change of zone at once.
import { dateTimeFormat, relativeWords } from "@/lib/format";

const DAY_MS = 86_400_000;

/** Days a day line counts back in words ("6 gün önce"); from the seventh it says the date. */
export const DAYS_IN_WORDS = 6;

const DAY_MONTH: Intl.DateTimeFormatOptions = { day: "numeric", month: "long", timeZone: "UTC" };
const DAY_MONTH_YEAR: Intl.DateTimeFormatOptions = { ...DAY_MONTH, year: "numeric" };
const CLOCK: Intl.DateTimeFormatOptions = {
  hour: "2-digit",
  minute: "2-digit",
  hourCycle: "h23",
  timeZone: "UTC",
};
const FULL: Intl.DateTimeFormatOptions = { ...DAY_MONTH_YEAR, ...CLOCK, weekday: "long" };
const WEEKDAY_SHORT: Intl.DateTimeFormatOptions = { weekday: "short", timeZone: "UTC" };
const DAY_MON: Intl.DateTimeFormatOptions = { day: "numeric", month: "short", timeZone: "UTC" };
const DAY_MON_YEAR: Intl.DateTimeFormatOptions = { ...DAY_MON, year: "numeric" };

/** `ms`'s wall time on this computer's clock (to the minute), as the UTC instant that reads so. */
function wallTime(ms: number): number {
  const date = new Date(ms);
  return Date.UTC(
    date.getFullYear(),
    date.getMonth(),
    date.getDate(),
    date.getHours(),
    date.getMinutes(),
  );
}

/** The local calendar day of `ms`: whole days since 1 January 1970 on this computer's calendar. */
export function localDay(ms: number): number {
  return Math.floor(wallTime(ms) / DAY_MS);
}

/** Two moments on one local calendar day. */
export function sameLocalDay(a: number, b: number): boolean {
  return localDay(a) === localDay(b);
}

/** Calendar days from `at`'s day to `now`'s: 0 today, 1 yesterday; below 0 ahead of the clock. */
export function daysAgo(at: number, now: number): number {
  return localDay(now) - localDay(at);
}

/**
 * The day line: "Bugün", "Dün", "3 gün önce" (two to six days), then "28 Ekim", with the year when
 * it is another year ("28 Ekim 2025") / "Today", "Yesterday", "3 days ago", "28 October",
 * "28 October 2025". `words` carry the strings' "Bugün" and "Dün".
 */
export function dayLineText(
  at: number,
  now: number,
  words: { today: string; yesterday: string },
): string {
  const days = daysAgo(at, now);
  if (days <= 0) return words.today;
  if (days === 1) return words.yesterday;
  if (days <= DAYS_IN_WORDS) return relativeWords(-days, "day");
  const sameYear = new Date(at).getFullYear() === new Date(now).getFullYear();
  return dateTimeFormat(sameYear ? DAY_MONTH : DAY_MONTH_YEAR).format(wallTime(at));
}

/** A group's time: "14:05", "00:30" (24 hours in both languages). */
export function clockTime(at: number): string {
  return dateTimeFormat(CLOCK).format(wallTime(at));
}

/**
 * The full time a pointer resting on a message shows: "28 Ekim 2026 Çarşamba 14:05" /
 * "Wednesday, 28 October 2026 at 14:05".
 */
export function fullTime(at: number): string {
  return dateTimeFormat(FULL).format(wallTime(at));
}

/**
 * The conversation list's stamp of its newest message (CHAT-UX2), short for a narrow row: the time
 * today ("14:05"), `yesterday` ("Dün"), the weekday within the last week ("Pzt" / "Mon"), then the
 * date ("28 Eki", "28 Eki 2025" in another year / "28 Oct", "28 Oct 2025").
 */
export function listStamp(at: number, now: number, yesterday: string): string {
  const days = daysAgo(at, now);
  if (days <= 0) return clockTime(at);
  if (days === 1) return yesterday;
  if (days <= DAYS_IN_WORDS) return dateTimeFormat(WEEKDAY_SHORT).format(wallTime(at));
  const sameYear = new Date(at).getFullYear() === new Date(now).getFullYear();
  return dateTimeFormat(sameYear ? DAY_MON : DAY_MON_YEAR).format(wallTime(at));
}
