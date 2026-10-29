import { describe, expect, it } from "vitest";
import { enSetup } from "./en-setup";
import { trSetup } from "./tr-setup";

/**
 * The settings-as-questions texts (SETUP-UX-L) keep the rules of the wired locales (parity.test.ts,
 * voice.test.ts): the same keys and placeholders, no Turkish letters in English, a singular where an
 * English count ends in a plural noun, "sen" in Turkish, and none of what the taste gate bans in a
 * player's text (exclamation marks, semicolons, middle dots, dashes, arrows, hype).
 */
function flatten(value: unknown, prefix = "", out = new Map<string, string>()) {
  if (typeof value === "string") {
    out.set(prefix, value);
  } else if (value && typeof value === "object") {
    for (const [name, child] of Object.entries(value)) {
      flatten(child, prefix ? `${prefix}.${name}` : name, out);
    }
  }
  return out;
}

const turkish = flatten(trSetup);
const english = flatten(enSetup);
const base = (key: string) => key.replace(/_(zero|one|two|few|many|other)$/, "");
const placeholders = (text: string) =>
  [...new Set([...text.matchAll(/\{\{\s*([^}\s,]+)[^}]*\}\}/g)].map((m) => m[1] ?? ""))]
    .filter((name) => name !== "count")
    .sort()
    .join(",");

describe("Setup texts", () => {
  it("have the same keys in both languages", () => {
    const turkishBases = new Set([...turkish.keys()].map(base));
    const englishBases = new Set([...english.keys()].map(base));
    expect([...turkishBases].filter((key) => !englishBases.has(key))).toEqual([]);
    expect([...englishBases].filter((key) => !turkishBases.has(key))).toEqual([]);
  });

  it("name the same placeholders", () => {
    const mismatched = [...turkish]
      .filter(([key, text]) => {
        const b = base(key);
        const other = english.get(key) ?? english.get(`${b}_other`) ?? english.get(b);
        return other !== undefined && placeholders(other) !== placeholders(text);
      })
      .map(([key]) => key);
    expect(mismatched).toEqual([]);
  });

  it("show no Turkish letters in English", () => {
    expect([...english].filter(([, text]) => /[çğıöşüÇĞİÖŞÜ]/.test(text))).toEqual([]);
  });

  it("have an English singular wherever a count is followed by a noun", () => {
    const missing = [...english]
      .filter(([key]) => !/_(one|other)$/.test(key))
      .filter(([, text]) => /\{\{count\}\} [a-z]+s\b/.test(text))
      .map(([key]) => key);
    expect(missing).toEqual([]);
  });

  it("speak to the player informally", () => {
    const formal = [...turkish].filter(([, text]) =>
      /lütfen|deneyin|[ae]bilirsiniz|siniz\b/.test(text.toLocaleLowerCase("tr")),
    );
    expect(formal).toEqual([]);
  });

  it("use none of the banned marks or words", () => {
    const banned = /[!;·—→]|mükemmel|sorunsuz|kesintisiz|eşsiz|seamless|unleash|elevate/i;
    expect([...turkish, ...english].filter(([, text]) => banned.test(text))).toEqual([]);
  });
});
