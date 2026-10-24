// Pure helpers of the mod center: which recommended mods end up installed for a set of
// choices (the `requires` closure, as the launcher computes it) and what changed.
import type { ModCategory, ModCenterItem, ModSelection, SyncSummary } from "@/lib/api";
import { matchesQuery } from "@/lib/search";

export type Choices = Record<string, boolean>;

/** The player's current choices (libraries are never choices). */
export function choicesOf(items: ModCenterItem[]): Choices {
  return Object.fromEntries(items.filter((i) => !i.library).map((i) => [i.id, i.selected]));
}

/**
 * Installed mods for `choices`: the selected ones plus, transitively, what they require, and what
 * the launcher's own in-game overlay needs (`launcherNeeds`: Fabric API while it is on).
 */
export function enabledSet(items: ModCenterItem[], choices: Choices): Set<string> {
  const byId = new Map(items.map((i) => [i.id, i]));
  const enabled = new Set<string>();
  const visit = (id: string) => {
    if (enabled.has(id)) return;
    const item = byId.get(id);
    if (!item) return;
    enabled.add(id);
    for (const required of item.requires) visit(required);
  };
  for (const item of items) {
    if ((!item.library && choices[item.id]) || item.launcherNeeds) visit(item.id);
  }
  return enabled;
}

/** Enabled mods that need `id`, by title, in list order. */
export function neededBy(items: ModCenterItem[], enabled: Set<string>, id: string): string[] {
  return items
    .filter((i) => i.id !== id && enabled.has(i.id) && i.requires.includes(id))
    .map((i) => i.title);
}

/** [`neededBy`] for `item`, plus `overlay` (the switch's name) when the in-game overlay needs it. */
export function neededFor(
  items: ModCenterItem[],
  enabled: Set<string>,
  item: ModCenterItem,
  overlay: string,
): string[] {
  const needed = neededBy(items, enabled, item.id);
  return item.launcherNeeds ? [...needed, overlay] : needed;
}

/** Titles of what `id` brings along (its requirements, transitively), except `skip`. */
export function bringsAlong(
  items: ModCenterItem[],
  id: string,
  skip: Set<string> = new Set(),
): string[] {
  const byId = new Map(items.map((i) => [i.id, i]));
  const found: string[] = [];
  const seen = new Set<string>([id]);
  const visit = (current: string) => {
    for (const required of byId.get(current)?.requires ?? []) {
      if (seen.has(required)) continue;
      seen.add(required);
      const item = byId.get(required);
      if (item && !skip.has(required)) found.push(item.title);
      visit(required);
    }
  };
  visit(id);
  return found;
}

/** The selection to send: every choice that differs from the recorded one, plus reverts. */
export function pendingSelection(
  items: ModCenterItem[],
  choices: Choices,
  revert: Set<string>,
): ModSelection | null {
  const changed: Choices = {};
  for (const item of items) {
    if (item.library) continue;
    const next = choices[item.id] ?? item.selected;
    if (next !== item.selected) changed[item.id] = next;
  }
  if (Object.keys(changed).length === 0 && revert.size === 0) return null;
  return { items: changed, revert: [...revert] };
}

export const categoryOrder: ModCategory[] = [
  "performance",
  "visual",
  "interface",
  "gameplay",
  "other",
  "library",
];

/**
 * What a plan changes, for the bar: mod center content (mods, resource and shader packs) is
 * counted; settings and other pack files are not choices, so they only raise `otherFiles`.
 */
export function totals(summary: SyncSummary) {
  const content = (c: SyncSummary["added"]) => c.mods + c.resourcePacks + c.shaderPacks;
  const rest = (c: SyncSummary["added"]) => c.configs + c.other;
  return {
    added: content(summary.added),
    updated: content(summary.updated),
    removed: content(summary.removed),
    otherFiles: rest(summary.added) + rest(summary.updated) + rest(summary.removed) > 0,
  };
}

/** The page "Mod sayfası" opens: the item's `pageUrl` when it is an absolute https:// URL. */
export function safePageUrl(url: string | undefined): string | undefined {
  if (!url) return undefined;
  try {
    return new URL(url).protocol === "https:" ? url : undefined;
  } catch {
    return undefined;
  }
}

/** Case-insensitive Turkish search over title and summary. */
export function matches(item: ModCenterItem, query: string): boolean {
  // Folded (lib/search): "iris" finds "Iris", "isik" finds "Işık".
  return matchesQuery(item.title, query) || matchesQuery(item.summary ?? "", query);
}
