// Pure rules of content installs and updates (jobs.ts, parts.tsx), apart from the backend and
// React so they can be tested on their own (REVIEW2).
import type { ContentKind, InstalledContent } from "@/lib/api";

/** A running install/update is keyed by `instanceId:projectId`, or `instanceId:kind:all`. */
export const projectJobKey = (instanceId: string, projectId: string) =>
  `${instanceId}:${projectId}`;
export const bulkJobKey = (instanceId: string, kind: ContentKind) => `${instanceId}:${kind}:all`;

/**
 * The job an update of `entries` runs as: a single entry from Modrinth under its project's key,
 * anything else under the kind's "Tümünü güncelle" key. `null` while that "Tümünü güncelle" job
 * runs: it already covers the entries (its bar may not show yet, `revealWait`), so a press on one
 * mod's "Güncelle" meanwhile starts nothing.
 */
export function updateJobKey(
  jobs: Readonly<Record<string, unknown>>,
  instanceId: string,
  kind: ContentKind,
  entries: ReadonlyArray<Pick<InstalledContent, "projectId">>,
): string | null {
  const bulk = bulkJobKey(instanceId, kind);
  if (jobs[bulk]) return null;
  const single = entries.length === 1 ? entries[0] : undefined;
  return single?.projectId !== undefined ? projectJobKey(instanceId, single.projectId) : bulk;
}

/**
 * A job's bar takes its button's place only once the job has run this long. A job that fails at
 * once (no room on the disk, no connection) leaves the button where it was and its toast says
 * why, instead of a "Hazırlanıyor" bar that flashes for a tenth of a second first (POLISH2).
 */
export const JOB_REVEAL_MS = 150;

/** How much longer the bar of a job this old waits (0: it shows now; a job that started before
 *  the list was opened is `Infinity` old and shows at once). */
export function revealWait(ageMs: number): number {
  return Math.max(0, JOB_REVEAL_MS - ageMs);
}
