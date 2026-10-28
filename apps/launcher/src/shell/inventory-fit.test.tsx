// INV-HEADS: a server's menu art wider than the window is never cut (the owner's "KİTAP" menu),
// and a host with a fixed height can have the inventory fit both of its sides (`fit="contain"`).
import { renderToString } from "react-dom/server";
import { describe, expect, it } from "vitest";
import {
  SAMPLE_BOOK,
  SAMPLE_INVENTORY,
} from "../../../../packages/ui/src/components/inventory/fixtures";
import { InventoryView } from "../../../../packages/ui/src/components/inventory/inventory-view";
import { titleRuns } from "../../../../packages/ui/src/components/inventory/text";
import { artBox, layoutTitle } from "../../../../packages/ui/src/components/inventory/title";
import type { TitleGlyph } from "../../../../packages/ui/src/components/inventory/types";

// The book menu's glyphs as IA-ART gives them: back 48 from the title's x, then a picture 256 wide.
const BOOK: ReadonlyMap<number, TitleGlyph> = new Map([
  [0xf80a, { advance: -48 }],
  [0xe002, { advance: 257, image: { src: "book.png", width: 256, height: 76, ascent: 13 } }],
]);
const glyph = (codePoint: number) => BOOK.get(codePoint);

describe("a server's art past the window", () => {
  it("grows the art's box on every side it reaches", () => {
    const window = { width: 176, height: 166 };
    expect(artBox(window, [])).toEqual({ left: 0, top: 0, width: 176, height: 166 });
    expect(artBox(window, [{ x: 0, y: 0, width: 176, height: 72, src: "" }])).toEqual({
      left: 0,
      top: 0,
      width: 176,
      height: 166,
    });
    expect(artBox(window, [{ x: -40, y: -10, width: 256, height: 76, src: "" }])).toEqual({
      left: -40,
      top: -10,
      width: 256,
      height: 176,
    });
    expect(artBox(window, [{ x: 150, y: 160, width: 60, height: 20, src: "" }])).toEqual({
      left: 0,
      top: 0,
      width: 210,
      height: 180,
    });
  });

  it("scales to the whole book and keeps the window and its line under the art's place", () => {
    const open = SAMPLE_BOOK.open;
    const layout = layoutTitle(titleRuns(open.title), 8, 6, glyph);
    expect(layout.pictures).toEqual([{ x: -40, y: 0, width: 256, height: 76, src: "book.png" }]);
    const html = renderToString(
      <InventoryView
        inventory={SAMPLE_INVENTORY}
        window={SAMPLE_BOOK}
        titleGlyph={glyph}
        onOp={async () => ({ ok: true })}
      />,
    );
    // The scale's width is the art's (256), not the window's (176).
    expect(html).toContain("--pl-inv-w:256");
    expect(html).toContain("--pl-inv-gw:256");
    // The window starts 40 GUI pixels in, the picture at the art's edge, the line under the window.
    expect(html).toContain("left:calc(40 * var(--pl-inv-px))");
    expect(html).toContain("left:calc(0 * var(--pl-inv-px))");
    expect(html).toContain(
      "margin-left:calc(40 * var(--pl-inv-px));width:calc(176 * var(--pl-inv-px))",
    );
    expect(html).not.toContain('data-fit="contain"');
  });

  it("fits a fixed height on request: the art's height and the line under it", () => {
    const html = renderToString(
      <InventoryView
        inventory={SAMPLE_INVENTORY}
        window={SAMPLE_BOOK}
        titleGlyph={glyph}
        fit="contain"
        onOp={async () => ({ ok: true })}
      />,
    );
    expect(html).toContain('data-fit="contain"');
    expect(html).toContain("--pl-inv-h:168");
    // The line under the window: its 8 px gap and the hand slot's 18 GUI pixels at 2 px (36).
    expect(html).toContain("--pl-inv-under:44px");
    const picture = renderToString(
      <InventoryView inventory={SAMPLE_INVENTORY} mode="static" fit="contain" label="Envanter" />,
    );
    expect(picture).toContain("--pl-inv-under:0px");
  });
});
