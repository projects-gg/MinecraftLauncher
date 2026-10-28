// The one place the inventory draws an item: ITEMART's `ItemSprite` (`@pl/ui/items`: the game's
// textures from a vanilla atlas, Projects' own art on its servers, both given by
// `ItemArtProvider`; a quiet stand-in when neither has the item). The grey panel turns its light
// edge for dark icons off (`--pl-item-rim: transparent`, inventory.css).
import { createContext, useContext } from "react";
import { ItemSprite, type ItemSpriteSize } from "../../items";

/** CSS pixels per GUI pixel, as the inventory measured them (2 until it has). */
export const InventoryScale = createContext(2);

export interface InventorySpriteProps {
  /** "minecraft:diamond_sword" or "diamond_sword". */
  id: string;
  potion?: string;
  color?: number;
  enchanted?: boolean;
  /** Projects' own items (IA-ART): custom_model_data and item_model. */
  cmd?: number;
  model?: string;
  /** A player head's skin (INV-HEADS): its texture's hash on Mojang's texture server. */
  skin?: string;
  label?: string;
  className?: string;
}

/**
 * 16 GUI pixels in CSS pixels. ItemSprite's sizes are whole multiples of 16, but its layers scale
 * from any size, and a GUI pixel of whole device pixels can be a fraction of a CSS pixel (2.4 at
 * 125 %): this keeps the texels on the panel's own device pixel grid.
 */
export function InventorySprite(props: InventorySpriteProps) {
  const px = useContext(InventoryScale);
  return <ItemSprite {...props} size={(16 * px) as ItemSpriteSize} />;
}
