// The Projects sticker sets (CHAT2, docs: chat2.md §8): one manifest per set, shared by the
// launcher, Studio (from the submodule) and the friends service's allow list. Set pl1 is "Küp" in
// 32 stickers and four themes. A message carries only the reference ("pl1/gg"); every client draws
// the sticker from the files that came with it. The launcher's image URLs are in `./images`
// (bundler only), so this module also loads where there is no bundler (the service, tests).
import pl1Manifest from "../pl1/manifest.json";

export type StickerLanguage = "tr" | "en";

/**
 * How a sticker moves once when it arrives and when the pointer comes onto it (owner, 22 Oct
 * 2026): one pattern per sticker, transform and opacity only, never a loop. The manifest says in
 * words what each one does, so the phone and the overlay move the same way.
 */
export const STICKER_MOTIONS = [
  "bounce",
  "wobble",
  "beat",
  "shake",
  "shiver",
  "wave",
  "nod",
  "sway",
  "float",
  "pop",
  "tilt",
  "swing",
  "sink",
  "glitch",
] as const;
export type StickerMotion = (typeof STICKER_MOTIONS)[number];

export interface StickerTheme {
  id: string;
  name: Record<StickerLanguage, string>;
}

export interface Sticker {
  /** What a message carries: "pl1/gg". */
  ref: string;
  set: string;
  id: string;
  theme: string;
  /** Place in the set, from 1. */
  order: number;
  name: Record<StickerLanguage, string>;
  keywords: Record<StickerLanguage, string[]>;
  /** The words on its plate, if any ("GG", "SAĞ OL"). */
  caption: string | null;
  motion: StickerMotion;
  /** Its cell in the overlay's atlas. */
  atlas: { column: number; row: number };
}

export interface StickerSet {
  set: string;
  version: number;
  themes: StickerTheme[];
  /** In the set's order. */
  stickers: Sticker[];
}

/** A sticker reference as the service accepts one (chat2.md §8.4): "<set>/<id>". */
export const STICKER_REF = /^[a-z0-9]{1,8}\/[a-z0-9-]{1,24}$/;

class ManifestError extends Error {}

function fail(message: string): never {
  throw new ManifestError(`Sticker manifest: ${message}`);
}

const isRecord = (value: unknown): value is Record<string, unknown> =>
  typeof value === "object" && value !== null && !Array.isArray(value);
const text = (value: unknown, what: string): string =>
  typeof value === "string" && value.trim() ? value : fail(`${what} is not a text`);
const texts = (value: unknown, what: string): string[] =>
  Array.isArray(value) && value.every((item) => typeof item === "string" && item.trim())
    ? (value as string[])
    : fail(`${what} is not a list of texts`);
const count = (value: unknown, what: string): number =>
  Number.isInteger(value) && (value as number) >= 0
    ? (value as number)
    : fail(`${what} is not a count`);

/**
 * Reads a set's manifest and checks it whole: a broken one throws (the tests run it on every
 * manifest in the repository), so the app never meets half a set.
 */
export function parseStickerSet(json: unknown): StickerSet {
  if (!isRecord(json)) fail("not an object");
  const set = text(json.set, "set");
  if (!/^[a-z0-9]{1,8}$/.test(set)) fail(`set "${set}" is not a set name`);
  const version = count(json.version, "version");
  if (version !== 1) fail(`version ${version} is unknown`);

  if (!Array.isArray(json.themes) || !json.themes.length) fail("no themes");
  const themes = json.themes.map((theme, index): StickerTheme => {
    if (!isRecord(theme)) fail(`theme ${index} is not an object`);
    return {
      id: text(theme.id, `theme ${index} id`),
      name: { tr: text(theme.tr, `theme ${index} tr`), en: text(theme.en, `theme ${index} en`) },
    };
  });
  const themeIds = new Set(themes.map((theme) => theme.id));
  if (themeIds.size !== themes.length) fail("two themes share an id");

  if (!isRecord(json.formats) || !isRecord(json.formats.overlay)) fail("no overlay format");
  const columns = count(json.formats.overlay.columns, "atlas columns");
  const rows = count(json.formats.overlay.rows, "atlas rows");
  if (!isRecord(json.motions)) fail("no motions");
  for (const motion of STICKER_MOTIONS) text(json.motions[motion], `motion ${motion}`);

  if (!Array.isArray(json.stickers) || !json.stickers.length) fail("no stickers");
  const cells = new Set<string>();
  const stickers = json.stickers.map((raw, index): Sticker => {
    if (!isRecord(raw)) fail(`sticker ${index} is not an object`);
    const id = text(raw.id, `sticker ${index} id`);
    if (!/^[a-z0-9-]{1,24}$/.test(id)) fail(`id "${id}" is not a sticker id`);
    const ref = text(raw.ref, `${id} ref`);
    if (ref !== `${set}/${id}` || !STICKER_REF.test(ref)) fail(`${id}: ref "${ref}"`);
    const theme = text(raw.theme, `${id} theme`);
    if (!themeIds.has(theme)) fail(`${id}: unknown theme "${theme}"`);
    if (!isRecord(raw.name) || !isRecord(raw.keywords)) fail(`${id}: no name or keywords`);
    const motion = text(raw.motion, `${id} motion`) as StickerMotion;
    if (!STICKER_MOTIONS.includes(motion)) fail(`${id}: unknown motion "${motion}"`);
    if (raw.caption !== null) text(raw.caption, `${id} caption`);
    if (!isRecord(raw.files) || !isRecord(raw.files.launcher)) fail(`${id}: no launcher image`);
    if (raw.files.launcher.path !== `launcher/${id}.webp`) fail(`${id}: launcher image path`);
    if (!isRecord(raw.atlas)) fail(`${id}: no atlas cell`);
    const column = count(raw.atlas.column, `${id} atlas column`);
    const row = count(raw.atlas.row, `${id} atlas row`);
    if (column >= columns || row >= rows) fail(`${id}: atlas cell outside the atlas`);
    if (cells.has(`${column},${row}`)) fail(`${id}: atlas cell taken`);
    cells.add(`${column},${row}`);
    return {
      ref,
      set,
      id,
      theme,
      order: count(raw.order, `${id} order`),
      name: { tr: text(raw.name.tr, `${id} name tr`), en: text(raw.name.en, `${id} name en`) },
      keywords: {
        tr: texts(raw.keywords.tr, `${id} keywords tr`),
        en: texts(raw.keywords.en, `${id} keywords en`),
      },
      caption: (raw.caption as string | null) ?? null,
      motion,
      atlas: { column, row },
    };
  });
  if (new Set(stickers.map((sticker) => sticker.id)).size !== stickers.length) {
    fail("two stickers share an id");
  }
  const orders = stickers.map((sticker) => sticker.order).sort((a, b) => a - b);
  if (orders.some((order, index) => order !== index + 1)) fail("the order is not 1, 2, 3…");
  stickers.sort((a, b) => a.order - b.order);
  return { set, version, themes, stickers };
}

/** Every set that comes with this build. */
export const STICKER_SETS: StickerSet[] = [parseStickerSet(pl1Manifest)];

const byRef = new Map(
  STICKER_SETS.flatMap((stickerSet) => stickerSet.stickers.map((s) => [s.ref, s] as const)),
);

/** The sticker a message names, or undefined for one this build does not have (a newer set). */
export function findSticker(ref: string): Sticker | undefined {
  return byRef.get(ref);
}

/** Whether the reference names a sticker of this build (the service's allow list). */
export function isKnownSticker(ref: string): boolean {
  return STICKER_REF.test(ref) && byRef.has(ref);
}
