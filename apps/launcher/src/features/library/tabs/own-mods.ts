// "Kendi modların" (MODHUB2): which files of the mods folder are the player's own. Every mod has
// one home in the "Modlar" tab: a ready-made set's files are its set's, the official pack's are
// its recommended ones (only its instances have that section), the rest are the player's.
import type { ModEntry } from "@/lib/api";

/** The file name without the `.disabled` a switched-off mod carries. */
export const baseName = (fileName: string) => fileName.replace(/\.disabled$/, "");

/**
 * The player's own mods of `mods`. `packFile` tells whether the content scan saw a file as the
 * pack's (it knows files the instance list does not mark yet).
 */
export function ownMods(
  mods: readonly ModEntry[],
  managed: boolean,
  packFile: (fileName: string) => boolean = () => false,
): ModEntry[] {
  return mods.filter(
    (mod) => !mod.modSet && !(managed && (mod.fromPack || packFile(baseName(mod.fileName)))),
  );
}
