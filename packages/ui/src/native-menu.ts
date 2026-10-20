// WebView2's own context menu (Back, Refresh, Save as, Print, More tools and, in debug builds,
// Inspect) is on by default and Tauri leaves it on: a right click anywhere outside our own menus
// opened it, Refresh reloaded the app, and so did Shift+F10 or the menu key inside a sheet (the
// menu key opens ours on keydown, then WebView2 sent its own on keyup to the focused item: E2E2,
// 19 Oct 2026). Our menus (`ContextMenu`, Radix) take the event first and prevent it; whatever is
// left is prevented here, except where the native menu helps: in a text field (cut, copy, paste)
// and over a text selection (copy).

/** Input types whose value is text the player types (the native menu's cut, copy and paste). */
const TEXT_INPUTS = new Set(["", "text", "search", "email", "url", "tel", "password", "number"]);

/** What the guard reads of an event's target: an element, or anything else (text, window). */
interface TargetLike {
  closest?: (selector: string) => unknown;
}

interface FieldLike {
  tagName?: string;
  type?: string;
  isContentEditable?: boolean;
}

/** True where WebView2's own menu is useful: in a text field or with text selected. */
export function keepsNativeMenu(target: unknown, selectedText: string): boolean {
  if (selectedText.trim() !== "") return true;
  const element = target as TargetLike | null;
  if (!element || typeof element.closest !== "function") return false;
  const field = element.closest("input, textarea, [contenteditable]") as FieldLike | null;
  if (!field) return false;
  if (field.tagName === "TEXTAREA") return true;
  if (field.tagName === "INPUT") return TEXT_INPUTS.has((field.type ?? "").toLowerCase());
  return field.isContentEditable === true;
}

/** What `fromPortal` reads of a menu's trigger element. */
interface TriggerLike {
  contains(node: unknown): boolean;
}

/**
 * A right click (or the keyboard's) that reaches a context menu's trigger only through React, from
 * a portal rendered inside its tree: an open menu, a dialog. React passes such an event up its own
 * tree, so the panel around a friend row heard the menu key's `contextmenu` (WebView2 sends it on
 * keyup to the focused item of the row's menu, which had just opened) and opened its own menu over
 * the row's; a right click on an open menu's item opened a second menu the same way (E2E5,
 * 20 Oct 2026). It is not a click on the trigger, so the trigger's menu stays shut. In a text field
 * or over selected text the click is left alone, as anywhere else.
 */
export function fromPortal(trigger: TriggerLike, target: unknown, selectedText: string): boolean {
  if (trigger.contains(target)) return false;
  return !keepsNativeMenu(target, selectedText);
}

/** The document calls the guard needs, so tests can stand in. */
export interface MenuDocument {
  addEventListener(type: "contextmenu", listener: (event: MouseEvent) => void): void;
  removeEventListener(type: "contextmenu", listener: (event: MouseEvent) => void): void;
  getSelection(): { toString(): string } | null;
}

/**
 * Installs the guard once, before anything renders (each app's main.tsx). It listens on the
 * document in the bubble phase, after our menus had their say (they prevent the event they
 * open on). Returns a function that removes it.
 */
export function guardNativeContextMenu(
  doc: MenuDocument | undefined = globalThis.document,
): () => void {
  if (!doc) return () => {};
  const onContextMenu = (event: MouseEvent) => {
    if (event.defaultPrevented) return;
    if (keepsNativeMenu(event.target, doc.getSelection()?.toString() ?? "")) return;
    event.preventDefault();
  };
  doc.addEventListener("contextmenu", onContextMenu);
  return () => doc.removeEventListener("contextmenu", onContextMenu);
}
