// Spans of time in an AFK sentence take whole words (design.md §7: "4 dakika sonra yeniden
// bağlanacak", "1 saniye ile 7 gün arasında"), never "240 sn" or "604.800". The words are the event
// card's (`newsColumn.span`), so both read the same.
import type { TFunction } from "i18next";

type Unit = "days" | "hours" | "minutes" | "seconds";

const unit = (t: TFunction, name: Unit, count: number) => t(`newsColumn.span.${name}`, { count });

/**
 * A wait still to come: seconds under a minute ("45 saniye"), then minutes to the nearest ("4
 * dakika" from 3.5 to 4.5 minutes left), then hours and minutes ("1 saat 5 dakika").
 */
export function waitWords(secs: number, t: TFunction): string {
  if (secs < 60) return unit(t, "seconds", Math.max(1, Math.round(secs)));
  const minutes = Math.round(secs / 60);
  if (minutes < 60) return unit(t, "minutes", minutes);
  const hours = Math.floor(minutes / 60);
  const rest = minutes % 60;
  return rest ? `${unit(t, "hours", hours)} ${unit(t, "minutes", rest)}` : unit(t, "hours", hours);
}

/**
 * In how many whole seconds a countdown now at `secs` reads otherwise in `waitWords`: each second
 * under a minute; above it only where the rounded minutes change (89, 149, ... seconds left, and
 * 59 when "1 dakika" turns into seconds), so a line showing it renders then and not every second.
 */
export function waitWordsChangeIn(secs: number): number {
  if (secs <= 60) return 1;
  const minutes = Math.round(secs / 60);
  const next = minutes === 1 ? 59 : 60 * minutes - 31;
  return Math.max(1, secs - next);
}

/**
 * A limit of a field in seconds, exactly, in the largest unit it fills whole: "7 gün", "1 saat",
 * "1 dakika", "90 saniye", "0 saniye".
 */
export function limitWords(secs: number, t: TFunction): string {
  if (secs >= 86_400 && secs % 86_400 === 0) return unit(t, "days", secs / 86_400);
  if (secs >= 3600 && secs % 3600 === 0) return unit(t, "hours", secs / 3600);
  if (secs >= 60 && secs % 60 === 0) return unit(t, "minutes", secs / 60);
  return unit(t, "seconds", secs);
}
