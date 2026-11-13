// @vitest-environment happy-dom
// SQOL-U3: how wide the room panel may be, how its width lays it out, the separator's keys, and the
// width kept for each place on this computer.
import { afterEach, describe, expect, it } from "vitest";
import {
  clampWidth,
  KEY_STEP,
  KEY_STEP_BIG,
  keyWidth,
  layoutFor,
  toggledWidth,
  usePanelSize,
  WIDE_FIRST,
  WIDTH_CAP,
  widthLimits,
} from "./panel-size";

afterEach(() => {
  usePanelSize.setState({
    chosen: { modal: null, docked: null },
    lastWide: { modal: null, docked: null },
    dragging: null,
  });
});

describe("the panel's limits", () => {
  it("over the page: today's 440 px up to the window less 320, never past 1100", () => {
    expect(widthLimits("modal", 1280)).toEqual({ min: 440, max: 960 });
    expect(widthLimits("modal", 1024)).toEqual({ min: 440, max: 704 });
    expect(widthLimits("modal", 1920)).toEqual({ min: 440, max: WIDTH_CAP });
    // A window narrower than the sheet: the sheet is most of it, and no wider.
    expect(widthLimits("modal", 400)).toEqual({ min: 376, max: 376 });
  });

  it("beside the stage: today's third of the window, the stage keeping at least 480 px", () => {
    // 1280 - 60 (rail) - 24 (the stage's padding) - 480.
    expect(widthLimits("docked", 1280)).toEqual({ min: 384, max: 716 });
    expect(widthLimits("docked", 1024)).toEqual({ min: 360, max: 460 });
    expect(widthLimits("docked", 1920)).toEqual({ min: 440, max: WIDTH_CAP });
  });

  it("keeps a width within them; none chosen is today's", () => {
    const limits = { min: 384, max: 716 };
    expect(clampWidth(null, limits)).toBe(384);
    expect(clampWidth(200, limits)).toBe(384);
    expect(clampWidth(900, limits)).toBe(716);
    expect(clampWidth(600.4, limits)).toBe(600);
    expect(clampWidth(Number.NaN, limits)).toBe(384);
  });
});

describe("the layout a width gives", () => {
  it("is one column under 600 px, two from 600, with more air from 900", () => {
    expect(layoutFor(440)).toBe("stacked");
    expect(layoutFor(599)).toBe("stacked");
    expect(layoutFor(600)).toBe("wide");
    expect(layoutFor(899)).toBe("wide");
    expect(layoutFor(900)).toBe("wider");
  });
});

describe("the separator's keys", () => {
  const limits = { min: 440, max: 960 };

  it("widen with ← and narrow with → (by 64 with Shift), Home and End to the ends", () => {
    expect(keyWidth("ArrowLeft", false, 500, limits)).toBe(500 + KEY_STEP);
    expect(keyWidth("ArrowRight", false, 500, limits)).toBe(500 - KEY_STEP);
    expect(keyWidth("ArrowLeft", true, 500, limits)).toBe(500 + KEY_STEP_BIG);
    expect(keyWidth("ArrowRight", true, 470, limits)).toBe(440);
    expect(keyWidth("ArrowLeft", true, 950, limits)).toBe(960);
    expect(keyWidth("Home", false, 700, limits)).toBe(440);
    expect(keyWidth("End", false, 700, limits)).toBe(960);
    expect(keyWidth("a", false, 700, limits)).toBeNull();
  });

  it("toggle with Enter between today's width and the last wide one", () => {
    expect(toggledWidth(800, limits, 800)).toBe(440);
    expect(toggledWidth(440, limits, 800)).toBe(800);
    // None yet: a first wide width, within the limits.
    expect(toggledWidth(440, limits, null)).toBe(WIDE_FIRST);
    expect(toggledWidth(440, { min: 440, max: 704 }, null)).toBe(704);
  });
});

describe("the width kept", () => {
  const limits = { min: 384, max: 716 };

  it("is kept for each place, today's as none, the last wide one remembered", () => {
    const { choose } = usePanelSize.getState();
    choose("docked", 640, limits);
    expect(usePanelSize.getState().chosen).toEqual({ modal: null, docked: 640 });
    expect(usePanelSize.getState().lastWide.docked).toBe(640);
    choose("docked", 300, limits);
    expect(usePanelSize.getState().chosen.docked).toBeNull();
    // Back to today's width: the wide one stays to come back to.
    expect(usePanelSize.getState().lastWide.docked).toBe(640);
  });

  it("ends a drag when a width is chosen", () => {
    usePanelSize.getState().drag(600);
    expect(usePanelSize.getState().dragging).toBe(600);
    usePanelSize.getState().choose("modal", 600, { min: 440, max: 960 });
    expect(usePanelSize.getState().dragging).toBeNull();
  });

  it("is kept on this computer, without the drag; a damaged value reads as none", async () => {
    usePanelSize.getState().choose("modal", 700, { min: 440, max: 960 });
    const kept = JSON.parse(localStorage.getItem("pl.room-panel") ?? "{}");
    expect(kept.state).toEqual({
      chosen: { modal: 700, docked: null },
      lastWide: { modal: 700, docked: null },
    });
    localStorage.setItem(
      "pl.room-panel",
      JSON.stringify({ state: { chosen: { modal: "wide", docked: -4 } }, version: 1 }),
    );
    await usePanelSize.persist.rehydrate();
    expect(usePanelSize.getState().chosen).toEqual({ modal: null, docked: null });
  });
});
