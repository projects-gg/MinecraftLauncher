import { guardNativeContextMenu, keepsNativeMenu, type MenuDocument } from "@pl/ui";
import { describe, expect, it } from "vitest";

// WebView2's own context menu (Back, Refresh, Print...) stays only where it helps (E2E2).

/** An element as the guard sees it: `closest` finds the nearest field up its chain. */
function at(field: { tagName: string; type?: string; isContentEditable?: boolean } | null) {
  return { closest: () => field };
}

describe("keepsNativeMenu", () => {
  it("keeps it in text fields: cut, copy and paste", () => {
    expect(keepsNativeMenu(at({ tagName: "INPUT", type: "text" }), "")).toBe(true);
    expect(keepsNativeMenu(at({ tagName: "INPUT", type: "" }), "")).toBe(true);
    expect(keepsNativeMenu(at({ tagName: "INPUT", type: "Search" }), "")).toBe(true);
    expect(keepsNativeMenu(at({ tagName: "INPUT", type: "password" }), "")).toBe(true);
    expect(keepsNativeMenu(at({ tagName: "TEXTAREA" }), "")).toBe(true);
    expect(keepsNativeMenu(at({ tagName: "DIV", isContentEditable: true }), "")).toBe(true);
  });

  it("drops it elsewhere: buttons, switches, check boxes, sliders, the page itself", () => {
    expect(keepsNativeMenu(at(null), "")).toBe(false);
    expect(keepsNativeMenu(at({ tagName: "INPUT", type: "checkbox" }), "")).toBe(false);
    expect(keepsNativeMenu(at({ tagName: "INPUT", type: "range" }), "")).toBe(false);
    expect(keepsNativeMenu(at({ tagName: "INPUT", type: "file" }), "")).toBe(false);
    expect(keepsNativeMenu(at({ tagName: "DIV", isContentEditable: false }), "")).toBe(false);
    // The window or a text node: nothing to look up.
    expect(keepsNativeMenu(null, "")).toBe(false);
    expect(keepsNativeMenu({}, "")).toBe(false);
  });

  it("keeps it over selected text (copy)", () => {
    expect(keepsNativeMenu(at(null), "Kış etkinliği")).toBe(true);
    expect(keepsNativeMenu(at(null), "  \n")).toBe(false);
  });
});

describe("guardNativeContextMenu", () => {
  function page(selected = "") {
    let listener: ((event: MouseEvent) => void) | null = null;
    const doc: MenuDocument = {
      addEventListener: (_type, fn) => {
        listener = fn;
      },
      removeEventListener: (_type, fn) => {
        if (listener === fn) listener = null;
      },
      getSelection: () => ({ toString: () => selected }),
    };
    /** A right click (or Shift+F10, the menu key) on `target`; `taken`: one of our menus opened. */
    const press = (target: unknown, taken = false) => {
      let prevented = taken;
      let calls = 0;
      const event = {
        target,
        get defaultPrevented() {
          return prevented;
        },
        preventDefault: () => {
          calls += 1;
          prevented = true;
        },
      } as unknown as MouseEvent;
      listener?.(event);
      return { prevented, calls };
    };
    return { doc, press, listening: () => listener !== null };
  }

  it("prevents what none of our menus took, and leaves text fields alone", () => {
    const { doc, press } = page();
    guardNativeContextMenu(doc);
    expect(press(at(null))).toEqual({ prevented: true, calls: 1 });
    expect(press(at({ tagName: "INPUT", type: "text" }))).toEqual({ prevented: false, calls: 0 });
    // Our own menu (Radix ContextMenu) prevented it first: the guard does not touch it.
    expect(press(at(null), true)).toEqual({ prevented: true, calls: 0 });
  });

  it("leaves the menu over selected text", () => {
    const { doc, press } = page("2.4 yama notları");
    guardNativeContextMenu(doc);
    expect(press(at(null))).toEqual({ prevented: false, calls: 0 });
  });

  it("is removed by the function it returns, and does nothing without a document", () => {
    const { doc, listening } = page();
    const remove = guardNativeContextMenu(doc);
    expect(listening()).toBe(true);
    remove();
    expect(listening()).toBe(false);
    expect(() => guardNativeContextMenu(undefined)()).not.toThrow();
  });
});
