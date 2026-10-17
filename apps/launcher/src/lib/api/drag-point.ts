import type { DragPoint } from "./types";

/**
 * Tauri reports drag positions in physical pixels from the webview's top-left corner; the page
 * lays out in CSS pixels (on a 150% display, physical 300 is CSS 200).
 */
export function cssPoint(physical: DragPoint, scale: number): DragPoint {
  const factor = Number.isFinite(scale) && scale > 0 ? scale : 1;
  return { x: physical.x / factor, y: physical.y / factor };
}
