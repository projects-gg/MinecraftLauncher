// SRV-ACC "Sunucu Hesapların": which of the player's server accounts the account center offers, what
// the row shows of them, and the list's digest that "Şimdi Değil" keeps. Pure, so it is tested.
import type { AccountSummary } from "@/lib/api";

/** Minecraft's rule for player names (the backend adds only these). */
const PLAYER_NAME = /^[A-Za-z0-9_]{3,16}$/;

/** How long a list read stays good: the center asks again at most this often. */
export const SERVER_ACCOUNTS_STALE_MS = 5 * 60_000;

/** How long "Eklendi" and its "Geri Al" stay on the row. */
export const ADDED_MS = 8_000;

/**
 * The names of `list` the launcher has no account for (a player name in any letter case; servers
 * see one player), in the list's order, each once. Names that are not player names are left out.
 */
export function missingNames(
  list: readonly string[],
  accounts: readonly Pick<AccountSummary, "username">[],
): string[] {
  const have = new Set(accounts.map((account) => account.username.toLowerCase()));
  const out: string[] = [];
  for (const name of list) {
    const key = name.toLowerCase();
    if (!PLAYER_NAME.test(name) || have.has(key)) continue;
    have.add(key);
    out.push(name);
  }
  return out;
}

/**
 * The list's digest for "Şimdi Değil": the same names in any order and letter case give the same
 * one, so the row stays quiet until the list itself changes (FNV-1a, 32 bits, hex).
 */
export function listDigest(list: readonly string[]): string {
  const text = [...new Set(list.map((name) => name.toLowerCase()))].sort().join("\n");
  let hash = 0x811c9dc5;
  for (let i = 0; i < text.length; i++) {
    hash ^= text.charCodeAt(i);
    hash = Math.imul(hash, 0x01000193);
  }
  return (hash >>> 0).toString(16).padStart(8, "0");
}

/**
 * The names the row's line shows: the most that fit `room` together with the "+N" after them
 * (`width` measures a text as the line draws it), at least one; and how many more there are.
 */
export function fitNames(
  names: readonly string[],
  room: number,
  width: (text: string) => number,
  more: (count: number) => string,
): { shown: string[]; more: number } {
  for (let count = names.length; count > 1; count--) {
    const rest = names.length - count;
    const text = names.slice(0, count).join(", ") + (rest > 0 ? ` ${more(rest)}` : "");
    if (width(text) <= room) return { shown: names.slice(0, count), more: rest };
  }
  return { shown: names.slice(0, 1), more: Math.max(0, names.length - 1) };
}
