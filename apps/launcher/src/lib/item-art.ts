// The game's own item pictures for the 7/24 Bulut inventory and AFK+'s pickers (ITEMART): the
// launcher draws the atlas from the player's installed Minecraft (nothing of the game ships with
// us, and the CSP keeps the site's CDN atlas out). The browser preview has none, so ItemSprite
// shows its stand-ins there.
import type { TitleGlyph } from "@pl/ui/inventory";
import {
  type ItemArtDoc,
  type ItemArtOverlay,
  type VanillaAtlas,
  vanillaAtlas,
} from "@pl/ui/items";
import { convertFileSrc } from "@tauri-apps/api/core";
import { commands } from "@/bindings";
import { isTauri } from "./platform";

const loading = new Map<string, Promise<VanillaAtlas | undefined>>();

/**
 * The vanilla item atlas of a Minecraft version (else of the newest installed one); undefined
 * while no version is installed. Asked once per version; a miss is asked again next time.
 */
export function vanillaItemArt(minecraft?: string): Promise<VanillaAtlas | undefined> {
  const key = minecraft ?? "";
  const hit = loading.get(key);
  if (hit) return hit;
  const load = (async () => {
    if (!isTauri()) return undefined;
    const result = await commands.itemsAtlas(minecraft ?? null).catch(() => undefined);
    const files = result?.status === "ok" ? result.data : null;
    if (!files) return undefined;
    const large = files.large ? convertFileSrc(files.large) : undefined;
    return vanillaAtlas(files.index, convertFileSrc(files.image), large);
  })();
  loading.set(key, load);
  void load.then((atlas) => {
    if (!atlas) loading.delete(key);
  });
  return load;
}

/** The document's glyphs as the content format writes them (`@pl/ui` reads only the item parts). */
interface GlyphEntry {
  char: string;
  advance: number;
  image?: { file: { path: string }; width: number; height: number; ascent: number };
}

/** The Projects server's own item looks and menu pictures (IA-ART's signed document). */
export interface ItemOverlay {
  overlay: ItemArtOverlay;
  /** The servers it belongs to: a menu title's pictures show only there. */
  hosts: string[];
  /** A menu title's private-use character: the picture it draws, or how far it moves the title. */
  glyph: (codePoint: number) => TitleGlyph | undefined;
}

let overlayLoad: Promise<ItemOverlay | undefined> | null = null;

/**
 * The content index's item art document with its pictures on this computer (fetched once,
 * verified); undefined without one (and in the browser preview). Asked once per run; a miss is
 * asked again next time.
 */
export function itemArtOverlay(): Promise<ItemOverlay | undefined> {
  if (overlayLoad) return overlayLoad;
  const load = (async (): Promise<ItemOverlay | undefined> => {
    if (!isTauri()) return undefined;
    const result = await commands.itemsOverlay().catch(() => undefined);
    const data = result?.status === "ok" ? result.data : null;
    if (!data) return undefined;
    let doc: ItemArtDoc & { glyphs?: GlyphEntry[] };
    try {
      doc = JSON.parse(data.doc) as ItemArtDoc & { glyphs?: GlyphEntry[] };
    } catch {
      return undefined;
    }
    const local = new Map(data.files.map((file) => [file.path, convertFileSrc(file.local)]));
    const glyphs = new Map<number, TitleGlyph>();
    for (const entry of doc.glyphs ?? []) {
      const codePoint = entry.char.codePointAt(0);
      if (codePoint === undefined) continue;
      const src = entry.image ? local.get(entry.image.file.path) : undefined;
      glyphs.set(
        codePoint,
        entry.image && src
          ? {
              advance: entry.advance,
              image: {
                src,
                width: entry.image.width,
                height: entry.image.height,
                ascent: entry.image.ascent,
              },
            }
          : { advance: entry.advance },
      );
    }
    return {
      overlay: { doc, atlasUrl: (file) => local.get(file.path) ?? "" },
      hosts: doc.hosts ?? [],
      glyph: (codePoint) => glyphs.get(codePoint),
    };
  })();
  overlayLoad = load;
  void load.then((overlay) => {
    if (!overlay) overlayLoad = null;
  });
  return load;
}
