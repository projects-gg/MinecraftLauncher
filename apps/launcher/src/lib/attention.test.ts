import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { usePrefs } from "@/stores/prefs";
import { notify, showNotification, windowAway } from "./attention";
import { type AwayWindow, refreshWindowAway, trackWindowAway } from "./window-away";

const desktopNotify = vi.fn();
vi.mock("@/bindings", () => ({
  commands: { desktopNotify: (...args: unknown[]) => desktopNotify(...args) },
}));
type Click = (event: { payload: number }) => void;
const clicks: { listener?: Click } = {};
vi.mock("@tauri-apps/api/event", () => ({
  listen: async (_event: string, listener: Click) => {
    clicks.listener = listener;
    return () => {};
  },
}));
vi.mock("./platform", () => ({ isTauri: () => true }));

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

describe("system notifications", () => {
  beforeEach(() => {
    desktopNotify.mockReset();
    usePrefs.setState({ desktopNotifications: true });
  });

  const idOf = (call: number) => desktopNotify.mock.calls[call]?.[2] as number;

  it("a click opens what its own notification was about, once", async () => {
    desktopNotify.mockResolvedValue(true);
    const chat = vi.fn();
    const crew = vi.fn();
    expect(await showNotification("Ayşe", "Akşam Bedwars?", chat)).toBe(true);
    expect(await showNotification("Ekibin seni çağırıyor", undefined, crew)).toBe(true);
    expect(desktopNotify.mock.calls[0]).toEqual(["Ayşe", "Akşam Bedwars?", idOf(0)]);
    expect(desktopNotify.mock.calls[1]).toEqual(["Ekibin seni çağırıyor", null, idOf(1)]);
    expect(idOf(0)).not.toBe(idOf(1));

    clicks.listener?.({ payload: idOf(1) });
    clicks.listener?.({ payload: idOf(1) });
    expect(crew).toHaveBeenCalledTimes(1);
    expect(chat).not.toHaveBeenCalled();
  });

  it("keeps nothing for a notification the system did not take", async () => {
    const chat = vi.fn();
    desktopNotify.mockResolvedValue(false);
    expect(await showNotification("Ayşe", "selam", chat)).toBe(false);
    desktopNotify.mockRejectedValue(new Error("gone"));
    expect(await showNotification("Ayşe", "selam", chat)).toBe(false);
    clicks.listener?.({ payload: idOf(0) });
    clicks.listener?.({ payload: idOf(1) });
    expect(chat).not.toHaveBeenCalled();
  });

  it("follows the player's choice in Settings", async () => {
    desktopNotify.mockResolvedValue(true);
    usePrefs.setState({ desktopNotifications: false });
    await notify("Kurulum bitti", "Projects hazır");
    expect(desktopNotify).not.toHaveBeenCalled();
    usePrefs.setState({ desktopNotifications: true });
    await notify("Kurulum bitti", "Projects hazır");
    expect(desktopNotify).toHaveBeenCalledTimes(1);
  });
});
