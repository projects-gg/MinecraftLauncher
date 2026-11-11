// @vitest-environment happy-dom
// Screen sharing on the room panel and the pill (SHARE): "Ekranını Paylaş" where the room shares;
// "Yayında" with who watches and "Durdur" while the player shares; a person who shares has the mark
// and "İzle"; nothing of it in a room that does not share or on a computer that cannot.
import { setAppReducedMotion, TooltipProvider } from "@pl/ui";
import { act, cleanup, fireEvent, render, screen } from "@testing-library/react";
import type { ReactNode } from "react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { useVoiceUi } from "@/features/voice/store";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { HIDDEN_VOICE_STATE, type VoiceState } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import { ShareFooterRow, WatchButton } from "./panel";
import { SharePillPart } from "./pill-part";
import { useShareUi } from "./store";

beforeAll(() => setAppReducedMotion(true));
afterAll(() => setAppReducedMotion(false));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useVoiceUi.setState({ state: HIDDEN_VOICE_STATE });
  useShareUi.setState({ picker: false });
});

const scenario = (name: string): VoiceState =>
  createVoiceMock({ scenario: name, timers: false }).current();

function show(state: VoiceState, node: ReactNode) {
  act(() => useVoiceUi.getState().setState(state));
  return render(<TooltipProvider>{node}</TooltipProvider>);
}

describe("screen sharing on the panel", () => {
  it("offers sharing only where the room shares and this computer can", () => {
    show(scenario("room"), <ShareFooterRow />);
    expect(screen.queryByText("Ekranını Paylaş")).toBeNull();
    cleanup();
    show(scenario("shareOffer"), <ShareFooterRow />);
    fireEvent.click(screen.getByText("Ekranını Paylaş"));
    expect(useShareUi.getState().picker).toBe(true);
    cleanup();
    const state = scenario("shareOffer");
    show(
      { ...state, share: state.share && { ...state.share, canShare: false } },
      <ShareFooterRow />,
    );
    expect(screen.queryByText("Ekranını Paylaş")).toBeNull();
  });

  it("says who watches the player's share and stops it", () => {
    const stop = vi.spyOn(shareApi, "stop").mockResolvedValue();
    show(scenario("shareLive"), <ShareFooterRow />);
    expect(screen.getByText("Yayında")).toBeTruthy();
    expect(screen.getByText("Oyun")).toBeTruthy();
    expect(screen.getByText("2 kişi izliyor")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Paylaşımı durdur" }));
    expect(stop).toHaveBeenCalledWith(1);
  });

  it("asks for nothing while the room keeps sharing to its leaders", () => {
    show(scenario("shareNotAllowed"), <ShareFooterRow />);
    const button = screen.getByText("Ekranını Paylaş").closest("button");
    expect(button?.getAttribute("aria-disabled")).toBe("true");
    if (button) fireEvent.click(button);
    expect(useShareUi.getState().picker).toBe(false);
  });

  it("watches someone who shares", () => {
    const watch = vi.spyOn(shareApi, "watch").mockResolvedValue(null);
    show(scenario("shareLiveRow"), <WatchButton person="301" name="GeceKurdu_" />);
    fireEvent.click(screen.getByRole("button", { name: "GeceKurdu_ yayınını izle" }));
    expect(watch).toHaveBeenCalledWith("301", 1);
    cleanup();
    // Watched already: no second "İzle".
    show(scenario("shareWatch"), <WatchButton person="301" name="GeceKurdu_" />);
    expect(screen.queryByText("İzle")).toBeNull();
  });

  it("puts a calm mark and the count in the pill while sharing", () => {
    show(scenario("shareLive"), <SharePillPart />);
    expect(screen.getByRole("button", { name: "Yayındasın, 2 kişi izliyor" })).toBeTruthy();
    cleanup();
    show(scenario("shareOffer"), <SharePillPart />);
    expect(screen.getByRole("button", { name: "Ekranını Paylaş" })).toBeTruthy();
  });
});
