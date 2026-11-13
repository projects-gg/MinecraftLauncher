// @vitest-environment happy-dom
// The stage's tiles (SQOL-U1). The player's own tile (sqol.md §1.2): what this computer really
// sends shows on its bar whenever the bar shows (the pointer over the tile, the keys on it),
// "Yayın Bilgileri" or not. A watched share paused or minimized stops its frames by design: no
// "Düşük Kalite" then, nor in the moment it comes back.
import { TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import type { ShareState, VoiceLiveShare, VoiceWatch } from "@/share/types";
import { CHANGE_CALM_MS } from "@/share/watch";
import { useShareUi } from "./store";
import { OwnTile, StreamTile } from "./stream-tile";

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

describe("a watched share's tile on a weak connection", () => {
  afterEach(() => {
    vi.useRealTimers();
    window.history.replaceState(null, "", "/");
  });

  it("says Düşük Kalite only while its share is live, and a moment after it comes back", () => {
    vi.useFakeTimers();
    // The preview's switch: this share's connection reads weak.
    window.history.replaceState(null, "", "/?weak=301");
    const watch: VoiceWatch = { person: "301", n: 1, layer: "f", phase: "live" };
    const live = (st?: ShareState): VoiceLiveShare => ({
      person: "301",
      n: 1,
      kind: "game",
      audio: true,
      preset: "normal",
      fps: 30,
      watchers: 1,
      since: 1,
      ...(st ? { st } : {}),
    });
    const tile = (st?: ShareState) => (
      <TooltipProvider>
        <StreamTile watch={watch} live={live(st)} full={false} idle={false} two={false} first />
      </TooltipProvider>
    );
    const chip = () => screen.queryByText("Düşük Kalite");
    const view = render(tile());
    expect(chip()).not.toBeNull();
    view.rerender(tile("hold"));
    expect(chip()).toBeNull();
    view.rerender(tile("min"));
    expect(chip()).toBeNull();
    view.rerender(tile());
    expect(chip()).toBeNull();
    act(() => vi.advanceTimersByTime(CHANGE_CALM_MS));
    expect(chip()).not.toBeNull();
  });
});
