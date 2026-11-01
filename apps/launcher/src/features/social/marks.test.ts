import { MARK_COLORS, MARK_GLYPHS, MARK_SHAPES } from "@pl/ui";
import { describe, expect, it } from "vitest";
import type { MarkDef } from "@/lib/api/social-types";
import { en } from "@/locales/en";
import { tr } from "@/locales/tr";
import {
  BUILTIN_MARK_DEFS,
  heldMarks,
  hiddenAfter,
  markedName,
  markItems,
  sentenceParts,
  type Translate,
} from "./marks";

/** A tiny translator over the real strings (`{{x}}` placeholders only). */
function translator(strings: Record<string, unknown>): Translate {
  return (key, options = {}) => {
    const found = key
      .split(".")
      .reduce<unknown>(
        (node, part) =>
          node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
        strings,
      );
    const text = typeof found === "string" ? found : key;
    return text.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(options[name] ?? ""));
  };
}

const CLOUD: MarkDef = {
  key: "cloud",
  name: { tr: "Bulut AFK kullanıcısı", en: "Cloud AFK user" },
  color: "teal",
  glyph: "cloud",
  shape: "round",
};

describe("profile badges (BADGES-UI)", () => {
  it("carries the service's built-in catalogue, every look one this version draws", () => {
    expect(BUILTIN_MARK_DEFS.map((def) => def.key)).toEqual([
      "admin",
      "moderator",
      "staff",
      "verified",
      "premium",
    ]);
    for (const def of BUILTIN_MARK_DEFS) {
      expect(MARK_COLORS).toContain(def.color);
      expect(MARK_GLYPHS).toContain(def.glyph);
      expect(MARK_SHAPES).toContain(def.shape);
    }
  });

  it("draws the badges the catalogue knows, in the keys' order, named in the language", () => {
    const defs = [...BUILTIN_MARK_DEFS, CLOUD];
    const turkish = markItems(["admin", "cloud"], defs, false);
    expect(turkish.map((item) => [item.key, item.label])).toEqual([
      ["admin", "Projects yöneticisi"],
      ["cloud", "Bulut AFK kullanıcısı"],
    ]);
    expect(turkish[1]).toMatchObject({ color: "teal", glyph: "cloud", shape: "round" });
    expect(markItems(["premium"], defs, true)[0]?.label).toBe("Premium Club member");
    // A key a newer catalogue made, a look this version cannot draw, a key twice: left out.
    const newer = { ...CLOUD, key: "m20", color: "gold" };
    expect(markItems(["m21", "m20", "verified", "verified"], [...defs, newer], false)).toEqual([
      expect.objectContaining({ key: "verified" }),
    ]);
    expect(markItems(undefined, defs, false)).toEqual([]);
  });

  it("names every badge in a row's name for screen readers, and nothing without one", () => {
    const t = translator(tr);
    const items = markItems(["admin", "premium"], BUILTIN_MARK_DEFS, false);
    expect(markedName("GeceKurdu_", items, t)).toBe(
      "GeceKurdu_, Projects yöneticisi, Premium Club üyesi",
    );
    expect(markedName("GeceKurdu_", [], t)).toBe("GeceKurdu_");
    const e = translator(en);
    expect(markedName("GeceKurdu_", markItems(["verified"], BUILTIN_MARK_DEFS, true), e)).toBe(
      "GeceKurdu_, Verified account",
    );
  });

  it("lists the player's badges shown and kept off in the catalogue's order", () => {
    const held = heldMarks(["premium", "staff"], ["verified", "staff"], BUILTIN_MARK_DEFS, false);
    expect(held.map((one) => [one.item.key, one.shown])).toEqual([
      ["staff", true],
      ["verified", false],
      ["premium", true],
    ]);
    // A service from before badges: nothing to choose.
    expect(heldMarks(undefined, undefined, BUILTIN_MARK_DEFS, false)).toEqual([]);
  });

  it("says which badges stay off after a switch", () => {
    const held = heldMarks(["staff", "premium"], ["verified"], BUILTIN_MARK_DEFS, false);
    expect(hiddenAfter(held, "premium", false)).toEqual(["verified", "premium"]);
    expect(hiddenAfter(held, "verified", true)).toEqual([]);
    expect(hiddenAfter(held, "staff", true)).toEqual(["verified"]);
  });

  it("splits a sentence around its name, whatever the language's order", () => {
    expect(sentenceParts((name) => `${name} seni birlikte oynamaya çağırıyor`)).toEqual([
      "",
      " seni birlikte oynamaya çağırıyor",
    ]);
    expect(sentenceParts((name) => `Davet eden: ${name}.`)).toEqual(["Davet eden: ", "."]);
    // Without the name, or with it twice, the sentence stays whole.
    expect(sentenceParts(() => "Ekibinden 4 kişi Projects'te")).toBeNull();
    expect(sentenceParts((name) => `${name} ve ${name}`)).toBeNull();
  });
});
