import { describe, expect, it } from "vitest";
import { en } from "./en";
import { tr } from "./tr";

/**
 * Turkish and English stay in step (POLISH6). English falls back to Turkish for a missing key,
 * so a forgotten string shows Turkish to English players without any error: every Turkish string
 * needs its English one, English keeps no string Turkish lost, the two name the same
 * placeholders, English never shows Turkish letters, and an English count that ends in a plural
 * noun has its singular (`_one`).
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

const turkish = flatten(tr);
const english = flatten(en);
const base = (key: string) => key.replace(/_(zero|one|two|few|many|other)$/, "");
const turkishBases = new Set([...turkish.keys()].map(base));

/** The English of a Turkish key: the same key, or the plural forms of its base. */
function englishOf(key: string): string[] {
  const b = base(key);
  return [
    english.get(key),
    english.get(b),
    english.get(`${b}_other`),
    english.get(`${b}_one`),
  ].filter((text): text is string => text !== undefined);
}

const placeholders = (text: string) =>
  [...new Set([...text.matchAll(/\{\{\s*([^}\s,]+)[^}]*\}\}/g)].map((m) => m[1] ?? ""))]
    .filter((name) => name !== "count")
    .sort()
    .join(",");

/**
 * Keys whose English names a placeholder differently on purpose: the importer passes both forms
 * ("Minecraft Launcher'daki dünyaların" reads the source first, "your worlds from the Minecraft
 * Launcher" last; a Turkish sentence may start with the item, so it has a capitalised `What`). A
 * crew's mute end gives Turkish the ending its time takes ("14:30'a kadar"); English needs none,
 * and neither does an upcoming event's start ("20:00'de başlıyor") or a cloud account's last known
 * state ("14:05'te oyunda").
 */
const OWN_PLACEHOLDERS = [
  /^import\.what\./,
  /^import\.outcome\.(optionsKept|unreadable|failed)$/,
  /^teams\.mute\.until(Today|Tomorrow|Later)$/,
  /^newsColumn\.starts\./,
  /^cloudStale\.last\./,
];

/** English strings that name Turkish letters on purpose. */
const TURKISH_LETTERS_ALLOWED = new Set(["accounts.name.turkish"]);

describe("English texts", () => {
  it("cover every Turkish text", () => {
    const missing = [...turkish.keys()].filter((key) => englishOf(key).length === 0);
    expect(missing).toEqual([]);
  });

  it("keep no text Turkish no longer has", () => {
    const stale = [...english.keys()].filter((key) => !turkishBases.has(base(key)));
    expect(stale).toEqual([]);
  });

  it("name the same placeholders", () => {
    const differing: string[] = [];
    for (const [key, text] of turkish) {
      if (OWN_PLACEHOLDERS.some((pattern) => pattern.test(key))) continue;
      for (const other of englishOf(key)) {
        if (placeholders(other) !== placeholders(text)) {
          differing.push(`${key}: ${text} | ${other}`);
        }
      }
    }
    expect(differing).toEqual([]);
  });

  it("show no Turkish letters", () => {
    const leftovers = [...english.entries()]
      .filter(([key, text]) => !TURKISH_LETTERS_ALLOWED.has(key) && /[çğıöşüÇĞİÖŞÜ]/.test(text))
      .map(([key, text]) => `${key}: ${text}`);
    expect(leftovers).toEqual([]);
  });

  it("have a singular where a count is followed by a plural noun", () => {
    const missingOne = [...english.entries()]
      .filter(([key]) => !/_(one|other)$/.test(key))
      .filter(([, text]) => /\{\{count\}\} [a-z]+s\b/.test(text))
      .filter(([key]) => !english.has(`${key}_one`))
      .map(([key, text]) => `${key}: ${text}`);
    expect(missingOne).toEqual([]);
  });
});
