// @vitest-environment happy-dom
// BETA2-FIX (REL-BETA203 observation 4): the title bar's live region holds an announcement only while
// a screen reader takes it, and never in a language the interface has left (an invite's Turkish
// sentence stayed there after the switch to English).
import { act, cleanup, render } from "@testing-library/react";
import { afterEach, beforeAll, beforeEach, describe, expect, it, vi } from "vitest";
import i18n, { loadEnglish, setLanguage } from "@/lib/i18n";
import { ANNOUNCE_CLEAR_MS, LiveRegion } from "./live-region";
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

  it("a newer announcement takes the place of one waiting for its frame, and its own time", () => {
    const view = render(<LiveRegion />);
    act(() => announce("first"));
    act(() => vi.advanceTimersToNextFrame());
    act(() => vi.advanceTimersByTime(ANNOUNCE_CLEAR_MS - 100));
    act(() => announce("second"));
    act(() => announce("third"));
    act(() => vi.advanceTimersToNextFrame());
    expect(said(view)).toBe("third");
    // The first one's clearing does not cut the newest short.
    act(() => vi.advanceTimersByTime(200));
    expect(said(view)).toBe("third");
    act(() => vi.advanceTimersByTime(ANNOUNCE_CLEAR_MS));
    expect(said(view)).toBe("");
  });
});
