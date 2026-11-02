// @vitest-environment happy-dom
// The title bar's room (VOICE-L3): nothing without a room or a call; "Aranıyor…" with "İptal" while
// the player's call rings; "Cevap yok" with "Tekrar ara" after it; the room with "Mikrofon" and
// "Ayrıl", whose names say what a press does; a moderator's mute holds the microphone.
import { setAppReducedMotion, TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { voiceApi } from "@/lib/api/voice-api";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { HIDDEN_VOICE_STATE, type VoiceState } from "@/lib/api/voice-types";
import { VoicePill } from "./pill";
import { useVoiceUi } from "./store";

// The pill fades. happy-dom leaves a cancelled animation's `finished` rejection unhandled (a
// browser marks it handled), so a fade cut short by the cleanup failed the run; here it is handled
// (as in rule-assist.test.tsx). Motion reduced keeps the fades only; the words are the same.
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
  vi.restoreAllMocks();
  useVoiceUi.setState({ state: HIDDEN_VOICE_STATE, ended: null, speaking: [], meTalking: false });
});

const scenario = (name: string): VoiceState =>
  createVoiceMock({ scenario: name, timers: false }).current();

function show(state: VoiceState) {
  act(() => useVoiceUi.getState().setState(state));
  return render(
    <TooltipProvider>
      <VoicePill />
    </TooltipProvider>,
  );
}

describe("the voice pill", () => {
  it("shows nothing without a room or a call", () => {
    const { container } = show(scenario("ready"));
    expect(container.textContent).toBe("");
  });

  it("says the call rings, and İptal ends it", () => {
    const leave = vi.spyOn(voiceApi, "leave").mockResolvedValue();
    show(scenario("calling"));
    expect(screen.getByText("Aranıyor…")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Aramayı iptal et" }));
    expect(leave).toHaveBeenCalledOnce();
  });

  it("keeps an unanswered call as Cevap yok, with Tekrar ara", () => {
    const call = vi.spyOn(voiceApi, "call").mockResolvedValue({
      refused: null,
      retryAfter: null,
      until: null,
    });
    act(() => useVoiceUi.getState().setEnded({ kind: "noAnswer", friend: "302" }));
    show(scenario("ready"));
    expect(screen.getByText("Cevap yok")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Tekrar ara" }));
    expect(call).toHaveBeenCalledWith("302");
  });

  it("names the microphone's button by what a press does", () => {
    show(scenario("room"));
    expect(screen.getByRole("button", { name: "Mikrofonu kapat" })).toBeTruthy();
    expect(screen.getByRole("button", { name: "Ayrıl" })).toBeTruthy();
    cleanup();
    show(scenario("muted"));
    expect(screen.getByRole("button", { name: "Mikrofonu aç" })).toBeTruthy();
  });

  it("holds the microphone while a moderator's mute runs", () => {
    const mute = vi.spyOn(voiceApi, "setMute").mockResolvedValue();
    show(scenario("modMuted"));
    const button = screen.getByRole("button", { name: "Bir moderatör seni susturdu." });
    expect(button.getAttribute("aria-disabled")).toBe("true");
    fireEvent.click(button);
    expect(mute).not.toHaveBeenCalled();
  });

  it("says a weak connection in the room's name", () => {
    show(scenario("weak"));
    expect(screen.getByRole("button", { name: /Bağlantı zayıf/ })).toBeTruthy();
  });
});
