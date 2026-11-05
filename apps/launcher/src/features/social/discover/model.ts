// "Ekip bul" (CREW-DISCOVER), pure rules (model.test.ts): searching and filtering the listing on
// this computer, when the kept copy is asked for again, the leader's text as the service counts
// it, and the words of the wave's time ("Bugün 18:00'de").
import {
  DISCOVER_REVALIDATE_MS,
  DISCOVER_TEXT_MAX,
  DISCOVER_TEXT_MIN,
  type DiscoverCrew,
  type DiscoverListing,
  type DiscoverStatus,
  type DiscoverTag,
} from "@/lib/api/discover-types";
import { timeLocative } from "@/lib/event-card";
import { matchesQuery } from "@/lib/search";

/** A crew's size as the filter groups it. */
export type CrewSize = "any" | "small" | "mid" | "big";

export interface DiscoverFilter {
  query: string;
  /** Tags a crew must all have. */
  tags: DiscoverTag[];
  /** Only crews that take newcomers at once ("Hemen katılınabilenler"). */
  open: boolean;
  /** Only crews active this week. */
  active: boolean;
  size: CrewSize;
}

export const NO_FILTER: DiscoverFilter = {
  query: "",
  tags: [],
  open: false,
  active: false,
  size: "any",
};

/** Small 2-9, mid 10-19, big 20 and more (a crew holds 30). */
export function sizeOf(members: number): Exclude<CrewSize, "any"> {
  return members >= 20 ? "big" : members >= 10 ? "mid" : "small";
}

/** How many of the menu's filters are on (the button's count; the tags show on their own). */
export function menuFilters(filter: DiscoverFilter): number {
  return Number(filter.open) + Number(filter.active) + Number(filter.size !== "any");
}

/** Whether anything narrows the list. */
export function filtering(filter: DiscoverFilter): boolean {
  return filter.query.trim() !== "" || filter.tags.length > 0 || menuFilters(filter) > 0;
}

/** The crews the filter keeps, in the listing's own order (the day's, the same for everyone). */
export function filterCrews(
  crews: readonly DiscoverCrew[],
  filter: DiscoverFilter,
  tagWords: (tag: DiscoverTag) => string,
): DiscoverCrew[] {
  const query = filter.query.trim();
  return crews.filter(
    (crew) =>
      (!query ||
        matchesQuery(crew.name, query) ||
        matchesQuery(crew.tag, query) ||
        matchesQuery(crew.text, query) ||
        crew.tags.some((tag) => matchesQuery(tagWords(tag), query))) &&
      filter.tags.every((tag) => crew.tags.includes(tag)) &&
      (!filter.open || crew.join === "open") &&
      (!filter.active || crew.active) &&
      (filter.size === "any" || sizeOf(crew.members) === filter.size),
  );
}

/** A copy of the listing this computer keeps, and when it was asked for (ms). */
export interface KeptListing {
  listing: DiscoverListing;
  etag: string | null;
  fetchedAt: number;
}

/**
 * What opening "Ekip bul" does with the kept copy: `fetch` without one or once the wave it names
 * has passed; `revalidate` (with its ETag: a quick "unchanged") when it is a quarter of an hour
 * old; otherwise `keep`.
 */
export function freshness(
  kept: KeptListing | null,
  nowMs: number,
): "fetch" | "revalidate" | "keep" {
  if (!kept) return "fetch";
  if (nowMs >= kept.listing.next * 1000) return "fetch";
  return nowMs - kept.fetchedAt >= DISCOVER_REVALIDATE_MS ? "revalidate" : "keep";
}

/** The leader's text as the service will keep it: one paragraph, spaces collapsed, trimmed. */
export function cleanText(text: string): string {
  // Invisible characters go, as the service drops them (format and control ones but spacing).
  return text
    .replace(/[\p{Cf}\p{Co}]|(?!\s)\p{Cc}/gu, "")
    .replace(/\s+/gu, " ")
    .trim();
}

/** Its length as the service counts it (characters, not UTF-16 units). */
export function textLength(text: string): number {
  return [...cleanText(text)].length;
}

/** Whether the service takes it: 10-160 characters with at least 5 letters. */
export function textReady(text: string): boolean {
  const clean = cleanText(text);
  const length = [...clean].length;
  return (
    length >= DISCOVER_TEXT_MIN &&
    length <= DISCOVER_TEXT_MAX &&
    (clean.match(/\p{L}/gu)?.length ?? 0) >= 5
  );
}

/** The wave's day and time: today, tomorrow or later ("Bugün 18:00'de"). */
export function waveWhen(
  waveAt: number,
  nowMs: number,
): { day: "today" | "tomorrow" | "later"; hour: number; minute: number } {
  const at = new Date(waveAt * 1000);
  const dayOf = (d: Date) => new Date(d.getFullYear(), d.getMonth(), d.getDate()).getTime();
  const days = Math.round((dayOf(at) - dayOf(new Date(nowMs))) / 86_400_000);
  return {
    day: days <= 0 ? "today" : days === 1 ? "tomorrow" : "later",
    hour: at.getHours(),
    minute: at.getMinutes(),
  };
}

/** The Turkish ending of the wave's time ("18:00'de"). */
export function waveEnding(hour: number, minute: number): string {
  return timeLocative(hour, minute);
}

/** Whole days until `untilSec` (at least one: "1 gün sonra"). */
export function daysUntil(untilSec: number, nowMs: number): number {
  return Math.max(1, Math.ceil((untilSec * 1000 - nowMs) / 86_400_000));
}

/** Whether every requirement the service shows holds (the editor opens only then). */
export function checksHold(checks: DiscoverStatus["checks"]): boolean {
  return (
    checks.visible &&
    checks.members.have >= checks.members.need &&
    checks.crewDays.have >= checks.crewDays.need &&
    checks.leaderDays.have >= checks.leaderDays.need
  );
}

/** The editor may send: a ready text, at most 3 tags, and something that is not already there. */
export function maySend(
  status: DiscoverStatus,
  text: string,
  tags: readonly DiscoverTag[],
): boolean {
  if (!textReady(text) || tags.length > 3 || status.editsLeft <= 0) return false;
  const now = status.changeWaits || status.state === "waiting" ? status.draft : status.listed;
  if (!now) return true;
  return (
    now.text !== cleanText(text) ||
    now.tags.length !== tags.length ||
    now.tags.some((tag) => !tags.includes(tag))
  );
}
