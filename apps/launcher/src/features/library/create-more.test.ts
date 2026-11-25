import { describe, expect, it } from "vitest";
import { customMemoryStart, landingTab, memoryCeiling } from "./create-more";

describe("Daha Fazla Seçenek", () => {
  it("keeps 1 GB for the system and never offers less than 2 GB", () => {
    expect(memoryCeiling(16_384)).toBe(15_360);
    // Rounded down to the slider's 512 MB steps.
    expect(memoryCeiling(8_000)).toBe(6_656);
    expect(memoryCeiling(2_048)).toBe(2_048);
  });

  it("starts 'Özel' at the amount chosen before, else 4 GB, within the ceiling", () => {
    expect(customMemoryStart(16_384, null)).toBe(4_096);
    expect(customMemoryStart(16_384, 6_144)).toBe(6_144);
    expect(customMemoryStart(4_096, null)).toBe(3_072);
  });

  it("lands on the new instance's settings only when the section was open", () => {
    expect(landingTab(true)).toBe("settings");
    expect(landingTab(false)).toBeUndefined();
  });
});
