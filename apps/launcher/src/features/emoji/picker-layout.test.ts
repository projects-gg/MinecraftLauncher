import { describe, expect, it } from "vitest";
import { PANEL_HEIGHT, PANEL_MIN_HEIGHT, panelHeightFor } from "./picker-layout";

// QOL-209: the picker opens as tall as the window allows beside its trigger.
describe("panelHeightFor", () => {
  const gap = 22;

  it("opens whole where either side has room", () => {
    // The composer's button at the bottom of a 640 px window: all the room above.
    expect(panelHeightFor({ top: 590, bottom: 622 }, 640, gap)).toBe(PANEL_HEIGHT);
    // A message's button near the top of a tall window: room below.
    expect(panelHeightFor({ top: 80, bottom: 112 }, 900, gap)).toBe(PANEL_HEIGHT);
  });

  it("takes the roomier side in a short window", () => {
    // A message in the middle of a 1280 × 633 window: 278 px above, 299 px below.
    expect(panelHeightFor({ top: 300, bottom: 312 }, 633, gap)).toBe(299);
  });

  it("never goes under the least height", () => {
    expect(panelHeightFor({ top: 200, bottom: 232 }, 400, gap)).toBe(PANEL_MIN_HEIGHT);
  });
});
