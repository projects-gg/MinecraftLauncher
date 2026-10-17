// The first moments after an update restart: the player lands on the page they left and
// "Yenilikler" shows the new version's notes once. The backend's restart note carries both
// (`update_restart_info`, taken once); the notes copy the UI kept in local storage is the fallback,
// and the only copy when an older launcher installed this version.
import { commands, type RestartInfo } from "@/bindings";
import { isTauri } from "@/lib/platform";

export interface ReleaseNotes {
  version: string;
  notes: string;
}

let pending: Promise<RestartInfo | null> | null = null;

/** What the restart left for the UI. One backend call per start, shared by every caller. */
export function loadAfterUpdate(): Promise<RestartInfo | null> {
  if (!isTauri()) return Promise.resolve(null);
  pending ??= commands.updateRestartInfo().catch(() => null);
  return pending;
}

/** The notes to show: the restart note's, else the stored copy (already checked for the version). */
export function chooseNotes(
  after: RestartInfo | null,
  stored: ReleaseNotes | null,
): ReleaseNotes | null {
  const notes = after?.notes;
  if (after && notes?.trim()) return { version: after.version, notes };
  return stored;
}

/**
 * The page to return to, if the player is still on the start page (a link or a quick click may
 * already have taken them elsewhere; that wins).
 */
export function landingRoute(after: RestartInfo | null, currentPath: string): string | null {
  const route = after?.route;
  if (!route?.startsWith("/") || route.startsWith("//")) return null;
  if (currentPath !== "/" || route === "/") return null;
  return route;
}
