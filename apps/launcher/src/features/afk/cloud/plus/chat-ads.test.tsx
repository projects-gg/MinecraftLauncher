// @vitest-environment happy-dom
// "Sohbete ilan" as the service plans it: the minutes (the ones under the server's floor wait, said
// once), the next ad drawn as a line of the game's chat with "Başka örnek" going through the next
// few, a spam warning, today's stop, no active order, and nothing at all from an older service.
import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import "@/lib/i18n";
import { ChatAds } from "./chat-ads";
import type { PlusAdPreview } from "./types";

const ignore = () => {};

function plan(parts: Partial<PlusAdPreview> = {}): PlusAdPreview {
  return {
    floorMin: 30,
    everyMin: 60,
    lines: [
      "Alıyorum: 4 Elytra, PM at",
      "Alıyorum: 3 Ölümsüzlük Totemi, PM at",
      "Satıyorum: 64 Elmas",
    ],
    namesPending: false,
    nextAt: null,
    warning: null,
    stoppedToday: false,
    ...parts,
  };
}

/** The part as the settings show it, for the bot "Neon_Drift". */
function show(
  ads: PlusAdPreview | null,
  minutes: number | null = null,
  onMinutes: (minutes: number) => void = ignore,
) {
  return render(<ChatAds plan={ads} minutes={minutes} name="Neon_Drift" onMinutes={onMinutes} />);
}

/** Each chip's words, whether it is the pick and whether it can be picked. */
function chips() {
  return screen.getAllByRole("radio").map((chip) => [
    chip.textContent,
    chip.getAttribute("aria-checked") === "true",
    !(chip as HTMLButtonElement).disabled,
  ]);
}

/** The chat line as a screen reader reads it; null while none shows. */
const said = (container: HTMLElement) =>
  container.querySelector("[aria-live]")?.textContent ?? null;

afterEach(() => cleanup());

describe("Sohbete ilan", () => {
  it("shows nothing from an older service (the switch stays as it was)", () => {
    const { container } = show(null);
    expect(container.innerHTML).toBe("");
  });

  it("offers the minutes, the service's hour while none is picked", () => {
    const picks: number[] = [];
    show(plan(), null, (minutes) => picks.push(minutes));
    expect(chips()).toEqual([
      ["30 dk", false, true],
      ["45 dk", false, true],
      ["1 saat", true, true],
      ["2 saat", false, true],
    ]);
    expect(screen.queryByText(/Bu sunucuda en az/)).toBeNull();
    fireEvent.click(screen.getByRole("radio", { name: "45 dk" }));
    expect(picks).toEqual([45]);
  });

  it("closes the minutes under the server's floor and says why once", () => {
    const picks: number[] = [];
    show(plan({ floorMin: 60 }), 60, (minutes) => picks.push(minutes));
    expect(chips().map((chip) => chip[2])).toEqual([false, false, true, true]);
    expect(screen.getAllByText("Bu sunucuda en az 60 dk.")).toHaveLength(1);
    fireEvent.click(screen.getByRole("radio", { name: "30 dk" }));
    expect(picks).toEqual([]);
  });

  it("draws the next ad as a chat line and goes through the next few", () => {
    const { container } = show(plan());
    expect(said(container)).toBe("Örnek ilan: Neon_Drift: Alıyorum: 4 Elytra, PM at");
    const next = () => fireEvent.click(screen.getByRole("button", { name: "Başka örnek" }));
    next();
    expect(said(container)).toBe("Örnek ilan: Neon_Drift: Alıyorum: 3 Ölümsüzlük Totemi, PM at");
    next();
    expect(said(container)).toBe("Örnek ilan: Neon_Drift: Satıyorum: 64 Elmas");
    next();
    expect(said(container)).toBe("Örnek ilan: Neon_Drift: Alıyorum: 4 Elytra, PM at");
  });

  it("has no other example to show with one line", () => {
    const { container } = show(plan({ lines: ["Satıyorum: 64 Elmas"] }));
    expect(said(container)).toBe("Örnek ilan: Neon_Drift: Satıyorum: 64 Elmas");
    expect(screen.queryByRole("button", { name: "Başka örnek" })).toBeNull();
  });

  it("says a spam warning doubled the minutes, and when today's ads stopped", () => {
    const warning = { at: 1_792_900_000, everyMin: 120 };
    const { container, rerender } = show(plan({ everyMin: 120, warning }), 60);
    const warned = "Sunucu spam uyarısı verdi. Aralık bugünlük iki katı: 120 dk.";
    expect(screen.getByText(warned)).toBeTruthy();
    expect(said(container)).toBe("Örnek ilan: Neon_Drift: Alıyorum: 4 Elytra, PM at");
    // A second warning stopped today's ads: that alone, no ad and no button; the minutes stay.
    const stopped = plan({ everyMin: 120, warning, stoppedToday: true });
    rerender(<ChatAds plan={stopped} minutes={60} name="Neon_Drift" onMinutes={ignore} />);
    expect(screen.getByText("Bugünlük ilan durdu.")).toBeTruthy();
    expect(screen.queryByText(warned)).toBeNull();
    expect(said(container)).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
    expect(screen.getAllByRole("radio")).toHaveLength(4);
  });

  it("says no ad goes while no order is active", () => {
    const { container } = show(plan({ lines: [] }));
    expect(screen.getByText("Etkin sipariş yokken ilan verilmez.")).toBeTruthy();
    expect(said(container)).toBeNull();
    expect(screen.queryByRole("button")).toBeNull();
  });

  it("says it is ready once in game while the item names are not known", () => {
    const { container } = show(plan({ lines: [], namesPending: true }));
    expect(screen.getByText("ProBot AI oyuna girince hazır olur.")).toBeTruthy();
    expect(screen.queryByText("Etkin sipariş yokken ilan verilmez.")).toBeNull();
    expect(said(container)).toBeNull();
  });
});
