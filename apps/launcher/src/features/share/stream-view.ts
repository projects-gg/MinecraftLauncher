// The stream viewer's rules (SQOL-U1; sqol.md §1.1), apart from React and the browser so they can be
// tested: which layer a tile asks for ("Yayın Kalitesi"), how long a weak connection holds the small
// layer, how zoom and pan move the picture, and which rows the stream menu has, in its order.
import type { ShareLayer, ShareViewQuality } from "@/share/types";

/** Under "Otomatik", a tile narrower than this (CSS pixels) takes the small layer. */
export const SMALL_TILE_PX = 480;

// --- the layer -----------------------------------------------------------------------------------

export interface LayerInput {
  quality: ShareViewQuality;
  /** The tile's width in CSS pixels (null: not measured yet). */
  width: number | null;
  /** "Otomatik" holds the small layer while the connection is weak (`weakStep`). */
  weakHeld: boolean;
  /** The picture is zoomed in to read small text: the full layer, unless the connection is weak. */
  zoomed: boolean;
  /**
   * The share sends a small layer. Every share does today (pl-share preset.rs sends `f` and `q`);
   * one without would never be asked for it.
   */
  hasSmall: boolean;
}

/** The layer a tile asks for. */
export function layerFor(input: LayerInput): ShareLayer {
  if (!input.hasSmall) return "f";
  switch (input.quality) {
    case "high":
      return "f";
    case "saver":
      return "q";
    case "auto":
      if (input.weakHeld) return "q";
      if (input.zoomed) return "f";
      return input.width !== null && input.width < SMALL_TILE_PX ? "q" : "f";
  }
}

/** A weak connection under "Otomatik": the small layer, and the full one back after this long good. */
export const WEAK_BACK_MS = 30_000;
/** Weak again within this long after the full layer came back: the next wait doubles… */
export const WEAK_AGAIN_MS = 60_000;
/** …up to this. */
export const WEAK_BACK_MAX_MS = 120_000;

/**
 * Whether a weak connection holds the small layer. Without it a tile would flip between the layers:
 * the small one carries less, the connection looks good again, the full one makes it weak again.
 */
export interface WeakHold {
  held: boolean;
  /** Since when the connection is good again while held (null: not yet). */
  goodSince: number | null;
  /** How long it must stay good before the full layer comes back. */
  wait: number;
  /** When the full layer last came back. */
  backAt: number | null;
}

export const NO_WEAK_HOLD: WeakHold = { held: false, goodSince: null, wait: WEAK_BACK_MS, backAt: null };

/** One quality reading into the hold (`now` in milliseconds). */
export function weakStep(hold: WeakHold, weak: boolean, now: number): WeakHold {
  if (weak) {
    if (hold.held) return hold.goodSince === null ? hold : { ...hold, goodSince: null };
    const again = hold.backAt !== null && now - hold.backAt < WEAK_AGAIN_MS;
    return {
      held: true,
      goodSince: null,
      wait: again ? Math.min(hold.wait * 2, WEAK_BACK_MAX_MS) : WEAK_BACK_MS,
      backAt: hold.backAt,
    };
  }
  if (!hold.held) return hold;
  const since = hold.goodSince ?? now;
  if (now - since >= hold.wait) return { held: false, goodSince: null, wait: hold.wait, backAt: now };
  return hold.goodSince === null ? { ...hold, goodSince: now } : hold;
}

/** When the hold lets the full layer back if nothing changes meanwhile (null: not counting). */
export function weakBackAt(hold: WeakHold): number | null {
  return hold.held && hold.goodSince !== null ? hold.goodSince + hold.wait : null;
}

// --- zoom ----------------------------------------------------------------------------------------

/** The picture's zoom: its scale and where its top left corner sits in the tile (CSS pixels). */
export interface Zoom {
  s: number;
  x: number;
  y: number;
}

export const NO_ZOOM: Zoom = { s: 1, x: 0, y: 0 };
/** "%400" at most. */
export const ZOOM_MAX = 4;
/** One key press (Ctrl + = / Ctrl + -) zooms by this much. */
export const ZOOM_STEP = 1.25;
/** A zoom this close to 1 is none. */
const ZOOM_SNAP = 1.02;

const clamp = (value: number, min: number, max: number) => Math.min(max, Math.max(min, value));

/** Keeps the picture over its whole tile: no empty edge while zoomed. */
export function clampZoom(zoom: Zoom, width: number, height: number): Zoom {
  const s = clamp(zoom.s, 1, ZOOM_MAX);
  if (s < ZOOM_SNAP) return NO_ZOOM;
  return { s, x: clamp(zoom.x, width * (1 - s), 0), y: clamp(zoom.y, height * (1 - s), 0) };
}

/** Zooms by `factor`, keeping the point (`px`, `py`) of the tile where it is (the pointer's). */
export function zoomAt(
  zoom: Zoom,
  factor: number,
  px: number,
  py: number,
  width: number,
  height: number,
): Zoom {
  const s = clamp(zoom.s * factor, 1, ZOOM_MAX);
  // The picture's point under (px, py), in the picture's own pixels.
  const cx = (px - zoom.x) / zoom.s;
  const cy = (py - zoom.y) / zoom.s;
  return clampZoom({ s, x: px - cx * s, y: py - cy * s }, width, height);
}

/** Moves a zoomed picture (a drag, Shift + an arrow). */
export function panBy(zoom: Zoom, dx: number, dy: number, width: number, height: number): Zoom {
  if (zoom.s === 1) return zoom;
  return clampZoom({ ...zoom, x: zoom.x + dx, y: zoom.y + dy }, width, height);
}

/** Ctrl + the wheel: one notch (100 px) zooms by about a fifth; a touchpad's small steps follow it. */
export function wheelFactor(deltaY: number, deltaMode = 0): number {
  const px = deltaMode === 1 ? deltaY * 16 : deltaMode === 2 ? deltaY * 400 : deltaY;
  return clamp(Math.exp(-px * 0.002), 1 / 1.5, 1.5);
}

/** The zoom as the chip says it (250 for "%250"). */
export function zoomPercent(zoom: Zoom): number {
  return Math.round(zoom.s * 100);
}

// --- the stream menu -----------------------------------------------------------------------------

export type StreamRow =
  | "volume"
  | "mute"
  | "quality"
  | "duck"
  | "fullscreen"
  | "popout"
  | "popin"
  | "onTop"
  | "spotlight"
  | "sideBySide"
  | "info"
  | "report"
  | "stop";

export interface StreamPlace {
  /** The "Yayın" window: "Launcher'a Geri Al" and "Her Zaman Üstte", no report (no dialog there). */
  popout: boolean;
  /** Two shares on the stage: one can be brought forward. */
  two: boolean;
  /** This one is forward now. */
  forward: boolean;
}

/**
 * The stream menu's rows in the contract's order (sqol.md §1.1), `null` for a separator. The person's
 * own rows follow in the launcher's window (FIX2: first the line's own actions, then the person's).
 */
export function streamRows(place: StreamPlace): Array<StreamRow | null> {
  const where: StreamRow[] = place.popout ? ["popin", "onTop"] : ["popout"];
  const forward: StreamRow[] = place.two ? [place.forward ? "sideBySide" : "spotlight"] : [];
  return [
    "volume",
    "mute",
    "quality",
    "duck",
    null,
    "fullscreen",
    ...where,
    ...forward,
    "info",
    null,
    ...(place.popout ? [] : (["report"] as StreamRow[])),
    "stop",
  ];
}

// --- keys on a focused tile ----------------------------------------------------------------------

export type TileKey =
  | { kind: "fullscreen" }
  | { kind: "mute" }
  | { kind: "volume"; by: number }
  | { kind: "zoom"; by: "in" | "out" | "reset" }
  | { kind: "pan"; dx: number; dy: number };

/** What a key does on a focused tile (null: nothing; the key goes on). */
export function tileKey(
  event: Pick<KeyboardEvent, "key" | "code" | "ctrlKey" | "metaKey" | "altKey" | "shiftKey">,
  options: { onTile: boolean; zoomed: boolean; sound: boolean },
): TileKey | null {
  // AltGr reads as Ctrl + Alt on Windows (Turkish Q types "@", "{" with it): never a shortcut.
  if (event.altKey) return null;
  const ctrl = event.ctrlKey || event.metaKey;
  if (ctrl) {
    if (event.key === "=" || event.key === "+" || event.code === "NumpadAdd") {
      return { kind: "zoom", by: "in" };
    }
    if (event.key === "-" || event.code === "NumpadSubtract") return { kind: "zoom", by: "out" };
    if (event.key === "0" || event.code === "Numpad0") return { kind: "zoom", by: "reset" };
    return null;
  }
  const key = event.key.length === 1 ? event.key.toLocaleLowerCase("en") : event.key;
  if (!event.shiftKey && key === "f") return { kind: "fullscreen" };
  if (!event.shiftKey && key === "m" && options.sound) return { kind: "mute" };
  // The arrows only on the tile itself: on its buttons they belong to them (a menu's ↓ opens it).
  if (!options.onTile) return null;
  if (event.shiftKey) {
    if (!options.zoomed) return null;
    switch (event.key) {
      case "ArrowLeft":
        return { kind: "pan", dx: 1, dy: 0 };
      case "ArrowRight":
        return { kind: "pan", dx: -1, dy: 0 };
      case "ArrowUp":
        return { kind: "pan", dx: 0, dy: 1 };
      case "ArrowDown":
        return { kind: "pan", dx: 0, dy: -1 };
      default:
        return null;
    }
  }
  if (!options.sound) return null;
  if (event.key === "ArrowUp") return { kind: "volume", by: 5 };
  if (event.key === "ArrowDown") return { kind: "volume", by: -5 };
  return null;
}

/** The keys a tile answers, for `aria-keyshortcuts`. */
export function tileShortcuts(sound: boolean): string {
  return [
    "F",
    ...(sound ? ["M", "ArrowUp", "ArrowDown"] : []),
    "Control+=",
    "Control+-",
    "Control+0",
    "Shift+ArrowLeft",
    "Shift+ArrowRight",
    "Shift+ArrowUp",
    "Shift+ArrowDown",
    "Escape",
    "Shift+F10",
  ].join(" ");
}
