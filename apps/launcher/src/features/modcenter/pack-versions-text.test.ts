import { describe, expect, it } from "vitest";
import { shortVersion } from "./pack-versions-text";

describe("shortVersion", () => {
  it("drops the build suffix that names the game version", () => {
    expect(shortVersion("0.155.3+26.1.2")).toBe("0.155.3");
    expect(shortVersion("1.12.0")).toBe("1.12.0");
    expect(shortVersion("mc26.1.2-0.9.2-fabric")).toBe("mc26.1.2-0.9.2-fabric");
    expect(shortVersion("+odd")).toBe("+odd");
  });
});
