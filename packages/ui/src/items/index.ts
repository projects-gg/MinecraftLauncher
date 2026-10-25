// Item pictures (ITEMART): the game's own item textures from a vanilla atlas (built from a client
// jar by crates/pl-items), Projects' ItemsAdder art over them on its servers (IA-ART), and a
// quiet stand-in when neither has the item. docs/design.md §6 "Item sprites".

export {
  loadVanillaAtlas,
  pickVanillaVersion,
  VANILLA_ATLAS_BASE,
  VANILLA_MANIFEST_FORMAT,
  type VanillaAtlasManifest,
} from "./cdn";
export {
  ItemArtProvider,
  ItemSprite,
  type ItemSpriteProps,
  type ItemSpriteSize,
  pickDrawing,
  useItemArt,
} from "./item-sprite";
export { POTION_COLORS } from "./potions";
export {
  ALWAYS_GLINT,
  fullItemId,
  hasGlint,
  hostMatches,
  type ItemCategory,
  itemCategory,
  layerColor,
  potionColor,
  type ResolvedIcon,
  resolveIcon,
} from "./resolve";
export type {
  ItemArt,
  ItemArtAtlas,
  ItemArtDoc,
  ItemArtEntry,
  ItemArtFile,
  ItemArtOverlay,
  ItemRef,
  VanillaAtlas,
  VanillaAtlasIndex,
  VanillaIcon,
  VanillaLargeAtlas,
  VanillaLayer,
  VanillaLayerTint,
} from "./types";
export { VANILLA_INDEX_FORMAT, vanillaAtlas } from "./vanilla";
