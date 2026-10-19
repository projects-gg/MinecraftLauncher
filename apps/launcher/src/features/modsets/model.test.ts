import { describe, expect, it } from "vitest";
import type { SetCard, SetItem, SetVariant, SyncSummary } from "@/lib/api";
import { enModSets } from "@/locales/en-modsets";
import { trModSets } from "@/locales/tr-modsets";
import {
  bytesOf,
  changedChoices,
  countWords,
  describeSummary,
  enabledItems,
  fitFor,
  fitWords,
  iconFor,
  iconStrip,
  neededBy,
  newestVariant,
  setName,
  setSummary,
  splitItems,
  summaryWords,
} from "./model";

function item(id: string, extra: Partial<SetItem> = {}): SetItem {
  return {
    id,
    title: id.toUpperCase(),
    kind: "mod",
    iconUrl: `https://cdn.modrinth.com/data/${id}/icon.png`,
    defaultEnabled: true,
    library: false,
    requires: [],
    sizeBytes: 100,
    ...extra,
  };
}

function variant(minecraft: string, extra: Partial<SetVariant> = {}): SetVariant {
  return {
    minecraft,
    loader: "fabric",
    loaderVersion: "0.19.5",
    items: [
      item("zoom", { requires: ["api"] }),
      item("hud"),
      item("fire", { kind: "resourcePack", defaultEnabled: false, sizeBytes: 50 }),
      item("api", { library: true, sizeBytes: 1000, iconUrl: undefined }),
    ],
    mods: 2,
    resourcePacks: 0,
    shaderPacks: 0,
    downloadBytes: 1200,
    ...extra,
  };
}

function card(extra: Partial<SetCard> = {}): SetCard {
  return {
    id: "pvp",
    name: "PvP",
    summary: "Alçak ateş ve net HUD.",
    purpose: "pvp",
    variants: [variant("26.3"), variant("26.2")],
    ...extra,
  };
}

const emptyCounts = () => ({ mods: 0, resourcePacks: 0, shaderPacks: 0, configs: 0, other: 0 });

describe("set fit", () => {
  it("uses the variant of the instance's version and loader", () => {
    const fit = fitFor(card(), "26.2", "fabric");
    expect(fit.kind).toBe("fits");
    expect(fit.kind === "fits" && fit.variant.minecraft).toBe("26.2");
  });

  it("says which versions or loader a set is for", () => {
    expect(fitFor(card(), "26.1.2", "fabric")).toEqual({
      kind: "otherVersion",
      versions: ["26.3", "26.2"],
    });
    expect(fitFor(card(), "26.3", "neoForge")).toEqual({ kind: "otherLoader", loader: "fabric" });
    expect(fitFor(card(), "26.3", "vanilla")).toEqual({ kind: "otherLoader", loader: "fabric" });
    const loaderName = (loader: string) => loader.toUpperCase();
    expect(fitWords({ kind: "otherLoader", loader: "fabric" }, loaderName)).toEqual({
      key: "modSets.card.forLoader",
      values: { loader: "FABRIC" },
    });
    expect(fitWords({ kind: "otherVersion", versions: ["26.3", "26.2"] }, loaderName)).toEqual({
      key: "modSets.card.forVersions",
      values: { versions: "26.3, 26.2" },
    });
    expect(fitWords({ kind: "fits", variant: variant("26.3") }, loaderName)).toBeNull();
  });

  it("fits any loader with a packs-only variant", () => {
    const packs = card({
      variants: [
        variant("26.3", {
          loader: undefined,
          items: [item("fire", { kind: "resourcePack" })],
        }),
      ],
    });
    for (const loader of ["vanilla", "fabric", "neoForge"] as const) {
      expect(fitFor(packs, "26.3", loader).kind).toBe("fits");
    }
  });

  it("starts new instances on the newest version", () => {
    expect(newestVariant(card())?.minecraft).toBe("26.3");
    expect(newestVariant(card({ variants: [] }))).toBeUndefined();
  });
});

describe("set card", () => {
  it("shows the choices' icons, never the libraries", () => {
    const strip = iconStrip(variant("26.3"), 2);
    expect(strip.shown.map((i) => i.id)).toEqual(["zoom", "hud"]);
    expect(strip.more).toBe(1);
    expect(iconStrip(variant("26.3")).more).toBe(0);
  });

  it("counts what adding the set installs, without zeros", () => {
    expect(countWords({ mods: 12, resourcePacks: 3, shaderPacks: 0 })).toEqual([
      { key: "modSets.card.mods", values: { count: 12 } },
      { key: "modSets.card.resourcePacks", values: { count: 3 } },
    ]);
    expect(countWords({ mods: 0, resourcePacks: 0, shaderPacks: 1 })).toEqual([
      { key: "modSets.card.shaderPacks", values: { count: 1 } },
    ]);
  });

  it("speaks the interface language when the team wrote it", () => {
    const set = card({ nameEn: "PvP", summaryEn: "Low fire and a clear HUD." });
    expect(setSummary(set, true)).toBe("Low fire and a clear HUD.");
    expect(setSummary(set, false)).toBe("Alçak ateş ve net HUD.");
    expect(setName(card({ name: "Performans", nameEn: "Performance" }), true)).toBe("Performance");
    expect(setName(card({ name: "Performans", nameEn: " " }), true)).toBe("Performans");
    expect(setSummary(card({ summary: undefined }), true)).toBeUndefined();
  });

  it("gives a new instance the purpose's icon", () => {
    expect(iconFor("pvp")).toBe("swords");
    expect(iconFor("performance")).toBe("pickaxe");
    expect(iconFor("visual")).toBe("potion");
    expect(iconFor("other")).toBe("compass");
  });
});

describe("set choices", () => {
  const items = variant("26.3").items;

  it("installs the chosen items and what they need", () => {
    expect([...enabledItems(items, {})].sort()).toEqual(["api", "hud", "zoom"]);
    expect([...enabledItems(items, { zoom: false, fire: true })].sort()).toEqual(["fire", "hud"]);
    expect(bytesOf(items, enabledItems(items, {}))).toBe(1200);
    expect(neededBy(items, enabledItems(items, {}), "api")).toEqual(["ZOOM"]);
  });

  it("sends only the choices that differ from the defaults", () => {
    expect(changedChoices(items, { zoom: true, hud: false, api: false })).toEqual({ hud: false });
    expect(changedChoices(items, {})).toEqual({});
  });

  it("keeps libraries apart from the choices", () => {
    const { choices, libraries } = splitItems(items);
    expect(choices.map((i) => i.id)).toEqual(["zoom", "hud", "fire"]);
    expect(libraries.map((i) => i.id)).toEqual(["api"]);
  });
});

describe("set summary", () => {
  const summary: SyncSummary = {
    added: { ...emptyCounts(), mods: 2, resourcePacks: 1 },
    updated: { ...emptyCounts(), mods: 1 },
    removed: emptyCounts(),
    kept: 0,
    unchanged: 4,
    downloadBytes: 3_000_000,
    removeBytes: 0,
  };

  it("lists what changes by action and kind", () => {
    expect(summaryWords(summary)).toEqual([
      {
        key: "modSets.summary.added",
        items: [
          { key: "modSets.summary.mods", values: { count: 2 } },
          { key: "modSets.summary.resourcePacks", values: { count: 1 } },
        ],
      },
      {
        key: "modSets.summary.updated",
        items: [{ key: "modSets.summary.mods", values: { count: 1 } }],
      },
    ]);
  });

  it("reads as one sentence", () => {
    const t = (key: string, values?: Record<string, unknown>) =>
      `${key}${values ? JSON.stringify(values) : ""}`;
    const text = describeSummary(
      summary,
      t,
      (parts) => parts.join(" & "),
      (b) => `${b}B`,
    );
    expect(text).toContain("modSets.summary.added");
    expect(text).toContain("modSets.summary.download");
    expect(text).toContain("3000000B");
    const none = { ...summary, added: emptyCounts(), updated: emptyCounts(), downloadBytes: 0 };
    expect(describeSummary(none, t, (p) => p.join(), String)).toBe("modSets.summary.noFiles");
  });
});

/** Every string key of a locale tree (`a.b.c`), plural forms folded into their key. */
function keys(tree: unknown, prefix = ""): string[] {
  if (typeof tree === "string") return [prefix.replace(/_(one|other)$/, "")];
  if (!tree || typeof tree !== "object") return [];
  return Object.entries(tree).flatMap(([name, child]) =>
    keys(child, prefix ? `${prefix}.${name}` : name),
  );
}

describe("set words", () => {
  it("exist in both languages", () => {
    const tr = new Set(keys(trModSets));
    const en = new Set(keys(enModSets));
    expect([...tr].filter((k) => !en.has(k))).toEqual([]);
    expect([...en].filter((k) => !tr.has(k))).toEqual([]);
  });

  it("cover every line the model asks for", () => {
    const tr = new Set(keys(trModSets).map((k) => `modSets.${k}`));
    const used = [
      ...countWords({ mods: 1, resourcePacks: 1, shaderPacks: 1 }).map((w) => w.key),
      ...summaryWords({
        added: { ...emptyCounts(), mods: 1, resourcePacks: 1, shaderPacks: 1 },
        updated: { ...emptyCounts(), mods: 1 },
        removed: { ...emptyCounts(), mods: 1 },
        kept: 0,
        unchanged: 0,
        downloadBytes: 1,
        removeBytes: 0,
      }).flatMap((part) => [part.key, ...part.items.map((i) => i.key)]),
      "modSets.card.forLoader",
      "modSets.card.forVersions",
      "modSets.summary.noFiles",
      "modSets.summary.download",
      "modSets.kind.mod",
      "modSets.kind.resourcePack",
      "modSets.kind.shaderPack",
      "modSets.purpose.pvp",
      "modSets.purpose.performance",
      "modSets.purpose.visual",
      "modSets.purpose.other",
    ];
    expect(used.filter((key) => !tr.has(key))).toEqual([]);
  });

  it("speak to the player informally and never show technical words", () => {
    const strings = keys(trModSets).map((k) =>
      k.split(".").reduce<unknown>((o, part) => (o as Record<string, unknown>)[part], trModSets),
    );
    for (const text of strings) {
      expect(typeof text).toBe("string");
      // Placeholders ({{loader}}) are filled with player-facing names.
      const words = String(text).replace(/\{\{[^}]+\}\}/g, "");
      expect(words.toLocaleLowerCase("tr")).not.toMatch(
        /lütfen|deneyin|modrinth|json|sha1|sha512|\.jar|loader/,
      );
    }
  });

  it("keep Turkish letters out of the English words", () => {
    for (const key of keys(enModSets)) {
      const text = key
        .split(".")
        .reduce<unknown>((o, part) => (o as Record<string, unknown>)?.[part], enModSets);
      expect(String(text ?? "")).not.toMatch(/[ıİğĞşŞçÇöÖüÜ]/);
    }
  });
});
