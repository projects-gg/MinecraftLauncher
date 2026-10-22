import { readFileSync } from "node:fs";
import { describe, expect, it } from "vitest";
import { parseEmojiData } from "./data";
import { searchEmoji } from "./search";

const read = (lang: string) =>
  parseEmojiData(readFileSync(new URL(`./data/emoji.${lang}.tsv`, import.meta.url), "utf8"));
const tr = read("tr");
const en = read("en");
const top = (data: typeof tr, query: string, count = 5) =>
  searchEmoji(data, query)
    .slice(0, count)
    .map((entry) => entry.emoji);

describe("searchEmoji", () => {
  it("finds nothing for an empty query", () => {
    expect(searchEmoji(tr, "")).toEqual([]);
    expect(searchEmoji(tr, "   ")).toEqual([]);
  });

  it("puts the emoji whose name is the query first", () => {
    expect(top(tr, "ateş")[0]).toBe("🔥");
    expect(top(tr, "kırmızı kalp")[0]).toBe("❤️");
    expect(top(en, "fire")[0]).toBe("🔥");
    expect(top(tr, "kedi")[0]).toBe("🐈");
  });

  it("puts the emoji chats use most first among equal finds", () => {
    expect(top(tr, "kalp")[0]).toBe("❤️");
    expect(top(tr, "kalp")).toContain("🫀");
    expect(top(tr, "fire")[0]).toBe("🔥");
    expect(top(tr, "kahkaha")[0]).toBe("😂");
  });

  it("puts names with the word itself before names with longer words", () => {
    const eye = top(tr, "göz", 10);
    expect(eye[0]).toBe("👁️");
    expect(eye.indexOf("😉")).toBeLessThan(eye.indexOf("👀"));
    expect(eye).toContain("👀");
  });

  it("folds Turkish: case, dotted and dotless i, marks", () => {
    expect(top(tr, "gulmek")).toContain("😂");
    expect(top(tr, "GÜLMEK")).toEqual(top(tr, "gulmek"));
    expect(top(tr, "KIRMIZI KALP")[0]).toBe("❤️");
    expect(top(tr, "ates")[0]).toBe("🔥");
    expect(top(tr, "sirit")).toContain("😀");
    expect(top(tr, "SIRIT")).toEqual(top(tr, "sirit"));
  });

  it("finds English words in a Turkish search, after the Turkish ones", () => {
    expect(top(tr, "fire")).toContain("🔥");
    expect(top(tr, "skull")).toContain("💀");
    const kalp = searchEmoji(tr, "kalp");
    expect(kalp.length).toBeGreaterThan(5);
    expect(kalp[0]?.name).toContain("kalp");
  });

  it("finds flags by their country", () => {
    expect(top(tr, "türkiye")).toContain("🇹🇷");
    expect(top(tr, "turk")).toContain("🇹🇷");
    expect(top(en, "turkey")).toContain("🇹🇷");
  });

  it("needs every word of the query", () => {
    const both = searchEmoji(tr, "kırmızı kalp").map((entry) => entry.emoji);
    expect(both).toContain("❤️");
    expect(both).not.toContain("💙");
  });

  it("finds a pasted emoji, with or without its skin tone", () => {
    expect(top(tr, "😂")).toEqual(["😂"]);
    expect(top(tr, "👍🏽")).toEqual(["👍"]);
    expect(top(tr, "❤️")).toEqual(["❤️"]);
  });

  it("keeps Unicode's order within a rank and never repeats", () => {
    const found = searchEmoji(tr, "yüz");
    expect(new Set(found).size).toBe(found.length);
    expect(found.length).toBeGreaterThan(50);
  });
});
