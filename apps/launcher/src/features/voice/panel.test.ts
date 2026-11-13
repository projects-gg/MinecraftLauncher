// @vitest-environment happy-dom
// SQOL-U3: the room panel's own state: the chat field's focus asked for once, the docked width,
// and where the single-key voice shortcuts rest.
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { DOCK_MAX, DOCK_MIN, dockWidthFor, useToastsBeside } from "./dock";
import { SAY_QUIET_AFTER_MS, SAY_SPEAKING_AFTER_MS, useSpeakingWord } from "./speaking-word";
import { useVoiceUi } from "./store";
import { writesText } from "./typing";

beforeEach(() => {
  vi.useFakeTimers();
  useVoiceUi.setState({
    panel: false,
    page: "room",
    returnFocus: null,
    chatFocus: 0,
    chatFocusTaken: 0,
    dock: 0,
  });
});

afterEach(() => {
  vi.useRealTimers();
});

describe("the chat field's focus", () => {
  it("is asked for by an opening for the chat, and taken once", () => {
    useVoiceUi.getState().openPanel({ focus: "chat" });
    vi.runAllTimers();
    const s = useVoiceUi.getState();
    expect(s.panel).toBe(true);
    expect(s.takeChatFocus()).toBe(true);
    expect(useVoiceUi.getState().takeChatFocus()).toBe(false);
  });

  it("is not asked for by a plain opening", () => {
    useVoiceUi.getState().openPanel();
    vi.runAllTimers();
    expect(useVoiceUi.getState().takeChatFocus()).toBe(false);
  });

  it("goes with the panel when nothing took it", () => {
    useVoiceUi.getState().openPanel({ focus: "chat" });
    vi.runAllTimers();
    useVoiceUi.getState().closePanel();
    expect(useVoiceUi.getState().takeChatFocus()).toBe(false);
  });

  it("opened again while open keeps the page and where the focus goes back", () => {
    const from = document.createElement("button");
    useVoiceUi.getState().openPanel({ from });
    vi.runAllTimers();
    useVoiceUi.setState({ page: "invite" });
    useVoiceUi.getState().openPanel();
    vi.runAllTimers();
    expect(useVoiceUi.getState().page).toBe("invite");
    expect(useVoiceUi.getState().returnFocus).toBe(from);
    // Asked for the chat, it shows the room again.
    useVoiceUi.getState().openPanel({ focus: "chat" });
    vi.runAllTimers();
    expect(useVoiceUi.getState().page).toBe("room");
  });
});

describe("the docked width", () => {
  it("is a third of the window, between 360 and 440 px", () => {
    expect(dockWidthFor(1024)).toBe(DOCK_MIN);
    expect(dockWidthFor(1280)).toBe(384);
    expect(dockWidthFor(2560)).toBe(DOCK_MAX);
  });

  it("changes the store only when it changes", () => {
    const seen: number[] = [];
    const stop = useVoiceUi.subscribe((s) => seen.push(s.dock));
    useVoiceUi.getState().setDock(384);
    useVoiceUi.getState().setDock(384);
    useVoiceUi.getState().setDock(0);
    stop();
    expect(seen).toEqual([384, 0]);
  });
});

describe("toasts beside the docked panel", () => {
  it("move left of it only while it is docked", () => {
    const rule = () => document.head.querySelector("style[data-room-dock]")?.textContent ?? null;
    const view = renderHook(({ px }) => useToastsBeside(px), { initialProps: { px: 0 } });
    expect(rule()).toBeNull();
    view.rerender({ px: 384 });
    expect(rule()).toContain("calc(var(--offset-right) + 384px)");
    view.rerender({ px: 0 });
    expect(rule()).toBeNull();
    view.unmount();
  });
});

describe("useSpeakingWord", () => {
  it("says someone speaks after a moment of speech, and stops after a moment of quiet", () => {
    useVoiceUi.setState({ speaking: [] });
    const view = renderHook(() => useSpeakingWord("301"));
    expect(view.result.current).toBe(false);
    act(() => useVoiceUi.setState({ speaking: ["301"] }));
    act(() => vi.advanceTimersByTime(SAY_SPEAKING_AFTER_MS - 1));
    expect(view.result.current).toBe(false);
    act(() => vi.advanceTimersByTime(1));
    expect(view.result.current).toBe(true);
    // A breath between words changes nothing.
    act(() => useVoiceUi.setState({ speaking: [] }));
    act(() => vi.advanceTimersByTime(SAY_QUIET_AFTER_MS / 2));
    act(() => useVoiceUi.setState({ speaking: ["301"] }));
    act(() => vi.advanceTimersByTime(SAY_QUIET_AFTER_MS));
    expect(view.result.current).toBe(true);
    act(() => useVoiceUi.setState({ speaking: [] }));
    act(() => vi.advanceTimersByTime(SAY_QUIET_AFTER_MS));
    expect(view.result.current).toBe(false);
    view.unmount();
  });
});

describe("writesText", () => {
  it("knows the fields that take words", () => {
    const area = document.createElement("textarea");
    const text = document.createElement("input");
    const box = document.createElement("input");
    box.type = "checkbox";
    const read = document.createElement("textarea");
    read.readOnly = true;
    expect(writesText(area)).toBe(true);
    expect(writesText(text)).toBe(true);
    expect(writesText(box)).toBe(false);
    expect(writesText(read)).toBe(false);
    expect(writesText(document.createElement("button"))).toBe(false);
    expect(writesText(null)).toBe(false);
  });
});
