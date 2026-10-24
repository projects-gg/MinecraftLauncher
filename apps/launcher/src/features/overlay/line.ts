// The help line under Settings → Genel "Oyun içi katman" (OVL2): which of its lines shows. Kept
// free of UI imports so the rule is tested on its own.
import { hasOfficial } from "@/features/library/official";
import type { InstanceSummary, OfficialStatus } from "@/lib/api";

/** How the overlay opens (0), why "Kapansın" stops it (1), or that the official pack is missing (2). */
export type OverlayLine = 0 | 1 | 2;

/**
 * The line while the switch is on and the overlay cannot work says why, the missing pack first
 * (nothing else matters without it); with the switch off it says what turning it on gives.
 */
export function overlayLine(on: boolean, leaves: boolean, missing: boolean): OverlayLine {
  if (!on) return 0;
  if (missing) return 2;
  return leaves ? 1 : 0;
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

/**
 * Whether a Fabric instance on a Minecraft version the launcher ships the overlay's mod for
 * (`versions`, from the backend: `26.2` also takes `26.2.1`) is there: the launcher puts the mod
 * in it at its start (OVL-B), official pack or not.
 */
export function canCarryOverlay(
  instances: readonly InstanceSummary[] | undefined,
  versions: readonly string[],
): boolean {
  return (instances ?? []).some(
    (instance) =>
      instance.loader === "fabric" &&
      versions.some((v) => instance.mcVersion === v || instance.mcVersion.startsWith(`${v}.`)),
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
  return officialMissing(instances, official) && !canCarryOverlay(instances, versions);
}
