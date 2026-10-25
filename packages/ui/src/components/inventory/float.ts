// Where the game's tooltip goes: the top-left of its words, in CSS pixels (the frame and background
// reach 4 GUI pixels past them on every side). Plain data, so the view, its tests and the phone
// place it the same way.

export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/** What the tooltip belongs to: the mouse pointer, a focused slot, or a slot a finger tapped. */
export type TipAnchor =
  | { pointer: { x: number; y: number } }
  | { slot: Box; by: "keyboard" | "touch" };

/**
 * - The pointer: the game's offsets, the words 12 GUI pixels right of and 12 above it, to its left
 *   when they would leave the screen.
 * - A focused slot: the words 7 past its edge, clear of the focus brackets (2 out), to its right
 *   when they fit, else its left, level with the brackets' top.
 * - A tapped slot (and a focused one with no room either side): above the slot, centred on it and
 *   kept on screen, or below it near the top; never on the slot itself.
 */
export function tooltipPlace(
  anchor: TipAnchor,
  words: { width: number; height: number },
  viewport: { width: number; height: number },
  unit: number,
): { x: number; y: number } {
  const { width: w, height: h } = words;
  const { width: vw, height: vh } = viewport;
  const edge = 4 * unit;
  if ("pointer" in anchor) {
    const { x: px, y: py } = anchor.pointer;
    let x = px + 12 * unit;
    let y = py - 12 * unit;
    if (x + w + edge > vw) x = Math.max(px - 24 * unit - w, edge);
    if (y + h + edge > vh) y = vh - h - edge;
    return { x, y: Math.max(edge, y) };
  }
  const { slot, by } = anchor;
  if (by === "keyboard") {
    const right = slot.right + 7 * unit;
    const left = slot.left - 7 * unit - w;
    const fitsRight = right + w + edge <= vw;
    if (fitsRight || left >= edge) {
      return {
        x: fitsRight ? right : left,
        y: Math.max(edge, Math.min(slot.top + 2 * unit, vh - h - edge)),
      };
    }
  }
  const centre = (slot.left + slot.right) / 2;
  const x = Math.min(Math.max(centre - w / 2, edge), vw - w - edge);
  const above = slot.top - edge - h - unit;
  return { x, y: above < edge ? slot.bottom + edge + unit : above };
}
