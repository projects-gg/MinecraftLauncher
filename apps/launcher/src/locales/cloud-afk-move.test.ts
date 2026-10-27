import { describe, expect, it } from "vitest";
import { enCloudAfkMove } from "./en-cloud-afk-move";
import { trCloudAfkMove } from "./tr-cloud-afk-move";

/**
 * Movement's texts (FEAT-L) keep the rules of the wired locales: the same keys, the same
 * placeholders, no Turkish letters in English, no technical words, and the contract's own wording
 * (README "Movement" → texts in the apps), which the phone uses too.
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

const turkish = flatten(trCloudAfkMove);
const english = flatten(enCloudAfkMove);
const placeholders = (text: string) =>
  [...new Set([...text.matchAll(/\{\{\s*([^}\s,]+)[^}]*\}\}/g)].map((m) => m[1] ?? ""))]
    .sort()
    .join(",");

describe("movement texts", () => {
  it("have the same keys and placeholders in both languages", () => {
    expect([...turkish.keys()].filter((key) => !english.has(key))).toEqual([]);
    expect([...english.keys()].filter((key) => !turkish.has(key))).toEqual([]);
    expect(
      [...turkish].filter(
        ([key, text]) => placeholders(english.get(key) ?? "") !== placeholders(text),
      ),
    ).toEqual([]);
  });

  it("show no Turkish letters in English and no technical words to players", () => {
    expect([...english].filter(([, text]) => /[çğıöşüÇĞİÖŞÜ]/.test(text))).toEqual([]);
    const technical = /\b(movst|movres|socket|slot_|HTTP|API|JSON|yaw)\b/i;
    expect([...turkish, ...english].filter(([, text]) => technical.test(text))).toEqual([]);
  });

  it("use the contract's words", () => {
    expect(trCloudAfkMove.setting.line).toBe(
      "Açıkken hesap yürüyebilir; düşebilir ya da ölebilir.",
    );
    expect(enCloudAfkMove.setting.line).toBe("When on, the account can walk; it may fall or die.");
    expect(Object.values(trCloudAfkMove.pad).slice(1)).toEqual([
      "İleri",
      "Geri",
      "Sola",
      "Sağa",
      "Zıpla",
      "Eğil",
    ]);
    expect(trCloudAfkMove.main.go).toBe("Ana hesabının yanına git");
    expect(trCloudAfkMove.main.follow).toBe("Ana hesabını takip et");
    expect(trCloudAfkMove.far).toBe("En çok 256 blok uzağa gidebilir.");
    expect(trCloudAfkMove.noSlot).toBe("Şu an hareket için yer yok.");
    expect(trCloudAfkMove.oneAccount).toBe("Hareket aynı anda yalnız bir hesabında açık olabilir.");
    expect(trCloudAfkMove.timeout).toBe("2 dakikada varamadı, durdu.");
    expect(enCloudAfkMove.reset).toBe("The account was teleported or respawned; it stopped.");
    expect(enCloudAfkMove.dims.end).toBe("The End");
  });
});
