// How the modpack update dialog shows a list of changed files: add-ons by name, settings and
// other files as one count.
import type { KeptReason, PackChange } from "@/lib/api";

const ADDON_FOLDERS = new Set(["mods", "resourcepacks", "shaderpacks"]);

export interface ChangeSummary {
  /** Add-ons shown by name (at most `limit`). */
  named: PackChange[];
  /** Add-ons beyond `limit`. */
  more: number;
  /** Settings and other files (configs, options), counted. */
  files: number;
}

export function summarizeChanges(items: PackChange[], limit = 6): ChangeSummary {
  const addons = items
    .filter((item) => ADDON_FOLDERS.has(item.folder))
    .sort((a, b) => a.name.localeCompare(b.name, "tr"));
  return {
    named: addons.slice(0, limit),
    more: Math.max(0, addons.length - limit),
    files: items.length - addons.length,
  };
}

/** Kept files in the order a player cares about: their deletions and switches first. */
export function keptOrder(reason: KeptReason): number {
  return { disabled: 0, removed: 1, changed: 2, theirs: 3 }[reason];
}
