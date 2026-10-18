import i18n from "./i18n";

/**
 * The interface's `Intl` locale. English is British English, not American: day-first dates and
 * the 24-hour clock read like the Turkish ones the server's players know (L1, 18 Oct 2026).
 */
export function uiLocale(): "tr-TR" | "en-GB" {
  return i18n.language === "en" ? "en-GB" : "tr-TR";
}

/** `Intl` formatters are costly to build: one per locale and set of options, built on first use. */
const formatters = new Map<string, unknown>();

function cached<T>(kind: string, options: object | undefined, make: (locale: string) => T): T {
  const locale = uiLocale();
  const key = `${kind}|${locale}|${options ? JSON.stringify(options) : ""}`;
  let formatter = formatters.get(key) as T | undefined;
  if (!formatter) {
    formatter = make(locale);
    formatters.set(key, formatter);
  }
  return formatter;
}

/** Numbers in the interface's language ("1,5" / "1.5", "12.345" / "12,345"). */
export function numberFormat(options?: Intl.NumberFormatOptions): Intl.NumberFormat {
  return cached("number", options, (locale) => new Intl.NumberFormat(locale, options));
}

/** Dates and times in the interface's language. */
export function dateTimeFormat(options: Intl.DateTimeFormatOptions): Intl.DateTimeFormat {
  return cached("date", options, (locale) => new Intl.DateTimeFormat(locale, options));
}

/** "dün", "2 saat önce" / "yesterday", "2 hours ago". */
export function relativeTimeFormat(numeric: "auto" | "always" = "auto"): Intl.RelativeTimeFormat {
  return cached(
    "relative",
    { numeric },
    (locale) => new Intl.RelativeTimeFormat(locale, { numeric }),
  );
}

/**
 * A relative time with a word only where the word is plain: "dün", "yarın", "geçen ay", "bugün"
 * / "yesterday", "last month". Beyond one step Intl's words read dated ("evvelsi gün", "öbür gün"),
 * so those are numbers: "2 gün önce", "2 gün sonra".
 */
export function relativeWords(value: number, unit: Intl.RelativeTimeFormatUnit): string {
  return relativeTimeFormat(Math.abs(value) <= 1 ? "auto" : "always").format(value, unit);
}

/** "A, B ve C" / "A, B and C". */
export function listFormat(type: "conjunction" | "disjunction" = "conjunction"): Intl.ListFormat {
  return cached("list", { type }, (locale) => new Intl.ListFormat(locale, { style: "long", type }));
}

const BYTE_UNITS = ["B", "KB", "MB", "GB", "TB"] as const;

/**
 * "512 B", "1,5 MB" / "1.5 MB", "1 GB". Never four digits before the unit: from what would read
 * as 1000 of a unit (1000–1023 MB, or 999,96 MB rounding up) it switches to the next one.
 */
export function formatBytes(bytes: number): string {
  let value = Math.max(0, bytes);
  let unit = 0;
  const shown = () => (unit === 0 ? Math.round(value) : Math.round(value * 10) / 10);
  while (shown() >= 1000 && unit < BYTE_UNITS.length - 1) {
    value /= 1024;
    unit++;
  }
  const format = numberFormat(unit === 0 ? undefined : { maximumFractionDigits: 1 });
  return `${format.format(shown())} ${BYTE_UNITS[unit]}`;
}

export function formatSpeed(bytesPerSec: number): string {
  return i18n.t("units.perSecond", { size: formatBytes(bytesPerSec) });
}

export function formatCount(n: number): string {
  return numberFormat().format(n);
}

/** A share out of 100, the way the language writes it: "%42" in Turkish, "42%" in English. */
export function formatPercent(percent: number): string {
  return numberFormat({ style: "percent", maximumFractionDigits: 0 }).format(
    Math.round(percent) / 100,
  );
}

/** Memory: `6144` → "6 GB", `4608` → "4,5 GB" / "4.5 GB", `768` → "768 MB". */
export function formatMemory(mb: number): string {
  if (mb < 1024) return `${Math.round(mb)} MB`;
  return `${numberFormat({ maximumFractionDigits: 1 }).format(mb / 1024)} GB`;
}

/**
 * "az önce", "5 dakika önce", "3 saat önce", "dün", "2 gün önce", then the date ("3 Temmuz").
 * Under a minute it is "az önce" / "just now": Intl says "bu dakika" / "this minute", which read
 * oddly in "Son oynama …" and "… başladı".
 */
export function formatRelative(iso: string, now = Date.now()): string {
  const diff = new Date(iso).getTime() - now;
  const abs = Math.abs(diff);
  const minute = 60_000;
  const hour = 60 * minute;
  const day = 24 * hour;
  if (abs < minute) return diff <= 0 ? i18n.t("units.justNow") : relativeWords(1, "minute");
  if (abs < hour) return relativeWords(Math.round(diff / minute), "minute");
  if (abs < day) return relativeWords(Math.round(diff / hour), "hour");
  if (abs < 30 * day) return relativeWords(Math.round(diff / day), "day");
  return dateTimeFormat({ day: "numeric", month: "long" }).format(new Date(iso));
}

/** "2 sa 14 dk", "38 dk", "45 sn" / "2 h 14 min" (units from the strings, `units.*`). */
export function formatDuration(totalSecs: number): string {
  const seconds = Math.max(0, Math.round(totalSecs));
  const hours = Math.floor(seconds / 3600);
  const minutes = Math.floor((seconds % 3600) / 60);
  if (hours > 0) {
    return minutes > 0
      ? i18n.t("units.hoursMinutes", { hours, minutes })
      : i18n.t("units.hours", { hours });
  }
  if (minutes > 0) return i18n.t("units.minutes", { minutes });
  return i18n.t("units.seconds", { seconds });
}
