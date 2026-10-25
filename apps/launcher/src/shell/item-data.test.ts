import { createHash } from "node:crypto";
import {
  canCombine,
  enchantmentsFor,
  ITEM_DATA_FORMAT,
  type ItemDataManifest,
  itemDataFromText,
  itemName,
  levelText,
  loadItemData,
  parseItemData,
  pickItemDataVersion,
  searchEnchantments,
  searchItems,
} from "@pl/ui/items";
import { fold } from "@pl/ui/search";
import { describe, expect, it } from "vitest";

// A small file in the format `pl_items::data` writes (crates/pl-items/src/data.rs).
const DOC = {
  format: ITEM_DATA_FORMAT,
  mc: "26.3",
  src: { server: "ab".repeat(20) },
  items: [
    ["minecraft:stone", 64, 0, "Taş", "Stone", -1],
    ["minecraft:obsidian", 64, 0, "Obsidiyen", "Obsidian", -1, { a: ["obsi"] }],
    ["minecraft:diamond", 64, 0, "Elmas", "Diamond", -1, { a: ["dia"] }],
    [
      "minecraft:diamond_block",
      64,
      0,
      "Elmas Bloğu",
      "Block of Diamond",
      -1,
      { a: ["dia blok", "elmas blok"] },
    ],
    ["minecraft:diamond_sword", 1, 1561, "Elmas Kılıç", "Diamond Sword", 0, { a: ["dia kilic"] }],
    ["minecraft:elytra", 1, 432, "Elitra", "Elytra", 1, { a: ["kanat", "elytira"] }],
    ["minecraft:enchanted_book", 1, 0, "Büyülü Kitap", "Enchanted Book", 2],
    [
      "minecraft:music_disc_cat",
      1,
      0,
      "Müzik Diski",
      "Music Disc",
      -1,
      { d: ["C418 - cat", "C418 - cat"] },
    ],
    [
      "minecraft:music_disc_pigstep",
      1,
      0,
      "Müzik Diski",
      "Music Disc",
      -1,
      { d: ["Lena Raine - Pigstep", "Lena Raine - Pigstep"] },
    ],
    [
      "minecraft:sentry_armor_trim_smithing_template",
      64,
      0,
      "Demirci Şablonu",
      "Smithing Template",
      -1,
      { d: ["Nöbetçi Zırh Süslemesi", "Sentry Armor Trim"] },
    ],
    ["minecraft:beacon", 64, 0, "Fener", "Beacon", -1, { a: ["isaret feneri"] }],
    ["minecraft:lantern", 64, 0, "Fener", "Lantern", -1],
  ],
  ench: [
    ["minecraft:binding_curse", 1, "Bağlanma Laneti", "Curse of Binding", { c: 1 }],
    ["minecraft:mending", 1, "Onarım", "Mending", { a: ["mend"] }],
    ["minecraft:sharpness", 5, "Keskinlik", "Sharpness", { a: ["sharp"], x: [3] }],
    ["minecraft:smite", 5, "Çarpma", "Smite", { x: [2] }],
  ],
  groups: [[1, 2, 3], [1], [0, 1, 2, 3]],
};

const data = parseItemData(DOC);
if (!data) throw new Error("the test file does not parse");

const ids = (hits: { ref: string }[]) => hits.map((h) => h.ref.replace("minecraft:", ""));

describe("item data", () => {
  it("reads rows, extras and enchantment groups", () => {
    expect(data.minecraft).toBe("26.3");
    expect(data.items).toHaveLength(12);
    const sword = data.byId.get("minecraft:diamond_sword");
    expect(sword).toMatchObject({ stack: 1, maxDamage: 1561, aliases: ["dia kilic"] });
    expect(sword && itemName(sword, "tr")).toBe("Elmas Kılıç");
    expect(sword && enchantmentsFor(data, sword).map((e) => e.id)).toEqual([
      "minecraft:mending",
      "minecraft:sharpness",
      "minecraft:smite",
    ]);
    const book = data.byId.get("minecraft:enchanted_book");
    expect(book && enchantmentsFor(data, book)).toHaveLength(4);
    expect(data.byId.get("minecraft:stone")?.enchantments).toEqual([]);
    expect(data.byId.get("minecraft:music_disc_cat")?.desc).toEqual({
      tr: "C418 - cat",
      en: "C418 - cat",
    });
    const sharpness = data.enchantmentById.get("minecraft:sharpness");
    const smite = data.enchantmentById.get("minecraft:smite");
    const mending = data.enchantmentById.get("minecraft:mending");
    expect(data.enchantmentById.get("minecraft:binding_curse")?.curse).toBe(true);
    expect(sharpness?.curse).toBe(false);
    if (!sharpness || !smite || !mending) throw new Error("enchantments missing");
    expect(canCombine(data, sharpness, smite)).toBe(false);
    expect(canCombine(data, sharpness, mending)).toBe(true);
    expect(canCombine(data, sharpness, sharpness)).toBe(false);
  });

  it("refuses broken or newer files and reads past unknown extras", () => {
    type Doc = { items: unknown[][]; ench: unknown[][]; groups: number[][] };
    const changed = (edit: (doc: Doc) => void) => {
      const copy = structuredClone(DOC) as unknown as Doc;
      edit(copy);
      return parseItemData(copy);
    };
    expect(changed((d) => Object.assign(d, { format: 2 }))).toBeUndefined();
    expect(changed((d) => d.items[0]?.splice(5, 1, 9))).toBeUndefined();
    expect(
      changed((d) => d.items.push(["minecraft:stone", 64, 0, "Taş", "Stone", -1])),
    ).toBeUndefined();
    expect(
      changed((d) => d.items.push(["minecraft:x", 64, 0, "X", "X", -1, { a: ["Dia"] }])),
    ).toBeUndefined();
    expect(changed((d) => d.items.push(["minecraft:x", 0, 0, "X", "X", -1]))).toBeUndefined();
    expect(changed((d) => d.ench.push(["minecraft:x", 1, "X", "X", { x: [4] }]))).toBeUndefined();
    expect(changed((d) => d.groups.push([2, 1]))).toBeUndefined();
    expect(changed((d) => Object.assign(d, { items: [] }))).toBeUndefined();
    expect(
      changed((d) => d.items.push(["minecraft:x", 64, 0, "X", "X", -1, { a: ["x x"], z: 1 }])),
    ).toBeDefined();
    expect(itemDataFromText("{")).toBeUndefined();
    expect(itemDataFromText(JSON.stringify(DOC))?.items).toHaveLength(12);
  });

  it("picks the nearest version's file", () => {
    const file = { file: "v/items.0123456789ab.json", sha256: "0".repeat(64), bytes: 1 };
    const manifest: ItemDataManifest = {
      format: 1,
      default: "26.3",
      versions: Object.fromEntries(
        ["1.20.5", "1.21.4", "1.21.10", "26.1.2", "26.3"].map((v) => [v, file]),
      ),
    };
    expect(pickItemDataVersion(manifest, "26.1.2")).toBe("26.1.2");
    expect(pickItemDataVersion(manifest, "26.2")).toBe("26.1.2");
    expect(pickItemDataVersion(manifest, "1.21.11")).toBe("1.21.10");
    expect(pickItemDataVersion(manifest, "26.4-snapshot-5")).toBe("26.3");
    expect(pickItemDataVersion(manifest, "1.8.9")).toBe("1.20.5");
    expect(pickItemDataVersion(manifest)).toBe("26.3");
  });

  it("finds items the way players write them", () => {
    const tr = (query: string, limit?: number) =>
      ids(searchItems(data, query, { lang: "tr", limit }));
    expect(tr("elmas")).toEqual(["diamond", "diamond_block", "diamond_sword"]);
    expect(tr("ELMAS KILIÇ")[0]).toBe("diamond_sword");
    expect(tr("elmas kilic")[0]).toBe("diamond_sword");
    expect(tr("elmas kil")).toEqual(["diamond_sword"]);
    expect(tr("dia kılıç")[0]).toBe("diamond_sword");
    expect(tr("dia")[0]).toBe("diamond");
    expect(tr("Diamond Sword")[0]).toBe("diamond_sword");
    expect(tr("kanat")).toEqual(["elytra"]);
    expect(tr("elytra")).toEqual(["elytra"]);
    expect(tr("elitar")).toEqual(["elytra"]);
    expect(tr("nöbetçi")).toEqual(["sentry_armor_trim_smithing_template"]);
    expect(tr("pigstep")).toEqual(["music_disc_pigstep"]);
    expect(tr("müzik")).toEqual(["music_disc_cat", "music_disc_pigstep"]);
    expect(tr("fener")).toEqual(["beacon", "lantern"]);
    expect(tr("obsi")).toEqual(["obsidian"]);
    expect(tr("elmas", 2)).toHaveLength(2);
    expect(tr("")).toEqual([]);
    expect(tr("   ")).toEqual([]);
    expect(ids(searchItems(data, "block of", { lang: "en" }))).toEqual(["diamond_block"]);
    const extras = [{ ref: "custom:c1", names: ["Yakut"] }];
    const hits = searchItems(data, "yak", { lang: "tr", extras });
    expect(hits[0]).toMatchObject({ ref: "custom:c1", extra: extras[0] });
    expect(searchItems(undefined, "yakut", { lang: "tr", extras }).map((h) => h.ref)).toEqual([
      "custom:c1",
    ]);
  });

  it("finds enchantments, only the ones that fit when an item is given", () => {
    const tr = (q: string) => searchEnchantments(data, q, { lang: "tr" }).map((e) => e.id);
    expect(tr("sharp")).toEqual(["minecraft:sharpness"]);
    expect(tr("kes")).toEqual(["minecraft:sharpness"]);
    expect(tr("onarim")).toEqual(["minecraft:mending"]);
    const elytra = data.byId.get("minecraft:elytra");
    expect(searchEnchantments(data, "", { lang: "tr", item: elytra }).map((e) => e.id)).toEqual([
      "minecraft:mending",
    ]);
    expect(searchEnchantments(data, "sharp", { lang: "tr", item: elytra })).toEqual([]);
    expect([1, 4, 5, 10, 11].map(levelText)).toEqual(["I", "IV", "V", "X", "11"]);
  });

  it("loads a version from the manifest after checking its SHA-256", async () => {
    const body = new TextEncoder().encode(JSON.stringify(DOC));
    const sha = createHash("sha256").update(body).digest("hex");
    const manifest = {
      format: 1,
      default: "26.3",
      versions: {
        "26.3": { file: `26.3/items.${sha.slice(0, 12)}.json`, sha256: sha, bytes: body.length },
      },
    };
    const base = "https://cdn.test/itemdata/v1/";
    const asked: string[] = [];
    let corrupt = true;
    const get = async (url: string) => {
      asked.push(url);
      if (url === `${base}manifest.json`) return new TextEncoder().encode(JSON.stringify(manifest));
      if (url === `${base}26.3/items.${sha.slice(0, 12)}.json`) {
        return corrupt ? new TextEncoder().encode(" ") : body;
      }
      return undefined;
    };
    expect(await loadItemData("26.3.1", { base, get })).toBeUndefined();
    corrupt = false;
    const loaded = await loadItemData("26.3.1", { base, get });
    expect(loaded?.items).toHaveLength(12);
    expect(await loadItemData("26.3.1", { base, get })).toBe(loaded);
    expect(asked.filter((u) => u.endsWith(".json") && !u.endsWith("manifest.json"))).toHaveLength(
      2,
    );
    expect(await loadItemData("26.3", { base: "https://cdn.test/none/", get })).toBeUndefined();
    // A manifest naming a file anywhere but next to it, or under another hash, is not followed.
    const elsewhere = `https://evil.test/items.${sha.slice(0, 12)}.json`;
    for (const file of [
      elsewhere,
      `26.3/items.${"0".repeat(12)}.json`,
      `../items.${sha.slice(0, 12)}.json`,
    ]) {
      const odd = { ...manifest, versions: { "26.3": { ...manifest.versions["26.3"], file } } };
      const oddBase = `https://cdn.test/odd-${asked.length}/`;
      const oddGet = async (url: string) =>
        url === `${oddBase}manifest.json`
          ? new TextEncoder().encode(JSON.stringify(odd))
          : get(url);
      expect(await loadItemData("26.3", { base: oddBase, get: oddGet })).toBeUndefined();
    }
    expect(asked).not.toContain(elsewhere);
  });

  it("folds like pl_items::data::fold", () => {
    // The same vectors as crates/pl-items/src/data.rs `folds_like_the_interface`.
    const vectors: [string, string][] = [
      ["Elmas Kılıç", "elmas kilic"],
      ["IŞIK", "isik"],
      ["İnci", "inci"],
      ["Ölümsüzlük Totemi", "olumsuzluk totemi"],
      ["Rüzgâr Patlaması", "ruzgar patlamasi"],
      ["ÇĞİÖŞÜ çğıöşü", "cgiosu cgiosu"],
      ["Iron Ingot", "iron ingot"],
      ["Café crème", "cafe creme"],
      ["s\u0327apka", "sapka"],
      ["Jack o\u2019Lantern", "jack o\u2019lantern"],
      ["æøł", "æøł"],
    ];
    for (const [text, folded] of vectors) expect(fold(text)).toBe(folded);
  });
});
