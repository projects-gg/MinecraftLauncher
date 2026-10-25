// When something comes back or ends, as AFK+ says it on the launcher, the site and the phone: a
// time today ("18:20"), tomorrow ("yarın 04:00"), a weekday within the week ("Pazartesi 04:00"),
// else a date. Whole phrases per language, so a sentence reads right in both: "18:20'de" / "at
// 18:20", "30 Ekim'e" / "until 30 October". In the computer's own time zone (only working hours
// are Turkey time, by the contract).
import { type Lang, trWith } from "./words";

const locale = (lang: Lang) => (lang === "tr" ? "tr-TR" : "en-GB");

function dayKey(date: Date): string {
  return `${date.getFullYear()}-${date.getMonth()}-${date.getDate()}`;
}

export interface Moment {
  kind: "today" | "tomorrow" | "week" | "date";
  /** "18:20". */
  time: string;
  /** "Pazartesi" / "Monday", or "3 Kasım" / "3 November"; "" today and tomorrow. */
  day: string;
}

export function momentOf(at: number, nowMs: number, lang: Lang): Moment {
  const when = new Date(at * 1000);
  const now = new Date(nowMs);
  const time = new Intl.DateTimeFormat(locale(lang), { hour: "2-digit", minute: "2-digit" }).format(
    when,
  );
  const tomorrow = new Date(now);
  tomorrow.setDate(now.getDate() + 1);
  if (dayKey(when) === dayKey(now)) return { kind: "today", time, day: "" };
  if (dayKey(when) === dayKey(tomorrow)) return { kind: "tomorrow", time, day: "" };
  const days = (when.getTime() - now.getTime()) / 86_400_000;
  if (days > 0 && days < 6.5) {
    const day = new Intl.DateTimeFormat(locale(lang), { weekday: "long" }).format(when);
    return { kind: "week", time, day };
  }
  const day = new Intl.DateTimeFormat(locale(lang), { day: "numeric", month: "long" }).format(when);
  return { kind: "date", time, day };
}

/**
 * When something happens: "18:20'de", "yarın 04:00'te", "Pazartesi 04:00'te", "3 Kasım 04:00'te";
 * "at 18:20", "tomorrow at 04:00", "on Monday at 04:00", "on 3 November at 04:00".
 */
export function atPhrase(at: number, nowMs: number, lang: Lang, tomorrow: string): string {
  const m = momentOf(at, nowMs, lang);
  if (lang === "tr") {
    const time = trWith(m.time, "loc");
    return m.kind === "today"
      ? time
      : m.kind === "tomorrow"
        ? `${tomorrow} ${time}`
        : `${m.day} ${time}`;
  }
  return m.kind === "today"
    ? `at ${m.time}`
    : m.kind === "tomorrow"
      ? `${tomorrow} at ${m.time}`
      : `on ${m.day} at ${m.time}`;
}

/**
 * Until when (Turkish sentences go on with "kadar"): a day ("30 Ekim'e" / "until 30 October"), or
 * with `time` a time today or that day ("14:35'e" / "until 14:35").
 */
export function untilPhrase(at: number, nowMs: number, lang: Lang, time = false): string {
  const m = momentOf(at, nowMs, lang);
  if (time && m.kind === "today") {
    return lang === "tr" ? trWith(m.time, "dat") : `until ${m.time}`;
  }
  const day = dayText(at, lang);
  if (time) {
    return lang === "tr" ? trWith(`${day} ${m.time}`, "dat") : `until ${day} ${m.time}`;
  }
  return lang === "tr" ? trWith(day, "dat") : `until ${day}`;
}

/** "30 Ekim" / "30 October": the day only. */
export function dayText(at: number, lang: Lang): string {
  return new Intl.DateTimeFormat(locale(lang), { day: "numeric", month: "long" }).format(at * 1000);
}

/** "16–23 Ekim" / "16–23 October", across months "28 Eylül – 5 Ekim": a span of days, both in. */
export function rangeText(from: number, to: number, lang: Lang): string {
  return new Intl.DateTimeFormat(locale(lang), { day: "numeric", month: "long" }).formatRange(
    from * 1000,
    to * 1000,
  );
}

/** "14:05": the time of day of a line. */
export function clockText(at: number, lang: Lang): string {
  return new Intl.DateTimeFormat(locale(lang), { hour: "2-digit", minute: "2-digit" }).format(
    at * 1000,
  );
}

/** "24 Ekim 14:05" / "24 October 14:05": a receipt's time. */
export function dateTimeText(at: number, lang: Lang): string {
  return new Intl.DateTimeFormat(locale(lang), {
    day: "numeric",
    month: "long",
    hour: "2-digit",
    minute: "2-digit",
  }).format(at * 1000);
}

/** Whole minutes left until a moment, at least 0 ("9 dk kaldı"). */
export function minutesLeft(at: number, nowMs: number): number {
  return Math.max(0, Math.ceil((at * 1000 - nowMs) / 60_000));
}

/** The day a feed or message line belongs to, for its heading: "today", "yesterday" or a key. */
export function dayGroup(at: number, nowMs: number): "today" | "yesterday" | string {
  const when = new Date(at * 1000);
  const now = new Date(nowMs);
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (dayKey(when) === dayKey(now)) return "today";
  if (dayKey(when) === dayKey(yesterday)) return "yesterday";
  return dayKey(when);
}

/** Days and hours, or minutes under an hour: the words come from `unit`. */
export function durationParts(seconds: number): Array<["days" | "hours" | "minutes", number]> {
  const days = Math.floor(seconds / 86_400);
  const hours = Math.floor((seconds % 86_400) / 3_600);
  if (!days && !hours) return [["minutes", Math.max(1, Math.floor(seconds / 60))]];
  const parts: Array<["days" | "hours" | "minutes", number]> = [];
  if (days) parts.push(["days", days]);
  if (hours) parts.push(["hours", hours]);
  return parts;
}
