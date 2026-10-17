import { describe, expect, it } from "vitest";
import {
  firstStep,
  frameStats,
  framesLookSlow,
  hardwareTier,
  parsePerfPreview,
  type Tier,
} from "./suggest";

const frames = (ms: number, count: number) => Array.from({ length: count }, () => ms);

describe("hardwareTier", () => {
  it.each<[string, { memoryMb?: number; cores?: number }, Tier]>([
    ["4 GB laptop with a graphics share", { memoryMb: 3_480, cores: 4 }, "veryWeak"],
    ["4 GB reported in full", { memoryMb: 4_096, cores: 8 }, "veryWeak"],
    ["two threads", { memoryMb: 16_000, cores: 2 }, "veryWeak"],
    ["8 GB with 2 GB taken by graphics", { memoryMb: 5_950, cores: 8 }, "weak"],
    ["8 GB reported in full", { memoryMb: 8_030, cores: 12 }, "weak"],
    ["four threads", { memoryMb: 16_000, cores: 4 }, "weak"],
    ["12 GB and six threads", { memoryMb: 11_900, cores: 6 }, "capable"],
    ["16 GB and sixteen threads", { memoryMb: 16_288, cores: 16 }, "capable"],
  ])("%s", (_name, hardware, tier) => {
    expect(hardwareTier(hardware)).toBe(tier);
  });

  it("decides on what it knows and ignores missing or broken figures", () => {
    expect(hardwareTier({ cores: 8 })).toBe("capable");
    expect(hardwareTier({ memoryMb: 4_000 })).toBe("veryWeak");
    expect(hardwareTier({ memoryMb: 0, cores: 0 })).toBe("capable");
    expect(hardwareTier({ memoryMb: Number.NaN, cores: 3 })).toBe("weak");
    expect(hardwareTier({})).toBe("capable");
  });
});

describe("firstStep", () => {
  it("switches on a very weak computer, measures a weak one, leaves the rest", () => {
    expect(firstStep("veryWeak", false)).toBe("switch");
    expect(firstStep("weak", false)).toBe("measure");
    expect(firstStep("capable", false)).toBe("none");
  });

  it("does nothing when the mode is already on", () => {
    expect(firstStep("veryWeak", true)).toBe("none");
    expect(firstStep("weak", true)).toBe("none");
  });
});

describe("frame verdict", () => {
  it("finds nothing wrong at 60 or 144 frames a second", () => {
    expect(framesLookSlow(frameStats(frames(16.7, 180)))).toBe(false);
    expect(framesLookSlow(frameStats(frames(6.9, 430)))).toBe(false);
  });

  it("calls a steady 30 frames a second slow (median over 22 ms)", () => {
    const stats = frameStats(frames(33.3, 90));
    expect(stats.medianMs).toBeCloseTo(33.3);
    expect(framesLookSlow(stats)).toBe(true);
  });

  it("calls a stutter slow when a fifth of the frames run long", () => {
    const stutter = [...frames(16.7, 80), ...frames(50, 20)];
    const stats = frameStats(stutter);
    expect(stats.medianMs).toBeCloseTo(16.7);
    expect(stats.longShare).toBeCloseTo(0.2);
    expect(framesLookSlow(stats)).toBe(true);
  });

  it("forgives a few hitches", () => {
    const hitches = [...frames(16.7, 170), ...frames(120, 3), 900];
    expect(framesLookSlow(frameStats(hitches))).toBe(false);
  });

  it("stays on the edges as written", () => {
    expect(framesLookSlow(frameStats(frames(22, 100)))).toBe(false);
    expect(framesLookSlow(frameStats(frames(22.5, 100)))).toBe(true);
    // 34 ms exactly is not a long frame.
    expect(framesLookSlow(frameStats([...frames(16.7, 60), ...frames(34, 40)]))).toBe(false);
  });

  it("takes the middle of an even count", () => {
    expect(frameStats([10, 20, 30, 40]).medianMs).toBe(25);
  });

  it("has no verdict without frames", () => {
    const stats = frameStats([]);
    expect(stats.frames).toBe(0);
    expect(framesLookSlow(stats)).toBe(false);
  });
});

describe("parsePerfPreview", () => {
  it("reads the preview flag", () => {
    expect(parsePerfPreview("?perf=veryweak")?.hardware).toEqual({ memoryMb: 4096, cores: 2 });
    expect(hardwareTier(parsePerfPreview("?perf=veryweak")?.hardware ?? {})).toBe("veryWeak");
    const weak = parsePerfPreview("?news=a&perf=weak");
    expect(weak && hardwareTier(weak.hardware)).toBe("weak");
    expect(weak?.frameLoadMs).toBeGreaterThan(20);
  });

  it("ignores everything else", () => {
    expect(parsePerfPreview("")).toBeNull();
    expect(parsePerfPreview("?perf=fast")).toBeNull();
  });
});
