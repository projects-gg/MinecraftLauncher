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
 * How the room panel arrives: sliding in (opened), or in place (its shell changed while it showed,
 * or it is back from behind the full-screen stage), with the focus where it was in it: the chat's
 * field, the panel, or not in it at all.
 */
export interface Arrival {
  moved: boolean;
  focus: "field" | "panel" | null;
  /** Back beside the stage from behind its full screen: in place, the focus left where it is. */
  back?: boolean;
}

export const FRESH: Arrival = { moved: false, focus: null };
const BACK: Arrival = { moved: true, focus: null, back: true };

/**
 * The panel's arrival now, from where it was a moment ago (`placed`; `behind`: open but under the
 * full-screen stage) and where it is (`now`), or null when nothing arrives. Its shell changing in
 * place keeps the focus where it was in it (`focusIn`, read only then). Back beside the stage from
 * behind its full screen, it shows in place and leaves the focus where leaving full screen put it
 * (the player asked for the stage, not the panel; "Sohbet" asks for the chat's field itself); a
 * sheet over the page is a dialog and takes the focus as on opening.
 */
export function arrivalOf(
  placed: { docked: boolean; open: boolean; behind: boolean },
  now: { docked: boolean; open: boolean },
  focusIn: () => Arrival["focus"],
): Arrival | null {
  if (now.open && placed.open && placed.docked !== now.docked) {
    return { moved: true, focus: focusIn() };
  }
  if (now.open && !placed.open) return placed.behind && now.docked ? BACK : FRESH;
  return null;
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
