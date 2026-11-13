// How wide the room panel is (SQOL-U3; the owner, 13 Nov: "köşesinden tutup sola çekerek
// genişletebilsinler ve ona göre düzenlensin"): its separator on the left edge widens it by a drag or
// the keys, from today's width up to what leaves the page its room (over the page, the window less
// 320 px, at most 1100; beside the stream's stage, the stage keeps at least 480 px and shrinks by
// itself, reading the docked width). The width chosen is kept for each place on this computer
// (`pl.room-panel`; not in the settings backup: it belongs to this screen) and a smaller window
// clamps it without forgetting it. From 600 px the panel lays out in two columns, the people beside
// the chat (`layoutFor`).
import { create } from "zustand";
import { persist } from "zustand/middleware";
import { dockWidthFor } from "./dock";

/** Where the panel is: a sheet over the page, or docked beside the stage. */
export type PanelPlace = "modal" | "docked";

/** One column under 600 px (today's), two from 600, with more air from 900. */
export type PanelLayout = "stacked" | "wide" | "wider";

export interface WidthLimits {
  min: number;
  max: number;
}

/** The sheet's own width over the page (today's). */
export const MODAL_WIDTH = 440;
/** No panel goes wider: lines past it are too long to read, the page too small beside it. */
export const WIDTH_CAP = 1100;
/** Over the page, the page keeps at least this much of the window. */
export const PAGE_KEEP = 320;
/** Beside the stage, the stage keeps at least this much. */
export const STAGE_KEEP = 480;
/** The rail (60 px) and the stage's padding on both sides (12 + 12). */
const STAGE_EDGES = 84;
export const WIDE_AT = 600;
export const WIDER_AT = 900;
/** The first wide width, before the player chose one (Enter, a double click). */
export const WIDE_FIRST = 720;
export const KEY_STEP = 16;
export const KEY_STEP_BIG = 64;

/** How narrow and how wide the panel may be in `place` for a window `windowWidth` px wide. */
export function widthLimits(place: PanelPlace, windowWidth: number): WidthLimits {
  const min =
    place === "modal"
      ? Math.min(MODAL_WIDTH, Math.round(windowWidth * 0.94))
      : dockWidthFor(windowWidth);
  const room = place === "modal" ? windowWidth - PAGE_KEEP : windowWidth - STAGE_EDGES - STAGE_KEEP;
  return { min, max: Math.max(min, Math.min(WIDTH_CAP, room)) };
}

/** A width within `limits`; none chosen is today's (the narrowest). */
export function clampWidth(width: number | null | undefined, limits: WidthLimits): number {
  if (width === null || width === undefined || !Number.isFinite(width)) return limits.min;
  return Math.round(Math.min(limits.max, Math.max(limits.min, width)));
}

export function layoutFor(width: number): PanelLayout {
  return width >= WIDER_AT ? "wider" : width >= WIDE_AT ? "wide" : "stacked";
}

/**
 * The width a key on the separator asks for (it sits on the panel's left edge: ← widens), or null
 * for a key it does not take: ← and → by 16 px, with Shift by 64; Home the narrowest, End the
 * widest.
 */
export function keyWidth(
  key: string,
  shift: boolean,
  width: number,
  limits: WidthLimits,
): number | null {
  const step = shift ? KEY_STEP_BIG : KEY_STEP;
  switch (key) {
    case "ArrowLeft":
      return clampWidth(width + step, limits);
    case "ArrowRight":
      return clampWidth(width - step, limits);
    case "Home":
      return limits.min;
    case "End":
      return limits.max;
    default:
      return null;
  }
}

/** Enter or a double click: today's width, or back to the last wide one (a first one before any). */
export function toggledWidth(width: number, limits: WidthLimits, lastWide: number | null): number {
  if (width > limits.min) return limits.min;
  return clampWidth(lastWide ?? WIDE_FIRST, limits);
}

interface PanelSize {
  /** The width chosen in each place (null: today's). */
  chosen: Record<PanelPlace, number | null>;
  /** The last width wider than today's in each place (Enter and a double click go back to it). */
  lastWide: Record<PanelPlace, number | null>;
  /** The width while the separator is dragged (not kept). */
  dragging: number | null;
  drag: (width: number | null) => void;
  /** A width the player settled on (a drag let go, a key): kept, the drag over. */
  choose: (place: PanelPlace, width: number, limits: WidthLimits) => void;
}

const NONE: Record<PanelPlace, number | null> = { modal: null, docked: null };

/** A kept width as it reads back: a positive whole number of pixels, or none. */
function widthOf(value: unknown): number | null {
  return typeof value === "number" && Number.isFinite(value) && value > 0
    ? Math.round(value)
    : null;
}

function placesOf(value: unknown): Record<PanelPlace, number | null> {
  const raw = (typeof value === "object" && value !== null ? value : {}) as Record<string, unknown>;
  return { modal: widthOf(raw.modal), docked: widthOf(raw.docked) };
}

export const usePanelSize = create<PanelSize>()(
  persist(
    (set) => ({
      chosen: NONE,
      lastWide: NONE,
      dragging: null,
      drag: (dragging) => set({ dragging }),
      choose: (place, width, limits) =>
        set((s) => {
          const at = clampWidth(width, limits);
          const wide = at > limits.min;
          return {
            dragging: null,
            chosen: { ...s.chosen, [place]: wide ? at : null },
            lastWide: wide ? { ...s.lastWide, [place]: at } : s.lastWide,
          };
        }),
    }),
    {
      name: "pl.room-panel",
      version: 1,
      partialize: (s) => ({ chosen: s.chosen, lastWide: s.lastWide }),
      // A hand-edited or damaged value reads as none.
      merge: (persisted, current) => {
        const kept = (persisted ?? {}) as Partial<Pick<PanelSize, "chosen" | "lastWide">>;
        return { ...current, chosen: placesOf(kept.chosen), lastWide: placesOf(kept.lastWide) };
      },
    },
  ),
);
