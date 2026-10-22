import { describe, expect, it } from "vitest";
import { bigEmojiCount, codePoints, insertText } from "./insert";

const ZWJ = String.fromCodePoint(0x200d);
const VS16 = String.fromCodePoint(0xfe0f);
const family = `👨${ZWJ}👩${ZWJ}👧${ZWJ}👦`;

describe("insertText", () => {
  it("puts the emoji at the caret and the caret after it", () => {
    expect(insertText("selam", 5, 5, "👋", 300)).toEqual({ value: "selam👋", caret: 7 });
    expect(insertText("selam", 0, 0, "👋", 300)).toEqual({ value: "👋selam", caret: 2 });
  });

  it("replaces a selection, whichever way it was made", () => {
    expect(insertText("bu çok iyi", 3, 6, "🔥", 300)).toEqual({ value: "bu 🔥 iyi", caret: 5 });
    expect(insertText("bu çok iyi", 6, 3, "🔥", 300)).toEqual({ value: "bu 🔥 iyi", caret: 5 });
  });

  it("keeps a caret beyond the text at its end", () => {
    expect(insertText("ab", 9, 9, "😂", 300)).toEqual({ value: "ab😂", caret: 4 });
  });

  it("refuses what would pass the limit, counted in code points", () => {
    const full = "a".repeat(299);
    expect(insertText(full, 299, 299, "😂", 300)?.value).toBe(`${full}😂`);
    expect(insertText(full, 299, 299, family, 300)).toBeNull();
    expect(codePoints(family)).toBe(7);
  });
});

describe("bigEmojiCount", () => {
  it("counts a message of one to three emoji", () => {
    expect(bigEmojiCount("😂")).toBe(1);
    expect(bigEmojiCount("😂😂😂")).toBe(3);
    expect(bigEmojiCount(" 😂 🔥 ")).toBe(2);
    expect(bigEmojiCount(`❤${VS16}`)).toBe(1);
    expect(bigEmojiCount("🇹🇷")).toBe(1);
    expect(bigEmojiCount(family)).toBe(1);
    expect(bigEmojiCount(`1${VS16}⃣`)).toBe(1);
  });

  it("is 0 for text, for more than three, and for nothing", () => {
    expect(bigEmojiCount("😂😂😂😂")).toBe(0);
    expect(bigEmojiCount("haha 😂")).toBe(0);
    expect(bigEmojiCount("1")).toBe(0);
    expect(bigEmojiCount("")).toBe(0);
    expect(bigEmojiCount("   ")).toBe(0);
  });
});
