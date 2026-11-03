// @vitest-environment happy-dom
// VOICE-CALL: starting a call asks first ("{name} aransın mı?" with "Ara" and "Vazgeç") unless the
// player turned "Aramadan önce sor" off; the friend's card always has its voice button while voice
// chat is on (faded with one plain reason when the friend cannot be called); a call with one friend
// shows its time and its stage; the meter lights as many bars as the voice reaches.
import { setAppReducedMotion, TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { useSocialUi } from "@/features/social/store";
import { voiceApi } from "@/lib/api/voice-api";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { HIDDEN_VOICE_STATE, VOICE_OK, type VoiceState } from "@/lib/api/voice-types";
import { askToCall, CallConfirm } from "./call-confirm";
import { callStart, elapsedText } from "./call-time";
import { FriendCallButton } from "./friend-call";
import { levelNow, MIC, paintLevels, restLevels, usePainter } from "./levels";
import { FULL_RANGE, litBars, SPEECH_RANGE, shortBars } from "./meter";
import { isOneToOne } from "./stage";
import { useVoiceUi } from "./store";

// The dialog fades. happy-dom leaves a cancelled animation's `finished` rejection unhandled (a
// browser marks it handled), so a fade cut short by the cleanup failed the run; here it is handled
// (as in pill.test.tsx).
const animate = Element.prototype.animate;
beforeAll(() => {
  setAppReducedMotion(true);
  Element.prototype.animate = function handledAnimate(
    this: Element,
    keyframes: Keyframe[] | PropertyIndexedKeyframes | null,
    options?: number | KeyframeAnimationOptions,
  ) {
    const animation = animate.call(this, keyframes, options);
    animation.finished.catch(() => undefined);
    return animation;
  };
});
afterAll(() => {
  setAppReducedMotion(false);
  Element.prototype.animate = animate;
});
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useVoiceUi.setState({ state: HIDDEN_VOICE_STATE, ended: null, speaking: [], meTalking: false });
});

const scenario = (name: string, options: { ask?: boolean } = {}): VoiceState =>
  createVoiceMock({ scenario: name, timers: false, ...options }).current();

/** Lets the dialog's delayed opening and its entrance run. */
async function settle() {
  await act(async () => {
    for (let i = 0; i < 5; i++) await new Promise((resolve) => setTimeout(resolve, 0));
  });
}

describe("the call's time", () => {
  it("reads as a clock: minutes and seconds, hours past the first one", () => {
    expect(elapsedText(0)).toBe("00:00");
    expect(elapsedText(252)).toBe("04:12");
    expect(elapsedText(3599.9)).toBe("59:59");
    expect(elapsedText(3600 + 4 * 60 + 12)).toBe("1:04:12");
    expect(elapsedText(-5)).toBe("00:00");
  });

  it("belongs to a call with one friend only", () => {
    expect(callStart(scenario("call1"))).not.toBeNull();
    // Ringing, a group, a crew room: none.
    expect(callStart(scenario("calling"))).toBeNull();
    expect(callStart(scenario("room"))).toBeNull();
    expect(callStart(scenario("crew"))).toBeNull();
  });

  it("puts a call with one friend, or one that rings, on the stage", () => {
    expect(isOneToOne(scenario("call1"))).toBe(true);
    expect(isOneToOne(scenario("calling"))).toBe(true);
    expect(isOneToOne(scenario("room"))).toBe(false);
    expect(isOneToOne(scenario("crew"))).toBe(false);
    expect(isOneToOne(scenario("ready"))).toBe(false);
  });
});

describe("asking before a call", () => {
  it('asks "{name} aransın mı?", and "Ara" calls', async () => {
    const call = vi.spyOn(voiceApi, "call").mockResolvedValue(VOICE_OK);
    act(() => useVoiceUi.getState().setState(scenario("ready")));
    render(
      <TooltipProvider>
        <CallConfirm />
      </TooltipProvider>,
    );
    act(() => askToCall("302"));
    await settle();
    // The friends list does not know 302 here: "Bir oyuncu".
    expect(screen.getByText("Bir oyuncu aransın mı?")).toBeTruthy();
    expect(call).not.toHaveBeenCalled();
    fireEvent.click(screen.getByRole("button", { name: "Ara" }));
    await settle();
    expect(call).toHaveBeenCalledWith("302");
  });

  it('starts on "Vazgeç", which calls nobody (a held Enter never rings)', async () => {
    const call = vi.spyOn(voiceApi, "call").mockResolvedValue(VOICE_OK);
    act(() => useVoiceUi.getState().setState(scenario("ready")));
    render(
      <TooltipProvider>
        <CallConfirm />
      </TooltipProvider>,
    );
    act(() => askToCall("302"));
    await settle();
    const cancel = screen.getByRole("button", { name: "Vazgeç" });
    expect(document.activeElement).toBe(cancel);
    fireEvent.click(cancel);
    await settle();
    expect(call).not.toHaveBeenCalled();
  });

  it('calls at once with "Aramadan önce sor" off', async () => {
    const call = vi.spyOn(voiceApi, "call").mockResolvedValue(VOICE_OK);
    act(() => useVoiceUi.getState().setState(scenario("ready", { ask: false })));
    act(() => askToCall("302"));
    await settle();
    expect(call).toHaveBeenCalledWith("302");
  });
});

describe("the friend's card (VOICE-CALL)", () => {
  function card(state: VoiceState) {
    act(() => useVoiceUi.getState().setState(state));
    return render(
      <TooltipProvider>
        <FriendCallButton friend={{ id: "302" }} onDone={() => {}} />
      </TooltipProvider>,
    );
  }

  it('offers "Sesli arama başlat", asked first', async () => {
    useSocialUi.setState((s) => ({
      view: { ...s.view, friends: [{ ...s.view.friends[0], id: "302", voice: "ready" } as never] },
    }));
    const call = vi.spyOn(voiceApi, "call").mockResolvedValue(VOICE_OK);
    card(scenario("ready"));
    fireEvent.click(screen.getByRole("button", { name: "Sesli arama başlat" }));
    await settle();
    // Asked, not called yet (the dialog is mounted by VoiceHost).
    expect(call).not.toHaveBeenCalled();
  });

  it("keeps the button for a friend who cannot be called now, with a plain reason", () => {
    useSocialUi.setState((s) => ({
      view: {
        ...s.view,
        friends: [{ ...s.view.friends[0], id: "302", voice: undefined } as never],
      },
    }));
    const call = vi.spyOn(voiceApi, "call").mockResolvedValue(VOICE_OK);
    card({ ...scenario("ready"), presence: {} });
    const button = screen.getByRole("button", { name: "Sesli arama başlat" });
    expect(button.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(button);
    expect(call).not.toHaveBeenCalled();
  });

  it("has nothing while voice chat is off for the player", () => {
    const { container } = card(scenario("disabled"));
    expect(container.textContent).toBe("");
  });
});

describe("the meters", () => {
  it("light as many bars as the voice reaches, short under a hand-set sensitivity", () => {
    expect(litBars(0, 5, SPEECH_RANGE)).toBe(0);
    expect(litBars(0.29, 5, SPEECH_RANGE)).toBe(0);
    expect(litBars(0.42, 5, SPEECH_RANGE)).toBe(1);
    expect(litBars(0.9, 5, SPEECH_RANGE)).toBe(5);
    expect(litBars(1, 5, SPEECH_RANGE)).toBe(5);
    expect(litBars(0.5, 24, FULL_RANGE)).toBe(12);
    expect(shortBars(null, 5, SPEECH_RANGE)).toBe(0);
    expect(shortBars(0.6, 5, SPEECH_RANGE)).toBe(2);
    expect(shortBars(0.3, 48, FULL_RANGE)).toBe(14);
  });

  it("paint only what moved, and rest once when the levels stop", () => {
    const seen: Array<[number, boolean]> = [];
    function Probe() {
      usePainter(MIC, (level, open) => seen.push([level, open]));
      return null;
    }
    render(<Probe />);
    // It gets the level now at once.
    expect(seen).toEqual([[0, false]]);
    act(() => paintLevels({ me: 0.6, open: true, people: [{ id: "302", level: 0.7 }] }));
    act(() => paintLevels({ me: 0.6, open: true, people: [] }));
    expect(seen).toEqual([
      [0, false],
      [0.6, true],
    ]);
    expect(levelNow("302").level).toBe(0);
    act(() => restLevels());
    act(() => restLevels());
    expect(seen.at(-1)).toEqual([0, false]);
    expect(seen).toHaveLength(3);
  });
});
