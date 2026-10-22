// "Son kullanılanlar" (CHAT2 §3.1, §8.5): the emoji and stickers the player sent last, newest
// first, kept on this computer only (they change all the time; the settings backup would upload
// them for nothing). The quick reactions read the same list (§7).
import type { EmojiData, EmojiEntry } from "./data";
import { bareEmoji } from "./tone";

/** How many the picker keeps: three rows of emoji, two of stickers. */
export const RECENT_EMOJI = 24;
export const RECENT_STICKERS = 8;

/** The list with `item` first, once, at most `max` long. */
export function remember(list: readonly string[], item: string, max: number): string[] {
  return [item, ...list.filter((other) => other !== item)].slice(0, max);
}

/** Only strings, once each, at most `max` (a stored list may come from an older version). */
export function cleanList(value: unknown, max: number): string[] {
  if (!Array.isArray(value)) return [];
  const out: string[] = [];
  for (const item of value) {
    if (typeof item === "string" && item && !out.includes(item)) out.push(item);
    if (out.length === max) break;
  }
  return out;
}

/** The recent emoji that are in the data (by their bare form), in the recents' order. */
export function recentEntries(data: EmojiData, recents: readonly string[]): EmojiEntry[] {
  const byBare = new Map(data.entries.map((entry) => [bareEmoji(entry.emoji), entry]));
  const out: EmojiEntry[] = [];
  for (const emoji of recents) {
    const entry = byBare.get(bareEmoji(emoji));
    if (entry && !out.includes(entry)) out.push(entry);
  }
  return out;
}
