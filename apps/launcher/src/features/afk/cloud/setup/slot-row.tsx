// Nine of the game's inventory slots in a row, as at GUI scale 2 (36 px apart, items 32 px): how
// many of an item a spending limit covers (SETUP-UX-L, the site's slot-row.tsx). Stacked as the game
// stacks: an item that stacks fills a slot up to its stack size with its count in the corner in the
// game's own digits; one that does not (an elytra, a totem) takes a slot each. Past nine slots the
// row stays full; the sentence under it says the number. Decorative: that sentence carries the
// meaning. The items come from the player's own game (empty slots until its atlas is there).
import { PixelLine } from "@pl/ui/inventory";
import { ItemSprite, useItemArt } from "@pl/ui/items";
import type { CSSProperties } from "react";

const SLOTS = 9;

export function SlotRow({ item, count, stack }: { item: string; count: number; stack: number }) {
  const art = useItemArt();
  const per = Math.max(1, stack);
  const filled = Math.min(SLOTS, Math.ceil(Math.max(0, count) / per));
  let left = Math.max(0, count);
  return (
    <div
      aria-hidden
      className="grid w-max max-w-full grid-cols-[repeat(9,36px)] rounded-[4px] bg-obsidian-950/60 ring-1 ring-white/6"
      style={{ "--pl-inv-px": "2px" } as CSSProperties}
    >
      {Array.from({ length: SLOTS }, (_, index) => {
        const here = index < filled ? Math.min(per, left) : 0;
        left -= here;
        return (
          <span
            // biome-ignore lint/suspicious/noArrayIndexKey: the row's nine fixed slots
            key={index}
            className="relative grid size-[36px] place-items-center shadow-[inset_1px_1px_0_rgb(0_0_0/0.55),inset_-1px_-1px_0_rgb(255_255_255/0.05)]"
          >
            {here > 0 && art?.vanilla ? <ItemSprite id={item} size={32} /> : null}
            {here > 1 ? (
              <span className="absolute right-px bottom-px">
                <PixelLine text={String(here)} color="#fbf6ec" shadow="#3e3d3b" />
              </span>
            ) : null}
          </span>
        );
      })}
    </div>
  );
}
