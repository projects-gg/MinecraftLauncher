// @vitest-environment happy-dom
// A menu row that sets a value (@pl/ui `MenuItem.slider`; VOICE's person volume): read out as a
// slider with its words ("%140"), ← and → step it, PageUp/PageDown ten steps, Home/End its ends,
// the value kept once the steps rest (not a settings write per press); a selection keeps the menu
// open; the value follows its owner unless a drag holds it.
import { Menu, type MenuItem, setAppReducedMotion, TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";

beforeAll(() => setAppReducedMotion(true));
afterAll(() => setAppReducedMotion(false));
afterEach(() => cleanup());

function volume(value: number, onChange: (v: number, final: boolean) => void): MenuItem {
  return {
    label: "Ses seviyesi",
    onSelect: () => {},
    slider: {
      value,
      min: 0,
      max: 200,
      step: 5,
      notch: 100,
      format: (v) => `%${v}`,
      onChange,
    },
  };
}

function open(items: Array<MenuItem | null>) {
  render(
    <TooltipProvider>
      <Menu items={items} trigger={<button type="button">Aç</button>} />
    </TooltipProvider>,
  );
  const trigger = screen.getByRole("button", { name: "Aç" });
  act(() => {
    fireEvent.pointerDown(trigger, { button: 0, pointerType: "mouse" });
  });
  return screen.getByRole("slider", { name: "Ses seviyesi" });
}

describe("a menu's slider row", () => {
  it("reads out as a slider with its words", () => {
    const slider = open([volume(140, () => {})]);
    expect(slider.getAttribute("aria-valuenow")).toBe("140");
    expect(slider.getAttribute("aria-valuetext")).toBe("%140");
    expect(slider.getAttribute("aria-valuemax")).toBe("200");
    // The sign is set in Figtree apart from the figure; the words read as one.
    expect(slider.textContent).toContain("%140");
  });

  it("steps with the arrows and jumps with the page and end keys, within its ends", () => {
    const change = vi.fn();
    const slider = open([volume(100, change)]);
    fireEvent.keyDown(slider, { key: "ArrowRight" });
    fireEvent.keyDown(slider, { key: "ArrowLeft" });
    fireEvent.keyDown(slider, { key: "PageUp" });
    fireEvent.keyDown(slider, { key: "End" });
    fireEvent.keyDown(slider, { key: "End" });
    fireEvent.keyDown(slider, { key: "Home" });
    expect(change.mock.calls).toEqual([
      [105, false],
      [100, false],
      [150, false],
      [200, false],
      [200, false],
      [0, false],
    ]);
  });

  it("keeps a key step's value once the steps rest, not at each press", () => {
    const change = vi.fn();
    const slider = open([volume(100, change)]);
    vi.useFakeTimers({ toFake: ["setTimeout", "clearTimeout"] });
    try {
      fireEvent.keyDown(slider, { key: "ArrowRight" });
      vi.advanceTimersByTime(400);
      fireEvent.keyDown(slider, { key: "ArrowRight" });
      vi.advanceTimersByTime(499);
      expect(change.mock.calls.filter(([, final]) => final)).toEqual([]);
      vi.advanceTimersByTime(1);
      expect(change).toHaveBeenLastCalledWith(110, true);
      expect(change.mock.calls.filter(([, final]) => final)).toHaveLength(1);
      // Leaving the row keeps it at once.
      fireEvent.keyDown(slider, { key: "ArrowLeft" });
      fireEvent.focusOut(slider);
      expect(change).toHaveBeenLastCalledWith(105, true);
      vi.advanceTimersByTime(1_000);
      expect(change.mock.calls.filter(([, final]) => final)).toHaveLength(2);
      // So does the menu closing.
      fireEvent.keyDown(slider, { key: "Home" });
      cleanup();
      expect(change).toHaveBeenLastCalledWith(0, true);
      expect(change.mock.calls.filter(([, final]) => final)).toHaveLength(3);
    } finally {
      vi.useRealTimers();
    }
  });

  it("keeps the menu open when chosen", () => {
    const slider = open([volume(100, () => {}), { label: "Sustur", onSelect: () => {} }]);
    fireEvent.keyDown(slider, { key: "Enter" });
    expect(screen.getByRole("slider", { name: "Ses seviyesi" })).toBeTruthy();
  });
});
