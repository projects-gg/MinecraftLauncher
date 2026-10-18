import type { GameLogSession } from "@/lib/api";
import { uiLocale } from "@/lib/format";
import i18n from "@/lib/i18n";

const sameDay = (a: Date, b: Date) =>
  a.getFullYear() === b.getFullYear() &&
  a.getMonth() === b.getMonth() &&
  a.getDate() === b.getDate();

/** "Bugün 21:14", "Dün 19:59", "12 Ekim 19:59", "3 Mart 2025 10:02". */
export function formatWhen(iso: string | undefined, now = new Date()): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const locale = uiLocale();
  const time = date.toLocaleTimeString(locale, { hour: "2-digit", minute: "2-digit" });
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (sameDay(date, now)) return i18n.t("library.when.today", { time });
  if (sameDay(date, yesterday)) return i18n.t("library.when.yesterday", { time });
  const day = date.toLocaleDateString(locale, {
    day: "numeric",
    month: "long",
    year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
  });
  return `${day} ${time}`;
}

/** "Bugün", "Dün", "12 Ekim", "3 Mart 2025" (a day without a time). */
export function formatDay(iso: string | undefined, now = new Date()): string {
  if (!iso) return "";
  const date = new Date(iso);
  if (Number.isNaN(date.getTime())) return "";
  const locale = uiLocale();
  const yesterday = new Date(now);
  yesterday.setDate(now.getDate() - 1);
  if (sameDay(date, now)) return i18n.t("library.when.todayDay");
  if (sameDay(date, yesterday)) return i18n.t("library.when.yesterdayDay");
  return date.toLocaleDateString(locale, {
    day: "numeric",
    month: "long",
    year: date.getFullYear() === now.getFullYear() ? undefined : "numeric",
  });
}

/**
 * When one of the game's own log files was written: the day of a dated archive
 * (`2026-10-15-1.log.gz`), else the last write ("Bugün 21:14").
 */
export function logFileWhen(file: GameLogSession, now = new Date()): string {
  return file.startedAt ? formatDay(file.startedAt, now) : formatWhen(file.updatedAt, now);
}
