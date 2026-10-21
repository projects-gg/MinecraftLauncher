import { describe, expect, it } from "vitest";
import { canBlock, NOTE_MAX, noteLength, noteOf, REPORT_REASONS, reasonKey } from "./report";

describe("reports", () => {
  it("offer only the reasons the service takes for each target", () => {
    expect(REPORT_REASONS.player).not.toContain("inappropriate");
    expect(REPORT_REASONS.crew).not.toContain("profile");
    expect(REPORT_REASONS.message).not.toContain("name");
    expect(REPORT_REASONS.message[0]).toBe("harassment");
    expect(reasonKey("player", "name")).toBe("reports.reasons.playerName");
    expect(reasonKey("crew", "name")).toBe("reports.reasons.crewName");
    expect(reasonKey("message", "spam")).toBe("reports.reasons.spam");
  });

  it("count the note as the service does", () => {
    expect(noteLength("  çok   kötü \n söz  ")).toBe(12);
    expect(noteOf("   ")).toBeUndefined();
    expect(noteOf(" kaba  ")).toBe("kaba");
    expect(noteLength("ş".repeat(NOTE_MAX))).toBe(NOTE_MAX);
  });

  it("block players and senders, never a crew", () => {
    expect(canBlock("player")).toBe(true);
    expect(canBlock("message")).toBe(true);
    expect(canBlock("crew")).toBe(false);
  });
});
