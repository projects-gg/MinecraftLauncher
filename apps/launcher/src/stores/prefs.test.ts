import { describe, expect, it } from "vitest";
import { migratePrefs, usePrefs } from "./prefs";

describe("saved preferences of older versions", () => {
  it("turn the old only language into the automatic one", () => {
    // Turkish was the only language before version 3: never the player's pick.
    expect(migratePrefs({ language: "tr", sounds: false }, 2)).toEqual({
      language: "auto",
      sounds: false,
      closeBehavior: "quit",
    });
    expect(migratePrefs({ sounds: true }, 2).language).toBe("auto");
  });

  it("keep English, which only a player could have chosen", () => {
    expect(migratePrefs({ language: "en" }, 2).language).toBe("en");
  });

  it("leave current preferences alone", () => {
    expect(migratePrefs({ language: "tr" }, 3).language).toBe("tr");
    expect(migratePrefs({ language: "tr", closeBehavior: "tray" }, 4)).toEqual({
      language: "tr",
      closeBehavior: "tray",
    });
  });

  it("move everyone from the old tray switch to the new close default", () => {
    // On by default and saved with every other value: a saved `true` is almost always the
    // untouched default, and a saved `false` already meant quitting.
    for (const closeToTray of [true, false, undefined]) {
      const migrated = migratePrefs({ closeToTray, sounds: false }, 3);
      expect(migrated).toEqual({ sounds: false, closeBehavior: "quit" });
      expect(migrated).not.toHaveProperty("closeToTray");
    }
  });
});

describe("the close button's preference", () => {
  it("quits unless something runs, for a new player", () => {
    expect(usePrefs.getInitialState().closeBehavior).toBe("quit");
  });
});
