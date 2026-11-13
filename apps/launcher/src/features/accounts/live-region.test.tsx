// @vitest-environment happy-dom
// BETA2-FIX (REL-BETA203 observation 4): the title bar's live region holds an announcement only while
// a screen reader takes it, and never in a language the interface has left (an invite's Turkish
// sentence stayed there after the switch to English).
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import i18n, { loadEnglish, setLanguage } from "@/lib/i18n";
import { ANNOUNCE_CLEAR_MS, ANNOUNCE_MAX, LiveRegion } from "./live-region";
import { announce } from "./use-accounts";

const INVITE = "BuzKalesi seni oyuna çağırıyor";

beforeAll(async () => {
  await loadEnglish();
});

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(async () => {
  cleanup();
  vi.useRealTimers();
  await setLanguage("tr");
});

function said(view: ReturnType<typeof render>): string | null | undefined {
  return view.container.querySelector('[aria-live="polite"]')?.textContent;
}

describe("LiveRegion", () => {
  it("says an announcement on the next frame and lets it go once read", () => {
    const view = render(<LiveRegion />);
    act(() => announce(INVITE));
    expect(said(view)).toBe("");
    act(() => vi.advanceTimersToNextFrame());
    expect(said(view)).toBe(INVITE);
    act(() => vi.advanceTimersByTime(ANNOUNCE_CLEAR_MS - 1));
    expect(said(view)).toBe(INVITE);
    act(() => vi.advanceTimersByTime(1));
    expect(said(view)).toBe("");
  });

  it("drops a sentence of the old language when the language changes", async () => {
    const view = render(<LiveRegion />);
    act(() => announce(INVITE));
    act(() => vi.advanceTimersToNextFrame());
    expect(said(view)).toBe(INVITE);
    await act(async () => {
      await setLanguage("en");
    });
    expect(i18n.language).toBe("en");
    expect(said(view)).toBe("");
    // What comes after the switch is said as usual, in the new language.
    act(() => announce(i18n.t("social.notices.invite", { name: "BuzKalesi" })));
    act(() => vi.advanceTimersToNextFrame());
    expect(said(view)).toBe("BuzKalesi invites you to play");
  });

  it("a language change before the frame keeps the old sentence out", async () => {
    const view = render(<LiveRegion />);
    act(() => announce(INVITE));
    await act(async () => {
      await setLanguage("en");
    });
    act(() => vi.advanceTimersToNextFrame());
    expect(said(view)).toBe("");
  });

  // SQOL-U3: a burst (two people coming in at once, three chat messages) is heard whole.
  it("keeps every announcement of a burst as a line of its own, each leaving on its own time", () => {
    const view = render(<LiveRegion />);
    act(() => announce("first"));
    act(() => vi.advanceTimersToNextFrame());
    act(() => vi.advanceTimersByTime(ANNOUNCE_CLEAR_MS - 100));
    act(() => announce("second"));
    act(() => announce("third"));
    act(() => vi.advanceTimersToNextFrame());
    expect(lines(view)).toEqual(["first", "second", "third"]);
    // The first one's time ends; the newer ones keep theirs.
    act(() => vi.advanceTimersByTime(200));
    expect(lines(view)).toEqual(["second", "third"]);
    act(() => vi.advanceTimersByTime(ANNOUNCE_CLEAR_MS));
    expect(said(view)).toBe("");
  });

  it("reads only what is added, and the same words again as a new line", () => {
    const view = render(<LiveRegion />);
    const region = view.container.querySelector('[aria-live="polite"]');
    expect(region?.getAttribute("aria-relevant")).toBe("additions");
    expect(region?.getAttribute("aria-atomic")).toBeNull();
    act(() => announce("KorAtesi odaya katıldı."));
    act(() => vi.advanceTimersToNextFrame());
    act(() => announce("KorAtesi odaya katıldı."));
    act(() => vi.advanceTimersToNextFrame());
    expect(lines(view)).toEqual(["KorAtesi odaya katıldı.", "KorAtesi odaya katıldı."]);
  });

  it("holds at most ANNOUNCE_MAX lines, the newest", () => {
    const view = render(<LiveRegion />);
    act(() => {
      for (let i = 1; i <= ANNOUNCE_MAX + 2; i++) announce(`line ${i}`);
    });
    act(() => vi.advanceTimersToNextFrame());
    const shown = lines(view);
    expect(shown).toHaveLength(ANNOUNCE_MAX);
    expect(shown.at(-1)).toBe(`line ${ANNOUNCE_MAX + 2}`);
    expect(shown[0]).toBe("line 3");
  });
});

function lines(view: ReturnType<typeof render>): string[] {
  return [...view.container.querySelectorAll('[aria-live="polite"] > p')].map(
    (p) => p.textContent ?? "",
  );
}
