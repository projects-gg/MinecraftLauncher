import { describe, expect, it } from "vitest";
import { fitNames, listDigest, missingNames } from "./model";

const accounts = (...names: string[]) => names.map((username) => ({ username }));

describe("missingNames", () => {
  it("offers only player names the launcher has in no letter case, each once", () => {
    expect(
      missingNames(
        ["KorAtesi", "x7Ender", "gece_kusu", "Gece_Kusu", "ab", "bad name", "Çağrı_1", "Tilki_42"],
        accounts("koratesi", "TILKI_42"),
      ),
    ).toEqual(["x7Ender", "gece_kusu"]);
  });

  it("offers nothing when every name is there", () => {
    expect(missingNames(["KorAtesi"], accounts("KORATESI"))).toEqual([]);
    expect(missingNames([], accounts("KorAtesi"))).toEqual([]);
  });
});

describe("listDigest", () => {
  it("ignores order, letter case and repeats", () => {
    expect(listDigest(["KorAtesi", "x7Ender"])).toBe(
      listDigest(["X7ENDER", "koratesi", "x7ender"]),
    );
  });

  it("changes when the list changes", () => {
    expect(listDigest(["KorAtesi"])).not.toBe(listDigest(["KorAtesi", "x7Ender"]));
    expect(listDigest([])).toMatch(/^[0-9a-f]{8}$/);
  });
});

describe("fitNames", () => {
  const width = (text: string) => text.length;
  const more = (count: number) => `+${count}`;
  const names = ["KorAtesi", "x7Ender", "Gece_Kusu", "Tilki_42"];

  it("shows every name that fits with the rest counted", () => {
    expect(fitNames(names, 100, width, more)).toEqual({ shown: names, more: 0 });
    // "KorAtesi, x7Ender +2" is 20 characters.
    expect(fitNames(names, 20, width, more)).toEqual({ shown: ["KorAtesi", "x7Ender"], more: 2 });
    expect(fitNames(names, 19, width, more)).toEqual({ shown: ["KorAtesi"], more: 3 });
  });

  it("keeps one name however narrow the line is", () => {
    expect(fitNames(names, 3, width, more)).toEqual({ shown: ["KorAtesi"], more: 3 });
    expect(fitNames(["KorAtesi"], 3, width, more)).toEqual({ shown: ["KorAtesi"], more: 0 });
    expect(fitNames([], 30, width, more)).toEqual({ shown: [], more: 0 });
  });
});
