// @vitest-environment happy-dom
// SQOL-U3: a call with one friend on the room panel: the player's own face says "Sen" only to
// screen readers (the owner: everyone knows who they are), and its time is in the panel's header
// ("Sesli Arama (12:34)"), not under the faces.
import { setAppReducedMotion, TooltipProvider } from "@pl/ui";
import { act, cleanup, render, screen } from "@testing-library/react";
import { afterAll, afterEach, beforeAll, describe, expect, it } from "vitest";
import "@/lib/i18n";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { HIDDEN_VOICE_STATE } from "@/lib/api/voice-types";
import { CallStage } from "./stage";
import { useVoiceUi } from "./store";

beforeAll(() => setAppReducedMotion(true));
afterAll(() => setAppReducedMotion(false));
afterEach(() => {
  cleanup();
  useVoiceUi.setState({ state: HIDDEN_VOICE_STATE, ended: null, speaking: [], meTalking: false });
});

function show(stacked = false) {
  const state = createVoiceMock({ scenario: "call1", timers: false }).current();
  act(() => useVoiceUi.getState().setState(state));
  const room = state.room;
  if (!room) throw new Error("call1 has a room");
  return render(
    <TooltipProvider>
      <CallStage room={room} stacked={stacked} marksOf={() => null} />
    </TooltipProvider>,
  );
}

describe("a call's stage", () => {
  it('keeps "Sen" for screen readers only', () => {
    show();
    const you = screen.getByText("Sen");
    expect(you.classList.contains("sr-only")).toBe(true);
  });

  it("has no clock under the faces: the header has it", () => {
    show();
    expect(screen.queryByRole("timer")).toBeNull();
  });

  it("stands its two faces one over the other in a wide panel's people column", () => {
    const { container } = show(true);
    const seats = container.querySelector("[data-voice-stage] > div");
    expect(seats?.className).toContain("flex-col");
  });
});
