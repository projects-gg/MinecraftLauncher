import { describe, expect, it } from "vitest";
import { afkRunning, confirmMatches, removedNames } from "./model";

describe("afkRunning", () => {
  const afk = (...kinds: string[]) => kinds.map((kind) => ({ state: { kind } }));

  it("is off while every AFK account rests", () => {
    expect(afkRunning(undefined)).toBe(false);
    expect(afkRunning(afk())).toBe(false);
    expect(afkRunning(afk("stopped", "error", "clientFailed"))).toBe(false);
  });

  it("is on while one runs or is on its way", () => {
    for (const kind of [
      "queued",
      "preparing",
      "starting",
      "connecting",
      "connected",
      "reconnecting",
      "stopping",
    ]) {
      expect(afkRunning(afk("stopped", kind)), kind).toBe(true);
    }
  });
});

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
