// @vitest-environment happy-dom
// The stage (SQOL-U1). "Yayın bitti.": a watched share whose sharer stopped keeps its place for two
// seconds (in the corner when it was there); "İzlemeyi Bırak", watching moving to the other window
// and leaving the room show nothing. Esc in full screen leaves it only when nothing above took it.
// "Hâlâ izliyor musun?" centres on the stage the docked room panel leaves visible.
import { setAppReducedMotion, TooltipProvider } from "@pl/ui";
import { act, cleanup, render, renderHook, screen } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { useVoiceUi } from "@/features/voice/store";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { HIDDEN_VOICE_STATE } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import type { VoiceLiveShare, VoiceWatch } from "@/share/types";
import { ENDED_MS, StageBody, stagePlace, useEndedShares, useEscLeavesFull } from "./stage";
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

  it("stays when the key belongs to a field (a menu's, the room chat's)", () => {
    const hook = renderHook(() => useEscLeavesFull(true));
    const field = document.body.appendChild(document.createElement("input"));
    esc(field);
    act(() => vi.runAllTimers());
    field.remove();
    hook.unmount();
    expect(useShareUi.getState().full).toBe(true);
  });
});

describe("Hâlâ izliyor musun? with the room panel docked", () => {
  // The card fades. happy-dom leaves a cancelled animation's `finished` rejection unhandled (a
  // browser marks it handled): here it is handled (live-card.test.tsx).
  const animate = Element.prototype.animate;
  beforeAll(() => {
    setAppReducedMotion(true);
    Element.prototype.animate = function handledAnimate(
      this: Element,
      keyframes: Keyframe[] | PropertyIndexedKeyframes | null,
      options?: number | KeyframeAnimationOptions,
    ) {
      const animation = animate.call(this, keyframes, options);
      const cancel = animation.cancel;
      animation.cancel = function handledCancel(this: Animation) {
        this.finished.catch(() => undefined);
        cancel.call(this);
      };
      return animation;
    };
  });
  afterAll(() => {
    setAppReducedMotion(false);
    Element.prototype.animate = animate;
  });
  afterEach(() => {
    cleanup();
    act(() => useVoiceUi.getState().setState(HIDDEN_VOICE_STATE));
  });

  /** The page stage asking whether the player still watches, the room panel `dock` px wide. */
  function asking(dock: number) {
    const state = createVoiceMock({ scenario: "shareLive", timers: false }).current();
    const share = state.share;
    if (!share) throw new Error("no share");
    act(() => useVoiceUi.getState().setState({ ...state, share: { ...share, stillAsking: true } }));
    render(
      <TooltipProvider>
        <StageBody watches={[]} popout={false} dock={dock} />
      </TooltipProvider>,
    );
    return screen.getByRole("alertdialog");
  }

  it("covers and centres on the stage the panel leaves visible", () => {
    expect(asking(320).style.right).toBe("320px");
  });

  it("covers the whole stage without the panel", () => {
    expect(asking(0).style.right).toBe("");
  });
});
