import { afterEach, describe, expect, it, vi } from "vitest";
import { windowAway } from "./attention";
import { type AwayWindow, refreshWindowAway, trackWindowAway } from "./window-away";

vi.mock("@tauri-apps/plugin-notification", () => ({
  isPermissionGranted: vi.fn(),
  requestPermission: vi.fn(),
  sendNotification: vi.fn(),
}));

const pageFocus = (focused: boolean) => vi.stubGlobal("document", { hasFocus: () => focused });

describe("windowAway", () => {
  afterEach(() => {
    vi.unstubAllGlobals();
  });

  it("is in front while the window is shown and the page has the focus", () => {
    pageFocus(true);
    expect(windowAway()).toBe(false);
  });

  it("is away behind another window, where the page loses the focus", () => {
    pageFocus(false);
    expect(windowAway()).toBe(true);
  });

  it("is away in the tray even while the page still holds the focus", async () => {
    pageFocus(true);
    const win: AwayWindow = {
      isVisible: async () => false,
      isMinimized: async () => false,
      onFocusChanged: async () => () => {},
      onResized: async () => () => {},
    };
    const stop = trackWindowAway(win);
    await refreshWindowAway();
    expect(windowAway()).toBe(true);
    stop();
  });
});
