// @vitest-environment happy-dom
// The AFK panel's size (AFK-UX): where a drag lets go, the edge's keys and the header's button, the
// layout's width class, and the size kept for the next open.
import { TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { Dialog } from "radix-ui";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import {
  AfkSidePanel,
  PanelSizeButton,
  placeAfterDrag,
  useAfkPanel,
  useAfkPanelSize,
} from "./side-panel";

function Probe() {
  const { full, wide } = useAfkPanel();
  return <p data-testid="probe">{`${full ? "full" : "narrow"} ${wide ? "wide" : "single"}`}</p>;
}

function Panel({ onClose = () => {} }: { onClose?: () => void }) {
  return (
    <TooltipProvider>
      <AfkSidePanel open onClose={onClose}>
        <Dialog.Title>KasaBekcisi konsolu</Dialog.Title>
        <PanelSizeButton />
        <Probe />
      </AfkSidePanel>
    </TooltipProvider>
  );
}

// happy-dom rejects an animation's `finished` when it is cancelled (motion's own, at unmount)
// and nobody listens there: the rejection is handled here, the cancel stays as it is. Kept for the
// whole file: motion stops its animations in a frame after the last test (the file's environment
// is its own).
const cancel = Animation.prototype.cancel;
beforeAll(() => {
  Animation.prototype.cancel = function (this: Animation) {
    this.finished.catch(() => undefined);
    cancel.call(this);
  };
});

beforeEach(() => {
  useAfkPanelSize.setState({ full: false });
});

afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
});

describe("placeAfterDrag", () => {
  const narrow = 560;
  const full = 1280;

  it("lets go onto the nearest place", () => {
    expect(placeAfterDrag(600, 0, narrow, full)).toBe("narrow");
    expect(placeAfterDrag(400, 0, narrow, full)).toBe("narrow");
    expect(placeAfterDrag(1000, 0, narrow, full)).toBe("full");
    expect(placeAfterDrag(300, 0, narrow, full)).toBe("closed");
  });

  it("lets a flick decide by its direction", () => {
    expect(placeAfterDrag(620, 0.8, narrow, full)).toBe("full");
    expect(placeAfterDrag(1100, -0.8, narrow, full)).toBe("narrow");
    expect(placeAfterDrag(560, -0.8, narrow, full)).toBe("closed");
    // A slow drag is no flick: the position decides.
    expect(placeAfterDrag(900, 0.2, narrow, full)).toBe("full");
  });
});

describe("the panel's edge and button", () => {
  it("moves between today's width and the whole window from the keyboard", async () => {
    render(<Panel />);
    const edge = await screen.findByRole("separator", { name: "Panel genişliği" });
    expect(edge.getAttribute("aria-valuetext")).toBe("Normal genişlik");
    expect(screen.getByTestId("probe").textContent).toBe("narrow single");

    act(() => {
      fireEvent.keyDown(edge, { key: "ArrowLeft" });
    });
    expect(useAfkPanelSize.getState().full).toBe(true);
    expect(edge.getAttribute("aria-valuetext")).toBe("Tam genişlik");
    expect(edge.getAttribute("aria-valuenow")).toBe("100");
    // happy-dom's window is 1024 px wide: the whole window lays the parts side by side.
    expect(screen.getByTestId("probe").textContent).toBe("full wide");

    act(() => {
      fireEvent.keyDown(edge, { key: "Home" });
    });
    expect(useAfkPanelSize.getState().full).toBe(false);
    act(() => {
      fireEvent.keyDown(edge, { key: "End" });
    });
    expect(useAfkPanelSize.getState().full).toBe(true);
    act(() => {
      fireEvent.keyDown(edge, { key: "Enter" });
    });
    expect(useAfkPanelSize.getState().full).toBe(false);
    // The arrows move between the two sizes: closing stays Esc's.
    const closed = vi.fn();
    cleanup();
    render(<Panel onClose={closed} />);
    const again = await screen.findByRole("separator");
    act(() => {
      fireEvent.keyDown(again, { key: "ArrowRight" });
    });
    expect(useAfkPanelSize.getState().full).toBe(false);
    expect(closed).not.toHaveBeenCalled();
  });

  it("says what its button does and does it", async () => {
    render(<Panel />);
    const widen = await screen.findByRole("button", { name: "Paneli genişlet" });
    act(() => {
      fireEvent.click(widen);
    });
    expect(useAfkPanelSize.getState().full).toBe(true);
    const narrow = screen.getByRole("button", { name: "Paneli daralt" });
    act(() => {
      fireEvent.click(narrow);
    });
    expect(useAfkPanelSize.getState().full).toBe(false);
  });

  it("opens at the size the player left it", async () => {
    useAfkPanelSize.setState({ full: true });
    render(<Panel />);
    const edge = await screen.findByRole("separator");
    expect(edge.getAttribute("aria-valuetext")).toBe("Tam genişlik");
    expect(screen.getByRole("button", { name: "Paneli daralt" })).toBeTruthy();
    const stored = JSON.parse(localStorage.getItem("pl.afkPanel") ?? "{}");
    expect(stored.state?.full).toBe(true);
  });
});
