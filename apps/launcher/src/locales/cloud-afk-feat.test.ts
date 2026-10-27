import { describe, expect, it } from "vitest";
import { enCloudAfkFeat } from "./en-cloud-afk-feat";
import { trCloudAfkFeat } from "./tr-cloud-afk-feat";

/**
 * The cloud features' texts (FEAT-L) keep the rules of the wired locales: the same keys, the same
 * placeholders, no Turkish letters in English, no technical words, and the contract's own wording
 * where it names a text (README "Feature errors").
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

const turkish = flatten(trCloudAfkFeat);
const english = flatten(enCloudAfkFeat);
const base = (key: string) => key.replace(/_(zero|one|two|few|many|other)$/, "");
const placeholders = (text: string) =>
  [...new Set([...text.matchAll(/\{\{\s*([^}\s,]+)[^}]*\}\}/g)].map((m) => m[1] ?? ""))]
    .filter((name) => name !== "count")
    .sort()
    .join(",");

describe("cloud feature texts", () => {
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

  it("show no Turkish letters in English and no technical words to players", () => {
    expect([...english].filter(([, text]) => /[çğıöşüÇĞİÖŞÜ]/.test(text))).toEqual([]);
    const technical = /\b(regex|placeholder|wseq|invop|socket|HTTP|API|JSON)\b/i;
    // The save checks' texts are the contract's own words (README "Feature errors"), the same on
    // the site and the phone.
    const contract = (key: string) => key.startsWith("settings.issues.");
    expect(
      [...turkish, ...english].filter(([key, text]) => !contract(key) && technical.test(text)),
    ).toEqual([]);
  });

  it("use the contract's words where it names them", () => {
    expect(trCloudAfkFeat.inventory.notRunning).toBe("Hesap oyunda değilken envanter görünmez.");
    expect(trCloudAfkFeat.inventory.restart).toBe("Envanter bir sonraki bağlanmada açılır.");
    expect(trCloudAfkFeat.settings.issues.placeholder_part).toBe("Bu kalıpta {{n}}. parça yok.");
    expect(enCloudAfkFeat.settings.issues.placeholder_start).toBe(
      "A reply can't start with a placeholder.",
    );
    expect(enCloudAfkFeat.inventory.version).toBe(
      "The inventory isn't available on this Minecraft version.",
    );
    expect(trCloudAfkFeat.bulk.reasons.expired).toBe("süresi dolmuş");
  });
});
