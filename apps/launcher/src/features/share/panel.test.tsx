// @vitest-environment happy-dom
// Screen sharing on the room panel and the pill (SHARE): "Ekranını Paylaş" where the room shares;
// the player's own share as a card (SQOL: its name with "YAYINDA", its quality, who watches,
// "Ekranı Değiştir", "⋯" and "Durdur"; its states on the same line) while they share; a person who
// shares has "İzle"; nothing of it in a room that does not share or on a computer that cannot.
import { setAppReducedMotion, TooltipProvider } from "@pl/ui";
import {
  act,
  cleanup,
  fireEvent,
  render,
  renderHook,
  screen,
  within,
} from "@testing-library/react";
import type { ReactNode } from "react";
import { afterAll, afterEach, beforeAll, describe, expect, it, vi } from "vitest";
import "@/lib/i18n";
import { useVoiceUi } from "@/features/voice/store";
import { createVoiceMock } from "@/lib/api/voice-mock";
import { HIDDEN_VOICE_STATE, type VoiceState } from "@/lib/api/voice-types";
import { shareApi } from "@/share/api";
import { resetOwnUi, useOwnUi } from "./own-store";
import { ShareFooterRow, useShareRuleItems, WatchButton } from "./panel";
import { SharePillPart } from "./pill-part";
import { useShareUi } from "./store";

beforeAll(() => setAppReducedMotion(true));
afterAll(() => setAppReducedMotion(false));
afterEach(() => {
  cleanup();
  vi.restoreAllMocks();
  useVoiceUi.setState({ state: HIDDEN_VOICE_STATE });
  useShareUi.setState({ picker: false });
  resetOwnUi();
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

  it("shows the player's share with its quality and who watches, and stops it", () => {
    const stop = vi.spyOn(shareApi, "stop").mockResolvedValue();
    show(scenario("shareLive"), <ShareFooterRow />);
    const card = screen.getByRole("group", { name: "Paylaşımın: Oyun" });
    expect(within(card).getByText("Oyun")).toBeTruthy();
    expect(within(card).getByText("YAYINDA")).toBeTruthy();
    expect(within(card).getByText("Normal · 720p")).toBeTruthy();
    expect(within(card).getByText("2 kişi izliyor")).toBeTruthy();
    fireEvent.click(within(card).getByRole("button", { name: "Paylaşımı Durdur" }));
    expect(stop).toHaveBeenCalledWith(1);
  });

  it("switches the source, and opens the card's menu from the keyboard", () => {
    const hold = vi.spyOn(shareApi, "hold").mockResolvedValue(null);
    show(scenario("shareLive"), <ShareFooterRow />);
    fireEvent.click(screen.getByRole("button", { name: "Ekranı Değiştir" }));
    expect(useOwnUi.getState().switching).toBe(1);
    expect(useShareUi.getState().picker).toBe(true);
    // Shift+F10 anywhere in the card opens its "⋯" menu, named after what it shows.
    fireEvent.keyDown(screen.getByRole("button", { name: "Paylaşımı Durdur" }), {
      key: "F10",
      shiftKey: true,
    });
    const menu = screen.getByRole("menu", { name: "Oyun" });
    expect(within(menu).getByRole("menuitem", { name: "Ekranı Değiştir…" })).toBeTruthy();
    expect(within(menu).getByRole("menuitem", { name: "Kalite" })).toBeTruthy();
    expect(
      within(menu)
        .getByRole("menuitemcheckbox", { name: "Sesi de Paylaş" })
        .getAttribute("aria-checked"),
    ).toBe("true");
    expect(within(menu).getByRole("menuitem", { name: "Gelişmiş" })).toBeTruthy();
    fireEvent.click(within(menu).getByRole("menuitem", { name: "Duraklat" }));
    expect(hold).toHaveBeenCalledWith(1, true);
  });

  it("says a paused share and resumes it", () => {
    const hold = vi.spyOn(shareApi, "hold").mockResolvedValue(null);
    show(scenario("sharePaused"), <ShareFooterRow />);
    expect(screen.getByText("Duraklatıldı")).toBeTruthy();
    // Not live meanwhile: no "YAYINDA".
    expect(screen.queryByText("YAYINDA")).toBeNull();
    fireEvent.click(screen.getByRole("button", { name: "Sürdür" }));
    expect(hold).toHaveBeenCalledWith(1, false);
  });

  it("lets a double click's second press on Sürdür not press the Ekranı Değiştir in its place", () => {
    const hold = vi.spyOn(shareApi, "hold").mockResolvedValue(null);
    show(scenario("sharePaused"), <ShareFooterRow />);
    fireEvent.click(screen.getByRole("button", { name: "Sürdür" }), { detail: 1 });
    expect(hold).toHaveBeenCalledWith(1, false);
    // Live again at once: the same place says "Ekranı Değiştir", and the double click's second
    // press lands on it.
    fireEvent.click(screen.getByRole("button", { name: "Ekranı Değiştir" }), { detail: 2 });
    expect(useOwnUi.getState().switching).toBeNull();
    expect(useShareUi.getState().picker).toBe(false);
  });

  it("says a minimised window, and asks for another screen when the window closed", () => {
    show(scenario("shareMin"), <ShareFooterRow />);
    expect(screen.getByText("Pencere küçültüldü.")).toBeTruthy();
    cleanup();
    show(scenario("shareLost"), <ShareFooterRow />);
    expect(screen.getByText("Paylaştığın pencere kapandı.")).toBeTruthy();
    expect(screen.getByText("Paylaşımı Durdur")).toBeTruthy();
    fireEvent.click(screen.getByRole("button", { name: "Başka Ekran Seç" }));
    expect(useOwnUi.getState().switching).toBe(1);
    cleanup();
    show(scenario("shareLostGame"), <ShareFooterRow />);
    expect(screen.getByText("Oyun kapandı.")).toBeTruthy();
  });

  it("keeps only stopping where the service does not let a live share change", () => {
    show(scenario("shareNoEdit"), <ShareFooterRow />);
    expect(screen.queryByRole("button", { name: "Ekranı Değiştir" })).toBeNull();
    expect(screen.queryByRole("button", { name: "Paylaşım Seçenekleri" })).toBeNull();
    expect(screen.getByRole("button", { name: "Paylaşımı Durdur" })).toBeTruthy();
  });

  it("gives a friend room's leader the share rule for the header menu, not the footer", () => {
    const sharers = vi.spyOn(shareApi, "setSharers").mockResolvedValue(true);
    show(scenario("shareOffer"), <ShareFooterRow />);
    expect(screen.queryByText("Kim Paylaşabilir")).toBeNull();
    const { result } = renderHook(() => useShareRuleItems());
    const [rule] = result.current;
    expect(rule?.label).toBe("Kim Paylaşabilir");
    expect(rule?.items?.map((item) => [item?.label, item?.radio, item?.checked])).toEqual([
      ["Herkes", true, true],
      ["Lider ve Moderatörler", true, false],
    ]);
    rule?.items?.[1]?.onSelect();
    expect(sharers).toHaveBeenCalledWith("mods");
    cleanup();
    // Someone who may not set it: nothing to place.
    act(() => useVoiceUi.getState().setState(scenario("shareNotAllowed")));
    expect(renderHook(() => useShareRuleItems()).result.current).toEqual([]);
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

  it("puts a calm mark and the count in the pill, its right click the share's menu", () => {
    show(scenario("shareLive"), <SharePillPart />);
    const pill = screen.getByRole("button", { name: "Yayındasın, 2 kişi izliyor" });
    fireEvent.contextMenu(pill);
    const menu = screen.getByRole("menu", { name: "Oyun" });
    expect(within(menu).getByRole("menuitem", { name: "Paylaşımı Durdur" })).toBeTruthy();
    cleanup();
    show(scenario("shareOffer"), <SharePillPart />);
    expect(screen.getByRole("button", { name: "Ekranını Paylaş" })).toBeTruthy();
  });
});
