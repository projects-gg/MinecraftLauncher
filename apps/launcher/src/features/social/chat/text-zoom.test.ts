import { describe, expect, it } from "vitest";
import type { KeyPress } from "@/lib/shortcuts";
import {
  DEFAULT_TEXT_SIZE,
  stepTextSize,
  TEXT_SIZES,
  textSizeOf,
  WHEEL_START,
  type WheelZoom,
  wheelZoom,
  writingZoomOf,
  zoomKeyOf,
  zoomOf,
} from "./text-zoom";

const press = (key: string, code: string, extra: Partial<KeyPress> = {}): KeyPress => ({
  key,
  code,
  ctrlKey: true,
  metaKey: false,
  altKey: false,
  shiftKey: false,
  ...extra,
});

describe("the chat's text size", () => {
  it("is one of the steps, the default for anything unreadable", () => {
    expect(TEXT_SIZES).toEqual([90, 100, 110, 125, 150]);
    expect(textSizeOf(110)).toBe(110);
    expect(textSizeOf(120)).toBe(125);
    expect(textSizeOf(500)).toBe(150);
    expect(textSizeOf(10)).toBe(90);
    // A size saved before the steps changed reads as the nearest one.
    expect(textSizeOf(80)).toBe(90);
    for (const bad of [undefined, null, "125", Number.NaN, Number.POSITIVE_INFINITY, {}]) {
      expect(textSizeOf(bad)).toBe(DEFAULT_TEXT_SIZE);
    }
  });

  it("steps along and stops at the ends", () => {
    expect(stepTextSize(100, 1)).toBe(110);
    expect(stepTextSize(110, 1)).toBe(125);
    expect(stepTextSize(125, 1)).toBe(150);
    expect(stepTextSize(150, 1)).toBe(150);
    expect(stepTextSize(100, -1)).toBe(90);
    expect(stepTextSize(90, -1)).toBe(90);
    expect(stepTextSize(90, 9)).toBe(150);
    expect(stepTextSize(100, 0)).toBe(100);
  });

  it("zooms the writing only above 100 %", () => {
    expect(zoomOf(125)).toBe(1.25);
    expect(zoomOf(90)).toBe(0.9);
    expect(writingZoomOf(90)).toBe(1);
    expect(writingZoomOf(150)).toBe(1.5);
  });
});

describe("the text size's keys", () => {
  it("reads +, -, = and 0 on Windows and Linux with Ctrl", () => {
    expect(zoomKeyOf(press("+", "NumpadAdd"), "windows")).toBe("in");
    expect(zoomKeyOf(press("-", "NumpadSubtract"), "windows")).toBe("out");
    expect(zoomKeyOf(press("0", "Numpad0"), "linux")).toBe("reset");
    // US layout: "=" is the plus key unshifted, "+" with Shift.
    expect(zoomKeyOf(press("=", "Equal"), "windows")).toBe("in");
    expect(zoomKeyOf(press("+", "Equal", { shiftKey: true }), "windows")).toBe("in");
    expect(zoomKeyOf(press("_", "Minus", { shiftKey: true }), "windows")).toBe("out");
    expect(zoomKeyOf(press("0", "Digit0"), "windows")).toBe("reset");
  });

  it("reads the Turkish Q layout", () => {
    // "+" is Shift+4, "-" sits where US has "=", "0" is the digit.
    expect(zoomKeyOf(press("+", "Digit4", { shiftKey: true }), "windows")).toBe("in");
    expect(zoomKeyOf(press("-", "Equal"), "windows")).toBe("out");
    expect(zoomKeyOf(press("0", "Digit0"), "windows")).toBe("reset");
    // The key right of 0 writes "*": nothing.
    expect(zoomKeyOf(press("*", "Minus"), "windows")).toBeNull();
    // Shift+0 writes "=": bigger, as in browsers.
    expect(zoomKeyOf(press("=", "Digit0", { shiftKey: true }), "windows")).toBe("in");
  });

  it("finds 0 by its key where the layout writes something else there", () => {
    expect(zoomKeyOf(press("à", "Digit0"), "windows")).toBe("reset");
    expect(zoomKeyOf(press("Insert", "Numpad0"), "windows")).toBe("reset");
  });

  it("needs exactly the platform's modifier and never AltGr", () => {
    expect(zoomKeyOf(press("+", "NumpadAdd", { ctrlKey: false }), "windows")).toBeNull();
    expect(zoomKeyOf(press("+", "NumpadAdd", { metaKey: true }), "windows")).toBeNull();
    // AltGr arrives as Ctrl+Alt on Windows.
    expect(zoomKeyOf(press("-", "Equal", { altKey: true }), "windows")).toBeNull();
    expect(
      zoomKeyOf(press("0", "Digit0", { getModifierState: (k) => k === "AltGraph" }), "windows"),
    ).toBeNull();
    // macOS: ⌘, not Ctrl.
    expect(zoomKeyOf(press("+", "Equal", { ctrlKey: false, metaKey: true }), "macos")).toBe("in");
    expect(zoomKeyOf(press("+", "Equal"), "macos")).toBeNull();
  });

  it("ignores other keys and IME composition", () => {
    expect(zoomKeyOf(press("k", "KeyK"), "windows")).toBeNull();
    expect(zoomKeyOf(press("1", "Digit1"), "windows")).toBeNull();
    expect(zoomKeyOf(press("+", "NumpadAdd", { isComposing: true }), "windows")).toBeNull();
    expect(zoomKeyOf(press("Process", "NumpadAdd", { keyCode: 229 }), "windows")).toBeNull();
  });
});

describe("a Ctrl+wheel turn", () => {
  const turn = (state: WheelZoom, deltaY: number, at: number, mode = 0) =>
    wheelZoom(state, deltaY, mode, at);

  it("makes one step per mouse notch, up bigger and down smaller", () => {
    expect(turn(WHEEL_START, -100, 0).steps).toBe(1);
    expect(turn(WHEEL_START, 100, 0).steps).toBe(-1);
    // A wheel that reports lines (Firefox, some mice).
    expect(turn(WHEEL_START, -3, 0, 1).steps).toBe(1);
    expect(turn(WHEEL_START, 1, 0, 2).steps).toBe(-1);
  });

  it("adds a touchpad's small pieces up first", () => {
    let state = WHEEL_START;
    let steps = 0;
    for (let i = 0; i < 4; i += 1) {
      const next = turn(state, -12, i * 16);
      state = next.state;
      steps += next.steps;
    }
    expect(steps).toBe(0);
    const fifth = turn(state, -12, 64);
    expect(fifth.steps).toBe(1);
    expect(fifth.state.pending).toBe(0);
  });

  it("starts over after a pause or a change of direction", () => {
    const first = turn(WHEEL_START, -40 + 15, 0);
    expect(first.steps).toBe(0);
    // Long after: the old pieces are gone.
    expect(turn(first.state, -30 + 4, 1_000).state.pending).toBe(-26);
    // The other way at once: the old pieces are gone too.
    expect(turn(first.state, 20, 10).state.pending).toBe(20);
  });

  it("does nothing without a turn", () => {
    expect(turn(WHEEL_START, 0, 0)).toEqual({ steps: 0, state: WHEEL_START });
    expect(turn(WHEEL_START, Number.NaN, 0).steps).toBe(0);
  });
});
