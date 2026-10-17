// A pack's optional files in the import preview: names, folders and what still gets downloaded.
import type { PackPreview } from "@/lib/api";

export type OptionalFolder = "mods" | "resourcepacks" | "shaderpacks" | "other";

export function optionalFolder(folder: string): OptionalFolder {
  return folder === "mods" || folder === "resourcepacks" || folder === "shaderpacks"
    ? folder
    : "other";
}

/**
 * A readable name from a file name until Modrinth names the project:
 * `distanthorizons-2.3.0-b-1.21.1.jar` → `Distanthorizons`.
 */
export function prettyFileName(fileName: string): string {
  const stem = fileName.replace(/\.(jar|zip|litemod)$/i, "");
  const cut = stem.search(/[-_+ ](?:v?\d|mc\d)/i);
  const base = (cut > 0 ? stem.slice(0, cut) : stem).replace(/[-_]+/g, " ").trim() || stem;
  // File names are English: "iris" must become "Iris", not the Turkish "İris".
  return base.charAt(0).toUpperCase() + base.slice(1);
}

/** Downloads left after the player turned some optional files off. */
export function optionalTotals(
  preview: Pick<PackPreview, "downloadFiles" | "downloadBytes" | "optional">,
  off: Set<string>,
): { files: number; bytes: number } {
  let files = preview.downloadFiles;
  let bytes = preview.downloadBytes;
  for (const file of preview.optional) {
    if (!off.has(file.path)) continue;
    files -= 1;
    bytes -= file.size;
  }
  return { files: Math.max(0, files), bytes: Math.max(0, bytes) };
}
