import { describe, expect, it } from "vitest";
import { formatBytes, formatDuration, formatSpeed } from "./format";
import i18n from "./i18n";

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;

describe("formatBytes", () => {
  it("keeps small sizes in their own unit", () => {
    expect(formatBytes(0)).toBe("0 B");
    expect(formatBytes(999)).toBe("999 B");
    expect(formatBytes(1.5 * KB)).toBe("1,5 KB");
    expect(formatBytes(798.7 * KB)).toBe("798,7 KB");
    expect(formatBytes(999.9 * MB)).toBe("999,9 MB");
    expect(formatBytes(1.5 * GB)).toBe("1,5 GB");
  });

  it("never shows four digits before the unit", () => {
    expect(formatBytes(1000)).toBe("1 KB");
    expect(formatBytes(1023)).toBe("1 KB");
    expect(formatBytes(1000 * KB)).toBe("1 MB");
    expect(formatBytes(1010.7 * MB)).toBe("1 GB");
    expect(formatBytes(1023.9 * MB)).toBe("1 GB");
    // 999,96 MB would round to "1.000 MB".
    expect(formatBytes(999.96 * MB)).toBe("1 GB");
  });

  it("treats negative input as nothing", () => {
    expect(formatBytes(-5)).toBe("0 B");
  });
});

describe("formatSpeed", () => {
  it("adds the per-second suffix", () => {
    expect(formatSpeed(2.5 * MB)).toBe("2,5 MB/sn");
  });
});

describe("formatDuration", () => {
  it("says hours, minutes and seconds with their units", () => {
    expect(formatDuration(2 * 3600 + 14 * 60 + 40)).toBe("2 sa 14 dk");
    expect(formatDuration(3 * 3600 + 20)).toBe("3 sa");
    expect(formatDuration(38 * 60 + 20)).toBe("38 dk");
    expect(formatDuration(45)).toBe("45 sn");
    expect(formatDuration(-3)).toBe("0 sn");
  });

  it("takes its units from the strings", async () => {
    await i18n.changeLanguage("en");
    try {
      expect(formatDuration(2 * 3600 + 14 * 60)).toBe("2 h 14 min");
      expect(formatDuration(45)).toBe("45 s");
    } finally {
      await i18n.changeLanguage("tr");
    }
  });
});
