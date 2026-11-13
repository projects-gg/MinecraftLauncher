// @vitest-environment happy-dom
// SQOL-U3: the room panel opened from the stage ("Sohbet", a preview's press), over its full
// screen too.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useShareStage, useShareUi } from "@/features/share/store";
import { useVoiceUi } from "../store";
import { openRoomPanel } from "./unread";

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
  useVoiceUi.setState({ panel: false });
  useShareUi.setState({ full: false });
  useShareStage.setState({ mode: null });
});

describe("openRoomPanel", () => {
  it("brings a full-screen stage back to the page first, then opens the panel on its chat", () => {
    useShareUi.setState({ full: true });
    useShareStage.setState({ mode: "full" });
    const asked = useVoiceUi.getState().chatFocus;
    openRoomPanel({ focus: "chat" });
    // SQOL-U1's own way out of full screen: the panel then docks beside the stage.
    expect(useShareUi.getState().full).toBe(false);
    vi.runAllTimers();
    expect(useVoiceUi.getState().panel).toBe(true);
    expect(useVoiceUi.getState().chatFocus).toBe(asked + 1);
  });

  it("leaves a stage on the page as it is", () => {
    useShareStage.setState({ mode: "page" });
    const before = useShareUi.getState();
    openRoomPanel({ focus: "chat" });
    vi.runAllTimers();
    expect(useShareUi.getState()).toBe(before);
    expect(useVoiceUi.getState().panel).toBe(true);
  });
});
