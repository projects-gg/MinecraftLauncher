import { describe, expect, it } from "vitest";
import { checkOrderText, readAmount, ruleHint } from "./order-rules";
import intake from "./vectors/afkplus-intake-v1.json";

// The free rule check runs as an instant hint here and exactly at the Worker; the contract's
// vectors (projects-studio services/cloud-afk/test-vectors) hold for both.

interface RuleCase {
  name: string;
  text: string;
  ctx: Record<string, number>;
  result: string;
  notes: string[];
}

/**
 * Cases the friends service's word list decides: the launcher never carries the list (the
 * service answers them on "Kontrol et"), so the hint lets them through.
 */
const WORD_LIST: readonly string[] = ["a listed word"];

const rules = intake.rules as RuleCase[];

describe("AFK+ typed orders: the free rule check (afkplus-intake-v1.json)", () => {
  it.each(rules.map((c) => [c.name, c] as const))("%s", (_name, c) => {
    const check = checkOrderText(c.text, c.ctx);
    if (WORD_LIST.includes(c.name)) {
      expect(check.result).toBe("ok");
      return;
    }
    expect(check.result).toBe(c.result);
    expect(check.notes).toEqual(c.notes);
  });

  it("names the reason and the rule for the hint", () => {
    expect(ruleHint(checkOrderText("önce ben öderim elytra 40k"))).toEqual({
      reason: "loosen",
      rule: "prepay",
    });
    expect(ruleHint(checkOrderText("elytra alırım 40k"))).toBeNull();
    expect(ruleHint(checkOrderText("ab"))).toEqual({ reason: "length", rule: undefined });
  });

  it("reads an amount typed in a field", () => {
    expect(readAmount("40 bin")).toBe(40_000);
    expect(readAmount("40k")).toBe(40_000);
    expect(readAmount("40.000")).toBe(40_000);
    expect(readAmount("1,5m")).toBe(1_500_000);
    expect(readAmount("1.5")).toBeNull();
    expect(readAmount("40 tl")).toBeNull();
    expect(readAmount("0")).toBeNull();
    expect(readAmount("12 13")).toBeNull();
    expect(readAmount("200m", 10_000_000)).toBeNull();
  });
});
