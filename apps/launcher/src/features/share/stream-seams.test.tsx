// @vitest-environment happy-dom
// The own tile's live picture (SQOL-U1): asked only while someone can see it, as its card asks: not
// with the window in the tray or hidden (the stage stays mounted there), not in Performance mode,
// not while the share is paused, minimized, starting or lost its source.
import { act, renderHook } from "@testing-library/react";
import { beforeEach, describe, expect, it, vi } from "vitest";
import { useVoiceUi } from "@/features/voice/store";
import { createVoiceMock } from "@/lib/api/voice-mock";
import type { VoiceMyShare } from "@/share/types";
import { markLost, resetOwnUi } from "./own-store";
import { useOwnPreviewFrame } from "./stream-seams";

const { perf, asked } = vi.hoisted(() => ({
  perf: { performanceMode: false, unseen: false },
  asked: [] as boolean[],
}));
vi.mock("@pl/ui", async (importOriginal) => ({
  ...(await importOriginal<typeof import("@pl/ui")>()),
  usePerf: () => perf,
}));
vi.mock("./own-live", async (importOriginal) => ({
  ...(await importOriginal<typeof import("./own-live")>()),
  useOwnSharePreview: (_n: number, on: boolean) => {
    asked.push(on);
    return null;
  },
}));

/** The player shares the game (share 1), with `patch` on it. */
function sharing(patch: Partial<VoiceMyShare> = {}) {
  const state = createVoiceMock({ scenario: "shareLive", timers: false }).current();
  const share = state.share;
  if (!share) throw new Error("no share");
  act(() => {
    useVoiceUi.getState().setState({
      ...state,
      share: { ...share, mine: share.mine.map((m) => ({ ...m, ...patch })) },
    });
  });
}

/** Whether the own tile of share 1 asks for live frames now. */
function asks(): boolean | undefined {
  asked.length = 0;
  const hook = renderHook(() => useOwnPreviewFrame(1));
  const last = asked.at(-1);
  hook.unmount();
  return last;
}

beforeEach(() => {
  perf.performanceMode = false;
  perf.unseen = false;
  resetOwnUi();
});

describe("the own tile's live picture", () => {
  it("is asked for while the window shows a live share", () => {
    sharing();
    expect(asks()).toBe(true);
  });

  it("is not asked for with the window in the tray or hidden", () => {
    sharing();
    perf.unseen = true;
    expect(asks()).toBe(false);
  });

  it("is not asked for in Performance mode", () => {
    sharing();
    perf.performanceMode = true;
    expect(asks()).toBe(false);
  });

  it("is not asked for while the share is paused, minimized or starting", () => {
    sharing({ st: "hold" });
    expect(asks()).toBe(false);
    sharing({ st: "min" });
    expect(asks()).toBe(false);
    sharing({ phase: "starting" });
    expect(asks()).toBe(false);
  });

  it("is not asked for once a notice said its window went away", () => {
    sharing();
    act(() => markLost(1, "closed", null));
    expect(asks()).toBe(false);
  });
});
