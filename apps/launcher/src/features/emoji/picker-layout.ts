// The picker's size and tabs, apart from the panel so the button loads without it.

/** chat2.md §3.1: 336 × 384 px. */
export const PANEL_WIDTH = 336;
export const PANEL_HEIGHT = 384;
/** The least a short window gets: the rows around two rows of emoji. */
export const PANEL_MIN_HEIGHT = 240;

/**
 * How tall the picker opens beside its trigger (QOL-209): whole where either side has room, else
 * as tall as the roomier side (Radix then opens it there), never under `PANEL_MIN_HEIGHT`. A
 * message's picker in the middle of a 1280 × 633 window ran 80 px past the window either way.
 * `gap` is the panel's offset from the trigger plus its distance from the window's edge.
 */
export function panelHeightFor(
  trigger: { top: number; bottom: number },
  viewportHeight: number,
  gap: number,
): number {
  const room = Math.max(trigger.top - gap, viewportHeight - trigger.bottom - gap);
  if (room >= PANEL_HEIGHT) return PANEL_HEIGHT;
  return Math.max(PANEL_MIN_HEIGHT, Math.floor(room));
}

export type PickerTab = "emoji" | "stickers";

/** Keys for skeleton cells (a fixed list: they never move). */
export const SKELETON_KEYS = Array.from({ length: 96 }, (_, index) => `cell-${index}`);
