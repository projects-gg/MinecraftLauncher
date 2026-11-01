import { describe, expect, it } from "vitest";
import {
  cleanName,
  discordIdOf,
  discordIsAccount,
  identitiesOf,
  linkableOf,
  linkedDate,
  microsoftOffer,
  ownNameOffered,
  proofProvider,
  suggestedName,
  yearIn,
} from "./identity";

const NOW = Date.UTC(2026, 10, 1, 12);

describe("whose Discord a profile has", () => {
  it("reads a profile that does not say as an account opened with Discord", () => {
    expect(discordIdOf({ id: "123456789012345678" })).toBe("123456789012345678");
    expect(discordIdOf({ id: "9123456789012345678", discordId: null })).toBeNull();
    expect(discordIdOf({ id: "9123456789012345678", discordId: "80351110224678912" })).toBe(
      "80351110224678912",
    );
  });

  it("keeps the Projects skin and 7/24 Bulut to accounts opened with Discord", () => {
    expect(discordIsAccount({ id: "123456789012345678" })).toBe(true);
    expect(discordIsAccount({ id: "123456789012345678", discordId: "123456789012345678" })).toBe(
      true,
    );
    expect(discordIsAccount({ id: "9123456789012345678", discordId: "80351110224678912" })).toBe(
      false,
    );
    expect(discordIsAccount({ id: "9123456789012345678", discordId: null })).toBe(false);
    expect(discordIsAccount(undefined)).toBe(false);
  });
});

describe("the player's ways in", () => {
  it("lists Discord first and offers only what is not linked", () => {
    const me = {
      username: "gecekurdu",
      identities: [
        { provider: "microsoft" as const, linkedOn: "2026-11-01", isNew: true },
        { provider: "discord" as const, linkedOn: "2026-10-16", isNew: false, hint: "gecekurdu" },
      ],
    };
    expect(identitiesOf(me).map((way) => way.provider)).toEqual(["discord", "microsoft"]);
    expect(linkableOf(me, ["discord", "microsoft"])).toEqual([]);
    const old = { username: "ayse" };
    expect(identitiesOf(old)).toEqual([
      { provider: "discord", linkedOn: "", isNew: false, hint: "ayse" },
    ]);
    // Microsoft only while the service offers it.
    expect(linkableOf(old, undefined)).toEqual([]);
    expect(linkableOf(old, ["discord", "microsoft"])).toEqual(["microsoft"]);
  });

  it("offers Microsoft under the linked ways until it opens accounts", () => {
    expect(microsoftOffer({})).toBe("none");
    expect(microsoftOffer({ providers: ["discord"], signUpWays: ["discord"] })).toBe("none");
    expect(microsoftOffer({ providers: ["discord", "microsoft"], signUpWays: ["discord"] })).toBe(
      "linked",
    );
    expect(microsoftOffer({ providers: ["discord", "microsoft"] })).toBe("linked");
    expect(
      microsoftOffer({ providers: ["discord", "microsoft"], signUpWays: ["discord", "microsoft"] }),
    ).toBe("open");
    // New accounts without sign-in open nothing.
    expect(microsoftOffer({ providers: ["discord"], signUpWays: ["discord", "microsoft"] })).toBe(
      "none",
    );
  });

  it("offers a name of one's own as the service takes it", () => {
    const discord = { id: "123456789012345678", nameSource: "discord" as const, ownNames: true };
    // An account with Discord: only while Microsoft signs in.
    expect(ownNameOffered(discord, "none")).toBe(false);
    expect(ownNameOffered(discord, "linked")).toBe(true);
    expect(ownNameOffered(discord, "open")).toBe(true);
    // Without Discord, or with the placeholder: always.
    expect(ownNameOffered({ ...discord, discordId: null }, "none")).toBe(true);
    expect(ownNameOffered({ ...discord, nameSource: "unset" as const }, "none")).toBe(true);
    // An older service takes no name.
    expect(ownNameOffered({ ...discord, ownNames: false, discordId: null }, "open")).toBe(false);
  });

  it("proves a removal with the other way", () => {
    expect(proofProvider({ provider: "microsoft", unlink: false })).toBe("microsoft");
    expect(proofProvider({ provider: "microsoft", unlink: true })).toBe("discord");
    expect(proofProvider({ provider: "discord", unlink: true })).toBe("microsoft");
  });
});

describe("when a way was linked", () => {
  it("says the month with its Turkish ending", () => {
    const months = [
      "Ocak'ta",
      "Şubat'ta",
      "Mart'ta",
      "Nisan'da",
      "Mayıs'ta",
      "Haziran'da",
      "Temmuz'da",
      "Ağustos'ta",
      "Eylül'de",
      "Ekim'de",
      "Kasım'da",
      "Aralık'ta",
    ];
    months.forEach((month, index) => {
      const linkedOn = `2026-${String(index + 1).padStart(2, "0")}-25`;
      expect(linkedDate(linkedOn, "tr", NOW)).toBe(`25 ${month}`);
    });
    expect(linkedDate("2026-10-25", "en", NOW)).toBe("25 October");
  });

  it("adds the year of another one, with its own ending", () => {
    expect(linkedDate("2025-03-03", "tr", NOW)).toBe("3 Mart 2025'te");
    expect(linkedDate("2025-03-03", "en", NOW)).toBe("3 March 2025");
    expect([2020, 2021, 2023, 2026, 2027, 2029, 2030, 2040, 2060, 2070, 2000].map(yearIn)).toEqual([
      "'de",
      "'de",
      "'te",
      "'da",
      "'de",
      "'da",
      "'da",
      "'ta",
      "'ta",
      "'te",
      "'de",
    ]);
  });

  it("says nothing for a date it cannot read", () => {
    expect(linkedDate("", "tr", NOW)).toBeNull();
    expect(linkedDate("2026-13-01", "tr", NOW)).toBeNull();
    expect(linkedDate("25.10.2026", "en", NOW)).toBeNull();
  });
});

describe("profile names", () => {
  it("follows the service's rules", () => {
    expect(cleanName("  Gece   Kurdu ")).toBe("Gece Kurdu");
    expect(cleanName("ÇılgınÖğrenci_7")).toBe("ÇılgınÖğrenci_7");
    expect(cleanName("Kâmil")).toBe("Kâmil");
    expect(cleanName("x1_")).toBe("x1_");
    expect(cleanName("abcdefghijklmnop")).toBe("abcdefghijklmnop");
    for (const bad of ["ab", "a_ _", "_a_", "abcdefghijklmnopq", "Ayşe!", "tab\tname", "José"]) {
      expect(cleanName(bad), bad).toBeNull();
    }
  });

  it("suggests the active Minecraft account's name when it passes them", () => {
    expect(
      suggestedName([
        { username: "GeceKurdu_", active: true },
        { username: "Lavci_42", active: false },
      ]),
    ).toBe("GeceKurdu_");
    expect(suggestedName([{ username: "x_", active: true }])).toBe("");
    expect(suggestedName([{ username: "Lavci_42", active: false }])).toBe("");
    expect(suggestedName(undefined)).toBe("");
  });
});
