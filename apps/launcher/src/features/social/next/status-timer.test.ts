import { describe, expect, it } from "vitest";
import { STATUS_PRESETS, statusUntil, timerDelay, timerVerdict } from "./status-timer";

describe("timed statuses", () => {
  // 3 Nov 2026 14:17:20 local.
  const now = new Date(2026, 10, 3, 14, 17, 20).getTime();

  it("offers the crew mute's four choices", () => {
    expect(STATUS_PRESETS).toEqual(["hour", "eightHours", "morning", "forever"]);
  });

  it("ends on the next whole minute's hour, tomorrow at eight, or never", () => {
    expect(new Date(statusUntil("hour", now) ?? 0).getHours()).toBe(15);
    expect(new Date(statusUntil("hour", now) ?? 0).getMinutes()).toBe(18);
    const morning = new Date(statusUntil("morning", now) ?? 0);
    expect([morning.getDate(), morning.getHours(), morning.getMinutes()]).toEqual([4, 8, 0]);
    expect(statusUntil("forever", now)).toBeNull();
  });

  it("waits, ends, or drops when the status changed meanwhile", () => {
    const timer = { status: "dnd" as const, until: now + 60_000 };
    expect(timerVerdict(null, "dnd", now)).toBe("none");
    expect(timerVerdict(timer, "dnd", now)).toBe("wait");
    expect(timerVerdict(timer, undefined, now)).toBe("wait");
    expect(timerVerdict(timer, "dnd", now + 60_000)).toBe("end");
    // Changed by hand or on another device: the timer goes, nothing else changes.
    expect(timerVerdict(timer, "online", now)).toBe("drop");
    expect(timerVerdict(timer, "invisible", now + 120_000)).toBe("drop");
    expect(timerDelay(timer, now)).toBe(60_000);
    expect(timerDelay(timer, now + 59_900)).toBe(1_000);
  });
});
