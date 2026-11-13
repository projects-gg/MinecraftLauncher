// @vitest-environment happy-dom
// The stage (SQOL-U1). "Yayın bitti.": a watched share whose sharer stopped keeps its place for two
// seconds (in the corner when it was there); "İzlemeyi Bırak", watching moving to the other window
// and leaving the room show nothing. Esc in full screen leaves it only when nothing above took it.
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { shareApi } from "@/share/api";
import type { VoiceLiveShare, VoiceWatch } from "@/share/types";
import { ENDED_MS, stagePlace, useEndedShares, useEscLeavesFull } from "./stage";
import { useShareUi } from "./store";

const live = (person: string): VoiceLiveShare => ({
  person,
  n: 1,
  kind: "game",
  audio: true,
  preset: "normal",
  fps: 30,
  watchers: 1,
  since: 1,
});

const watch = (person: string): VoiceWatch => ({ person, n: 1, layer: "f", phase: "live" });

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

function rig(first: { watches: VoiceWatch[]; live: VoiceLiveShare[]; on: boolean }) {
  return renderHook((props) => useEndedShares(props.watches, props.live, props.on), {
    initialProps: first,
  });
}

describe("the Yayın bitti card", () => {
  it("stays two seconds where a share that ended was", () => {
    const hook = rig({ watches: [watch("301")], live: [live("301")], on: true });
    hook.rerender({ watches: [], live: [], on: true });
    expect(hook.result.current.map((e) => e.key)).toEqual(["301:1"]);
    act(() => {
      vi.advanceTimersByTime(ENDED_MS + 10);
    });
    expect(hook.result.current).toEqual([]);
  });

  it("is not there after İzlemeyi Bırak or a move to the other window (the share goes on)", () => {
    const hook = rig({ watches: [watch("301")], live: [live("301")], on: true });
    hook.rerender({ watches: [], live: [live("301")], on: true });
    expect(hook.result.current).toEqual([]);
  });

  it("is not there when the player leaves the room", () => {
    const hook = rig({ watches: [watch("301")], live: [live("301")], on: true });
    hook.rerender({ watches: [], live: [], on: false });
    expect(hook.result.current).toEqual([]);
  });

  it("goes when the same share is watched again", () => {
    const hook = rig({ watches: [watch("301")], live: [live("301")], on: true });
    hook.rerender({ watches: [], live: [], on: true });
    hook.rerender({ watches: [watch("301")], live: [live("301")], on: true });
    expect(hook.result.current).toEqual([]);
  });
});

describe("where the stage shows", () => {
  const none = { watches: 0, ended: false, own: false, small: false, full: false };

  it("is not there with nothing to show", () => {
    expect(stagePlace(none)).toEqual({ mode: null, corner: null });
  });

  it("keeps a share that ended in the corner it was in, not over the page", () => {
    expect(stagePlace({ ...none, watches: 1, small: true })).toEqual({
      mode: "corner",
      corner: "watch",
    });
    expect(stagePlace({ ...none, ended: true, small: true })).toEqual({
      mode: "corner",
      corner: "ended",
    });
  });

  it("is on the page otherwise, and over everything in full screen", () => {
    expect(stagePlace({ ...none, ended: true })).toEqual({ mode: "page", corner: null });
    expect(stagePlace({ ...none, own: true, small: true })).toEqual({ mode: "page", corner: null });
    expect(stagePlace({ ...none, watches: 2, small: true, full: true })).toEqual({
      mode: "full",
      corner: null,
    });
  });
});

describe("Esc in full screen", () => {
  beforeEach(() => {
    vi.spyOn(shareApi, "fullscreen").mockResolvedValue();
    useShareUi.setState({ full: true });
  });
  afterEach(() => {
    vi.restoreAllMocks();
    useShareUi.setState({ full: false });
  });

  const esc = (on: EventTarget) =>
    on.dispatchEvent(
      new KeyboardEvent("keydown", { key: "Escape", bubbles: true, cancelable: true }),
    );

  it("leaves it when nothing above took the key", () => {
    const hook = renderHook(() => useEscLeavesFull(true));
    esc(document.body);
    act(() => vi.runAllTimers());
    hook.unmount();
    expect(useShareUi.getState().full).toBe(false);
  });

  it("stays when the room chat's previews took it, even heard after the stage", () => {
    const hook = renderHook(() => useEscLeavesFull(true));
    const take = (event: Event) => event.preventDefault();
    window.addEventListener("keydown", take);
    esc(document.body);
    act(() => vi.runAllTimers());
    window.removeEventListener("keydown", take);
    hook.unmount();
    expect(useShareUi.getState().full).toBe(true);
  });

  it("stays when the key belongs to a field", () => {
    const hook = renderHook(() => useEscLeavesFull(true));
    const field = document.body.appendChild(document.createElement("input"));
    esc(field);
    act(() => vi.runAllTimers());
    field.remove();
    hook.unmount();
    expect(useShareUi.getState().full).toBe(true);
  });
});
