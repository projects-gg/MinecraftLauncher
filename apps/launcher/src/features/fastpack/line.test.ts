import { describe, expect, it } from "vitest";
import { fastPackLine } from "./line";

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
