import { cn, prewarmImages } from "@pl/ui";
import compass from "@/assets/art/icons/icon-compass.webp";
import dragonEgg from "@/assets/art/icons/icon-dragon-egg.webp";
import grass from "@/assets/art/icons/icon-grass.webp";
import map from "@/assets/art/icons/icon-map.webp";
import pickaxe from "@/assets/art/icons/icon-pickaxe.webp";
import potion from "@/assets/art/icons/icon-potion.webp";
import swords from "@/assets/art/icons/icon-swords.webp";
import tower from "@/assets/art/icons/icon-tower.webp";
import type { InstanceIcon as IconKey } from "@/lib/api";

export const instanceIcons: Record<IconKey, string> = {
  grass,
  swords,
  pickaxe,
  potion,
  "dragon-egg": dragonEgg,
  tower,
  map,
  compass,
};

/**
 * Decodes every instance icon in idle moments after start-up. The palette opens without
 * animation and the picker lists all instances at once: icons decoded on that first opening
 * drew a few frames late (3 of 4 still empty after 60 ms, Q2).
 */
export function prewarmInstanceIcons() {
  prewarmImages(Object.values(instanceIcons));
}

/** Instance icon on a chamfered, softly lit tile. */
export function InstanceIcon({
  icon,
  size = 44,
  className,
}: {
  icon: IconKey;
  size?: number;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "chamfer relative inline-flex shrink-0 items-center justify-center bg-[radial-gradient(circle_at_50%_35%,rgb(232_154_44/0.28),rgb(27_24_32/0.95)_70%)] [--cut:7px]",
        className,
      )}
      style={{ width: size, height: size }}
    >
      {/* Warmed beforehand (prewarmInstanceIcons): drawn in the same frame as the tile. */}
      <img
        src={instanceIcons[icon]}
        alt=""
        decoding="sync"
        draggable={false}
        className="size-[78%] object-contain drop-shadow-[0_2px_4px_rgb(10_9_12/0.6)]"
      />
    </span>
  );
}
