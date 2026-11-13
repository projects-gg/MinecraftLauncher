// @vitest-environment happy-dom
// The picker (SHARE, SQOL "Ekranı Değiştir"): one choice per opening. The closing panel still
// takes the second press of a double click: it must neither switch twice nor start two shares.
import { setAppReducedMotion, TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { useVoiceUi } from "@/features/voice/store";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { HIDDEN_VOICE_STATE } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import { openSwitch, resetOwnUi } from "./own-store";
import { SharePicker } from "./picker";
import { useShareUi } from "./store";

// The dialog fades out as a choice closes it. happy-dom leaves a cancelled animation's `finished`
// rejection unhandled (a browser marks it handled): here it is handled (live-card.test.tsx).
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
  useVoiceUi.setState({ state: HIDDEN_VOICE_STATE });
  useShareUi.setState({ picker: false });
  resetOwnUi();
});

const enter = (scenario: string) =>
  act(() => useVoiceUi.getState().setState(createVoiceMock({ scenario, timers: false }).current()));

const picker = () =>
  render(
    <TooltipProvider>
      <SharePicker />
    </TooltipProvider>,
  );

describe("the picker", () => {
  it("switches once however fast a choice is pressed twice", async () => {
    const switchSource = vi.spyOn(shareApi, "switchSource").mockResolvedValue(null);
    enter("shareTwoMine");
    act(() => openSwitch(1));
    picker();
    const choice = await screen.findByRole("button", { name: "Kale planı - Paint" });
    fireEvent.click(choice, { detail: 1 });
    fireEvent.click(choice, { detail: 2 });
    expect(switchSource).toHaveBeenCalledTimes(1);
    expect(switchSource).toHaveBeenCalledWith(
      1,
      { kind: "window", id: "0x2a01" },
      "Kale planı - Paint",
    );
  });

  it("starts one share however fast Paylaş is pressed twice", async () => {
    const start = vi.spyOn(shareApi, "start").mockResolvedValue(null);
    enter("shareOffer");
    act(() => useShareUi.getState().openPicker());
    picker();
    // The game is chosen as soon as the list comes.
    await screen.findByRole("radio", { name: /Oyunu Paylaş/ });
    const go = screen.getByRole("button", { name: "Paylaş" });
    fireEvent.click(go, { detail: 1 });
    fireEvent.click(go, { detail: 2 });
    expect(start).toHaveBeenCalledTimes(1);
  });
});
