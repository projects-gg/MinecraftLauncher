// @vitest-environment happy-dom
// SQOL-U3: the room panel's separator: a focusable vertical separator with its value, its keys,
// Enter and a double click between today's width and the last wide one, and a drag a frame at a
// time that Esc gives back.
import { TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { PanelResizer } from "./panel-resize";
import { usePanelSize, type WidthLimits } from "./panel-size";

const LIMITS: WidthLimits = { min: 440, max: 960 };

function show(width: number, limits = LIMITS) {
  return render(
    <TooltipProvider>
      <PanelResizer place="modal" width={width} limits={limits} windowWidth={1280} />
    </TooltipProvider>,
  );
}

const separator = () => screen.getByRole("separator", { name: "Panel Genişliği" });
const chosen = () => usePanelSize.getState().chosen.modal;

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  cleanup();
  vi.useRealTimers();
  usePanelSize.setState({
    chosen: { modal: null, docked: null },
    lastWide: { modal: null, docked: null },
    dragging: null,
  });
});

describe("the panel's separator", () => {
  it("is a focusable vertical separator with the panel's width as its value", () => {
    show(640);
    const edge = separator();
    expect(edge.getAttribute("aria-orientation")).toBe("vertical");
    expect(edge.tabIndex).toBe(0);
    expect(edge.getAttribute("aria-valuenow")).toBe("640");
    expect(edge.getAttribute("aria-valuemin")).toBe("440");
    expect(edge.getAttribute("aria-valuemax")).toBe("960");
    // How much of the window, and the layout it gives.
    expect(edge.getAttribute("aria-valuetext")).toBe("%50, iki sütun");
  });

  it("widens with ← and narrows with →, by 64 with Shift; Home and End go to the ends", () => {
    show(500);
    fireEvent.keyDown(separator(), { key: "ArrowLeft" });
    expect(chosen()).toBe(516);
    fireEvent.keyDown(separator(), { key: "ArrowRight", shiftKey: true });
    expect(chosen()).toBeNull();
    fireEvent.keyDown(separator(), { key: "End" });
    expect(chosen()).toBe(960);
    fireEvent.keyDown(separator(), { key: "Home" });
    // Today's width: kept as none.
    expect(chosen()).toBeNull();
  });

  it("goes with Enter or a double click between today's width and the last wide one", () => {
    usePanelSize.setState({ lastWide: { modal: 800, docked: null } });
    show(440);
    fireEvent.keyDown(separator(), { key: "Enter" });
    expect(chosen()).toBe(800);
    cleanup();
    show(800);
    fireEvent.doubleClick(separator());
    expect(chosen()).toBeNull();
  });

  it("follows a drag a frame at a time and keeps where it is let go", () => {
    show(440);
    const edge = separator();
    fireEvent.pointerDown(edge, { button: 0, pointerId: 1, clientX: 840 });
    fireEvent.pointerMove(edge, { pointerId: 1, clientX: 700 });
    fireEvent.pointerMove(edge, { pointerId: 1, clientX: 640 });
    // Nothing before the frame: one width a frame, the last asked for.
    expect(usePanelSize.getState().dragging).toBeNull();
    act(() => {
      vi.advanceTimersByTime(20);
    });
    expect(usePanelSize.getState().dragging).toBe(640);
    fireEvent.pointerUp(edge, { pointerId: 1, clientX: 640 });
    expect(chosen()).toBe(640);
    expect(usePanelSize.getState().dragging).toBeNull();
  });

  it("gives the width back on Esc during a drag, and the panel stays", () => {
    show(440);
    const edge = separator();
    const closes = vi.fn();
    window.addEventListener("keydown", closes);
    fireEvent.pointerDown(edge, { button: 0, pointerId: 1, clientX: 840 });
    fireEvent.pointerMove(edge, { pointerId: 1, clientX: 600 });
    act(() => {
      vi.advanceTimersByTime(20);
    });
    expect(usePanelSize.getState().dragging).toBe(680);
    fireEvent.keyDown(document.body, { key: "Escape" });
    window.removeEventListener("keydown", closes);
    expect(usePanelSize.getState().dragging).toBeNull();
    expect(chosen()).toBeNull();
    // The panel's own Esc (on the way out) never hears it.
    expect(closes).not.toHaveBeenCalled();
  });
});
