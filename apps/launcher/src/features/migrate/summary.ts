// The v1 launcher's summary (IMPORT-POLISH): what its game data brought, said as the things the
// player knows (worlds, screenshots, packs), never as a count of files.
import type { LegacyImportReport, LegacyScan } from "@/lib/api";

/** A kind of game data the v1 summary names (`import.v1.things.<kind>`). */
export type LegacyThing = "worlds" | "screenshots" | "packs";

/**
 * The kinds the game data line names: the ones the old launcher had and the player picked, or
 * `null` when they cannot be named for sure. A note means something stayed behind (the worlds
 * when the disk was full, the server list): the line then names no kind and the note says what.
 * `worlds`: the player picked the worlds.
 */
export function broughtThings(
  report: LegacyImportReport,
  scan: LegacyScan | undefined,
  worlds: boolean,
): LegacyThing[] | null {
  if (!scan || report.notes.length > 0) return null;
  const things: LegacyThing[] = [];
  if (worlds && scan.worlds > 0) things.push("worlds");
  if (scan.screenshots > 0) things.push("screenshots");
  if (scan.resourcePacks + scan.shaderPacks > 0) things.push("packs");
  return things.length > 0 ? things : null;
}
