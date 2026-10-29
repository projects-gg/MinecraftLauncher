// Global keyboard shortcuts (docs/tasks/q2.md, proposal 10): a small, fixed set, each held with
// Ctrl (⌘ on macOS) and nothing else. None of them types a character, so they never fight a text
// field, and none collides with Turkish AltGr letters: Windows reports AltGr as Ctrl+Alt (AltGr+3
// is "#" on Turkish Q, AltGr+Q is "@"), and Shift changes the key. Pure apart from the DOM helpers
// at the end; shortcuts.test.ts checks the matcher.
import type { DesktopOs } from "./platform";

export type ShortcutId = "palette" | "play" | "home" | "library" | "news" | "afk" | "settings";

export type PageShortcut = Extract<ShortcutId, "home" | "library" | "news" | "afk" | "settings">;

/** Every shortcut, in the order the palette's list shows them. */
export const SHORTCUTS: readonly ShortcutId[] = [
  "palette",
  "play",
  "home",
  "library",
  "news",
  "afk",
  "settings",
];

/** The page each page shortcut opens: the rail's order (Ctrl+1 … Ctrl+4), Ctrl+, for Settings. */
export const SHORTCUT_PAGES = {
  home: "/",
  library: "/library",
  news: "/news",
  afk: "/afk",
  settings: "/settings",
} as const satisfies Record<PageShortcut, string>;

export function isPageShortcut(id: ShortcutId): id is PageShortcut {
  return id in SHORTCUT_PAGES;
}

interface Chord {
  /** `KeyboardEvent.key` values, lower case: what the layout types (Turkish Q has "," by Enter). */
  keys: readonly string[];
  /** `KeyboardEvent.code` values: the number row's keys, for layouts that shift their digits. */
  codes?: readonly string[];
  /** The key cap after the modifier, also the key's name in `aria-keyshortcuts`. */
  cap: string;
}

const CHORDS: Record<ShortcutId, Chord> = {
  palette: { keys: ["k"], cap: "K" },
  play: { keys: ["enter"], cap: "Enter" },
  home: { keys: ["1"], codes: ["Digit1"], cap: "1" },
  library: { keys: ["2"], codes: ["Digit2"], cap: "2" },
  news: { keys: ["3"], codes: ["Digit3"], cap: "3" },
  afk: { keys: ["4"], codes: ["Digit4"], cap: "4" },
  settings: { keys: [","], cap: "," },
};

/** The key cap of Escape, which closes the palette, dialogs, sheets and menus (Radix does). */
export const ESCAPE_KEYS: readonly string[] = ["Esc"];

/** What the matcher reads from a `KeyboardEvent`. */
export interface KeyPress {
  key: string;
  code: string;
  ctrlKey: boolean;
  metaKey: boolean;
  altKey: boolean;
  shiftKey: boolean;
  repeat?: boolean;
  isComposing?: boolean;
  keyCode?: number;
  getModifierState?: (key: string) => boolean;
}

/**
 * The shortcut a key press means, if any. Exactly one modifier: Ctrl on Windows and Linux, ⌘ on
 * macOS. Held keys (auto-repeat) and IME composition never count, so holding Ctrl+K does not
 * flicker the palette and holding Ctrl+Enter starts one game.
 */
export function matchShortcut(event: KeyPress, os: DesktopOs): ShortcutId | null {
  if (event.repeat || event.isComposing || event.keyCode === 229) return null;
  if (event.altKey || event.shiftKey || event.getModifierState?.("AltGraph")) return null;
  const mac = os === "macos";
  if (mac ? !event.metaKey || event.ctrlKey : !event.ctrlKey || event.metaKey) return null;
  const key = event.key.toLowerCase();
  for (const id of SHORTCUTS) {
    const chord = CHORDS[id];
    if (chord.keys.includes(key) || chord.codes?.includes(event.code)) return id;
  }
  return null;
}

/** The key caps of a shortcut: ["Ctrl", "1"], or ["⌘", "1"] on macOS. */
export function shortcutKeys(id: ShortcutId, os: DesktopOs): string[] {
  const mac = os === "macos";
  const cap = CHORDS[id].cap;
  return [mac ? "⌘" : "Ctrl", mac && cap === "Enter" ? "↩" : cap];
}

/** The `aria-keyshortcuts` value of a shortcut: "Control+1", or "Meta+1" on macOS. */
export function ariaKeyShortcuts(id: ShortcutId, os: DesktopOs): string {
  return `${os === "macos" ? "Meta" : "Control"}+${CHORDS[id].cap}`;
}

/** Where a key press lands, read at the press. */
export interface ShortcutContext {
  /** The welcome flow covers the app. */
  blocked: boolean;
  /** A dialog, sheet, menu or popover other than the palette is open. */
  overlay: boolean;
  paletteOpen: boolean;
  /** The focus is in a text field. */
  typing: boolean;
  /** A control already used the key (`defaultPrevented`). */
  handled: boolean;
}

/**
 * Whether a shortcut runs now. Nothing runs under another layer: Esc closes it first. Ctrl+K opens
 * and closes the palette from anywhere, also from a text field (cmdk's own Ctrl+K moves its
 * selection up and marks the key handled, so that does not count). The pages type nothing, so
 * they work in text fields and from the palette, which they close. OYNA never starts from a text
 * field, where Ctrl+Enter may mean something; the palette presses it itself, as its "Oyna" rows'
 * key caps say (command-palette.tsx: cmdk took Ctrl+Enter as Enter on the row under the arrows).
 */
export function shortcutAllowed(id: ShortcutId, context: ShortcutContext): boolean {
  if (context.blocked || context.overlay) return false;
  if (id === "palette") return true;
  if (context.handled) return false;
  if (id === "play") return !context.typing && !context.paletteOpen;
  return true;
}

/**
 * WebView2's browser keys that only break a launcher: reload (F5 or Ctrl+R, also with Shift or
 * Ctrl) would start the whole interface over, Ctrl+P prints the window, F7 asks about caret
 * browsing. Find (Ctrl+F) stays: pages with a search field use it, the others show WebView2's
 * find bar, which works.
 */
export function isBrowserKey(event: KeyPress, os: DesktopOs): boolean {
  const key = event.key.toLowerCase();
  if (key === "f5" || key === "f7") return true;
  const modifier = os === "macos" ? event.metaKey : event.ctrlKey;
  return modifier && !event.altKey && (key === "r" || key === "p");
}

/** What `isTypingTarget` reads from the focused element (an `Element` in the app). */
export interface FocusedElement {
  tagName: string;
  type?: string;
  isContentEditable?: boolean;
  getAttribute(name: string): string | null;
}

const NON_TEXT_INPUTS = new Set([
  "button",
  "checkbox",
  "color",
  "file",
  "hidden",
  "image",
  "radio",
  "range",
  "reset",
  "submit",
]);

/** Whether a key press goes to a field that types text (the event's target). */
export function isTypingTarget(target: EventTarget | FocusedElement | null): boolean {
  if (!target || !("tagName" in target) || typeof target.tagName !== "string") return false;
  const element = target as FocusedElement;
  if (element.isContentEditable) return true;
  const tag = element.tagName.toLowerCase();
  if (tag === "textarea" || tag === "select") return true;
  if (tag === "input") return !NON_TEXT_INPUTS.has((element.type || "text").toLowerCase());
  const role = element.getAttribute("role");
  return role === "textbox" || role === "searchbox";
}

/**
 * An open dialog, sheet, menu or popover other than the palette (`cmdk-dialog`), the same layers
 * the router waits for before a page transition. Tooltips do not count; the mandatory update
 * screen is an `alertdialog`.
 */
const OVERLAY_SELECTOR =
  '[role="dialog"]:not([cmdk-dialog]), [role="alertdialog"], [role="menu"], [data-radix-popper-content-wrapper]:not(:has([role="tooltip"]))';

export function hasOpenOverlay(root: Pick<ParentNode, "querySelector">): boolean {
  return root.querySelector(OVERLAY_SELECTOR) !== null;
}

/** OYNA when a press starts a game ("Oyna", "Güncelle"), else null. */
function readyPlayButton(root: Pick<ParentNode, "querySelector">): HTMLButtonElement | null {
  const button = root.querySelector<HTMLButtonElement>("button[data-play-state]");
  const state = button?.dataset.playState;
  if (!button || button.disabled || (state !== "ready" && state !== "update")) return null;
  return button;
}

/** Whether a press of OYNA would start a game now (`pressPlayButton` would press). */
export function playButtonReady(root: Pick<ParentNode, "querySelector">): boolean {
  return readyPlayButton(root) !== null;
}

/**
 * Presses OYNA the way a click does (its double-press guard included), and only when a click
 * starts a game ("Oyna", "Güncelle"): while a launch is prepared or the game opens a click
 * cancels it, and a shortcut never does that. Returns whether it pressed.
 */
export function pressPlayButton(root: Pick<ParentNode, "querySelector">): boolean {
  const button = readyPlayButton(root);
  if (!button) return false;
  button.click();
  return true;
}
