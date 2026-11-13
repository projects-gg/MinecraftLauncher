// How wide the room panel is while it is docked beside the stream's stage (SQOL-U3): about a third
// of the window, 360 to 440 px. At the window's smallest (1024) a chat stays readable beside the
// stage, and wider windows give the stage the rest.
import { useEffect, useState, useSyncExternalStore } from "react";

export const DOCK_MIN = 360;
export const DOCK_MAX = 440;

export function dockWidthFor(windowWidth: number): number {
  return Math.round(Math.min(DOCK_MAX, Math.max(DOCK_MIN, windowWidth * 0.3)));
}

/**
 * While the panel is docked (`px` > 0), toasts rest left of it, not over its chat and buttons: one
 * rule for the toast stack's right edge (Sonner's `--offset-right`), only while it is needed. A
 * stylesheet of its own rather than a variable on the page's root: changing an inherited variable
 * there restyles the whole document (design.md §8, POLISH5).
 */
export function useToastsBeside(px: number) {
  useEffect(() => {
    if (px <= 0) return;
    const style = document.createElement("style");
    style.dataset.roomDock = "";
    style.textContent = `:root [data-sonner-toaster][data-x-position="right"]{right:calc(var(--offset-right) + ${px}px)}`;
    document.head.append(style);
    return () => style.remove();
  }, [px]);
}

function onResize(listener: () => void) {
  window.addEventListener("resize", listener);
  return () => window.removeEventListener("resize", listener);
}

/** The window is at least `min` px tall (followed while the caller shows). */
export function useWindowTaller(min: number): boolean {
  return useSyncExternalStore(
    onResize,
    () => window.innerHeight >= min,
    () => true,
  );
}

/** The docked width for the window now, followed while `active` (a resize settles it again). */
export function useDockWidth(active: boolean): number {
  const [width, setWidth] = useState(() =>
    dockWidthFor(typeof window === "undefined" ? 1280 : window.innerWidth),
  );
  useEffect(() => {
    if (!active) return;
    const measure = () => setWidth(dockWidthFor(window.innerWidth));
    measure();
    window.addEventListener("resize", measure);
    return () => window.removeEventListener("resize", measure);
  }, [active]);
  return width;
}
