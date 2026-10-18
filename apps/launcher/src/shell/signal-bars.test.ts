import { signalLevel } from "@pl/ui";
import { describe, expect, it } from "vitest";

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
