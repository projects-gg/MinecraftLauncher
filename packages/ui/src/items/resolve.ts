// Which picture an item gets (ITEMART): Projects' own art on its servers, else the game's texture,
// else a quiet stand-in. Order (agreed with IA-ART): an overlay entry with the same `model`, then
// one with the same item and `cmd`, then the vanilla icon, then the category drawing.
import { POTION_COLORS } from "./potions";
import type {
  ItemArt,
  ItemArtAtlas,
  ItemArtDoc,
  ItemArtEntry,
  ItemRef,
  VanillaIcon,
} from "./types";

/** `diamond_sword`, `minecraft:diamond_sword`, `Minecraft:Diamond_Sword` → `minecraft:diamond_sword`. */
export function fullItemId(id: string): string {
  const clean = id.trim().toLowerCase();
  return clean.includes(":") ? clean : `minecraft:${clean}`;
}

/**
 * The host part of a server address, lowercase, without the port or a trailing dot
 * (`PLAY.projects.gg.:25565` → `play.projects.gg`).
 */
function hostName(address: string): string {
  let host = address.trim();
  const colon = host.lastIndexOf(":");
  if (colon >= 0 && /^\d+$/.test(host.slice(colon + 1)) && !host.slice(0, colon).includes(":")) {
    host = host.slice(0, colon);
  }
  return host.replace(/\.+$/, "").toLowerCase();
}

const IPV4 = /^\d{1,3}(\.\d{1,3}){3}$/;

/**
 * Whether a server address belongs to one of the hosts: equal, or a subdomain (an IPv4 entry only
 * itself); port, case and a trailing dot ignored. No hosts: none (`pl_content` does the same).
 */
export function hostMatches(
  address: string | undefined,
  hosts: readonly string[] | undefined,
): boolean {
  const host = address ? hostName(address) : "";
  if (!host || !hosts) return false;
  return hosts.some((entry) => {
    const want = entry.trim().replace(/\.+$/, "").toLowerCase();
    if (!want) return false;
    if (IPV4.test(want)) return host === want;
    return host === want || host.endsWith(`.${want}`);
  });
}

interface OverlayLookup {
  byModel: Map<string, ItemArtEntry>;
  byItemCmd: Map<string, ItemArtEntry>;
}

const lookups = new WeakMap<ItemArtDoc, OverlayLookup>();

function lookupFor(doc: ItemArtDoc): OverlayLookup {
  const hit = lookups.get(doc);
  if (hit) return hit;
  const byModel = new Map<string, ItemArtEntry>();
  const byItemCmd = new Map<string, ItemArtEntry>();
  // The first entry claiming a key wins, as in `pl_content` (a valid document has no repeats).
  for (const entry of doc.items) {
    const model = entry.model ? fullItemId(entry.model) : undefined;
    if (model && !byModel.has(model)) byModel.set(model, entry);
    if (entry.item && entry.cmd !== undefined) {
      const key = `${fullItemId(entry.item)}#${entry.cmd}`;
      if (!byItemCmd.has(key)) byItemCmd.set(key, entry);
    }
  }
  const built = { byModel, byItemCmd };
  lookups.set(doc, built);
  return built;
}

export type ItemCategory =
  | "block"
  | "tool"
  | "weapon"
  | "armor"
  | "food"
  | "potion"
  | "book"
  | "other";

const CATEGORY_RULES: readonly (readonly [ItemCategory, RegExp])[] = [
  ["potion", /potion$/],
  ["book", /book$/],
  ["armor", /_(helmet|chestplate|leggings|boots|horse_armor)$|^elytra$|^wolf_armor$/],
  ["weapon", /_(sword|spear)$|arrow$|^(bow|crossbow|trident|mace)$/],
  [
    "tool",
    /_(pickaxe|axe|shovel|hoe|bucket|on_a_stick)$|^(shears|brush|fishing_rod|flint_and_steel|spyglass|compass|clock|lead)$/,
  ],
  [
    "food",
    /apple|bread|beef|porkchop|chicken|mutton|rabbit|cod|salmon|fish|potato|carrot|beetroot|melon_slice|berries|cookie|cake$|_pie$|_stew$|_soup$|kelp|^chorus_fruit$|^honey_bottle$|^rotten_flesh$|^spider_eye$/,
  ],
  [
    "block",
    /_(block|planks|log|wood|stem|hyphae|ore|bricks|tiles|slab|stairs|wall|fence|fence_gate|door|trapdoor|wool|carpet|concrete|concrete_powder|terracotta|glass|glass_pane|leaves|pillar|lantern|bars|button|pressure_plate|table|bed|shulker_box|copper)$|stone$|sand$|dirt$|deepslate$|^(gravel|obsidian|netherrack|pumpkin|melon|sponge|glass)$/,
  ],
];

/** A rough category from the id's path, for the stand-in drawing. */
export function itemCategory(id: string): ItemCategory {
  const path = fullItemId(id).split(":")[1] ?? "";
  for (const [category, pattern] of CATEGORY_RULES) if (pattern.test(path)) return category;
  return "other";
}

/** A potion's colour (0xRRGGBB) from its id (`long_`/`strong_` ignored), or undefined. */
export function potionColor(potion: string | undefined): number | undefined {
  if (!potion) return undefined;
  const path = (fullItemId(potion).split(":")[1] ?? "").replace(/^(long|strong)_/, "");
  return POTION_COLORS[path];
}

/** Where an item's picture comes from. */
export type ResolvedIcon =
  | {
      source: "overlay";
      entry: ItemArtEntry;
      atlas: ItemArtAtlas;
      url: string;
      /** `[x, y, size, size]` of the cell. */
      rect: [number, number, number, number];
    }
  | { source: "vanilla"; icon: VanillaIcon }
  | { source: "fallback"; category: ItemCategory };

/** Cell sizes a reader accepts (docs/content-format.md "Item art"). */
function usableAtlas(atlas: ItemArtAtlas): boolean {
  return atlas.size >= 8 && atlas.size <= 128 && atlas.size % 8 === 0 && atlas.columns > 0;
}

/**
 * The overlay grid atlas to draw a cell from: the largest one not bigger than the drawn size
 * (pixel art scaled up stays crisp, scaled down it loses lines), else the smallest.
 */
function pickAtlas(atlases: readonly ItemArtAtlas[], size: number): ItemArtAtlas | undefined {
  const sorted = atlases.filter(usableAtlas).sort((a, b) => a.size - b.size);
  return sorted.filter((atlas) => atlas.size <= size).pop() ?? sorted[0];
}

/** Resolves an item against the loaded art (`size` picks the overlay atlas). */
export function resolveIcon(item: ItemRef, art: ItemArt | undefined, size = 32): ResolvedIcon {
  const id = fullItemId(item.id);
  const overlay = art?.overlay;
  if (overlay && hostMatches(art?.host, overlay.doc.hosts)) {
    const lookup = lookupFor(overlay.doc);
    const entry =
      (item.model ? lookup.byModel.get(fullItemId(item.model)) : undefined) ??
      (item.cmd !== undefined ? lookup.byItemCmd.get(`${id}#${item.cmd}`) : undefined);
    const atlas = entry ? pickAtlas(overlay.doc.atlases, size) : undefined;
    const inAtlas =
      entry !== undefined &&
      atlas !== undefined &&
      entry.icon >= 0 &&
      (atlas.cells === undefined || entry.icon < atlas.cells);
    if (entry && atlas && inAtlas) {
      const x = (entry.icon % atlas.columns) * atlas.size;
      const y = Math.floor(entry.icon / atlas.columns) * atlas.size;
      return {
        source: "overlay",
        entry,
        atlas,
        url: overlay.atlasUrl(atlas.file),
        rect: [x, y, atlas.size, atlas.size],
      };
    }
  }
  const icon = art?.vanilla?.index.icons[id];
  if (icon) return { source: "vanilla", icon };
  return { source: "fallback", category: itemCategory(id) };
}

/**
 * A tinted layer's colour for this item: what the server sent, else the layer's default;
 * undefined when there is neither (the layer is then not drawn: undyed wolf armour).
 */
export function layerColor(
  item: ItemRef,
  source: string,
  fallback: string | undefined,
): string | undefined {
  const custom = typeof item.color === "number" ? item.color & 0xffffff : undefined;
  const picked = source === "potion" ? (custom ?? potionColor(item.potion)) : custom;
  return picked === undefined ? fallback : `#${picked.toString(16).padStart(6, "0")}`;
}

/** Items the game draws with the glint even without enchantments (`enchantment_glint_override`). */
export const ALWAYS_GLINT: ReadonlySet<string> = new Set([
  "minecraft:enchanted_book",
  "minecraft:enchanted_golden_apple",
  "minecraft:experience_bottle",
  "minecraft:nether_star",
  "minecraft:end_crystal",
  "minecraft:written_book",
  "minecraft:debug_stick",
]);

/** Whether the item shows the glint: as the server says, else as the game draws that item. */
export function hasGlint(item: ItemRef): boolean {
  return item.enchanted ?? ALWAYS_GLINT.has(fullItemId(item.id));
}
