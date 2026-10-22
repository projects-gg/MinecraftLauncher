// The picker's size and tabs, apart from the panel so the button loads without it.

/** chat2.md §3.1: 336 × 384 px. */
export const PANEL_WIDTH = 336;
export const PANEL_HEIGHT = 384;

export type PickerTab = "emoji" | "stickers";

/** Keys for skeleton cells (a fixed list: they never move). */
export const SKELETON_KEYS = Array.from({ length: 96 }, (_, index) => `cell-${index}`);
