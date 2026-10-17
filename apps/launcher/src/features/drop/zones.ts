// Where files dragged over the window would land: which zones take part, which one is under the
// cursor and how each looks meanwhile. Pure functions (zones.test.ts); the DOM and the drag
// events are in store.ts.
import type { DroppedFile } from "@/lib/api";

export interface Point {
  x: number;
  y: number;
}

/** A rectangle in CSS pixels from the window's top-left corner. */
export interface Box {
  left: number;
  top: number;
  right: number;
  bottom: number;
}

/**
 * Places with their own drop handling (the skin panel, the import step) take every drop while
 * they are open; zones shown on a page (empty tabs) present the window's own handling there.
 */
export type ZoneLayer = "page" | "claim";

/** The window-wide zone, drawn by DropHost when no other place shows one. */
export const WINDOW_ZONE = "window";

/** How far in from the window's edges the window-wide zone is drawn (CSS pixels). */
export const WINDOW_INSET = 22;

export interface ZoneBox {
  id: string;
  box: Box;
  /** Front to back: claims (2), page zones (1), the window (0). */
  depth: number;
}

export type ZoneState = "idle" | "ready" | "hover" | "recede" | "refuse" | "absorb";

export function contains(box: Box, point: Point): boolean {
  return point.x >= box.left && point.x <= box.right && point.y >= box.top && point.y <= box.bottom;
}

const area = (box: Box) => Math.max(0, box.right - box.left) * Math.max(0, box.bottom - box.top);

/** The zone under the cursor: the frontmost one, then the smallest (the most specific). */
export function zoneAt(point: Point | null, zones: ZoneBox[]): string | null {
  if (!point) return null;
  let best: ZoneBox | null = null;
  for (const zone of zones) {
    if (!contains(zone.box, point)) continue;
    if (
      !best ||
      zone.depth > best.depth ||
      (zone.depth === best.depth && area(zone.box) < area(best.box))
    ) {
      best = zone;
    }
  }
  return best?.id ?? null;
}

/** The window-wide zone's box: the whole window less a comfortable inset. */
export function windowBox(width: number, height: number, inset = WINDOW_INSET): Box {
  return { left: inset, top: inset, right: width - inset, bottom: height - inset };
}

/**
 * Where a page zone is probed to see whether it is on screen and not under a dialog or a sheet:
 * its middle first, then a point inside each corner; only points inside the window count.
 */
export function probePoints(box: Box, width: number, height: number): Point[] {
  const inset = 12;
  const points: Point[] = [
    { x: (box.left + box.right) / 2, y: (box.top + box.bottom) / 2 },
    { x: box.left + inset, y: box.top + inset },
    { x: box.right - inset, y: box.top + inset },
    { x: box.left + inset, y: box.bottom - inset },
    { x: box.right - inset, y: box.bottom - inset },
  ];
  return points.filter(
    (p) =>
      p.x >= box.left &&
      p.x <= box.right &&
      p.y >= box.top &&
      p.y <= box.bottom &&
      p.x >= 0 &&
      p.y >= 0 &&
      p.x < width &&
      p.y < height,
  );
}

export interface ZoneCandidate {
  id: string;
  layer: ZoneLayer;
  /** On screen and not covered (always true for claims: they take drops wherever they land). */
  shown: boolean;
}

/**
 * The zones a drag talks to, decided as it enters: an open place with its own handling (the
 * skin panel, the import step) takes it alone; else the zones shown on the page; else the
 * window-wide zone. While the window's drops are off (the welcome flow) only claims remain.
 */
export function activeZones(candidates: ZoneCandidate[], windowEnabled: boolean): string[] {
  const claims = candidates.filter((c) => c.layer === "claim").map((c) => c.id);
  if (claims.length > 0) return claims;
  if (!windowEnabled) return [];
  const shown = candidates.filter((c) => c.layer === "page" && c.shown).map((c) => c.id);
  return shown.length > 0 ? shown : [WINDOW_ZONE];
}

/**
 * The zone a drop is shown going into: the one under the cursor when it takes part, else the
 * newest claim (a place opened last is on top), else the first zone. Drops anywhere on the window
 * still count, as before: the zone only shows where they went.
 */
export function dropTarget(
  active: string[],
  hovered: string | null,
  newestClaim: string | null,
): string | null {
  if (hovered && active.includes(hovered)) return hovered;
  if (newestClaim && active.includes(newestClaim)) return newestClaim;
  return active[0] ?? null;
}

export interface ZoneInput {
  /** Files are being dragged and this zone takes part. */
  active: boolean;
  /** These files can go here (true while they are still being read). */
  accepts: boolean;
  /** The zone under the cursor, if any. */
  hovered: string | null;
  /** Whether the zone under the cursor would take the files. */
  hoveredAccepts: boolean;
  /** The zone that just took a drop. */
  absorbing: string | null;
}

/**
 * A zone's look: the one under the cursor lights up and the others recede; zones that refuse
 * the files stay calm and neutral; the zone that took a drop does its short "absorb".
 */
export function zoneState(id: string, input: ZoneInput): ZoneState {
  if (input.absorbing === id) return "absorb";
  if (!input.active) return "idle";
  if (!input.accepts) return "refuse";
  if (input.hovered === id) return "hover";
  if (input.hovered !== null && input.hoveredAccepts) return "recede";
  return "ready";
}

/** A refusing zone also steps back while the cursor is over another zone. */
export function zoneDimmed(id: string, state: ZoneState, hovered: string | null): boolean {
  return state === "refuse" && hovered !== null && hovered !== id;
}

/** A skin is a PNG file (the panel reads and checks it after the drop). */
export const isSkinFile = (path: string) => /\.png$/i.test(path);

/** Whether a modpack is among the files; files still being read look welcome. */
export function hasModpack(files: DroppedFile[] | null): boolean {
  return files ? files.some((f) => f.kind === "modpack") : true;
}
