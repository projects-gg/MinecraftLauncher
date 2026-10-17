import { describe, expect, it, vi } from "vitest";
import {
  ariaKeyShortcuts,
  type FocusedElement,
  hasOpenOverlay,
  isPageShortcut,
  isTypingTarget,
  type KeyPress,
  matchShortcut,
  pressPlayButton,
  SHORTCUT_PAGES,
  SHORTCUTS,
  type ShortcutContext,
  shortcutAllowed,
  shortcutKeys,
} from "./shortcuts";

/** A key press with Ctrl held, unless `more` says otherwise. */
function press(key: string, code: string, more: Partial<KeyPress> = {}): KeyPress {
  return { key, code, ctrlKey: true, metaKey: false, altKey: false, shiftKey: false, ...more };
}

/** Windows reports AltGr as Ctrl+Alt, with the AltGraph modifier state on. */
function altGr(key: string, code: string): KeyPress {
  return press(key, code, { altKey: true, getModifierState: (name) => name === "AltGraph" });
}

const win = (event: KeyPress) => matchShortcut(event, "windows");

describe("matchShortcut", () => {
  it("knows the set on Windows", () => {
    expect(win(press("k", "KeyK"))).toBe("palette");
    expect(win(press("Enter", "Enter"))).toBe("play");
    expect(win(press("1", "Digit1"))).toBe("home");
    expect(win(press("2", "Digit2"))).toBe("library");
    expect(win(press("3", "Digit3"))).toBe("news");
    expect(win(press("4", "Digit4"))).toBe("afk");
    expect(win(press(",", "Comma"))).toBe("settings");
  });

  it("reads letters typed with Caps Lock on", () => {
    expect(win(press("K", "KeyK"))).toBe("palette");
  });

  it("takes the numpad's digits and Enter", () => {
    expect(win(press("1", "Numpad1"))).toBe("home");
    expect(win(press("4", "Numpad4"))).toBe("afk");
    expect(win(press("Enter", "NumpadEnter"))).toBe("play");
  });

  it("finds the comma where Turkish layouts put it", () => {
    // Turkish Q: the comma is the key beside Enter; Turkish F: the key US keyboards mark "/".
    expect(win(press(",", "Backslash"))).toBe("settings");
    expect(win(press(",", "Slash"))).toBe("settings");
    // Turkish Q's "." (US "/") and "ç" (US ".") stay free.
    expect(win(press(".", "Slash"))).toBeNull();
    expect(win(press("ç", "Period"))).toBeNull();
  });

  it("finds the digits on layouts that shift them", () => {
    // AZERTY types "&" and "é" on the number row; Ctrl+1 and Ctrl+2 are those keys.
    expect(win(press("&", "Digit1"))).toBe("home");
    expect(win(press("é", "Digit2"))).toBe("library");
  });

  it("never takes AltGr letters (Ctrl+Alt on Windows)", () => {
    // Turkish Q: AltGr+1 ">", AltGr+3 "#", AltGr+4 "$", AltGr+7 "{", AltGr+Q "@", AltGr+, "`".
    expect(win(altGr(">", "Digit1"))).toBeNull();
    expect(win(altGr("#", "Digit3"))).toBeNull();
    expect(win(altGr("$", "Digit4"))).toBeNull();
    expect(win(altGr("{", "Digit7"))).toBeNull();
    expect(win(altGr("@", "KeyQ"))).toBeNull();
    expect(win(altGr("`", "Backslash"))).toBeNull();
    // Without the modifier state too, and with the state alone.
    expect(win(press("#", "Digit3", { altKey: true }))).toBeNull();
    expect(
      win(press("1", "Digit1", { getModifierState: (name) => name === "AltGraph" })),
    ).toBeNull();
    expect(win(press("Enter", "Enter", { altKey: true }))).toBeNull();
  });

  it("leaves Shift combinations alone", () => {
    // Turkish Q: Shift+1 is "!", Shift+, is ";".
    expect(win(press("!", "Digit1", { shiftKey: true }))).toBeNull();
    expect(win(press(";", "Backslash", { shiftKey: true }))).toBeNull();
    expect(win(press("K", "KeyK", { shiftKey: true }))).toBeNull();
    expect(win(press("Enter", "Enter", { shiftKey: true }))).toBeNull();
  });

  it("has no single-key shortcuts", () => {
    for (const [key, code] of [
      ["k", "KeyK"],
      ["1", "Digit1"],
      [",", "Backslash"],
      ["Enter", "Enter"],
      ["Escape", "Escape"],
    ] as const) {
      expect(win(press(key, code, { ctrlKey: false }))).toBeNull();
    }
  });

  it("leaves the other Ctrl keys to text fields and pages", () => {
    for (const [key, code] of [
      ["5", "Digit5"],
      ["0", "Digit0"],
      ["c", "KeyC"],
      ["v", "KeyV"],
      ["x", "KeyX"],
      ["z", "KeyZ"],
      ["a", "KeyA"],
      ["s", "KeyS"],
      ["f", "KeyF"],
      ["ı", "KeyI"],
      ["i", "Quote"],
      ["ş", "Semicolon"],
      ["ğ", "BracketLeft"],
      ["ü", "BracketRight"],
      ["ö", "Comma"],
    ] as const) {
      expect(win(press(key, code))).toBeNull();
    }
  });

  it("ignores held keys and IME composition", () => {
    expect(win(press("k", "KeyK", { repeat: true }))).toBeNull();
    expect(win(press("Enter", "Enter", { repeat: true }))).toBeNull();
    expect(win(press("1", "Digit1", { isComposing: true }))).toBeNull();
    expect(win(press("Process", "Digit1", { keyCode: 229 }))).toBeNull();
  });

  it("uses ⌘ on macOS and Ctrl on Windows and Linux", () => {
    const mac = (event: KeyPress) => matchShortcut(event, "macos");
    const cmd = { ctrlKey: false, metaKey: true };
    expect(mac(press("k", "KeyK", cmd))).toBe("palette");
    expect(mac(press("Enter", "Enter", cmd))).toBe("play");
    expect(mac(press(",", "Comma", cmd))).toBe("settings");
    expect(mac(press("k", "KeyK"))).toBeNull();
    expect(mac(press("k", "KeyK", { metaKey: true }))).toBeNull();
    // The Windows key is not Ctrl.
    expect(win(press("1", "Digit1", cmd))).toBeNull();
    expect(win(press("1", "Digit1", { metaKey: true }))).toBeNull();
    expect(matchShortcut(press("2", "Digit2"), "linux")).toBe("library");
  });
});

describe("shortcutAllowed", () => {
  const free: ShortcutContext = {
    blocked: false,
    overlay: false,
    paletteOpen: false,
    typing: false,
    handled: false,
  };
  const when = (more: Partial<ShortcutContext>) => ({ ...free, ...more });

  it("runs nothing under another layer or the welcome flow", () => {
    for (const id of SHORTCUTS) {
      expect(shortcutAllowed(id, free)).toBe(true);
      expect(shortcutAllowed(id, when({ overlay: true }))).toBe(false);
      expect(shortcutAllowed(id, when({ blocked: true }))).toBe(false);
    }
  });

  it("opens and closes the palette from anywhere", () => {
    expect(shortcutAllowed("palette", when({ typing: true }))).toBe(true);
    expect(shortcutAllowed("palette", when({ paletteOpen: true, typing: true }))).toBe(true);
    // cmdk marks its own Ctrl+K (move up) handled.
    expect(shortcutAllowed("palette", when({ paletteOpen: true, handled: true }))).toBe(true);
  });

  it("never starts a game from a text field or the palette", () => {
    expect(shortcutAllowed("play", when({ typing: true }))).toBe(false);
    expect(shortcutAllowed("play", when({ paletteOpen: true }))).toBe(false);
    expect(shortcutAllowed("play", when({ handled: true }))).toBe(false);
  });

  it("changes pages from text fields and the palette", () => {
    for (const id of SHORTCUTS.filter(isPageShortcut)) {
      expect(shortcutAllowed(id, when({ typing: true }))).toBe(true);
      expect(shortcutAllowed(id, when({ paletteOpen: true, typing: true }))).toBe(true);
      expect(shortcutAllowed(id, when({ handled: true }))).toBe(false);
    }
  });
});

describe("key caps", () => {
  it("names Ctrl and the key, ⌘ on macOS", () => {
    expect(shortcutKeys("palette", "windows")).toEqual(["Ctrl", "K"]);
    expect(shortcutKeys("play", "windows")).toEqual(["Ctrl", "Enter"]);
    expect(shortcutKeys("home", "linux")).toEqual(["Ctrl", "1"]);
    expect(shortcutKeys("settings", "windows")).toEqual(["Ctrl", ","]);
    expect(shortcutKeys("palette", "macos")).toEqual(["⌘", "K"]);
    expect(shortcutKeys("play", "macos")).toEqual(["⌘", "↩"]);
  });

  it("writes aria-keyshortcuts with the standard key names", () => {
    expect(ariaKeyShortcuts("palette", "windows")).toBe("Control+K");
    expect(ariaKeyShortcuts("play", "windows")).toBe("Control+Enter");
    expect(ariaKeyShortcuts("afk", "windows")).toBe("Control+4");
    expect(ariaKeyShortcuts("settings", "windows")).toBe("Control+,");
    expect(ariaKeyShortcuts("home", "macos")).toBe("Meta+1");
    expect(ariaKeyShortcuts("play", "macos")).toBe("Meta+Enter");
  });

  it("leads every page shortcut to a page", () => {
    expect(SHORTCUTS.filter(isPageShortcut).map((id) => SHORTCUT_PAGES[id])).toEqual([
      "/",
      "/library",
      "/news",
      "/afk",
      "/settings",
    ]);
  });
});

describe("isTypingTarget", () => {
  const element = (tagName: string, more: Partial<FocusedElement> = {}, role?: string) =>
    ({
      tagName,
      getAttribute: (name: string) => (name === "role" ? (role ?? null) : null),
      ...more,
    }) as FocusedElement;

  it("knows text fields", () => {
    for (const type of [
      "",
      "text",
      "search",
      "email",
      "password",
      "number",
      "url",
      "tel",
      "date",
    ]) {
      expect(isTypingTarget(element("INPUT", { type }))).toBe(true);
    }
    expect(isTypingTarget(element("TEXTAREA"))).toBe(true);
    expect(isTypingTarget(element("SELECT"))).toBe(true);
    expect(isTypingTarget(element("DIV", { isContentEditable: true }))).toBe(true);
    expect(isTypingTarget(element("DIV", {}, "textbox"))).toBe(true);
  });

  it("does not count buttons, switches and the page", () => {
    for (const type of ["checkbox", "radio", "range", "button", "submit", "color", "file"]) {
      expect(isTypingTarget(element("INPUT", { type }))).toBe(false);
    }
    expect(isTypingTarget(element("BUTTON"))).toBe(false);
    expect(isTypingTarget(element("BUTTON", {}, "combobox"))).toBe(false);
    expect(isTypingTarget(element("A"))).toBe(false);
    expect(isTypingTarget(element("BODY"))).toBe(false);
    expect(isTypingTarget(null)).toBe(false);
  });
});

describe("DOM helpers", () => {
  const rootWith = (found: unknown) => ({ querySelector: () => found }) as unknown as ParentNode;

  it("sees an overlay when one matches", () => {
    expect(hasOpenOverlay(rootWith(null))).toBe(false);
    expect(hasOpenOverlay(rootWith({}))).toBe(true);
  });

  it("presses OYNA only when a press starts a game", () => {
    const button = (playState: string, disabled = false) => ({
      disabled,
      dataset: { playState },
      click: vi.fn(),
    });
    for (const state of ["ready", "update"]) {
      const b = button(state);
      expect(pressPlayButton(rootWith(b))).toBe(true);
      expect(b.click).toHaveBeenCalledOnce();
    }
    // Preparing: a press would cancel. Starting, playing: the button is disabled.
    for (const b of [button("working"), button("launching", true), button("running", true)]) {
      expect(pressPlayButton(rootWith(b))).toBe(false);
      expect(b.click).not.toHaveBeenCalled();
    }
    expect(pressPlayButton(rootWith(null))).toBe(false);
  });
});
