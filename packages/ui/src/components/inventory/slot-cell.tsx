// One slot: the item with its count and wear bar, the game's highlight, and the focus brackets.
import { memo } from "react";
import type { SlotSpot } from "./layout";
import { FocusBrackets, LockMark } from "./panel-art";
import { PixelLine } from "./pixel-text";
import type { Stack } from "./rules";
import { InventorySprite } from "./sprite";
import { durabilityColor, durabilityWidth } from "./text";

/** The item as the game draws it in a 16 x 16 square: sprite, count, wear bar. */
export const ItemArt = memo(function ItemArt({ item }: { item: Stack }) {
  const wear = item.durability;
  const worn = wear && wear.max > 0 && wear.left < wear.max;
  return (
    <>
      <span className="pl-inv-item">
        <InventorySprite
          id={item.id}
          potion={item.potion}
          color={item.color}
          enchanted={item.enchanted}
          cmd={item.cmd}
          model={item.model}
          skin={item.skin}
        />
      </span>
      {worn ? (
        <span className="pl-inv-wear" aria-hidden>
          <span
            style={{
              width: `calc(${durabilityWidth(wear.left, wear.max)} * var(--pl-inv-px))`,
              background: durabilityColor(wear.left, wear.max),
            }}
          />
        </span>
      ) : null}
      {item.count > 1 ? (
        // The game's count: white with its shadow, its right edge one past the item's.
        <PixelLine
          className="pl-inv-count"
          text={String(item.count)}
          color="#ffffff"
          shadow="#3f3f3f"
        />
      ) : null}
    </>
  );
});

export interface SlotCellProps {
  spot: SlotSpot;
  /** The cell's left inside its row, in GUI pixels. */
  left: number;
  item: Stack | null;
  label: string;
  describedBy?: string;
  tabbable: boolean;
  pending: boolean;
  held: boolean;
  /** The game's highlight without a hover: its menu is open, its tooltip tapped. */
  lit: boolean;
  /** When a move tried here was held (looking only, a trade's lock): a lock shows for a moment. */
  nopeAt?: number;
  live: boolean;
  /** A slot a pick can choose: it says whether it is the chosen one (`aria-selected`). */
  selectable?: boolean;
  /** The chosen slot of a pick: the game's highlight stays and the brand's brackets mark it. */
  picked?: boolean;
}

export const SlotCell = memo(function SlotCell({
  spot,
  left,
  item,
  label,
  describedBy,
  tabbable,
  pending,
  held,
  lit,
  nopeAt,
  live,
  selectable = false,
  picked = false,
}: SlotCellProps) {
  return (
    // biome-ignore lint/a11y/useSemanticElements: a cell of the ARIA grid (positioned pixel art)
    <div
      className="pl-inv-cell"
      role="gridcell"
      data-slot={spot.key}
      data-pending={pending ? "" : undefined}
      data-held={held ? "" : undefined}
      data-lit={lit ? "" : undefined}
      data-picked={picked ? "" : undefined}
      aria-selected={selectable || picked ? picked : undefined}
      tabIndex={live ? (tabbable ? 0 : -1) : undefined}
      aria-label={live ? label : undefined}
      aria-describedby={live ? describedBy : undefined}
      aria-busy={live && pending ? true : undefined}
      style={{ left: `calc(${left} * var(--pl-inv-px))` }}
    >
      {/* The game's highlight is two layers now: most of it behind the item, a little over it. */}
      {live ? <span className="pl-inv-hl-back" aria-hidden /> : null}
      {item ? <ItemArt item={item} /> : null}
      {live ? (
        <>
          <span className="pl-inv-hl" aria-hidden />
          <FocusBrackets />
          {/* A new try shows the lock again (a new element, one run of its animation). */}
          {nopeAt !== undefined ? <LockMark key={nopeAt} /> : null}
        </>
      ) : null}
    </div>
  );
});
