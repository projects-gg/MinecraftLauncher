import { describe, expect, it } from "vitest";
import { automaticLanguage, inTurkey, languageOfTag, resolveLanguage } from "./i18n";

const signals = (
  language: string | null,
  timeZone: string | null,
  region: string | null = null,
) => ({
  language,
  timeZone,
  region,
});

describe("the launcher's language", () => {
  it("is Turkish in Turkey, even on an English system", () => {
    expect(automaticLanguage(signals("en-US", "Europe/Istanbul"))).toBe("tr");
    expect(automaticLanguage(signals("en-GB", "Asia/Istanbul"))).toBe("tr");
    // Northern Cyprus speaks Turkish.
    expect(automaticLanguage(signals("en-US", "Asia/Famagusta"))).toBe("tr");
  });

  it("is English on an English system abroad", () => {
    expect(automaticLanguage(signals("en-US", "Europe/Berlin"))).toBe("en");
    expect(automaticLanguage(signals("de-DE", "Europe/Berlin"))).toBe("en");
    expect(automaticLanguage(signals("en-US", "Europe/Nicosia"))).toBe("en");
    expect(automaticLanguage(signals(null, null))).toBe("en");
  });

  it("is Turkish on a Turkish system anywhere", () => {
    expect(automaticLanguage(signals("tr-TR", "Europe/Berlin"))).toBe("tr");
    expect(automaticLanguage(signals("tr", null))).toBe("tr");
  });

  it("counts Windows' home location in Turkey", () => {
    expect(inTurkey(signals("en-GB", "Europe/London", "TR"))).toBe(true);
    expect(automaticLanguage(signals("en-GB", "Europe/London", "TR"))).toBe("tr");
    expect(inTurkey(signals("en-GB", "Europe/London"))).toBe(false);
  });

  it("takes the player's pick first", () => {
    expect(resolveLanguage("en", signals("tr-TR", "Europe/Istanbul"))).toBe("en");
    expect(resolveLanguage("tr", signals("en-US", "Europe/Berlin"))).toBe("tr");
    expect(resolveLanguage("auto", signals("en-US", "Europe/Istanbul"))).toBe("tr");
    expect(resolveLanguage("auto", signals("en-US", "Europe/Berlin"))).toBe("en");
  });

  it("reads Turkish only from Turkish tags", () => {
    for (const tag of ["tr", "TR", "tr-TR", "tr_TR.UTF-8", "tr-Latn-TR", " tr-CY "]) {
      expect(languageOfTag(tag), tag).toBe("tr");
    }
    for (const tag of ["en-US", "de", "az-Latn-AZ", "ku-TR", "trv", "C", "", null, undefined]) {
      expect(languageOfTag(tag), String(tag)).toBe("en");
    }
  });
});
