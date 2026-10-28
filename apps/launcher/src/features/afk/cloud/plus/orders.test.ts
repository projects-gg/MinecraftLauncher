import { describe, expect, it } from "vitest";
import { bareMoney, moneyText } from "./orders";
import type { PlusServer } from "./types";

/** A server's money as the service says it: the name after amounts and whose it is. */
function server(label: string | null, fixed = false): Pick<PlusServer, "money"> {
  return {
    money: {
      label: label === null ? null : { tr: label, en: label },
      fixed,
      maxDeal: 10_000_000,
      currency: "smp",
    },
  };
}

describe("amounts in the server's money", () => {
  it("are bare where nothing names the money (Projects SMP)", () => {
    expect(moneyText(32_000, server(null), "tr")).toBe("32.000");
    expect(bareMoney(server(null))).toBe(true);
  });

  it("carry the name the player gave (Para birimi), the same in both languages", () => {
    expect(moneyText(32_000, server("TL"), "tr")).toBe("32.000 TL");
    expect(moneyText(32_000, server("TL"), "en")).toBe("32,000 TL");
    expect(bareMoney(server("TL"))).toBe(false);
  });

  it("carry the profile's own name (Projects Survival)", () => {
    expect(moneyText(37_500, server("ripple", true), "tr")).toBe("37.500 ripple");
    expect(bareMoney(server("ripple", true))).toBe(false);
  });
});
