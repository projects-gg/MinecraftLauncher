import { describe, expect, it } from "vitest";
import {
  graphicsChoices,
  hasAdvanced,
  pickFor,
  replacesCustom,
  savedChoice,
} from "./settings-form";

describe("graphics control", () => {
  it("shows the file's preset, the first launch's, or Özel", () => {
    expect(savedChoice({ kind: "preset", preset: "balanced" })).toBe("balanced");
    expect(savedChoice({ kind: "firstLaunch", preset: "smooth" })).toBe("smooth");
    expect(savedChoice({ kind: "custom" })).toBe("custom");
    expect(savedChoice({ kind: "unsupported" })).toBeNull();
  });

  it("offers Özel only while the file has values of its own", () => {
    expect(graphicsChoices("balanced")).toEqual(["smooth", "balanced", "quality"]);
    expect(graphicsChoices("custom")).toEqual(["smooth", "balanced", "quality", "custom"]);
    expect(graphicsChoices(null)).toEqual(["smooth", "balanced", "quality"]);
  });

  it("keeps a pick only when it changes the file", () => {
    expect(pickFor("balanced", "quality")).toBe("quality");
    expect(pickFor("balanced", "balanced")).toBeUndefined();
    expect(pickFor("custom", "smooth")).toBe("smooth");
    // "Özel" goes back to the player's own values.
    expect(pickFor("custom", "custom")).toBeUndefined();
  });

  it("says so only when a pick replaces the player's own values", () => {
    expect(replacesCustom("custom", "smooth")).toBe(true);
    expect(replacesCustom("custom", undefined)).toBe(false);
    expect(replacesCustom("balanced", "smooth")).toBe(false);
  });
});

describe("advanced settings", () => {
  const empty = { jvmArgs: "", preLaunch: "", postExit: "", wrapper: "" };

  it("start open when something is set in them", () => {
    expect(hasAdvanced(empty)).toBe(false);
    expect(hasAdvanced({ ...empty, jvmArgs: "  " })).toBe(false);
    expect(hasAdvanced({ ...empty, jvmArgs: "-XX:+UseG1GC" })).toBe(true);
    expect(hasAdvanced({ ...empty, wrapper: "prime-run" })).toBe(true);
    expect(hasAdvanced({ ...empty, postExit: "notify-send bitti" })).toBe(true);
  });
});
