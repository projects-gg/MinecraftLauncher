import { describe, expect, it } from "vitest";
import { migratePrefs } from "./prefs";

describe("saved preferences of older versions", () => {
  it("turn the old only language into the automatic one", () => {
    // Turkish was the only language before version 3: never the player's pick.
    expect(migratePrefs({ language: "tr", sounds: false }, 2)).toEqual({
      language: "auto",
      sounds: false,
    });
    expect(migratePrefs({ sounds: true }, 2).language).toBe("auto");
  });

  it("keep English, which only a player could have chosen", () => {
    expect(migratePrefs({ language: "en" }, 2).language).toBe("en");
  });

  it("leave current preferences alone", () => {
    expect(migratePrefs({ language: "tr" }, 3).language).toBe("tr");
  });
});
