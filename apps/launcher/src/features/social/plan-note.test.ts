import { describe, expect, it } from "vitest";
import { cleanPlanNote, PLAN_NOTE_MAX, planNoteLength, planNoteProblem } from "./plan-note";

describe("a plan's note", () => {
  it("goes on one line, cleaned as chat cleans", () => {
    expect(cleanPlanNote("  Bedwars,   sesli  gelin ")).toBe("Bedwars, sesli gelin");
    expect(cleanPlanNote("lobide\nbuluşalım")).toBe("lobide buluşalım");
    expect(cleanPlanNote("   ")).toBe("");
    expect(planNoteLength("şşş  ğ")).toBe(5);
  });

  it("says why it cannot go before anything is sent", () => {
    expect(planNoteProblem("Bedwars 21:00, sesli gelin")).toBeNull();
    expect(planNoteProblem("")).toBeNull();
    // projects.gg may be written; any other address may not.
    expect(planNoteProblem("play.projects.gg lobisinde")).toBeNull();
    expect(planNoteProblem("discord.gg/abc")).toBe("link");
    expect(planNoteProblem("beni ara 0532 123 45 67")).toBe("phone");
    expect(planNoteProblem("ş".repeat(PLAN_NOTE_MAX))).toBeNull();
    expect(planNoteProblem("ş".repeat(PLAN_NOTE_MAX + 1))).toBe("long");
  });
});
