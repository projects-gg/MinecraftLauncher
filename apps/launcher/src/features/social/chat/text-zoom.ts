// The chat's text size (CHAT-PROD; the owner, 27 Oct 2026: Ctrl + wheel and Ctrl + "+"/"-" over
// the chat should make its messages bigger or smaller). One of a few steps, as a browser zooms
// (Chrome's and Edge's between 90 and 150 %). It scales the conversation as a whole (the messages and
// what sits with them: emoji, stickers, reactions, cards, faces, times and spacing) and, above
// 100 %, the words being written; the panel around it and the launcher window keep their size.
// Kept on this computer (`pl.social`), never in the settings backup.
import type { DesktopOs } from "@/lib/platform";
import type { KeyPress } from "@/lib/shortcuts";

/**
 * The steps, in percent. Not below 90 %: the times, day lines and card details are 11–12 px at
 * 100 %, and at 80 % they fell to 9 px (unreadable; design.md: nothing below 11 px by default).
 */
export const TEXT_SIZES = [90, 100, 110, 125, 150] as const;
export type TextSize = (typeof TEXT_SIZES)[number];
export const DEFAULT_TEXT_SIZE: TextSize = 100;

/** A stored size as one of the steps: the nearest one, the default for anything unreadable. */
export function textSizeOf(value: unknown): TextSize {
  if (typeof value !== "number" || !Number.isFinite(value)) return DEFAULT_TEXT_SIZE;
  let best: TextSize = DEFAULT_TEXT_SIZE;
  for (const size of TEXT_SIZES) {
    if (Math.abs(size - value) < Math.abs(best - value)) best = size;
  }
  return best;
}

/** `steps` steps bigger (smaller when negative) than `size`, stopping at the ends. */
export function stepTextSize(size: number, steps: number): TextSize {
  const index = TEXT_SIZES.indexOf(textSizeOf(size));
  const next = Math.min(TEXT_SIZES.length - 1, Math.max(0, index + Math.trunc(steps)));
  return TEXT_SIZES[next] ?? DEFAULT_TEXT_SIZE;
}

/** The size as a CSS zoom factor. */
export function zoomOf(size: number): number {
  return textSizeOf(size) / 100;
}

/**
 * The zoom of the words being written: the composer grows with the messages, never shrinks with
 * them (a smaller chat shows more messages; the field stays as easy to read and press).
 */
export function writingZoomOf(size: number): number {
  return Math.max(1, zoomOf(size));
}

export type ZoomKey = "in" | "out" | "reset";

/**
 * What a key press does to the text size: Ctrl (⌘ on macOS) with "+" or "=" (bigger), "-" (smaller)
 * or "0" (back to 100 %), on any layout and on the number pad. Shift may be held: most layouts need
 * it for "+" (Turkish Q: Shift+4, US: Shift+=). AltGr (Ctrl+Alt on Windows) types characters
 * ("#", "@", "{" on Turkish Q) and never counts. A held key repeats, as a browser's zoom does.
 */
export function zoomKeyOf(event: KeyPress, os: DesktopOs): ZoomKey | null {
  if (event.isComposing || event.keyCode === 229) return null;
  if (event.altKey || event.getModifierState?.("AltGraph")) return null;
  const mac = os === "macos";
  if (mac ? !event.metaKey || event.ctrlKey : !event.ctrlKey || event.metaKey) return null;
  if (event.code === "NumpadAdd" || event.key === "+" || event.key === "=") return "in";
  if (event.code === "NumpadSubtract" || event.key === "-" || event.key === "_") return "out";
  // "0" by its character, or by its key where the layout writes something else there unshifted
  // (French AZERTY: "à").
  if (event.code === "Numpad0" || event.key === "0") return "reset";
  if (event.code === "Digit0" && !event.shiftKey) return "reset";
  return null;
}

/** What waits of a Ctrl+wheel turn (a touchpad's pinch comes in small pieces). */
export interface WheelZoom {
  /** Pixels turned in one direction since the last step. */
  pending: number;
  /** When the last piece came (ms). */
  at: number;
}

export const WHEEL_START: WheelZoom = { pending: 0, at: Number.NEGATIVE_INFINITY };

/** A turn this big (in pixels) is one mouse wheel notch: one step at once. */
const NOTCH_PX = 30;
/** Small pieces (a touchpad, a free-spinning wheel) make one step once they add up to this. */
const PIECES_PX = 50;
/** Pieces older than this are forgotten. */
const WHEEL_IDLE_MS = 400;
/** `deltaMode` lines and pages in pixels (Firefox and some mice report lines). */
const LINE_PX = 40;
const PAGE_PX = 800;

/**
 * The steps one Ctrl+wheel event makes, as browsers zoom: turning towards you (down) makes the text
 * smaller, away (up) bigger. A mouse notch is one step; smaller pieces add up first.
 */
export function wheelZoom(
  state: WheelZoom,
  deltaY: number,
  deltaMode: number,
  nowMs: number,
): { steps: number; state: WheelZoom } {
  const px = deltaMode === 1 ? deltaY * LINE_PX : deltaMode === 2 ? deltaY * PAGE_PX : deltaY;
  if (!Number.isFinite(px) || px === 0) return { steps: 0, state };
  const direction = -Math.sign(px);
  if (Math.abs(px) >= NOTCH_PX) {
    return { steps: direction, state: { pending: 0, at: nowMs } };
  }
  const fresh = nowMs - state.at <= WHEEL_IDLE_MS && Math.sign(state.pending) === Math.sign(px);
  const pending = (fresh ? state.pending : 0) + px;
  if (Math.abs(pending) < PIECES_PX) return { steps: 0, state: { pending, at: nowMs } };
  return { steps: direction, state: { pending: 0, at: nowMs } };
}
