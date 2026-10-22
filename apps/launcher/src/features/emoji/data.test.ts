import { readFileSync } from "node:fs";
import { fold } from "@pl/ui/search";
import { describe, expect, it } from "vitest";
import { displayName, EMOJI_GROUPS, parseEmojiData } from "./data";

const read = (lang: string) =>
  parseEmojiData(readFileSync(new URL(`./data/emoji.${lang}.tsv`, import.meta.url), "utf8"));
const tr = read("tr");
const en = read("en");
const cps = (text: string) => [...text].map((ch) => ch.codePointAt(0) as number);

describe("the emoji data", () => {
  it("lists the same emoji in both languages, in Unicode's order and groups", () => {
    expect(tr.lang).toBe("tr");
    expect(en.lang).toBe("en");
    expect(tr.entries.length).toBeGreaterThan(1900);
    expect(en.entries.map((e) => e.emoji)).toEqual(tr.entries.map((e) => e.emoji));
    expect(tr.entries[0]?.emoji).toBe("😀");
    // Groups come one after another, each once, in the picker's order.
    const order = tr.entries.map((e) => e.group).filter((g, i, all) => g !== all[i - 1]);
    expect(order).toEqual([...EMOJI_GROUPS]);
  });

  it("leaves out the middle finger, the subdivision flags, the components and toned forms", () => {
    for (const { emoji } of tr.entries) {
      const points = cps(emoji);
      expect(points).not.toContain(0x1f595);
      expect(points.some((cp) => cp >= 0xe0000)).toBe(false);
      expect(points.some((cp) => cp >= 0x1f3fb && cp <= 0x1f3ff)).toBe(false);
      expect(points.some((cp) => cp >= 0x1f9b0 && cp <= 0x1f9b3 && points.length === 1)).toBe(
        false,
      );
    }
    expect(tr.entries.some((e) => e.emoji === "🇹🇷")).toBe(true);
  });

  it("names every emoji and keeps its search words folded", () => {
    for (const data of [tr, en]) {
      for (const entry of data.entries) {
        expect(entry.name.trim().length, entry.emoji).toBeGreaterThan(0);
        for (const word of `${entry.words} ${entry.fallback}`.split(" ").filter(Boolean)) {
          expect(fold(word), `${entry.emoji} ${word}`).toBe(word);
        }
      }
    }
    expect(en.entries.every((e) => e.fallback === "")).toBe(true);
    expect(tr.entries.find((e) => e.emoji === "😂")?.name).toBe("sevinç gözyaşları");
  });

  it("gives skin tones to the people and their hands", () => {
    const toned = tr.entries.filter((e) => e.tones);
    expect(toned.length).toBeGreaterThan(300);
    for (const entry of toned) {
      if (entry.tones !== "rule") expect(entry.tones).toHaveLength(5);
    }
    expect(tr.entries.find((e) => e.emoji === "👍")?.tones).toBe("rule");
    expect(tr.entries.find((e) => e.emoji === "😀")?.tones).toBeNull();
    expect(tr.toneBases.has(0x1f44d)).toBe(true);
  });

  it("refuses a file of another kind", () => {
    expect(() => parseEmojiData("hello\tworld\n")).toThrow();
  });

  it("writes a name with its first letter in upper case, Turkish İ included", () => {
    expect(displayName("sevinç gözyaşları", "tr")).toBe("Sevinç gözyaşları");
    expect(displayName("işaret", "tr")).toBe("İşaret");
    expect(displayName("index pointing up", "en")).toBe("Index pointing up");
  });
});
