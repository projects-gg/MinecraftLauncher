// Reading a vanilla atlas (ITEMART): the index `pl_items` writes beside the atlas image, from the
// launcher's own drawing or the site's CDN.
import type { VanillaAtlas, VanillaAtlasIndex } from "./types";

/** The index format this code reads (`pl_items::INDEX_FORMAT`). */
export const VANILLA_INDEX_FORMAT = 1;

/**
 * A vanilla atlas from its index (parsed or JSON text), image URL and, when the index has a large
 * atlas (3D icons at 48 px), that image's URL; undefined when the index is of a newer format than
 * this code reads, or not an index at all.
 */
export function vanillaAtlas(
  index: unknown,
  imageUrl: string,
  largeUrl?: string,
): VanillaAtlas | undefined {
  let parsed: VanillaAtlasIndex;
  try {
    parsed = (typeof index === "string" ? JSON.parse(index) : index) as VanillaAtlasIndex;
  } catch {
    return undefined;
  }
  if (
    !parsed ||
    typeof parsed !== "object" ||
    parsed.format !== VANILLA_INDEX_FORMAT ||
    !parsed.icons ||
    !Array.isArray(parsed.size)
  ) {
    return undefined;
  }
  return largeUrl && parsed.large
    ? { index: parsed, imageUrl, largeUrl }
    : { index: parsed, imageUrl };
}
