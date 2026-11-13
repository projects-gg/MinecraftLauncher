// SQOL §1.3 "Erişilebilirlik": the room chat's new messages reach a screen reader gathered and
// polite: one as itself, more as their count.
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { burstWords, createGather, HIDDEN_GATHER_MS, SHOWN_GATHER_MS } from "./calm";

const t = (key: string, options?: Record<string, unknown>) =>
  options ? `${key} ${JSON.stringify(options)}` : key;

beforeEach(() => {
  vi.useFakeTimers();
});

afterEach(() => {
  vi.useRealTimers();
});

describe("createGather", () => {
  it("hands over what comes within the wait of the first, once", () => {
    const told: string[][] = [];
    const gather = createGather<string>((items) => told.push(items));
    gather.add("a", SHOWN_GATHER_MS);
    vi.advanceTimersByTime(SHOWN_GATHER_MS - 200);
    gather.add("b", SHOWN_GATHER_MS);
    gather.add("c", SHOWN_GATHER_MS);
    expect(told).toEqual([]);
    vi.advanceTimersByTime(200);
    expect(told).toEqual([["a", "b", "c"]]);
    // The next one starts its own wait.
    gather.add("d", SHOWN_GATHER_MS);
    vi.advanceTimersByTime(SHOWN_GATHER_MS);
    expect(told).toEqual([["a", "b", "c"], ["d"]]);
  });

  it("drops what waits when cleared", () => {
    const told: string[][] = [];
    const gather = createGather<string>((items) => told.push(items));
    gather.add("a", HIDDEN_GATHER_MS);
    gather.clear();
    vi.advanceTimersByTime(HIDDEN_GATHER_MS);
    expect(told).toEqual([]);
  });

  it("waits longer out of sight", () => {
    expect(HIDDEN_GATHER_MS).toBeGreaterThan(SHOWN_GATHER_MS);
  });
});

describe("burstWords", () => {
  const one = [{ name: "KorAtesi", line: "selam" }];
  const three = [...one, { name: "x7Ender", line: "naber" }, { name: "KorAtesi", line: "gel" }];

  it("tells one message as itself, more as their count", () => {
    expect(burstWords(one, true, t)).toBe('chat.notices.direct {"name":"KorAtesi","text":"selam"}');
    expect(burstWords(three, true, t)).toBe('chatCalm.newMessages {"count":3}');
  });

  it("says where out of sight", () => {
    expect(burstWords(one, false, t)).toBe(
      'roomChat.announce.one {"name":"KorAtesi","text":"selam"}',
    );
    expect(burstWords(three, false, t)).toBe('roomChat.announce.many {"count":3}');
  });
});
