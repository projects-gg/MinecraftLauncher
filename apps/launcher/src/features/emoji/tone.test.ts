import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseEmojiData } from "./data";
import { bareEmoji, isSkinTone, withTone } from "./tone";

const data = parseEmojiData(readFileSync(new URL("./data/emoji.tr.tsv", import.meta.url), "utf8"));
const entry = (emoji: string) => {
  const found = data.entries.find((e) => e.emoji === emoji);
  if (!found) throw new Error(`No ${emoji}`);
  return found;
};
const tone = (n: number) => String.fromCodePoint(0x1f3fa + n);
const VS16 = String.fromCodePoint(0xfe0f);
const ZWJ = String.fromCodePoint(0x200d);

describe("withTone", () => {
  it("adds the tone after the person", () => {
    expect(withTone(entry("👍"), 3, data.toneBases)).toBe(`👍${tone(3)}`);
    expect(withTone(entry("👋"), 5, data.toneBases)).toBe(`👋${tone(5)}`);
  });

  it("drops the presentation selector the tone replaces", () => {
    const up = entry(`☝${VS16}`);
    expect(withTone(up, 1, data.toneBases)).toBe(`☝${tone(1)}`);
  });

  it("tones every person of a sequence, and only the people", () => {
    const couple = entry(`👩${ZWJ}❤${VS16}${ZWJ}👨`);
    expect(withTone(couple, 2, data.toneBases)).toBe(
      `👩${tone(2)}${ZWJ}❤${VS16}${ZWJ}👨${tone(2)}`,
    );
    const hands = entry(`🧑${ZWJ}🤝${ZWJ}🧑`);
    expect(withTone(hands, 4, data.toneBases)).toBe(`🧑${tone(4)}${ZWJ}🤝${ZWJ}🧑${tone(4)}`);
  });

  it("leaves emoji without tones, and the default tone, as they are", () => {
    expect(withTone(entry("😀"), 3, data.toneBases)).toBe("😀");
    expect(withTone(entry("👍"), 0, data.toneBases)).toBe("👍");
  });
});

describe("bareEmoji and isSkinTone", () => {
  it("strips tones and selectors", () => {
    expect(bareEmoji(`👍${tone(3)}`)).toBe("👍");
    expect(bareEmoji(`❤${VS16}`)).toBe("❤");
  });

  it("knows the six tones", () => {
    expect([0, 1, 2, 3, 4, 5].every(isSkinTone)).toBe(true);
    expect(isSkinTone(6)).toBe(false);
    expect(isSkinTone("1")).toBe(false);
  });
});
