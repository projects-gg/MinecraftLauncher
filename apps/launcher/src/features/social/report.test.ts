import { describe, expect, it } from "vitest";
import {
  canBlock,
  NOTE_MAX,
  noteLength,
  noteOf,
  REPORT_REASONS,
  reasonKey,
  reasonsFor,
} from "./report";

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

  it("report a plan's maker for its note too (FIX2)", () => {
    const plan = { kind: "player" as const, id: "1", plan: { crew: "c", id: "p" } };
    expect(reasonsFor(plan)).toContain("inappropriate");
    expect(reasonsFor({ kind: "player", id: "1" })).not.toContain("inappropriate");
    expect(reasonKey("player", "inappropriate", true)).toBe("fix2.report.noteReason");
    expect(reasonKey("message", "inappropriate")).toBe("reports.reasons.inappropriate");
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
