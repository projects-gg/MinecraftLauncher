import { describe, expect, it } from "vitest";
import { confirmMatches, removedNames } from "./model";

describe("confirmMatches", () => {
  it("takes the word whatever its case, spaces or i", () => {
    for (const typed of ["sıfırla", "SIFIRLA", "Sıfırla", "sifirla", "SİFİRLA", "  sıfırla "]) {
      expect(confirmMatches(typed, "sıfırla"), typed).toBe(true);
    }
    for (const typed of ["reset", "RESET", " Reset"]) {
      expect(confirmMatches(typed, "reset"), typed).toBe(true);
    }
  });

  it("refuses anything else, and an empty word", () => {
    for (const typed of ["", "sıfırl", "sıfırlaa", "sıfır la", "evet", "reset"]) {
      expect(confirmMatches(typed, "sıfırla"), typed).toBe(false);
    }
    expect(confirmMatches("", "")).toBe(false);
    expect(confirmMatches("  ", " ")).toBe(false);
  });
});

describe("removedNames", () => {
  it("lists accounts and AFK accounts by name, in order", () => {
    expect(
      removedNames(
        [{ username: "GeceKartali" }, { username: "Blok_Ustasi" }],
        [{ nickname: "AFK_Bekci" }],
      ),
    ).toEqual({ accounts: ["GeceKartali", "Blok_Ustasi"], afk: ["AFK_Bekci"] });
  });

  it("is empty while nothing is loaded", () => {
    expect(removedNames(undefined, undefined)).toEqual({ accounts: [], afk: [] });
  });
});
