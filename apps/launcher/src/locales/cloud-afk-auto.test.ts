import { describe, expect, it } from "vitest";
import { enCloudAfkAuto } from "./en-cloud-afk-auto";
import { trCloudAfkAuto } from "./tr-cloud-afk-auto";

/**
 * Auto-fishing's and auto-mining's texts (FISHMINE-L) keep the rules of the wired locales: the same
 * keys, the same placeholders, no Turkish letters in English, no technical words, and the
 * contract's disclaimer word for word (owner, 20 Nov 2026).
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

const turkish = flatten(trCloudAfkAuto);
const english = flatten(enCloudAfkAuto);
const placeholders = (text: string) =>
  [...new Set([...text.matchAll(/\{\{\s*([^}\s,]+)[^}]*\}\}/g)].map((m) => m[1] ?? ""))]
    .sort()
    .join(",");

describe("auto-fishing and auto-mining texts", () => {
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
    const technical =
      /\b(autost|autores|socket|slot|HTTP|API|JSON|yaw|pitch|limited_|mode_limits)\b/i;
    expect([...turkish, ...english].filter(([, text]) => technical.test(text))).toEqual([]);
    expect([...turkish, ...english].filter(([, text]) => /[!—]/.test(text))).toEqual([]);
  });

  it("carry the disclaimer as the site does: the contract's sentence in two", () => {
    // One idea per sentence (QA-COPY O2): the contract's semicolon became a full stop, as on
    // afk.projects.gg (FISHMINE-WEB).
    expect(trCloudAfkAuto.disclaimer).toBe(
      "Her sunucuda tek tek denenmedi. Her sunucuda çalışacağı garanti edilmez.",
    );
    expect(enCloudAfkAuto.disclaimer).toBe(
      "Not tested server by server. It is not guaranteed to work on every server.",
    );
    expect([...turkish, ...english].filter(([, text]) => text.includes(";"))).toEqual([]);
  });

  it("say what the account does, never what the server makes of it, and where fishing works", () => {
    // FM-WEB's final words (fishmine-web 566799aa): no promise about the server (servers differ).
    expect(trCloudAfkAuto.fishing.lookHelp).toBe(
      "Atışlar arasında başını biraz çevirip geri döner.",
    );
    expect(enCloudAfkAuto.fishing.lookHelp).toBe(
      "Between casts it turns its head a little and back.",
    );
    expect([...turkish, ...english].filter(([, text]) => /\bAFK\b/.test(text))).toEqual([]);
    expect(trCloudAfkAuto.fishing.waterHint).toBe("Hesabını suyun kenarında, suya bakarken bırak.");
    expect(enCloudAfkAuto.fishing.waterHint).toBe(
      "Leave your account at the water's edge, facing the water.",
    );
    expect(trCloudAfkAuto.fishing.noWater).toBe(
      `Önünde su bulamadı. ${trCloudAfkAuto.fishing.waterHint}`,
    );
    expect(enCloudAfkAuto.fishing.noWater).toBe(
      `It can't find water in front of it. ${enCloudAfkAuto.fishing.waterHint}`,
    );
  });

  it("say a limited server's stored-on mode does not run there, as every surface does", () => {
    expect(trCloudAfkAuto.limitedOn).toBe("Bu sunucuda çalışmaz.");
    expect(enCloudAfkAuto.limitedOn).toBe("It doesn't run on this server.");
  });

  it("title the modes, tabs and buttons in Title Case", () => {
    const titles = [
      trCloudAfkAuto.fishing.title,
      trCloudAfkAuto.mining.title,
      trCloudAfkAuto.tabs.fishing,
      trCloudAfkAuto.tabs.mining,
      trCloudAfkAuto.limitedLink,
      trCloudAfkAuto.pause,
      trCloudAfkAuto.resume,
      trCloudAfkAuto.full.title,
      trCloudAfkAuto.blocks.title,
      trCloudAfkAuto.idle.title,
      trCloudAfkAuto.aim.title,
      trCloudAfkAuto.keep,
      trCloudAfkAuto.fishing.switch,
      trCloudAfkAuto.fishing.look,
      trCloudAfkAuto.mining.switch,
      trCloudAfkAuto.fishing.items,
      trCloudAfkAuto.fishing.last,
      trCloudAfkAuto.mining.items,
    ];
    const small = new Set(["ve", "ile", "de", "da", "için"]);
    const notTitle = titles.filter((title) =>
      title
        .split(" ")
        .some((word) => !small.has(word) && word[0] !== word[0]?.toLocaleUpperCase("tr")),
    );
    expect(notTitle).toEqual([]);
    expect([trCloudAfkAuto.fishing.title, trCloudAfkAuto.mining.title]).toEqual([
      "Otomatik Balık Tutma",
      "Otomatik Kazma",
    ]);
  });
});
