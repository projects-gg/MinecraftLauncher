import { describe, expect, it } from "vitest";
import {
  checkOrderText,
  GENERIC_MAX_DEAL,
  isMoneyQuestion,
  type RuleContext,
  readAmount,
  ruleHint,
  tokenize,
} from "./order-rules";
import intake from "./vectors/afkplus-intake-v1.json";

// The free rule check runs as an instant hint here and exactly at the Worker; the contract's
// vectors (projects-studio services/cloud-afk/test-vectors) hold for both.

interface RuleCase {
  name: string;
  text: string;
  /** The vectors' context: counts, the profile's cap and the server's money name (`currency`). */
  ctx: RuleContext;
  result: string;
  notes: string[];
}

/** A name, the text, the server's own money name, then what is read: value, shape, unit, rmt. */
type OwnRow = [string, string, string | null, number, string, string | null, boolean];

/**
 * afkplus-money-v1.json's `own` group (projects-studio, 28 Oct 2026): an amount written with the
 * server's own money name is the server's money; every other real-money word and sign still is.
 */
const OWN: OwnRow[] = [
  ["the server's TL", "45 tl", "TL", 45, "plain", null, false],
  ["the server's TL after k, upper case", "45k TL", "TL", 45_000, "unit", "k", false],
  ["the server's TL with an ending", "45 bin tl'ye", "TL", 45_000, "unit", "bin", false],
  ["the server's TL glued", "45tl", "tl", 45, "plain", null, false],
  ["the server's sign before", "$45", "$", 45, "plain", null, false],
  ["the server's sign before, spaced", "$ 45", "$", 45, "plain", null, false],
  ["the server's sign after", "45₺", "₺", 45, "plain", null, false],
  ["the server's lira with an ending", "100 liraya", "Lira", 100, "plain", null, false],
  ["another real-money word still is", "45 lira", "TL", 45, "plain", null, true],
  ["another sign still is", "€45", "$", 45, "plain", null, true],
  ["a real-money sign before the server's name", "$45 coin", "coin", 45, "plain", null, true],
  ["a name that is no real-money word", "45k coin", "coin", 45_000, "unit", "k", false],
  ["a name of two words", "45 altın para", "Altın Para", 45, "plain", null, false],
  ["no name: tl is real money", "45 tl", null, 45, "plain", null, true],
];

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

  it("asks about money that may be the game's, and refuses a rail without asking", () => {
    const dollars = "elytra alırım 100 dolar";
    expect(checkOrderText(dollars).result).toBe("question:rmt");
    // Asked on "Kontrol et", never as a hint while typing.
    expect(ruleHint(checkOrderText(dollars))).toBeNull();
    expect(checkOrderText(dollars, { inGame: true }).result).toBe("ok");
    const rail = checkOrderText("papara ile öderim elytra", { inGame: true });
    expect(ruleHint(rail)).toEqual({ reason: "loosen", rule: "rmt" });
    // The service's question offers "Bu oyun parası"; a rail's refusal never does.
    expect(isMoneyQuestion("question", "rmt")).toBe(true);
    expect(isMoneyQuestion("loosen", "rmt")).toBe(false);
    expect(isMoneyQuestion("question", null)).toBe(false);
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
    // The server's own money name: its amount is the server's money.
    expect(readAmount("40 tl", GENERIC_MAX_DEAL, "TL")).toBe(40);
    expect(readAmount("40 lira", GENERIC_MAX_DEAL, "TL")).toBeNull();
  });

  it.each(OWN)("the server's own money: %s", (_name, text, own, value, shape, unit, rmt) => {
    const { tokens } = tokenize(text, own);
    const read = tokens.map((token) => [token.value, token.shape, token.unit, token.rmt]);
    expect(read).toEqual([[value, shape, unit, rmt]]);
  });
});
