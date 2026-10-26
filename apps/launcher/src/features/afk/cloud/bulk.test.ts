import { describe, expect, it } from "vitest";
import { appliesTo, commandOf, lineOf, rowName, tally } from "./bulk";

describe("bulk management", () => {
  it("keeps a secret row's command and asks only for its hidden words", () => {
    expect(rowName("/login ******")).toBe("/login");
    expect(commandOf("/login ******")).toBe("/login");
    expect(lineOf("/login ******", "Gizli123")).toBe("/login Gizli123");
    expect(lineOf("/login ******", "  Gizli123 ")).toBe("/login Gizli123");
    // A whole command typed anyway is taken as it is.
    expect(lineOf("/login ******", "/login Gizli123")).toBe("/login Gizli123");
    expect(lineOf("/register ******", "/register a a")).toBe("/register a a");
    // A row hidden entirely, or one that is no command, is the typed words.
    expect(commandOf("******")).toBe("");
    expect(lineOf("******", "selam")).toBe("selam");
    expect(commandOf("selam ******")).toBe("");
    expect(lineOf("selam ******", "dünya")).toBe("dünya");
  });

  it("tallies results with the first failure's reason", () => {
    expect(
      tally([{ ok: true }, { ok: false, code: "BULUT-SURE" }, { ok: false, code: "here" }]),
    ).toEqual({
      done: 1,
      failed: 2,
      reason: "expired",
    });
    expect(tally([{ ok: false, code: "BULUT-?" }])).toEqual({
      done: 0,
      failed: 1,
      reason: "other",
    });
  });

  it("applies each action only to the accounts it fits", () => {
    const stopped = { desired: "stop", hold: null, expired: false };
    const running = { desired: "run", hold: null, expired: false };
    const held = { desired: "run", hold: { kind: "cause" }, expired: false };
    expect(appliesTo("start", stopped)).toBe(true);
    expect(appliesTo("start", running)).toBe(false);
    expect(appliesTo("start", held)).toBe(true);
    expect(appliesTo("stop", running)).toBe(true);
    expect(appliesTo("stop", stopped)).toBe(false);
    expect(appliesTo("restart", running)).toBe(true);
    expect(appliesTo("restart", held)).toBe(false);
  });
});
