// SHEET-UX: a card's menu opens below its "⋯", or beside it when the whole menu has no room below;
// turned upwards over a grid of cards it sat under the next card's own "⋯" (FEAT-L).
import { type MenuItem, menuHeight, placeBelowOrBeside } from "@pl/ui";
import { describe, expect, it } from "vitest";

const row = (label: string): MenuItem => ({ label, onSelect: () => undefined });

/** The 7/24 Bulut card's menu: nine rows in four groups (343 px in the launcher). */
const cloudCard: Array<MenuItem | null> = [
  row("Durdur"),
  row("Yeniden bağlan"),
  null,
  row("Konsol"),
  row("Envanter"),
  row("Hareket"),
  null,
  row("Ayarlar"),
  row("Ayarlarını kopyala"),
  null,
  row("Süreyi uzat"),
  row("afk.projects.gg'de aç"),
];

describe("menuHeight", () => {
  it("adds rows, separators and the panel's padding", () => {
    expect(menuHeight(cloudCard)).toBe(10 + 9 * 33 + 3 * 13);
    expect(Math.abs(menuHeight(cloudCard) - 343)).toBeLessThanOrEqual(4);
  });

  it("counts the strip of small actions", () => {
    const strip = [{ key: "a", label: "a", content: null, onSelect: () => undefined }];
    expect(menuHeight([row("Yanıtla")], strip)).toBe(10 + 33 + 50);
  });
});

describe("placeBelowOrBeside", () => {
  const height = menuHeight(cloudCard);

  it("stays below when the whole menu fits", () => {
    expect(placeBelowOrBeside({ bottom: 140 }, 760, height, "end", 6)).toEqual({
      side: "bottom",
      align: "end",
    });
  });

  it("opens to the left of a trigger at the card's end when it would not fit below", () => {
    // GeceBekcisi's "⋯" in the second row of cards at 1280 x 760.
    expect(placeBelowOrBeside({ bottom: 516 }, 760, height, "end", 6)).toEqual({
      side: "left",
      align: "start",
    });
  });

  it("opens to the right of a trigger at the start", () => {
    expect(placeBelowOrBeside({ bottom: 600 }, 640, height, "start", 6)).toEqual({
      side: "right",
      align: "start",
    });
  });

  it("counts the gap and the window's margin as taken", () => {
    const exact = 760 - height - 6 - 8;
    expect(placeBelowOrBeside({ bottom: exact }, 760, height, "end", 6).side).toBe("bottom");
    expect(placeBelowOrBeside({ bottom: exact + 1 }, 760, height, "end", 6).side).toBe("left");
  });
});
