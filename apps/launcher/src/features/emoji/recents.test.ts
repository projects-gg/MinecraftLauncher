import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseEmojiData } from "./data";
import { cleanList, RECENT_EMOJI, recentEntries, remember } from "./recents";

const data = parseEmojiData(readFileSync(new URL("./data/emoji.tr.tsv", import.meta.url), "utf8"));
const tone = (n: number) => String.fromCodePoint(0x1f3fa + n);

describe("remember", () => {
  it("puts the latest first, once", () => {
    expect(remember([], "😂", 24)).toEqual(["😂"]);
    expect(remember(["😂", "🔥"], "🔥", 24)).toEqual(["🔥", "😂"]);
    expect(remember(["😂", "🔥", "❤️"], "👍", 24)).toEqual(["👍", "😂", "🔥", "❤️"]);
  });

  it("keeps at most the limit, dropping the oldest", () => {
    let list: string[] = [];
    for (let i = 0; i < 30; i++) list = remember(list, `e${i}`, RECENT_EMOJI);
    expect(list).toHaveLength(RECENT_EMOJI);
    expect(list[0]).toBe("e29");
    expect(list.at(-1)).toBe("e6");
  });
});

describe("cleanList", () => {
  it("keeps only strings, once each, up to the limit", () => {
    expect(cleanList(["😂", 4, null, "😂", "", "🔥"], 24)).toEqual(["😂", "🔥"]);
    expect(cleanList("😂", 24)).toEqual([]);
    expect(cleanList(undefined, 24)).toEqual([]);
    expect(cleanList(["a", "b", "c"], 2)).toEqual(["a", "b"]);
  });
});

describe("recentEntries", () => {
  it("finds the recents in the data, whatever tone they were sent in", () => {
    const found = recentEntries(data, ["🔥", `👍${tone(3)}`, "not an emoji", "😂", "🔥"]);
    expect(found.map((entry) => entry.emoji)).toEqual(["🔥", "👍", "😂"]);
  });
});
