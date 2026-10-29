import { describe, expect, it } from "vitest";
import { fastPackLine, fastPackRow } from "./line";

describe("fastPackLine", () => {
  it("adds nothing while the switch works or nothing is known", () => {
    expect(fastPackLine(undefined)).toBe("none");
    expect(fastPackLine({ shown: true, official: true, available: true })).toBe("none");
    // Hidden behind Studio's flag: no line either.
    expect(fastPackLine({ shown: false, official: false, available: false })).toBe("none");
  });

  it("names where it does nothing", () => {
    expect(fastPackLine({ shown: true, official: true, available: false })).toBe("missing");
    // An install the launcher does not manage, whatever its version.
    expect(fastPackLine({ shown: true, official: false, available: true })).toBe("notHere");
    expect(fastPackLine({ shown: true, official: false, available: false })).toBe("notHere");
  });
});

describe("fastPackRow", () => {
  it("says what it does and follows the preference where it works", () => {
    expect(fastPackRow(true, "none", "26.1.2")).toEqual({
      help: "fastPack.help",
      version: "26.1.2",
      checked: true,
      disabled: false,
    });
    expect(fastPackRow(false, "none", "26.1.2").checked).toBe(false);
  });

  it("puts the reason in the line's place and holds the switch off where it does nothing", () => {
    expect(fastPackRow(true, "missing", "26.3")).toEqual({
      help: "fastPack.missing",
      version: "26.3",
      checked: false,
      disabled: true,
    });
    expect(fastPackRow(true, "notHere", "26.1.2")).toMatchObject({
      help: "fastPack.notHere",
      checked: false,
      disabled: true,
    });
    // The preference itself is never the reason: off or on, the switch is held the same way.
    expect(fastPackRow(false, "notHere", undefined)).toMatchObject({
      help: "fastPack.notHere",
      checked: false,
      disabled: true,
    });
  });

  it("claims nothing for a version it does not know", () => {
    expect(fastPackRow(true, "missing", undefined)).toMatchObject({
      help: "fastPack.help",
      checked: true,
      disabled: false,
    });
  });
});
