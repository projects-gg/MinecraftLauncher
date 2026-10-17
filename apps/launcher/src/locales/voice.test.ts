import { describe, expect, it } from "vitest";
import { tr } from "./tr";

/**
 * The launcher speaks to the player as "sen" and says "Tekrar dene", never with the formal
 * "siz" forms or "Lütfen" ("yeniden deneyin", "dönebilirsiniz", "oynayabilirsiniz"). pl-auth and
 * pl-mc guard their Turkish texts the same way.
 */
const FORMAL = /lütfen|deneyin|[ae]bilirsiniz/;

/** Every string of the Turkish texts (feature modules included) with its key. */
function* strings(value: unknown, key = ""): Generator<[string, string]> {
  if (typeof value === "string") {
    yield [key, value];
  } else if (value && typeof value === "object") {
    for (const [name, child] of Object.entries(value)) {
      yield* strings(child, key ? `${key}.${name}` : name);
    }
  }
}

describe("Turkish strings", () => {
  it("speak to the player informally", () => {
    const all = [...strings(tr)];
    // The walk reaches the nested sections and the feature modules.
    expect(all.length).toBeGreaterThan(1000);
    const formal = all
      .filter(([, text]) => FORMAL.test(text.toLocaleLowerCase("tr")))
      .map(([key, text]) => `${key}: ${text}`);
    expect(formal).toEqual([]);
  });
});
