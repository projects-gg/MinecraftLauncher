// The game's item tooltip: name in its rarity colour, contents, enchantments, lore and wear, in
// the game's own colours and drop shadows, drawn as whole pixels. Shown beside the pointer, or
// beside the slot for keys and touch; inside the slot's action menu it heads the menu instead (one
// surface, never two boxes on top of each other on a phone). Lines are 10 GUI pixels apart, the
// name 2 more, as in the game; a line wider than the screen allows wraps at its spaces.
import { type CSSProperties, forwardRef } from "react";
import { wrapText } from "./pixel-font";
import { PixelLine } from "./pixel-text";
import { shadowOf, type TooltipLine } from "./text";

/** How many GUI pixels of words fit: `room` CSS pixels at `px` CSS pixels each, less the frame. */
function wordsWidth(px: string, room: (viewport: number, unit: number) => number, frame: number) {
  const unit = Number.parseFloat(px) || 2;
  const viewport = typeof document === "undefined" ? 1280 : document.documentElement.clientWidth;
  return Math.max(16, Math.floor(room(viewport, unit) / unit) - frame);
}

/** The lines, each with the game's drop shadow (a quarter of its colour). */
export function TooltipLines({ lines, width }: { lines: TooltipLine[]; width: number }) {
  return (
    <div className="pl-inv-tip-lines">
      {lines.map((line, i) => (
        <div
          // biome-ignore lint/suspicious/noArrayIndexKey: lines have no identity beyond their place
          key={i}
          className="pl-inv-tip-line"
          data-text={line.text}
        >
          {wrapText(line.text, width).map((part, j) => (
            <PixelLine
              // biome-ignore lint/suspicious/noArrayIndexKey: a wrapped line's parts are its order
              key={j}
              text={part}
              color={line.color}
              shadow={shadowOf(line.color)}
              italic={line.italic}
            />
          ))}
        </div>
      ))}
    </div>
  );
}

export const ItemTooltip = forwardRef<
  HTMLDivElement,
  { lines: TooltipLine[]; shown: boolean; instant: boolean; px: string }
>(function ItemTooltip({ lines, shown, instant, px }, ref) {
  // At most 240 GUI pixels wide and 8 CSS pixels from the screen's edges; the frame takes 3 a side.
  const width = wordsWidth(px, (viewport, unit) => Math.min(viewport - 16, 240 * unit), 6);
  return (
    <div
      ref={ref}
      className="pl-inv-tip"
      data-shown={shown ? "" : undefined}
      data-instant={instant ? "" : undefined}
      style={{ "--pl-inv-px": px } as CSSProperties}
      aria-hidden
    >
      <TooltipLines lines={lines} width={width} />
    </div>
  );
});

/**
 * The item's tooltip at the top of its action menu, on the menu's own surface. `room`: the CSS
 * pixels the head may take (the menu stays within the game's window).
 */
export function MenuHead({ lines, px, room }: { lines: TooltipLine[]; px: string; room?: number }) {
  // As wide as the game's window at most, 16 CSS pixels from the screen's edges; 4 GUI pixels a side.
  const width = wordsWidth(
    px,
    (viewport, unit) => Math.min(viewport - 32, 176 * unit, room ?? Number.POSITIVE_INFINITY),
    8,
  );
  return (
    <div
      className="pl-inv-menu-head"
      style={
        {
          "--pl-inv-px": px,
          ...(room
            ? { maxWidth: `min(calc(100vw - 32px), calc(176 * var(--pl-inv-px)), ${room}px)` }
            : {}),
        } as CSSProperties
      }
      aria-hidden
    >
      <TooltipLines lines={lines} width={width} />
    </div>
  );
}
