import { afterEach, describe, expect, it } from "vitest";
import type { HealthIssue, KeyChange } from "@/lib/api";
import i18n, { loadEnglish } from "@/lib/i18n";
import { actionName, changeSpoken, keyClashText, keyLabel, newKeysLine } from "./key-texts";
import { issueText } from "./texts";

const t = i18n.t.bind(i18n);

// What a file from before the pack's key table gets moved (crates/pl-mc/data/key-bindings.json).
const ZOOM: KeyChange = {
  name: "key.wi_zoom.zoom",
  modName: "WI Zoom",
  action: "Zoom",
  partner: "key.voice_chat",
  key: "key.keyboard.v",
  to: "key.keyboard.c",
};
const GAMMA: KeyChange = {
  name: "key.gammautils.gammaToggle",
  modName: "Gamma Utils",
  action: "Toggle gamma",
  partner: "key.quickActions",
  key: "key.keyboard.g",
  to: "key.keyboard.x",
};
const ICONS: KeyChange = {
  name: "key.hide_icons",
  modName: "Simple Voice Chat",
  action: "Hide icons",
  partner: "key.gammautils.nightVisionToggle",
  key: "key.keyboard.h",
};
const SHADERS: KeyChange = {
  name: "iris.keybind.reload",
  modName: "Iris",
  action: "Reload shaders",
  partner: "gui.xaero_quick_eat",
  key: "key.keyboard.r",
};
const CHANGES = [ZOOM, GAMMA, ICONS, SHADERS];

afterEach(async () => {
  await i18n.changeLanguage("tr");
});

describe("key clash texts", () => {
  it("names keys the way the keyboard shows them", () => {
    expect(keyLabel(t, "key.keyboard.v")).toBe("V");
    expect(keyLabel(t, "key.keyboard.i")).toBe("I");
    expect(keyLabel(t, "key.keyboard.4")).toBe("4");
    expect(keyLabel(t, "key.keyboard.caps.lock")).toBe("Caps Lock");
    expect(keyLabel(t, "key.keyboard.left.shift")).toBe("Left Shift");
    expect(keyLabel(t, "key.mouse.4")).toBe("Fare 4");
    expect(keyLabel(t, "key.keyboard.unknown")).toBe("Atanmamış");
    expect(keyLabel(t, undefined)).toBe("Atanmamış");
  });

  it("says what each binding does, the table's words for others", () => {
    expect(CHANGES.map((c) => actionName(t, c))).toEqual([
      "Yakınlaştır",
      "Tam parlaklık",
      "Simgeleri gizle",
      "Gölgelendiricileri yenile",
    ]);
    expect(actionName(t, { ...ZOOM, name: "key.other.mod", action: "Do a thing" })).toBe(
      "Do a thing",
    );
  });

  it("has one short line for the finding and the toast", () => {
    expect(keyClashText(t, CHANGES)).toBe("Mod tuşları çakışıyor: V, G, H ve R.");
    expect(keyClashText(t, [ZOOM])).toBe("Mod tuşları çakışıyor: V.");
    expect(newKeysLine(t, CHANGES)).toBe("Yeni tuşlar: Yakınlaştır C ve Tam parlaklık X.");
    // Only keys taken away: nothing to list.
    expect(newKeysLine(t, [ICONS, SHADERS])).toBeUndefined();
    const issue: HealthIssue = {
      key: "KeyClash|key.wi_zoom.zoom,key.gammautils.gammaToggle|",
      severity: "warning",
      kind: "keyClash",
      mods: ["WI Zoom", "Gamma Utils"],
      bundled: false,
      files: ["options.txt"],
      offer: { label: "fix", count: 2 },
      keys: [ZOOM, GAMMA],
    };
    expect(issueText(t, issue, "26.1.2")).toBe("Mod tuşları çakışıyor: V ve G.");
  });

  it("reads each change aloud, before and after the fix", () => {
    expect(changeSpoken(t, ZOOM, false)).toBe("V yerine C");
    expect(changeSpoken(t, ICONS, false)).toBe("H tuşu kaldırılır");
    expect(changeSpoken(t, ZOOM, true)).toBe("Artık C");
    expect(changeSpoken(t, ICONS, true)).toBe("Artık atanmamış");
  });

  it("speaks English too", async () => {
    await loadEnglish();
    await i18n.changeLanguage("en");
    expect(keyClashText(t, CHANGES)).toBe("Some mods share keys: V, G, H and R.");
    expect(newKeysLine(t, CHANGES)).toBe("New keys: Zoom C and Full brightness X.");
    expect(keyLabel(t, undefined)).toBe("Not bound");
    expect(keyLabel(t, "key.mouse.5")).toBe("Mouse 5");
    expect(changeSpoken(t, ZOOM, false)).toBe("V becomes C");
  });
});
