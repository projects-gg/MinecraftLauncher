import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import "@/lib/i18n";
import type { VoicePerson } from "@/lib/api/voice-types";
import i18n from "@/lib/i18n";
import { closeInMenu, fitNames, namesLines, titleNames } from "./panel-rules";

const t = i18n.getFixedT("tr") as TFunction;

const names = ["KorAtesi", "x7Ender", "MaviTilki", "Zumrut42"];
/** A line `width` letters long. */
const upTo = (width: number) => (line: string) => line.length <= width;

describe("an unnamed room's names on a short line", () => {
  it("says fewer names and how many more, then the first name, never stopping inside one", () => {
    expect(namesLines(names, t)).toEqual([
      "KorAtesi, x7Ender ve 2 kişi daha",
      "KorAtesi ve 3 kişi daha",
      "KorAtesi…",
    ]);
    expect(namesLines(["KorAtesi", "x7Ender", "MaviTilki"], t)).toEqual([
      "KorAtesi, x7Ender, MaviTilki",
      "KorAtesi, x7Ender ve 1 kişi daha",
      "KorAtesi ve 2 kişi daha",
      "KorAtesi…",
    ]);
    expect(namesLines(["KorAtesi"], t)).toEqual(["KorAtesi"]);
    expect(namesLines([], t)).toEqual([]);
  });

  it("takes the longest line that fits, the shortest when none does", () => {
    expect(fitNames(names, upTo(40), t)).toBe("KorAtesi, x7Ender ve 2 kişi daha");
    expect(fitNames(names, upTo(30), t)).toBe("KorAtesi ve 3 kişi daha");
    expect(fitNames(names, upTo(12), t)).toBe("KorAtesi…");
    expect(fitNames(names, upTo(4), t)).toBe("KorAtesi…");
    expect(fitNames(["KorAtesi"], upTo(4), t)).toBe("KorAtesi");
  });

  it("fits only a title made of who is in the room", () => {
    const people = [{ id: "a" }, { id: "b" }] as VoicePerson[];
    const nameOf = (id: string) => (id === "a" ? "KorAtesi" : "x7Ender");
    expect(titleNames({ name: null, general: false, people }, nameOf)).toEqual([
      "KorAtesi",
      "x7Ender",
    ]);
    expect(titleNames({ name: "Taş Ocağı", general: false, people }, nameOf)).toBeNull();
    expect(titleNames({ name: "Genel", general: true, people }, nameOf)).toBeNull();
    expect(titleNames({ name: null, general: false, people: [] }, nameOf)).toBeNull();
  });
});

describe("the header's room menu", () => {
  const people = [{ id: "a" }] as VoicePerson[];

  it("offers closing the room to whoever may, with others in it and not in a call of two", () => {
    expect(closeInMenu({ canClose: true, people }, false)).toBe(true);
    expect(closeInMenu({ canClose: false, people }, false)).toBe(false);
    // Alone, closing is leaving.
    expect(closeInMenu({ canClose: true, people: [] }, false)).toBe(false);
    // A call of two: leaving is hanging up.
    expect(closeInMenu({ canClose: true, people }, true)).toBe(false);
  });
});
