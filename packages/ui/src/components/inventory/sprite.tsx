// The one place the inventory draws an item. ITEMART's `ItemSprite` (`@pl/ui/items`: the game's
// textures from a vanilla atlas, Projects' own art on its servers, given by `ItemArtProvider`) is
// not on this branch yet. Until it is, a plain stand-in with the same props draws a neutral square
// in a colour taken from the item's id. When ITEMART is in, this file becomes:
//
//   export function InventorySprite(props: InventorySpriteProps) {
//     const px = useContext(InventoryScale);
//     return <ItemSprite {...props} size={(16 * px) as ItemSpriteSize} />;
//   }
//
// (its sprites size their atlas layers in CSS pixels from `size`, so the GUI pixel goes in).
import { createContext } from "react";
import { cn } from "../../lib/cn";

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
  label?: string;
  className?: string;
}

function hue(id: string): number {
  let h = 0;
  for (let i = 0; i < id.length; i++) h = (h * 31 + id.charCodeAt(i)) >>> 0;
  return h % 360;
}

export function InventorySprite({ id, color, label, className }: InventorySpriteProps) {
  const fill =
    color === undefined ? `hsl(${hue(id)} 45% 52%)` : `#${color.toString(16).padStart(6, "0")}`;
  const svg = {
    viewBox: "0 0 16 16",
    shapeRendering: "crispEdges",
    className: cn("shrink-0", className),
    "data-item": id,
  } as const;
  const pixels = (
    <>
      <rect x={3} y={3} width={10} height={10} fill="rgb(0 0 0 / 0.45)" />
      <rect x={4} y={4} width={8} height={8} fill={fill} />
      <rect x={4} y={4} width={8} height={1} fill="rgb(255 255 255 / 0.35)" />
    </>
  );
  return label ? (
    <svg {...svg} role="img" aria-label={label}>
      {pixels}
    </svg>
  ) : (
    <svg {...svg} aria-hidden focusable="false">
      {pixels}
    </svg>
  );
}
