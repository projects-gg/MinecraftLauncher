// @vitest-environment happy-dom
// The layer a watched share's tile asks for (SQOL-U1): what it last asked for decides, never the
// launcher's report alone (E2E: after "Veri Tasarrufu", "Yüksek" and "Otomatik" met a stale report
// of the full layer and asked nothing; the reverse stuck too). A report that does not follow brings
// one more ask after a settle.
import { act, renderHook } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { useVoiceUi } from "@/features/voice/store";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { shareApi } from "@/share/api";
import type {
  ShareKind,
  ShareLayer,
  ShareState,
  ShareViewQuality,
  VoiceLiveShare,
  VoiceWatch,
} from "@/share/types";
import { CHANGE_CALM_MS, type WatchEngine } from "@/share/watch";
import { setPageEngine } from "./store";
import {
  forgetAskedLayers,
  LAYER_SETTLE_MS,
  useShareChangeCalm,
  useShareSettled,
  useWatchLayer,
} from "./stream-layer";
import { layerFor, WEAK_BACK_MS } from "./stream-view";

/** The layer a large tile (900 px) wants under a "Yayın Kalitesi" choice. */
const wants = (quality: ShareViewQuality): ShareLayer =>
  layerFor({ quality, width: 900, weakHeld: false, zoomed: false, hasSmall: true });

const watch = (layer: ShareLayer, phase: VoiceWatch["phase"] = "live"): VoiceWatch => ({
  person: "301",
  n: 1,
  layer,
  phase,
});

interface Props {
  watch: VoiceWatch;
  wanted: ShareLayer;
  measured?: boolean;
}

const tile = (first: Props) =>
  renderHook((p: Props) => useWatchLayer(p.watch, p.wanted, p.measured ?? true), {
    initialProps: first,
  });

let asked: ShareLayer[] = [];

beforeEach(() => {
  vi.useFakeTimers();
  forgetAskedLayers();
  asked = [];
  vi.spyOn(shareApi, "layer").mockImplementation(async (_person, _n, layer) => {
    asked.push(layer);
  });
});

afterEach(() => {
  vi.useRealTimers();
  vi.restoreAllMocks();
});

describe("the layer a tile asks for", () => {
  it("asks for Yüksek after Veri Tasarrufu while the launcher still reports the full layer", () => {
    const hook = tile({ watch: watch("f"), wanted: wants("auto") });
    expect(asked).toEqual([]);
    hook.rerender({ watch: watch("f"), wanted: wants("saver") });
    expect(asked).toEqual(["q"]);
    hook.rerender({ watch: watch("f"), wanted: wants("high") });
    expect(asked).toEqual(["q", "f"]);
  });

  it("asks for Otomatik's full layer on a large tile after Veri Tasarrufu", () => {
    const hook = tile({ watch: watch("f"), wanted: wants("saver") });
    expect(asked).toEqual(["q"]);
    hook.rerender({ watch: watch("f"), wanted: wants("auto") });
    expect(asked).toEqual(["q", "f"]);
  });

  it("asks for Veri Tasarrufu after Yüksek while the launcher still reports the small layer", () => {
    const hook = tile({ watch: watch("q"), wanted: wants("high") });
    expect(asked).toEqual(["f"]);
    hook.rerender({ watch: watch("q"), wanted: wants("saver") });
    expect(asked).toEqual(["f", "q"]);
  });

  it("asks for the full layer again when a weak connection's hold ends under a stale report", () => {
    const auto = (weakHeld: boolean) =>
      layerFor({ quality: "auto", width: 900, weakHeld, zoomed: false, hasSmall: true });
    const hook = tile({ watch: watch("f"), wanted: auto(false) });
    hook.rerender({ watch: watch("f"), wanted: auto(true) });
    expect(asked).toEqual(["q"]);
    act(() => vi.advanceTimersByTime(WEAK_BACK_MS));
    // The hold ends; the launcher still says the full layer (it never said the small one).
    hook.rerender({ watch: watch("f"), wanted: auto(false) });
    expect(asked).toEqual(["q", "q", "f"]);
  });

  it("asks once more when the report does not follow, then waits", () => {
    tile({ watch: watch("f"), wanted: "q" });
    expect(asked).toEqual(["q"]);
    act(() => vi.advanceTimersByTime(LAYER_SETTLE_MS));
    expect(asked).toEqual(["q", "q"]);
    act(() => vi.advanceTimersByTime(LAYER_SETTLE_MS * 10));
    expect(asked).toEqual(["q", "q"]);
  });

  it("asks no more once the report follows", () => {
    const hook = tile({ watch: watch("f"), wanted: "q" });
    hook.rerender({ watch: watch("q"), wanted: "q" });
    act(() => vi.advanceTimersByTime(LAYER_SETTLE_MS * 2));
    expect(asked).toEqual(["q"]);
  });

  it("waits while the watch is held and until the tile has its size", () => {
    const hook = tile({ watch: watch("f", "held"), wanted: "q" });
    hook.rerender({ watch: watch("f"), wanted: "q", measured: false });
    act(() => vi.advanceTimersByTime(LAYER_SETTLE_MS * 2));
    expect(asked).toEqual([]);
    hook.rerender({ watch: watch("f"), wanted: "q", measured: true });
    expect(asked).toEqual(["q"]);
  });

  it("remembers its ask past its tile (the corner and the page are two)", () => {
    tile({ watch: watch("f"), wanted: "q" }).unmount();
    const corner = tile({ watch: watch("f"), wanted: "q" });
    expect(asked).toEqual(["q"]);
    corner.rerender({ watch: watch("f"), wanted: "f" });
    expect(asked).toEqual(["q", "f"]);
  });

  it("forgets its ask once the watch ends", () => {
    const state = createVoiceMock({ scenario: "shareLive", timers: false }).current();
    const share = state.share;
    if (!share) throw new Error("no share");
    act(() =>
      useVoiceUi.getState().setState({ ...state, share: { ...share, watching: [watch("f")] } }),
    );
    tile({ watch: watch("f"), wanted: "q" }).unmount();
    act(() => useVoiceUi.getState().setState({ ...state, share: { ...share, watching: [] } }));
    // Watched again, on the full layer: the small one is asked anew.
    tile({ watch: watch("f"), wanted: "q" });
    expect(asked).toEqual(["q", "q"]);
  });
});

describe("a picture that changes on purpose", () => {
  const calm = vi.fn();

  beforeEach(() => {
    calm.mockClear();
    setPageEngine({ calm } as unknown as WatchEngine);
  });
  afterEach(() => setPageEngine(null));

  it("is no weak connection when the tile asks for another layer", () => {
    tile({ watch: watch("f"), wanted: "q" });
    expect(calm).toHaveBeenCalledWith("301", 1);
  });

  it("is no weak connection when its sharer pauses it, brings it back or shows another source", () => {
    type Live = Pick<VoiceLiveShare, "st" | "kind" | "fps" | "preset">;
    const live = (kind: ShareKind, st?: ShareState, fps = 30): Live =>
      st ? { kind, st, fps, preset: "normal" } : { kind, fps, preset: "normal" };
    const hook = renderHook((p: { live: Live }) => useShareChangeCalm("301", 1, p.live), {
      initialProps: { live: live("game") },
    });
    expect(calm).not.toHaveBeenCalled();
    hook.rerender({ live: live("game", "hold") });
    hook.rerender({ live: live("game") });
    hook.rerender({ live: live("window") });
    expect(calm).toHaveBeenCalledTimes(3);
    hook.rerender({ live: live("window", "live") });
    expect(calm).toHaveBeenCalledTimes(3);
    // E2E: a step to 15 fps, then another quality.
    hook.rerender({ live: live("window", "live", 15) });
    expect(calm).toHaveBeenCalledTimes(4);
    hook.rerender({ live: { ...live("window", "live", 15), preset: "high" } });
    expect(calm).toHaveBeenCalledTimes(5);
  });
});

describe("a share paused or minimized", () => {
  it("says nothing of the connection while stopped, nor for a moment after it came back", () => {
    const hook = renderHook((p: { st?: ShareState }) => useShareSettled(p.st ? { st: p.st } : {}), {
      initialProps: {} as { st?: ShareState },
    });
    expect(hook.result.current).toBe(true);
    hook.rerender({ st: "hold" });
    expect(hook.result.current).toBe(false);
    hook.rerender({ st: "min" });
    expect(hook.result.current).toBe(false);
    hook.rerender({ st: "live" });
    expect(hook.result.current).toBe(false);
    act(() => vi.advanceTimersByTime(CHANGE_CALM_MS - 1));
    expect(hook.result.current).toBe(false);
    act(() => vi.advanceTimersByTime(1));
    expect(hook.result.current).toBe(true);
  });
});
