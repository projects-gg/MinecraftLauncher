import { describe, expect, it } from "vitest";
import type { ChatReaction } from "@/lib/api/social-types";
import {
  QUICK_DEFAULTS,
  quickReactions,
  REACTION_WINDOW_MS,
  reactable,
  reactors,
  reactVerdict,
  sameEmoji,
  withMyReaction,
} from "./reactions";

const ME = "100";
const VS16 = String.fromCodePoint(0xfe0f);
const r = (e: string, by: string[]): ChatReaction => ({ e, by, mine: by.includes(ME) });

describe("quickReactions", () => {
  it("starts with the player's recent emoji, then the defaults, six of them", () => {
    expect(quickReactions([])).toEqual([...QUICK_DEFAULTS]);
    expect(quickReactions(["🎮", "👍", "💀"])).toEqual(["🎮", "👍", "💀", "❤️", "😂", "😭"]);
  });

  it("never offers the middle finger and counts ❤ and ❤️ once", () => {
    expect(quickReactions(["🖕", "❤", "🔥"])).toEqual(["❤", "🔥", "👍", "😂", "😭", "😮"]);
  });
});

describe("sameEmoji", () => {
  it("reads a heart with or without its selector as one, a skin tone as another", () => {
    expect(sameEmoji("❤", `❤${VS16}`)).toBe(true);
    expect(sameEmoji("👍", `👍${String.fromCodePoint(0x1f3fd)}`)).toBe(false);
  });
});

describe("reactVerdict", () => {
  it("takes back the player's own and adds a new one", () => {
    expect(reactVerdict([r("👍", [ME])], "👍")).toBe("remove");
    expect(reactVerdict([r("👍", ["301"])], "👍")).toBe("add");
    expect(reactVerdict(undefined, "🔥")).toBe("add");
  });

  it("stops at five of the player's and at ten kinds, but 'Ben de' always works", () => {
    const five = ["😂", "🔥", "👍", "😭", "😮"].map((e) => r(e, [ME]));
    expect(reactVerdict(five, "❤️")).toBe("mine");
    const ten = ["1", "2", "3", "4", "5", "6", "7", "8", "9", "10"].map((e) => r(e, ["301"]));
    expect(reactVerdict(ten, "❤️")).toBe("kinds");
    expect(reactVerdict(ten, "5")).toBe("add");
  });

  it("never takes the middle finger, in any tone", () => {
    expect(reactVerdict(undefined, "\u{1F595}")).toBe("refused");
    expect(reactVerdict(undefined, "\u{1F595}\u{1F3FD}")).toBe("refused");
  });
});

describe("withMyReaction", () => {
  it("adds the player at the end of a kind, or a new kind at the end", () => {
    expect(withMyReaction([r("👍", ["301"])], "👍", ME, true)).toEqual([
      { e: "👍", by: ["301", ME], mine: true },
    ]);
    expect(withMyReaction([r("👍", ["301"])], "🔥", ME, true)).toEqual([
      { e: "👍", by: ["301"], mine: false },
      { e: "🔥", by: [ME], mine: true },
    ]);
  });

  it("takes the player out, and a kind nobody is left in goes", () => {
    expect(withMyReaction([r("👍", ["301", ME])], "👍", ME, false)).toEqual([
      { e: "👍", by: ["301"], mine: false },
    ]);
    expect(withMyReaction([r("👍", [ME]), r("🔥", ["301"])], "👍", ME, false)).toEqual([
      { e: "🔥", by: ["301"], mine: false },
    ]);
  });

  it("leaves the reactions it was given as they were", () => {
    const before = [r("👍", ["301"])];
    withMyReaction(before, "👍", ME, true);
    expect(before).toEqual([r("👍", ["301"])]);
  });
});

describe("reactable and reactors", () => {
  const now = 10 * REACTION_WINDOW_MS;
  it("takes sent messages younger than a week", () => {
    expect(reactable({ state: "sent", id: 1, at: now - 1000 }, now)).toBe(true);
    expect(reactable({ state: "sending", at: now }, now)).toBe(false);
    expect(reactable({ state: "sent", id: 1, at: now - REACTION_WINDOW_MS - 1 }, now)).toBe(false);
    expect(reactable({ state: "sent", id: 1, at: now, deleted: { by: "301" } }, now)).toBe(false);
  });

  it("lists the player first, then in the order they reacted", () => {
    expect(reactors(r("👍", ["301", "302", ME]), ME)).toEqual([ME, "301", "302"]);
    expect(reactors(r("👍", ["301", "302"]), ME)).toEqual(["301", "302"]);
  });
});
