// The keyboard's right click in the friends panel: Shift+F10 or the menu key opens the menu of
// the focused row (or the panel's own) just under it. Not every engine sends a `contextmenu` for
// those keys, and where one does it lands in a corner of the element (the movable friends button
// does the same, shell/movable-menu.tsx).
import type { KeyboardEvent } from "react";

export function isMenuKey(event: Pick<KeyboardEvent, "key" | "shiftKey">): boolean {
  return event.key === "ContextMenu" || (event.key === "F10" && event.shiftKey);
}

/**
 * Opens the right-click menu `element` belongs to, as if it was right-clicked: under its start
 * (a row, a button), or `inside` its top corner (a whole panel).
 */
export function openMenuAt(element: HTMLElement, place: "below" | "inside" = "below") {
  const rect = element.getBoundingClientRect();
  element.dispatchEvent(
    new MouseEvent("contextmenu", {
      bubbles: true,
      cancelable: true,
      button: 2,
      clientX: rect.left + 12,
      clientY: place === "inside" ? rect.top + 12 : rect.bottom,
    }),
  );
}

/**
 * A row's keydown: Shift+F10 or the menu key opens the row's menu (and nothing around it gets the
 * key). True when it did.
 */
export function openMenuFromKey(event: KeyboardEvent<HTMLElement>): boolean {
  if (!isMenuKey(event)) return false;
  event.preventDefault();
  event.stopPropagation();
  openMenuAt(event.currentTarget);
  return true;
}
