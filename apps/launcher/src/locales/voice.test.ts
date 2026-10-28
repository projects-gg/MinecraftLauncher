import { describe, expect, it } from "vitest";
import { en } from "./en";
import { tr } from "./tr";

/**
 * The launcher speaks to the player as "sen" and says "Tekrar dene", never with the formal
 * "siz" forms or "Lütfen" ("yeniden deneyin", "dönebilirsiniz", "oynayabilirsiniz"). pl-auth and
 * pl-mc guard their Turkish texts the same way.
 */
const FORMAL = /lütfen|deneyin|[ae]bilirsiniz/;

/** One wording for trying again, in buttons and sentences alike (QA-COPY O5): "tekrar dene". */
const RETRY = /(?<![\p{L}])yeniden dene(?![\p{L}])/u;

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

  it('say "tekrar dene" to try again', () => {
    const other = [...strings(tr)]
      .filter(([, text]) => RETRY.test(text.toLocaleLowerCase("tr")))
      .map(([key, text]) => `${key}: ${text}`);
    expect(other).toEqual([]);
  });
});

/**
 * One idea per sentence (QA-COPY O2): a player's line never joins two with a semicolon, in either
 * language; nor with an em dash (docs/design.md §7).
 */
describe("Player-facing sentences", () => {
  it("never join two with a semicolon or an em dash", () => {
    const joined = [...strings(tr), ...strings(en)]
      .filter(([, text]) => /;|—/.test(text))
      .map(([key, text]) => `${key}: ${text}`);
    expect(joined).toEqual([]);
  });
});
