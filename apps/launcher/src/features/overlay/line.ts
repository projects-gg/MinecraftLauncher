// The help line under Settings → Genel "Oyun içi katman" (OVL2, OVL-SEL1): which of its lines
// shows, and the install its switch ("Resmî pakete geç") picks. Kept free of UI imports so
// the rules are tested on their own.
import { hasOfficial } from "@/features/library/official";
import type { InstanceSummary, OfficialStatus } from "@/lib/api";

/**
 * The lines, in the order they share one grid cell (the cell keeps the tallest one's height, so
 * nothing below moves when they take turns). Each that says it will not work names where or why,
 * and its action (if any) the fix:
 * - `opens`: how it opens (the switch is off, or nothing stands in its way);
 * - `noHome`: no install can carry it, with "Resmî paketi kur" (the official pack was deleted and
 *   no Fabric install of the player's is on a version the mod ships for);
 * - `closes`: "Oyun açılınca launcher: Kapansın" is chosen, with "Kapanmak yerine küçülsün";
 * - `notFabric`: the install Play opens is not Fabric (the backend's `NotFabric`);
 * - `version`: it is Fabric on a version the mod does not ship for (`NoJar`);
 * - `notYet` (OVL-AUTO): it is Fabric on a release newer than every version the launcher has a
 *   tested jar for, its own or the signed content's: "not out for Minecraft 26.4 yet". A tested jar
 *   comes through the content without a launcher update; an untested one never goes in;
 * - `waits`: one of those while its game is prepared or open: the command bar's picker is locked,
 *   so the line says where it can switch after the game.
 * `notFabric` and `version` say the same need ("Fabric 26.1.2–26.3", `versionRange`); they and
 * `notYet` switch Play to an install that carries it, named in the action.
 */
export const OVERLAY_LINES = [
  "opens",
  "noHome",
  "closes",
  "notFabric",
  "version",
  "notYet",
  "waits",
] as const;
export type OverlayLineKind = (typeof OVERLAY_LINES)[number];

export interface OverlayLine {
  kind: OverlayLineKind;
  /**
   * The install the line selects or names (`notFabric`, `version`, `notYet`, `waits`): none while
   * no other install can carry the overlay.
   */
  switchTo?: InstanceSummary;
}

export interface OverlayLineInput {
  /** The switch. */
  on: boolean;
  /** "Oyun açılınca launcher: Kapansın": the launcher is gone when the game would need it. */
  leaves: boolean;
  instances: readonly InstanceSummary[] | undefined;
  official: OfficialStatus | undefined;
  /**
   * The Minecraft versions the launcher has the mod for: its own jars', then the signed content's
   * (`overlay.state().versions`).
   */
  versions: readonly string[] | undefined;
  /** The install Play opens: the command bar's rule (`usePlayInstance`). */
  play: InstanceSummary | undefined;
  /** A game is prepared or open: the command bar's picker is locked, so nothing switches. */
  locked: boolean;
}

/**
 * The line while the switch is on and the overlay cannot work says why, the most basic blocker
 * first: no install at all can carry it, then "Kapansın" (no game gets it), then the install Play
 * opens. With the switch off it says what turning it on gives. While any list is unknown nothing
 * about the install shows, so the line never flickers while the page opens.
 */
export function overlayLine(input: OverlayLineInput): OverlayLine {
  const { on, leaves, instances, official, versions, play, locked } = input;
  if (!on) return { kind: "opens" };
  if (overlayHasNoHome(instances, official, versions)) return { kind: "noHome" };
  if (leaves) return { kind: "closes" };
  if (!instances || !official || !versions || !play) return { kind: "opens" };
  if (carriesOverlay(play, official, versions)) return { kind: "opens" };
  const switchTo = overlayHome(instances, official, versions);
  // The switch waits for the game; with nothing to switch to, the reason stays as it is.
  if (locked && switchTo) return { kind: "waits", switchTo };
  if (play.loader !== "fabric") return { kind: "notFabric", switchTo };
  return { kind: newerThanAll(play.mcVersion, versions) ? "notYet" : "version", switchTo };
}

/**
 * Whether `minecraft` is a release newer than every one of `versions` (OVL-AUTO's "henüz yok":
 * its tested jar is still to come). Snapshots and an empty list are not.
 */
export function newerThanAll(minecraft: string, versions: readonly string[]): boolean {
  const have = releaseNumbers(minecraft);
  if (!have || versions.length === 0) return false;
  return versions.every((shipped) => {
    const want = releaseNumbers(shipped);
    if (!want) return true;
    for (let i = 0; i < Math.max(have.length, want.length); i++) {
      const d = (have[i] ?? 0) - (want[i] ?? 0);
      if (d !== 0) return d > 0 && !shipsFor(shipped, minecraft);
    }
    return false;
  });
}

/**
 * The Minecraft versions the mod ships for, as one range for a sentence ("26.1.2–26.3"; one
 * version alone as it is). Patch releases in between count too (`shipsFor`). Releases in their
 * order, anything else after them.
 */
export function versionRange(versions: readonly string[]): string {
  const sorted = [...versions].sort((a, b) => {
    const x = releaseNumbers(a);
    const y = releaseNumbers(b);
    if (!x || !y) return x ? -1 : y ? 1 : a.localeCompare(b);
    for (let i = 0; i < Math.max(x.length, y.length); i++) {
      const d = (x[i] ?? 0) - (y[i] ?? 0);
      if (d !== 0) return d;
    }
    return 0;
  });
  const first = sorted[0];
  const last = sorted[sorted.length - 1];
  if (first === undefined || last === undefined) return "";
  return first === last ? first : `${first}–${last}`;
}

/**
 * Whether no instance follows the official pack, the library's rule (`hasOfficial`; a copy
 * counts). Unknown lists count as there, so the line never flickers while the page opens.
 */
export function officialMissing(
  instances: readonly InstanceSummary[] | undefined,
  official: OfficialStatus | undefined,
): boolean {
  if (!instances) return false;
  if (instances.length > 0 && !official) return false;
  return !hasOfficial(instances, official);
}

/** A release's numbers ("26.1.2" → 26, 1, 2); snapshots and pre-releases have none. */
function releaseNumbers(version: string): number[] | null {
  return /^\d+(\.\d+){1,2}$/.test(version) ? version.split(".").map(Number) : null;
}

/**
 * Whether the mod shipped for Minecraft `shipped` runs on `minecraft`. Every jar declares
 * `~<shipped>` (`mods/projects-overlay/build.gradle`), which the backend matches (`jar_for`): the
 * same major and minor with that patch or a later one, so `26.2` also takes `26.2.1` and `26.1.2`
 * takes `26.1.3`, never `26.1.1` or `26.20`.
 */
export function shipsFor(shipped: string, minecraft: string): boolean {
  const want = releaseNumbers(shipped);
  const have = releaseNumbers(minecraft);
  if (!want || !have) return shipped === minecraft;
  return have[0] === want[0] && have[1] === want[1] && (have[2] ?? 0) >= (want[2] ?? 0);
}

/**
 * Whether the launcher puts the overlay's mod in `instance` at its start (OVL-B): a Fabric
 * instance on a version it ships the mod for, official pack or not. A build without the jars
 * (`versions` empty) leaves it to the official pack's own item, as before OVL-B.
 */
export function carriesOverlay(
  instance: InstanceSummary,
  official: OfficialStatus | undefined,
  versions: readonly string[],
): boolean {
  if (versions.length === 0) return instance.managed || instance.id === official?.instanceId;
  return (
    instance.loader === "fabric" &&
    versions.some((shipped) => shipsFor(shipped, instance.mcVersion))
  );
}

/**
 * "Resmî paket kurulu değil" (with "Kur") while no instance can get the overlay: the official pack
 * is missing and no Fabric instance of the player's own is on a version the mod is shipped for.
 * Unknown versions count as a home, as unknown lists do.
 */
export function overlayHasNoHome(
  instances: readonly InstanceSummary[] | undefined,
  official: OfficialStatus | undefined,
  versions: readonly string[] | undefined,
): boolean {
  if (!versions) return false;
  return (
    officialMissing(instances, official) &&
    !(instances ?? []).some((instance) => carriesOverlay(instance, official, versions))
  );
}

/** When an instance was last played (ms); never played sorts last. */
function playedAt(instance: InstanceSummary): number {
  const at = instance.lastPlayedAt ? Date.parse(instance.lastPlayedAt) : Number.NaN;
  return Number.isNaN(at) ? Number.NEGATIVE_INFINITY : at;
}

/**
 * The install the line's switch selects: the official pack (the instance the status names, then a
 * copy that follows the pack), else the player's own install that carries the overlay; the last
 * played first within each, the list's order after that. None while no install carries it.
 */
export function overlayHome(
  instances: readonly InstanceSummary[],
  official: OfficialStatus | undefined,
  versions: readonly string[],
): InstanceSummary | undefined {
  const rank = (instance: InstanceSummary) =>
    instance.id === official?.instanceId ? 0 : instance.managed ? 1 : 2;
  let best: InstanceSummary | undefined;
  for (const instance of instances) {
    if (!carriesOverlay(instance, official, versions)) continue;
    if (
      !best ||
      rank(instance) < rank(best) ||
      (rank(instance) === rank(best) && playedAt(instance) > playedAt(best))
    ) {
      best = instance;
    }
  }
  return best;
}
