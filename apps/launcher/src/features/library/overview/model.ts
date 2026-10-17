// The instance overview ("Genel"): which shortcuts it shows, in what order, and how much room
// the instance takes. Pure, so the rules are tested without rendering.
import { parseAddress } from "@/features/resume/model";
import type { ResumeTarget, SavedServer } from "@/lib/api";

/** World rows under the "Devam et" card; one more when there is no card. */
export const WORLD_ROWS = 2;
/** Screenshot tiles. */
export const SHOT_TILES = 3;

/** The community server: featured whenever the instance lists it. */
const PROJECTS_HOST = "play.projects.gg";

/** The card that opens the play column. */
export type LeadCard = "resume" | "neverPlayed" | "nothing";

/** Never started from the launcher: no last session and no play time. */
export function neverPlayed(instance: { lastPlayedAt?: string; playTimeSecs: number }): boolean {
  return !instance.lastPlayedAt && instance.playTimeSecs <= 0;
}

/** The world folder "Devam et" opens, when it opens one. */
function resumeWorld(resume: ResumeTarget | null): string | null {
  return resume?.quickPlay.kind === "singleplayer" ? resume.quickPlay.world : null;
}

const serverKey = (host: string, port: number) => `${host}:${port}`;

/** The server "Devam et" joins (lowercase host and port), when it joins one. */
function resumeServer(resume: ResumeTarget | null): string | null {
  if (resume?.quickPlay.kind !== "multiplayer") return null;
  const parsed = parseAddress(resume.quickPlay.host);
  if (!parsed) return null;
  return serverKey(parsed.host, resume.quickPlay.port ?? parsed.port);
}

function addressKey(address: string): string | null {
  const parsed = parseAddress(address);
  return parsed ? serverKey(parsed.host, parsed.port) : null;
}

const playedAt = (world: { lastPlayedAt?: string }) => {
  const time = world.lastPlayedAt ? Date.parse(world.lastPlayedAt) : Number.NaN;
  return Number.isNaN(time) ? Number.NEGATIVE_INFINITY : time;
};

/**
 * The most recently played worlds, without the one "Devam et" opens (the card above shows it):
 * `others` says so, for the section's title. `more`: the worlds tab has some the list leaves out.
 */
export function recentWorlds<W extends { folder: string; lastPlayedAt?: string }>(
  worlds: readonly W[],
  resume: ResumeTarget | null,
): { shown: W[]; more: boolean; others: boolean } {
  const skip = resumeWorld(resume);
  // Stable sort: worlds played at the same moment keep the order they came in.
  const rest = worlds
    .filter((world) => world.folder !== skip)
    .sort((a, b) => playedAt(b) - playedAt(a));
  const rows = resume ? WORLD_ROWS : WORLD_ROWS + 1;
  return { shown: rest.slice(0, rows), more: rest.length > rows, others: skip !== null };
}

/**
 * The server worth one click: the community server when the instance lists it, else the first
 * of the player's own list; never the one "Devam et" already joins. `more`: other servers wait
 * in the servers tab.
 */
export function featuredServer(
  servers: readonly SavedServer[],
  resume: ResumeTarget | null,
): { server: SavedServer | null; more: boolean } {
  const skip = resumeServer(resume);
  const candidates = servers.filter(
    (server) => skip === null || addressKey(server.address) !== skip,
  );
  const server =
    candidates.find((s) => parseAddress(s.address)?.host === PROJECTS_HOST) ??
    candidates[0] ??
    null;
  return { server, more: candidates.length > 1 };
}

/** The newest screenshots (the list comes newest first). */
export function recentShots<S>(shots: readonly S[]): { shown: S[]; more: boolean } {
  return { shown: shots.slice(0, SHOT_TILES), more: shots.length > SHOT_TILES };
}

/**
 * The card on top of the play column: "Devam et" when there is somewhere to go back to; for an
 * instance never played (and no worlds or screenshots brought along), the one empty state with
 * "Oyna" (servers the launcher listed for it still show below); for a played instance with
 * nothing at all to show, the same card in other words. Otherwise the sections start at once.
 */
export function leadCard(input: {
  played: boolean;
  resume: boolean;
  worlds: number;
  shots: number;
  server: boolean;
}): LeadCard | null {
  if (input.resume) return "resume";
  if (input.worlds > 0 || input.shots > 0) return null;
  if (!input.played) return "neverPlayed";
  return input.server ? null : "nothing";
}

interface Sized {
  sizeBytes: number;
}

const total = (items: readonly Sized[] | undefined) =>
  (items ?? []).reduce((sum, item) => sum + Math.max(0, item.sizeBytes), 0);

/**
 * Room the instance's own files take: the mods of the current version, resource and shader
 * packs, worlds, screenshots and world backups. The game files every instance shares are not
 * counted (deleting the instance does not free them).
 */
export function instanceBytes(parts: {
  mods: readonly Sized[];
  resourcePacks: readonly Sized[];
  shaderPacks: readonly Sized[];
  worlds: readonly Sized[];
  screenshots?: readonly Sized[];
  backups?: readonly Sized[];
}): number {
  return (
    total(parts.mods) +
    total(parts.resourcePacks) +
    total(parts.shaderPacks) +
    total(parts.worlds) +
    total(parts.screenshots) +
    total(parts.backups)
  );
}
