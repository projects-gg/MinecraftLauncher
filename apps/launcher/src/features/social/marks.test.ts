import { describe, expect, it } from "vitest";
import { en } from "@/locales/en";
import { tr } from "@/locales/tr";
import { markedName, markLabel, sentenceParts, type Translate } from "./marks";

/** A tiny translator over the real strings (`{{x}}` placeholders only). */
function translator(strings: Record<string, unknown>): Translate {
  return (key, options = {}) => {
    const found = key
      .split(".")
      .reduce<unknown>(
        (node, part) =>
          node && typeof node === "object" ? (node as Record<string, unknown>)[part] : undefined,
        strings,
      );
    const text = typeof found === "string" ? found : key;
    return text.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(options[name] ?? ""));
  };
}

describe("profile marks (SOC-VERIFY)", () => {
  it("names the marks in both languages", () => {
    const t = translator(tr);
    expect(markLabel("verified", t)).toBe("Doğrulanmış hesap");
    expect(markLabel("admin", t)).toBe("Projects yöneticisi");
    const e = translator(en);
    expect(markLabel("verified", e)).toBe("Verified account");
    expect(markLabel("admin", e)).toBe("Projects admin");
  });

  it("says the mark in a row's name for screen readers, and nothing without one", () => {
    const t = translator(tr);
    expect(markedName("Ayşe", "verified", t)).toBe("Ayşe, Doğrulanmış hesap");
    expect(markedName("Ayşe", undefined, t)).toBe("Ayşe");
  });

  it("splits a sentence around its name, whatever the language's order", () => {
    expect(sentenceParts((name) => `${name} seni birlikte oynamaya çağırıyor`)).toEqual([
      "",
      " seni birlikte oynamaya çağırıyor",
    ]);
    expect(sentenceParts((name) => `Davet eden: ${name}.`)).toEqual(["Davet eden: ", "."]);
    // Without the name, or with it twice, the sentence stays whole.
    expect(sentenceParts(() => "Ekibinden 4 kişi Projects'te")).toBeNull();
    expect(sentenceParts((name) => `${name} ve ${name}`)).toBeNull();
  });
});
