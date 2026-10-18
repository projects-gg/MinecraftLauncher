import { afterEach, describe, expect, it } from "vitest";
import {
  dateTimeFormat,
  formatBytes,
  formatCount,
  formatDuration,
  formatMemory,
  formatPercent,
  formatRelative,
  formatSpeed,
  listFormat,
  uiLocale,
} from "./format";
import i18n, { loadEnglish } from "./i18n";

const KB = 1024;
const MB = KB * 1024;
const GB = MB * 1024;

async function inEnglish() {
  await loadEnglish();
  await i18n.changeLanguage("en");
}

afterEach(async () => {
  await i18n.changeLanguage("tr");
});

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

  it("writes decimals the English way in English", async () => {
    await inEnglish();
    expect(formatBytes(1.5 * GB)).toBe("1.5 GB");
    expect(formatBytes(798.7 * KB)).toBe("798.7 KB");
  });
});

describe("formatSpeed", () => {
  it("adds the per-second suffix of the language", async () => {
    expect(formatSpeed(2.5 * MB)).toBe("2,5 MB/sn");
    await inEnglish();
    expect(formatSpeed(2.5 * MB)).toBe("2.5 MB/s");
  });
});

describe("numbers and shares", () => {
  it("group thousands the language's way", async () => {
    expect(formatCount(12345)).toBe("12.345");
    await inEnglish();
    expect(formatCount(12345)).toBe("12,345");
  });

  it("put the percent sign where the language puts it", async () => {
    expect(formatPercent(42)).toBe("%42");
    expect(formatPercent(99.6)).toBe("%100");
    await inEnglish();
    expect(formatPercent(42)).toBe("42%");
  });

  it("say memory in GB from a gigabyte on", async () => {
    expect(formatMemory(6144)).toBe("6 GB");
    expect(formatMemory(4608)).toBe("4,5 GB");
    expect(formatMemory(768)).toBe("768 MB");
    await inEnglish();
    expect(formatMemory(4608)).toBe("4.5 GB");
  });

  it("join names with the language's word for and", async () => {
    expect(listFormat().format(["Sodium", "Iris", "Lithium"])).toBe("Sodium, Iris ve Lithium");
    await inEnglish();
    expect(listFormat().format(["Sodium", "Iris", "Lithium"])).toBe("Sodium, Iris and Lithium");
  });
});

describe("dates", () => {
  const now = new Date(2026, 9, 18, 12, 0).getTime();

  it("use British English next to Turkish", async () => {
    expect(uiLocale()).toBe("tr-TR");
    await inEnglish();
    expect(uiLocale()).toBe("en-GB");
    // Day first and a 24-hour clock, like the Turkish dates.
    const at = new Date(2026, 9, 25, 20, 5);
    expect(dateTimeFormat({ day: "numeric", month: "long" }).format(at)).toBe("25 October");
    expect(dateTimeFormat({ hour: "2-digit", minute: "2-digit" }).format(at)).toBe("20:05");
  });

  it("say how long ago in the language", async () => {
    const yesterday = new Date(now - 24 * 3600_000).toISOString();
    const twoDays = new Date(now - 48 * 3600_000).toISOString();
    const inTwoDays = new Date(now + 48 * 3600_000).toISOString();
    const earlier = new Date(now - 2 * 3600_000).toISOString();
    const seconds = new Date(now - 20_000).toISOString();
    expect(formatRelative(yesterday, now)).toBe("dün");
    // Numbers past one step: not Intl's dated "evvelsi gün" / "öbür gün".
    expect(formatRelative(twoDays, now)).toBe("2 gün önce");
    expect(formatRelative(inTwoDays, now)).toBe("2 gün sonra");
    expect(formatRelative(earlier, now)).toBe("2 saat önce");
    // Not Intl's "bu dakika".
    expect(formatRelative(seconds, now)).toBe("az önce");
    expect(formatRelative(new Date(2026, 6, 3).toISOString(), now)).toBe("3 Temmuz");
    await inEnglish();
    expect(formatRelative(yesterday, now)).toBe("yesterday");
    expect(formatRelative(twoDays, now)).toBe("2 days ago");
    expect(formatRelative(earlier, now)).toBe("2 hours ago");
    expect(formatRelative(seconds, now)).toBe("just now");
    expect(formatRelative(new Date(2026, 6, 3).toISOString(), now)).toBe("3 July");
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
    await inEnglish();
    expect(formatDuration(2 * 3600 + 14 * 60)).toBe("2 h 14 min");
    expect(formatDuration(45)).toBe("45 s");
  });
});
