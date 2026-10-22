import { focusMovedOn } from "@pl/ui";
import { describe, expect, it } from "vitest";
import { HOLD_MS, takesBack } from "./focus";

describe("an edit's focus", () => {
  it("comes back from a closing menu, never from the player", () => {
    const hold = { acted: false, startedAt: 1_000 };
    // The "…" menu gives the focus to its trigger ~120 ms after the field took it.
    expect(takesBack(hold, 1_120, false)).toBe(true);
    // Back on the field: nothing to do.
    expect(takesBack(hold, 1_130, true)).toBe(false);
    // The player pressed or typed meanwhile: wherever they went is theirs.
    expect(takesBack({ ...hold, acted: true }, 1_120, false)).toBe(false);
    // The hold ends by itself.
    expect(takesBack(hold, 1_000 + HOLD_MS, false)).toBe(false);
  });

  it("stays where a menu item put it", () => {
    const page: { body?: unknown; documentElement?: unknown } = {};
    page.body = { ownerDocument: page };
    page.documentElement = { ownerDocument: page };
    const field = { ownerDocument: page } as unknown as Element;
    const row = { ownerDocument: page } as unknown as Element;
    const menu = { contains: (node: unknown) => node === row };
    // "Düzenle" focused the composer: the closing menu leaves it there.
    expect(focusMovedOn(field, menu)).toBe(true);
    // Its focused row went away and the page has the focus: the trigger gets it back (keys).
    expect(focusMovedOn(page.body as Element, menu)).toBe(false);
    expect(focusMovedOn(null, menu)).toBe(false);
    // Still inside the menu (a submenu's row).
    expect(focusMovedOn(row, menu)).toBe(false);
  });
});
