// The shapes ItemSprite draws from (ITEMART). The vanilla atlas comes from the game's own client
// jar (crates/pl-items: built at run time in the launcher, ahead of time for the site); the
// overlay is Projects' ItemsAdder art (IA-ART's signed `itemArt` document), shown only on its hosts.

/** An item as the inventory bridge describes it. */
export interface ItemRef {
  /** `minecraft:diamond_sword` (a bare path such as `diamond_sword` means `minecraft:`). */
  id: string;
  /** `custom_model_data` as an integer (ItemsAdder picks Projects' items by material + this). */
  cmd?: number;
  /** The `minecraft:item_model` component, when the server set one. */
  model?: string;
  /** A potion's id (`minecraft:strong_healing`): the colour of potion-tinted layers. */
  potion?: string;
  /** A dyed or custom colour, 0xRRGGBB: leather pieces, wolf armour, custom potions. */
  color?: number;
  /**
   * Shows the enchantment glint. Unset means the item's own default: enchanted books, enchanted
   * golden apples, bottles o' enchanting, nether stars, end crystals and written books glint.
   */
  enchanted?: boolean;
}

/** A tinted layer: the client multiplies it by a colour the server can send. */
export interface VanillaLayerTint {
  source: "dye" | "potion" | "firework" | "map_color" | "team" | "custom_model_data" | string;
  /** `#rrggbb` when the server sends none; unset: the layer shows only with a colour (wolf armour's dye). */
  default?: string;
}

export interface VanillaLayer {
  /** `[x, y, width, height]` in the atlas. */
  rect: [number, number, number, number];
  tint?: VanillaLayerTint;
}

export interface VanillaIcon {
  kind: "flat" | "block" | "special";
  layers: VanillaLayer[];
  /** Its outline melts into our dark slots (obsidian, netherite): ItemSprite edges it there. */
  dark?: boolean;
  /** A 3D icon drawn again in the large atlas (`index.large`): `[x, y, width, height]`. */
  large?: [number, number, number, number];
}

/**
 * The second image of a vanilla atlas: its 3D icons drawn at `icon` (48) pixels, as the game draws
 * them at GUI scale 3, where the 32 px drawing scaled by 1.5 would show uneven pixels.
 */
export interface VanillaLargeAtlas {
  image: string;
  size: [number, number];
  icon: number;
}

/** `index.json` of a vanilla atlas (`pl_items::AtlasIndex`, format 1). */
export interface VanillaAtlasIndex {
  format: number;
  minecraft: string;
  image: string;
  size: [number, number];
  /** The still glint frame: an opaque square added (screen) over an enchanted icon, masked by it. */
  glint?: [number, number, number, number];
  large?: VanillaLargeAtlas;
  icons: Record<string, VanillaIcon>;
}

/** A loaded vanilla atlas: its index and where its images are (URLs or data URLs). */
export interface VanillaAtlas {
  index: VanillaAtlasIndex;
  imageUrl: string;
  /** The large atlas's image, loaded only by sprites drawn at its size. */
  largeUrl?: string;
  /**
   * The large image is known to be loaded already (a page drawn on the server for a screenshot):
   * sprites draw from it at once instead of showing their 32 px drawing until it has loaded.
   */
  largeReady?: boolean;
}

/** A file the signed content index points at. */
export interface ItemArtFile {
  path: string;
  sha256: string;
  size: number;
}

/**
 * One grid atlas of the overlay: square cells of `size` pixels (16 and 32 today; any multiple of 8
 * from 8 to 128 is read), `columns` per row, `cells` in all.
 */
export interface ItemArtAtlas {
  size: number;
  columns: number;
  cells?: number;
  file: ItemArtFile;
}

/**
 * One custom item: matched by `model`, or by `item` (full id) and `cmd`; `icon` is its cell. An
 * entry has `item` with `cmd`, or `model`, or all three.
 */
export interface ItemArtEntry {
  item?: string;
  cmd?: number;
  model?: string;
  icon: number;
  name?: string;
  pack?: string;
  id?: string;
}

/**
 * IA-ART's `itemArt` document (docs/content-format.md "Item art"); the sprite reads `hosts`,
 * `atlases` and `items`, the rest (`glyphs`, the menus' pictures) is for the inventory frame.
 */
export interface ItemArtDoc {
  schema?: number;
  /**
   * Servers the art belongs to (the host or a subdomain, any port; an IPv4 address only itself).
   * Without hosts it shows nowhere, as in `pl_content::item_art_host_matches`.
   */
  hosts?: string[];
  atlases: ItemArtAtlas[];
  items: ItemArtEntry[];
}

/** A loaded overlay: the document and how to reach its atlas files. */
export interface ItemArtOverlay {
  doc: ItemArtDoc;
  atlasUrl: (file: ItemArtFile) => string;
}

/** Everything ItemSprite may draw from. */
export interface ItemArt {
  vanilla?: VanillaAtlas;
  overlay?: ItemArtOverlay;
  /** The server the account is on (`play.projects.gg:25565`): the overlay shows only on its hosts. */
  host?: string;
}
