import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import { SLOW_STEPS, slowFor, slowLeft, slowSpan, slowUntil, slowWords } from "./slow";

const NOW = 1_800_000_000_000;

describe("slow mode (CHAT-CALM)", () => {
  it("waits only for plain members", () => {
    expect(slowFor({ role: "member", moderator: false, slow: 30 })).toBe(30);
    expect(slowFor({ role: "member", moderator: true, slow: 30 })).toBe(0);
    expect(slowFor({ role: "founder", moderator: false, slow: 30 })).toBe(0);
    // A crew from an older service, and none at all.
    expect(slowFor({ role: "member", moderator: false })).toBe(0);
    expect(slowFor(undefined)).toBe(0);
  });

  it("counts from the player's newest message that went or is on its way", () => {
    const messages = [
      { mine: true, state: "sent" as const, at: NOW - 40_000 },
      { mine: false, state: "sent" as const, at: NOW - 2_000 },
      { mine: true, state: "sending" as const, at: NOW - 12_000 },
      // One that did not go is not a message the service took.
      { mine: true, state: "failed" as const, at: NOW - 1_000 },
    ];
    expect(slowUntil(messages, 30)).toBe(NOW + 18_000);
    expect(slowLeft(slowUntil(messages, 30), NOW)).toBe(18);
    expect(slowLeft(slowUntil(messages, 30), NOW + 17_001)).toBe(1);
    expect(slowLeft(slowUntil(messages, 30), NOW + 18_000)).toBe(0);
    // No slow mode, nothing of the player's, nothing loaded: no wait.
    expect(slowUntil(messages, 0)).toBeNull();
    expect(slowUntil([{ mine: false, state: "sent", at: NOW }], 30)).toBeNull();
    expect(slowUntil(undefined, 30)).toBeNull();
    expect(slowLeft(null, NOW)).toBe(0);
  });

  it("offers the service's steps, up to half a minute", () => {
    expect([...SLOW_STEPS]).toEqual([0, 5, 10, 30]);
    const t = ((key: string, options?: Record<string, number>) =>
      `${key}:${Object.values(options ?? {}).join("")}`) as unknown as TFunction;
    // A label takes the short units, a sentence whole words.
    expect(SLOW_STEPS.map((step) => slowWords(t, step))).toEqual([
      "chatCalm.slow.off:",
      "units.seconds:5",
      "units.seconds:10",
      "units.seconds:30",
    ]);
    expect(slowSpan(t, 30)).toBe("chatCalm.slow.spanSeconds:30");
  });
});
