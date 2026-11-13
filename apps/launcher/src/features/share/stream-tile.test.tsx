// @vitest-environment happy-dom
// The player's own tile (SQOL-U1, sqol.md §1.2): what this computer really sends shows on its bar
// whenever the bar shows (the pointer over the tile, the keys on it), "Yayın Bilgileri" or not.
import { TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { useShareUi } from "./store";
import { OwnTile } from "./stream-tile";

const { SENDS } = vi.hoisted(() => ({ SENDS: "1226×720 · 30 fps · Donanım" }));

vi.mock("./stream-seams", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./stream-seams")>()),
  useOwnPreviewFrame: () => null,
  useOwnShareItems: () => [],
  useOwnShareInfo: () => SENDS,
}));

afterEach(() => {
  cleanup();
  useShareUi.setState({ info: false });
});

/** The own tile on the page; `shown` says whether its bar's words show now. */
function ownTile() {
  render(
    <TooltipProvider>
      <OwnTile n={1} full={false} idle={false} onClose={() => {}} />
    </TooltipProvider>,
  );
  const line = screen.getByText(SENDS);
  const tile = line.closest("section");
  if (!tile) throw new Error("no tile");
  const shown = () => line.parentElement?.classList.contains("opacity-100") ?? false;
  return { tile, shown };
}

describe("the own tile's bar", () => {
  it("says what this computer sends while the pointer is over the tile", () => {
    const { tile, shown } = ownTile();
    expect(useShareUi.getState().info).toBe(false);
    expect(shown()).toBe(false);
    fireEvent.pointerEnter(tile, { pointerType: "mouse" });
    expect(shown()).toBe(true);
    fireEvent.pointerLeave(tile, { pointerType: "mouse" });
    expect(shown()).toBe(false);
  });

  it("says it while the keys bring the focus to the tile", () => {
    const { tile, shown } = ownTile();
    fireEvent.keyDown(document.body, { key: "Tab" });
    act(() => tile.focus());
    expect(shown()).toBe(true);
  });
});
