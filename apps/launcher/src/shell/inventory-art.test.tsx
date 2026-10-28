// @vitest-environment happy-dom
// INV-HEADS: a host that sizes itself to the art (the site's large window) hears the art's size
// from the view, never from its inline styles.
import { cleanup, render } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import {
  SAMPLE_BOOK,
  SAMPLE_INVENTORY,
} from "../../../../packages/ui/src/components/inventory/fixtures";
import { InventoryView } from "../../../../packages/ui/src/components/inventory/inventory-view";
import { screenFor } from "../../../../packages/ui/src/components/inventory/layout";
import type { TitleGlyph } from "../../../../packages/ui/src/components/inventory/types";

const BOOK: ReadonlyMap<number, TitleGlyph> = new Map([
  [0xf80a, { advance: -48 }],
  [0xe002, { advance: 257, image: { src: "book.png", width: 256, height: 76, ascent: 13 } }],
]);
const glyph = (codePoint: number) => BOOK.get(codePoint);

afterEach(cleanup);

describe("the art's size for its host", () => {
  it("says the art's box and the line under the window, again when it changes", () => {
    const onArt = vi.fn();
    const { rerender } = render(
      <InventoryView
        inventory={SAMPLE_INVENTORY}
        window={SAMPLE_BOOK}
        titleGlyph={glyph}
        onOp={async () => ({ ok: true })}
        onArt={onArt}
      />,
    );
    expect(onArt).toHaveBeenLastCalledWith({ width: 256, height: 168, under: 44 });
    const calls = onArt.mock.calls.length;
    // A new callback alone says nothing new; the player's own screen does.
    const next = vi.fn();
    rerender(
      <InventoryView
        inventory={SAMPLE_INVENTORY}
        window={SAMPLE_BOOK}
        titleGlyph={glyph}
        onOp={async () => ({ ok: true })}
        onArt={next}
      />,
    );
    expect(onArt.mock.calls.length).toBe(calls);
    expect(next).not.toHaveBeenCalled();
    rerender(
      <InventoryView
        inventory={SAMPLE_INVENTORY}
        titleGlyph={glyph}
        onOp={async () => ({ ok: true })}
        onArt={next}
      />,
    );
    const own = screenFor(null);
    expect(next).toHaveBeenLastCalledWith({ width: own.width, height: own.height, under: 44 });
  });
});
