// @vitest-environment happy-dom
// "Yayın bitti." (SQOL-U1): a watched share whose sharer stopped keeps its place for two seconds;
// "İzlemeyi Bırak", watching moving to the other window and leaving the room show nothing.
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { VoiceLiveShare, VoiceWatch } from "@/share/types";
import { ENDED_MS, useEndedShares } from "./stage";

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
