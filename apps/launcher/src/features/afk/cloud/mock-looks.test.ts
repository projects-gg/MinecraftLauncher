// @vitest-environment happy-dom
// CONSOLE-COLOR: the preview's coloured lines (`&looks=1`) are the contract's: their parts cover
// them and their events pass @pl/ui's checks; without the switch the console is as it was.
import { lineRuns, safeEvents } from "@pl/ui/console";
import { afterEach, describe, expect, it } from "vitest";
import { kindLines } from "./mock-feat";

afterEach(() => window.history.replaceState(null, "", "/"));

describe("the preview's console looks", () => {
  it("gives every coloured line parts that cover it and events that are the contract's", () => {
    window.history.replaceState(null, "", "/?cloud=accounts&feat=on&looks=1");
    const lines = kindLines(14, 1_800_000_000, "Madenci07", "play.projects.gg").filter(
      (line) => line.st,
    );
    expect(lines.length).toBeGreaterThan(5);
    for (const line of lines) {
      const events = safeEvents(line.ev);
      expect(lineRuns(line.text, line.st, events), line.text).not.toBeNull();
      expect(
        events.every((event) => event !== null),
        line.text,
      ).toBe(true);
    }
  });

  it("leaves the console as it was without the switch", () => {
    window.history.replaceState(null, "", "/?cloud=accounts&feat=on");
    const lines = kindLines(14, 1_800_000_000, "Madenci07", "play.projects.gg");
    expect(lines.some((line) => line.st)).toBe(false);
    expect(lines.at(-1)?.at).toBe(1_800_000_000 - 37);
  });
});
