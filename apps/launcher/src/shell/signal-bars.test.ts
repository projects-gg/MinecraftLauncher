import { signalLevel, signalTone } from "@pl/ui";
import { describe, expect, it } from "vitest";

describe("signalTone", () => {
  it("colours the bars by how many are lit: all green, one missing amber, more red", () => {
    expect(signalTone(31)).toBe("good");
    expect(signalTone(149)).toBe("good");
    expect(signalTone(150)).toBe("fair");
    expect(signalTone(299)).toBe("fair");
    expect(signalTone(300)).toBe("poor");
    expect(signalTone(4200)).toBe("poor");
  });

  it("reads an unknown latency as good, as its five bars do", () => {
    expect(signalTone(null)).toBe("good");
    expect(signalTone(undefined)).toBe("good");
  });
});

describe("signalLevel", () => {
  it("lights the bars the game's server list lights", () => {
    expect(signalLevel(31)).toBe(5);
    expect(signalLevel(149)).toBe(5);
    expect(signalLevel(150)).toBe(4);
    expect(signalLevel(299)).toBe(4);
    expect(signalLevel(300)).toBe(3);
    expect(signalLevel(599)).toBe(3);
    expect(signalLevel(600)).toBe(2);
    expect(signalLevel(999)).toBe(2);
    expect(signalLevel(1000)).toBe(1);
    expect(signalLevel(4200)).toBe(1);
  });

  it("shows every bar when the latency is not known", () => {
    expect(signalLevel(null)).toBe(5);
    expect(signalLevel(undefined)).toBe(5);
  });
});
