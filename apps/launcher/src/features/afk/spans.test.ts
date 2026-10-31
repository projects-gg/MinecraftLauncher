import type { TFunction } from "i18next";
import { describe, expect, it } from "vitest";
import { tr } from "@/locales/tr";
import { limitWords, waitWords, waitWordsChangeIn } from "./spans";

// The Turkish strings, looked up like i18next does (Turkish has no plural forms here).
const t = ((key: string, values?: Record<string, unknown>) => {
  let value: unknown = tr;
  for (const part of key.split(".")) value = (value as Record<string, unknown> | undefined)?.[part];
  if (typeof value !== "string") throw new Error(`no text for ${key}`);
  return value.replace(/\{\{(\w+)\}\}/g, (_, name: string) => String(values?.[name]));
}) as TFunction;

describe("waitWords", () => {
  it("says seconds under a minute, then whole minutes, then hours and minutes", () => {
    expect(waitWords(45, t)).toBe("45 saniye");
    expect(waitWords(60, t)).toBe("1 dakika");
    expect(waitWords(240, t)).toBe("4 dakika");
    expect(waitWords(3600, t)).toBe("1 saat");
    expect(waitWords(3900, t)).toBe("1 saat 5 dakika");
  });

  it("rounds to the nearest minute, never to nothing", () => {
    expect(waitWords(89, t)).toBe("1 dakika");
    expect(waitWords(90, t)).toBe("2 dakika");
    expect(waitWords(209, t)).toBe("3 dakika");
    expect(waitWords(210, t)).toBe("4 dakika");
    expect(waitWords(0, t)).toBe("1 saniye");
  });
});

describe("waitWordsChangeIn", () => {
  it("is each second under a minute", () => {
    expect(waitWordsChangeIn(45)).toBe(1);
    expect(waitWordsChangeIn(60)).toBe(1);
  });

  it("is where the rounded minutes change above it", () => {
    // 240 s reads "4 dakika" down to 210 s; at 209 s it is "3 dakika".
    expect(waitWordsChangeIn(240)).toBe(31);
    expect(waitWords(240 - 31, t)).not.toBe(waitWords(240 - 30, t));
    // "1 dakika" holds from 89 s to 60 s, then "59 saniye".
    expect(waitWordsChangeIn(89)).toBe(30);
    expect(waitWordsChangeIn(90)).toBe(1);
  });
});

describe("limitWords", () => {
  it("names a limit in the largest unit it fills whole", () => {
    expect(limitWords(604_800, t)).toBe("7 gün");
    expect(limitWords(86_400, t)).toBe("1 gün");
    expect(limitWords(3600, t)).toBe("1 saat");
    expect(limitWords(60, t)).toBe("1 dakika");
    expect(limitWords(90, t)).toBe("90 saniye");
    expect(limitWords(0, t)).toBe("0 saniye");
  });
});
