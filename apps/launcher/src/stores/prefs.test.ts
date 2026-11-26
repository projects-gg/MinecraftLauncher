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

  it("keep the overlay's mod out for a player who took it out, once its tweaks come (DIALOG-FOCUS)", () => {
    // Both switches off: the mod was out of their games; its client tweaks start off too.
    expect(migratePrefs({ inGameOverlay: false, chatEmoji: false }, 4)).toEqual({
      inGameOverlay: false,
      chatEmoji: false,
      dialogFocus: false,
      passwordEye: false,
      tooltipFit: false,
    });
    // Either on, or never touched: the tweaks come with their default (on).
    expect(migratePrefs({ inGameOverlay: false, chatEmoji: true }, 4)).not.toHaveProperty(
      "dialogFocus",
    );
    expect(migratePrefs({ inGameOverlay: false }, 4)).not.toHaveProperty("dialogFocus");
    expect(migratePrefs({ language: "tr" }, 4)).not.toHaveProperty("dialogFocus");
    // Saved by this version: the player's own choice stays.
    expect(
      migratePrefs({ inGameOverlay: false, chatEmoji: false, dialogFocus: true }, 5).dialogFocus,
    ).toBe(true);
    expect(usePrefs.getInitialState().dialogFocus).toBe(true);
  });

  it("start the newer client tweaks off for a player with both switches off (TIP-EYE)", () => {
    // Saved by version 5: both switches off, the first tweak as the player left it.
    expect(migratePrefs({ inGameOverlay: false, chatEmoji: false, dialogFocus: true }, 5)).toEqual({
      inGameOverlay: false,
      chatEmoji: false,
      dialogFocus: true,
      passwordEye: false,
      tooltipFit: false,
    });
    // Either switch on, or never touched: they come with their default (on).
    for (const saved of [{ inGameOverlay: true, chatEmoji: false }, { chatEmoji: false }, {}]) {
      const migrated = migratePrefs({ ...saved, dialogFocus: false }, 5);
      expect(migrated).not.toHaveProperty("passwordEye");
      expect(migrated).not.toHaveProperty("tooltipFit");
      expect(migrated.dialogFocus).toBe(false);
    }
    // Saved by this version: the player's own choice stays.
    const kept = migratePrefs(
      { inGameOverlay: false, chatEmoji: false, passwordEye: true, tooltipFit: true },
      6,
    );
    expect([kept.passwordEye, kept.tooltipFit]).toEqual([true, true]);
    const fresh = usePrefs.getInitialState();
    expect([fresh.passwordEye, fresh.tooltipFit]).toEqual([true, true]);
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
