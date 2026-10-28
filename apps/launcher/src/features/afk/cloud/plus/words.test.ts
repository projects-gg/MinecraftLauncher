import { describe, expect, it } from "vitest";
import { enAfkPlus } from "@/locales/en-afkplus";
import { trAfkPlus } from "@/locales/tr-afkplus";
import { atPhrase, dayGroup, durationParts, minutesLeft, rangeText, untilPhrase } from "./time";
import {
  cardWords,
  dealText,
  enWords,
  firstClause,
  formatAmount,
  goodsText,
  parseAmount,
  qtyItem,
  trEnding,
  trWith,
  trWords,
  WORDS_FROM,
} from "./words";

describe("AFK+ state sentences on the card", () => {
  it("keep their first clause", () => {
    expect(firstClause("Kontörün bitti, hesabın AFK'da kalıyor")).toBe("Kontörün bitti");
    expect(firstClause("Weekly limit reached. It resets at 04:00")).toBe("Weekly limit reached");
    expect(firstClause("ProBot AI çalışıyor")).toBe("ProBot AI çalışıyor");
    // A number's own dot or comma is not a clause's end.
    expect(firstClause("Elytra 37.500'e alındı")).toBe("Elytra 37.500'e alındı");
  });

  it("read as a state in both languages", () => {
    for (const ai of [trAfkPlus.ai, enAfkPlus.ai]) {
      for (const [key, text] of Object.entries(ai)) {
        if (typeof text !== "string") continue;
        const brief = firstClause(text.replace("{{when}}", "18:20'de"));
        expect(brief.length, key).toBeGreaterThan(3);
        expect(brief, key).not.toMatch(/[,.;:]$/);
      }
    }
  });
});

describe("AFK+ amounts", () => {
  it("are said in words under a card's figure from 10.000 only", () => {
    // A bare "bin" under "1.000" read as "1.000 bin", a million.
    expect(cardWords(1_000, "tr")).toBeNull();
    expect(cardWords(9_999, "en")).toBeNull();
    expect(cardWords(10_000, "tr")).toBe("on bin");
    expect(cardWords(10_000, "en")).toBe("ten thousand");
    expect(WORDS_FROM).toBe(10_000);
  });

  it("are grouped by the language", () => {
    expect(formatAmount(37_500, "tr")).toBe("37.500");
    expect(formatAmount(37_500, "en")).toBe("37,500");
    expect(formatAmount(950, "tr")).toBe("950");
  });

  it("count items without guessing an English plural", () => {
    expect(qtyItem(64, "Elmas", "tr")).toBe("64 Elmas");
    expect(qtyItem(64, "Diamond", "en")).toBe("64\u00a0×\u00a0Diamond");
    expect(qtyItem("1", "Totem of Undying", "en")).toBe("1\u00a0×\u00a0Totem of Undying");
    // A deal's subject alone drops a single one's count; a price never does.
    expect(goodsText("1", "Totem of Undying", "en")).toBe("Totem of Undying");
    expect(goodsText(1, "Elytra", "tr")).toBe("1 Elytra");
  });

  it("write a deal as the site and the in-game card do", () => {
    expect(dealText(1, "Elytra", "37.500", "tr")).toBe("1 Elytra = 37.500");
    expect(dealText(1, "Elytra", "37,500", "en")).toBe("Elytra = 37,500");
    expect(dealText(2, "Netherite Ingot", qtyItem(9, "Diamond", "en"), "en")).toBe(
      "2\u00a0×\u00a0Netherite Ingot = 9\u00a0×\u00a0Diamond",
    );
    expect(dealText(2, "Netherite Ingot", qtyItem(1, "Diamond", "en"), "en")).toBe(
      "2\u00a0×\u00a0Netherite Ingot = 1\u00a0×\u00a0Diamond",
    );
  });

  it("say a span of days once", () => {
    const day = (d: number, m: number) => new Date(2026, m - 1, d, 12).getTime() / 1000;
    // ICU puts thin spaces around the range's dash (CLDR 42+); older ones put none.
    expect(rangeText(day(16, 10), day(23, 10), "tr")).toMatch(/^16\s?–\s?23 Ekim$/);
    expect(rangeText(day(16, 10), day(23, 10), "en")).toMatch(/^16\s?–\s?23 October$/);
    expect(rangeText(day(28, 9), day(5, 10), "tr")).toMatch(/^28 Eylül\s–\s5 Ekim$/);
  });

  it("are read out in words, so a zero too many shows", () => {
    expect(trWords(40_000)).toBe("kırk bin");
    expect(trWords(37_500)).toBe("otuz yedi bin beş yüz");
    expect(trWords(1_000)).toBe("bin");
    expect(trWords(1_250_000)).toBe("bir milyon iki yüz elli bin");
    expect(trWords(0)).toBe("sıfır");
    expect(enWords(40_000)).toBe("forty thousand");
    expect(enWords(37_500)).toBe("thirty-seven thousand five hundred");
    expect(enWords(1_000_000)).toBe("one million");
    // British English, as the interface's English (en-GB) says numbers.
    expect(enWords(7_680)).toBe("seven thousand six hundred and eighty");
    expect(enWords(40_005)).toBe("forty thousand and five");
    expect(enWords(1_250_000)).toBe("one million two hundred and fifty thousand");
    expect(enWords(120)).toBe("one hundred and twenty");
  });

  it("read the players' shorthands", () => {
    expect(parseAmount("40k")).toBe(40_000);
    expect(parseAmount("40 bin")).toBe(40_000);
    expect(parseAmount("40.000")).toBe(40_000);
    expect(parseAmount("40,000")).toBe(40_000);
    expect(parseAmount("1,5m")).toBe(1_500_000);
    expect(parseAmount("1.5 milyon")).toBe(1_500_000);
    expect(parseAmount("37.5k")).toBe(37_500);
    expect(parseAmount("120")).toBe(120);
    expect(parseAmount("")).toBeNull();
    expect(parseAmount("kırk")).toBeNull();
    expect(parseAmount("4.5")).toBeNull();
  });
});

describe("Turkish endings", () => {
  it("follow how the value is read", () => {
    expect(trWith("37.500", "dat")).toBe("37.500'e");
    expect(trWith("40.000", "gen")).toBe("40.000'in");
    expect(trWith("35.000", "gen")).toBe("35.000'in");
    expect(trWith("18:20", "loc")).toBe("18:20'de");
    expect(trWith("04:00", "loc")).toBe("04:00'te");
    expect(trWith("09:30", "loc")).toBe("09:30'da");
    expect(trWith("30 Ekim", "dat")).toBe("30 Ekim'e");
    expect(trWith("1 Mart", "loc")).toBe("1 Mart'ta");
    expect(trWith("2026", "dat")).toBe("2026'ya");
    expect(trWith("14:35", "dat")).toBe("14:35'e");
    // Player names: by their trailing digits when they have some; a short run of capitals at the
    // end is read letter by letter ("te-re", "ke"); else by their letters as a word.
    expect(trWith("Kaan_07", "dat")).toBe("Kaan_07'ye");
    expect(trWith("Zeynep_12", "dat")).toBe("Zeynep_12'ye");
    expect(trWith("Deniz_TR", "dat")).toBe("Deniz_TR'ye");
    expect(trWith("Deniz_TR", "abl")).toBe("Deniz_TR'den");
    expect(trWith("MertK", "dat")).toBe("MertK'ye");
    expect(trWith("Ahmet_AFK", "gen")).toBe("Ahmet_AFK'nin");
    expect(trWith("AHMET", "dat")).toBe("AHMET'e");
    expect(trWith("Baris_TR", "abl")).toBe("Baris_TR'den");
    expect(trWith("Hizli_Satis", "dat")).toBe("Hizli_Satis'e");
    expect(trEnding("Kasım", "dat")).toBe("a");
  });
});

describe("AFK+ times", () => {
  // Saturday 24 October 2026, 12:00 local time.
  const now = new Date(2026, 9, 24, 12, 0).getTime();
  const at = (days: number, hour: number, minute = 0) =>
    Math.floor(new Date(2026, 9, 24 + days, hour, minute).getTime() / 1000);

  it("say when in whole phrases", () => {
    expect(atPhrase(at(0, 18, 20), now, "tr", "yarın")).toBe("18:20'de");
    expect(atPhrase(at(0, 18, 20), now, "en", "tomorrow")).toBe("at 18:20");
    expect(atPhrase(at(1, 4), now, "tr", "yarın")).toBe("yarın 04:00'te");
    expect(atPhrase(at(1, 4), now, "en", "tomorrow")).toBe("tomorrow at 04:00");
    expect(atPhrase(at(2, 4), now, "tr", "yarın")).toBe("Pazartesi 04:00'te");
    expect(atPhrase(at(2, 4), now, "en", "tomorrow")).toBe("on Monday at 04:00");
    expect(atPhrase(at(10, 4), now, "en", "tomorrow")).toBe("on 3 November at 04:00");
  });

  it("say until when", () => {
    expect(untilPhrase(at(6, 12), now, "tr")).toBe("30 Ekim'e");
    expect(untilPhrase(at(6, 12), now, "en")).toBe("until 30 October");
    expect(untilPhrase(at(0, 14, 35), now, "tr", true)).toBe("14:35'e");
    expect(untilPhrase(at(0, 14, 35), now, "en", true)).toBe("until 14:35");
  });

  it("count minutes and group days", () => {
    expect(minutesLeft(at(0, 12, 8), now)).toBe(8);
    expect(minutesLeft(at(0, 11), now)).toBe(0);
    expect(dayGroup(at(0, 9), now)).toBe("today");
    expect(dayGroup(at(-1, 23), now)).toBe("yesterday");
    expect(dayGroup(at(-3, 9), now)).not.toBe("yesterday");
    expect(durationParts(36 * 3_600)).toEqual([
      ["days", 1],
      ["hours", 12],
    ]);
    expect(durationParts(20 * 60)).toEqual([["minutes", 20]]);
  });
});
