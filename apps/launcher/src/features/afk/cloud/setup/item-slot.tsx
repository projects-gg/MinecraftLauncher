// A question's picture (SETUP-UX-L, the site's item-slot.tsx): one of the game's own items in a dark
// take on the game's sunken inventory slot, its darker top-left edge and lighter bottom-right edge
// in our obsidian tones. The item comes from the vanilla atlas the launcher draws from the player's
// installed Minecraft (`PlusItemArt` around the panel): nothing of the game ships with us and no
// picture comes from the network. Drawn at whole pixels: the 16 px textures at 2x, 1x on the
// setup's rail. Decorative: the question beside it says what it is. Without the atlas (no game
// installed yet, the browser preview) there is no slot at all: an empty one read as a checkbox
// beside its question, and a category's stand-in could read as another item.
import { cn } from "@pl/ui";
import { ItemSprite, useItemArt } from "@pl/ui/items";

const SIZES = {
  sm: { box: 24, item: 16, radius: "rounded-[4px]" },
  md: { box: 40, item: 32, radius: "rounded-[6px]" },
  lg: { box: 44, item: 32, radius: "rounded-[6px]" },
} as const;

/** Whether the game's item pictures are here (the slots show only then). */
export function useHasItemArt(): boolean {
  return Boolean(useItemArt()?.vanilla);
}

export function ItemSlot({
  item,
  size = "md",
  className,
}: {
  /** `minecraft:…`, as ItemSprite takes it. */
  item: string;
  size?: keyof typeof SIZES;
  className?: string;
}) {
  const shown = useHasItemArt();
  if (!shown) return null;
  const { box, item: px, radius } = SIZES[size];
  return (
    <span
      aria-hidden
      className={cn(
        "relative grid shrink-0 place-items-center border border-white/6 bg-obsidian-950/60",
        "shadow-[inset_1px_1px_0_rgb(0_0_0/0.55),inset_-1px_-1px_0_rgb(255_255_255/0.05)]",
        radius,
        className,
      )}
      style={{ width: box, height: box }}
    >
      <ItemSprite id={item} size={px} />
    </span>
  );
}
