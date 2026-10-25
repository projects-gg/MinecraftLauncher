// The site's vanilla atlases (ITEMART): drawn ahead of time from the game's client jars
// (`pl-items`' `build_atlas` example with `--cdn`) and served from our CDN under content-addressed
// names that never change; a short-lived `manifest.json` names each version's index.
import type { VanillaAtlas } from "./types";
import { vanillaAtlas } from "./vanilla";

/** Where the site's vanilla atlases live (`manifest.json` and one folder per version). */
export const VANILLA_ATLAS_BASE = "https://cdn.projects.gg/itemart/vanilla/";

/** `manifest.json`: each Minecraft version's index (relative to the manifest) and the default. */
export interface VanillaAtlasManifest {
  format: number;
  /** The version to draw with when the account's is unknown (the Projects server's). */
  default: string;
  versions: Record<string, string>;
}

/** The manifest format this code reads. */
export const VANILLA_MANIFEST_FORMAT = 1;

/** `26.1.2` → `[26, 1, 2]`; a part that is not a number counts as 0. */
function versionParts(version: string): number[] {
  return version.split(".").map((part) => Number.parseInt(part, 10) || 0);
}

function compareVersions(a: string, b: string): number {
  const [pa, pb] = [versionParts(a), versionParts(b)];
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/**
 * The manifest's version to draw a game version's items with: its own atlas, else the newest
 * older one (the textures closest to what the player sees), else the default.
 */
export function pickVanillaVersion(
  manifest: VanillaAtlasManifest,
  minecraft?: string,
): string | undefined {
  const versions = Object.keys(manifest.versions).sort(compareVersions);
  if (minecraft) {
    if (manifest.versions[minecraft]) return minecraft;
    const older = versions.filter((v) => compareVersions(v, minecraft) <= 0);
    const newestOlder = older[older.length - 1];
    if (newestOlder) return newestOlder;
  }
  if (manifest.versions[manifest.default]) return manifest.default;
  return versions[versions.length - 1];
}

type FetchJson = (url: string) => Promise<unknown>;

const fetchJson: FetchJson = async (url) => {
  const response = await fetch(url);
  return response.ok ? response.json() : undefined;
};

const loading = new Map<string, Promise<VanillaAtlas | undefined>>();

/**
 * The vanilla atlas for a game version from the CDN (`pickVanillaVersion`); undefined when the CDN
 * cannot be reached or speaks a newer format (sprites then show their stand-ins). Asked once per
 * version; a miss is asked again next time.
 */
export function loadVanillaAtlas(
  minecraft?: string,
  base: string = VANILLA_ATLAS_BASE,
  get: FetchJson = fetchJson,
): Promise<VanillaAtlas | undefined> {
  const key = `${base}|${minecraft ?? ""}`;
  const hit = loading.get(key);
  if (hit) return hit;
  const load = (async () => {
    try {
      const manifestUrl = new URL("manifest.json", base);
      const manifest = (await get(manifestUrl.href)) as VanillaAtlasManifest | undefined;
      if (
        !manifest ||
        manifest.format !== VANILLA_MANIFEST_FORMAT ||
        typeof manifest.versions !== "object"
      ) {
        return undefined;
      }
      const version = pickVanillaVersion(manifest, minecraft);
      const path = version ? manifest.versions[version] : undefined;
      if (!path) return undefined;
      const indexUrl = new URL(path, manifestUrl);
      const index = (await get(indexUrl.href)) as
        | { image?: unknown; large?: { image?: unknown } }
        | undefined;
      if (!index || typeof index.image !== "string") return undefined;
      const large = index.large?.image;
      return vanillaAtlas(
        index,
        new URL(index.image, indexUrl).href,
        typeof large === "string" ? new URL(large, indexUrl).href : undefined,
      );
    } catch {
      return undefined;
    }
  })();
  loading.set(key, load);
  void load.then((atlas) => {
    if (!atlas) loading.delete(key);
  });
  return load;
}
