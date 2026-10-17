// The bundled profile pictures (`assets/avatars`). Everything comes from the manifest, in its
// order, so the set can grow without code changes. Ids are `preset:<slug>`, the ids pl-auth
// keeps for offline accounts and the friends service keeps for profiles.
import manifest from "@/assets/avatars/manifest.json";

// Written without a type argument on purpose: Vitest keeps transformed modules in a file cache
// and leaves out only files containing the literal `import.meta.glob(`. With `glob<string>(` the
// expanded file list was cached, so a picture added later was missing from tests until
// `vitest --clearCache`.
const files = import.meta.glob("../../assets/avatars/*.webp", {
  eager: true,
  query: "?url",
  import: "default",
}) as Record<string, string>;

export const PRESET_PREFIX = "preset:";
const SLUG = /^[a-z0-9-]{1,32}$/;

export interface PresetAvatar {
  /** `preset:<slug>` */
  id: string;
  slug: string;
  /** Turkish display name. */
  name: string;
  /** Turkish description of the picture. */
  alt: string;
  category: string;
  /** The picture's own colour: shown behind it until it has decoded. */
  accent: string;
  /** 128 px file. */
  small: string;
  /** 256 px file. */
  large: string;
}

export interface ManifestAvatar {
  slug: string;
  name: string;
  alt: string;
  category: string;
  accent: string;
  files: Record<string, string>;
}

/** Same rule as the backend: `preset:` and 1–32 characters of `[a-z0-9-]`. */
export function isPresetId(value: unknown): value is string {
  return (
    typeof value === "string" &&
    value.startsWith(PRESET_PREFIX) &&
    SLUG.test(value.slice(PRESET_PREFIX.length))
  );
}

function bundled(file: string | undefined): string | undefined {
  return file ? files[`../../assets/avatars/${file}`] : undefined;
}

/**
 * Manifest entries as pictures, in manifest order. Entries with a bad slug, a repeated slug or a
 * missing file are left out, so a half-updated set never shows a broken tile.
 */
export function readManifest(
  entries: readonly ManifestAvatar[],
  resolve: (file: string | undefined) => string | undefined = bundled,
): PresetAvatar[] {
  const seen = new Set<string>();
  const avatars: PresetAvatar[] = [];
  for (const entry of entries) {
    if (!SLUG.test(entry.slug) || seen.has(entry.slug)) continue;
    const small = resolve(entry.files["128"]) ?? resolve(entry.files["256"]);
    const large = resolve(entry.files["256"]) ?? small;
    if (!small || !large) continue;
    seen.add(entry.slug);
    avatars.push({
      id: `${PRESET_PREFIX}${entry.slug}`,
      slug: entry.slug,
      name: entry.name,
      alt: entry.alt,
      category: entry.category,
      accent: entry.accent,
      small,
      large,
    });
  }
  return avatars;
}

/** Every bundled picture, in picker order. */
export const PRESET_AVATARS: readonly PresetAvatar[] = readManifest(manifest.avatars);

const byId = new Map(PRESET_AVATARS.map((avatar) => [avatar.id, avatar]));

/** The bundled picture with this id; null for anything else (an unknown id shows the head). */
export function presetAvatar(id: string | null | undefined): PresetAvatar | null {
  return id ? (byId.get(id) ?? null) : null;
}
