// AFK+ order item data (FREE-ITEMS, afkplus-free.md §1.4): one Minecraft version's items with
// their stack size, durability, Turkish and English names, the game's grey line under the names it
// repeats (music discs, smithing templates, banner patterns), our aliases, and the enchantments
// that fit them. Built from the game's own files by crates/pl-items (`build_item_data`) and served
// from our CDN under content-addressed names. The format is `pl_items::data`'s
// (crates/pl-items/src/data.rs, "The file (format 1)"); this is its reader, the version choice and
// the search every picker shares (folded as every search in the app: `fold`).
import { fold } from "../search";

/** The item data file's format this code reads; a newer one is not read. */
export const ITEM_DATA_FORMAT = 1;

/** The manifest's format. */
export const ITEM_DATA_MANIFEST_FORMAT = 1;

/** Where the item data lives (`manifest.json` and one folder per version). */
export const ITEM_DATA_BASE = "https://cdn.projects.gg/itemdata/v1/";

export type ItemLang = "tr" | "en";

/** One version's file in the manifest. */
export interface ItemDataFile {
  /** Relative to the manifest: `26.3/items.<first 12 hex of its SHA-256>.json`. */
  file: string;
  sha256: string;
  bytes: number;
}

/** `manifest.json`: each version's file and the version to use when the account's is unknown. */
export interface ItemDataManifest {
  format: number;
  default: string;
  versions: Record<string, ItemDataFile>;
}

export interface ItemNames {
  readonly tr: string;
  readonly en: string;
}

export interface ItemInfo {
  /** `minecraft:diamond_sword`: the order's item reference and `ItemSprite`'s id. */
  readonly id: string;
  /** The most one stack holds (1, 16, 64). */
  readonly stack: number;
  /** Durability when new; 0 when the item takes no damage. */
  readonly maxDamage: number;
  readonly name: ItemNames;
  /** The grey line the game shows under a name that repeats ("Music Disc": "C418 - cat"). */
  readonly desc?: ItemNames;
  /** Our aliases, folded ("elitra", "kanat", "dia kilic"). */
  readonly aliases: readonly string[];
  /** Indexes into `ItemData.enchantments`: the enchantments that fit (any on an enchanted book). */
  readonly enchantments: readonly number[];
  /** An operator's item no one gets in survival (command blocks, barrier): pickers leave it out. */
  readonly operator: boolean;
}

export interface EnchantmentInfo {
  readonly id: string;
  readonly maxLevel: number;
  readonly name: ItemNames;
  /** Binding and vanishing: an order says whether a cursed item may pass. */
  readonly curse: boolean;
  /** Indexes of the enchantments it cannot share an item with. */
  readonly excludes: readonly number[];
  readonly aliases: readonly string[];
}

export interface ItemData {
  readonly minecraft: string;
  readonly items: readonly ItemInfo[];
  readonly enchantments: readonly EnchantmentInfo[];
  readonly byId: ReadonlyMap<string, ItemInfo>;
  readonly enchantmentById: ReadonlyMap<string, EnchantmentInfo>;
}

type Row = readonly unknown[];

const RESOURCE_ID = /^[a-z0-9_.-]+:[a-z0-9_./-]+$/;
const FILE = /^[0-9A-Za-z_-][0-9A-Za-z._-]*\/items\.([0-9a-f]{12})\.json$/;
const ALIAS = /^[a-z0-9]+( [a-z0-9]+)*$/;

const isRecord = (v: unknown): v is Record<string, unknown> =>
  typeof v === "object" && v !== null && !Array.isArray(v);

const isIndex = (v: unknown, below: number): v is number =>
  typeof v === "number" && Number.isInteger(v) && v >= 0 && v < below;

const isName = (v: unknown): v is string =>
  typeof v === "string" && v.length > 0 && v.length <= 240 && v.trim() === v;

/** Strictly ascending indexes below `below`. */
function ascending(list: unknown, below: number): list is number[] {
  return (
    Array.isArray(list) &&
    list.every((v, i) => isIndex(v, below) && (i === 0 || (list[i - 1] as number) < v))
  );
}

function aliasesOf(v: unknown): string[] | undefined {
  if (v === undefined) return [];
  if (!Array.isArray(v) || v.length > 24) return undefined;
  const out: string[] = [];
  for (const a of v) {
    if (typeof a !== "string" || a.length < 2 || a.length > 40 || !ALIAS.test(a)) return undefined;
    if (out.includes(a)) return undefined;
    out.push(a);
  }
  return out;
}

function parseEnchantment(row: unknown, count: number, index: number): EnchantmentInfo | undefined {
  if (!Array.isArray(row) || row.length < 4 || row.length > 5) return undefined;
  const [id, maxLevel, tr, en, extra] = row as Row;
  if (typeof id !== "string" || !RESOURCE_ID.test(id)) return undefined;
  if (typeof maxLevel !== "number" || !Number.isInteger(maxLevel) || maxLevel < 1 || maxLevel > 255)
    return undefined;
  if (!isName(tr) || !isName(en)) return undefined;
  let curse = false;
  let excludes: number[] = [];
  let aliases: string[] | undefined = [];
  if (extra !== undefined) {
    if (!isRecord(extra)) return undefined;
    aliases = aliasesOf(extra.a);
    if (extra.c !== undefined) {
      if (typeof extra.c !== "number") return undefined;
      curse = extra.c === 1;
    }
    if (extra.x !== undefined) {
      if (!ascending(extra.x, count) || extra.x.includes(index)) return undefined;
      excludes = extra.x;
    }
  }
  if (!aliases) return undefined;
  return { id, maxLevel, name: { tr, en }, curse, excludes, aliases };
}

function parseItem(row: unknown, groups: readonly number[][]): ItemInfo | undefined {
  if (!Array.isArray(row) || row.length < 6 || row.length > 7) return undefined;
  const [id, stack, maxDamage, tr, en, group, extra] = row as Row;
  if (typeof id !== "string" || !RESOURCE_ID.test(id)) return undefined;
  if (typeof stack !== "number" || !Number.isInteger(stack) || stack < 1 || stack > 99)
    return undefined;
  if (typeof maxDamage !== "number" || !Number.isInteger(maxDamage) || maxDamage < 0)
    return undefined;
  if (!isName(tr) || !isName(en)) return undefined;
  if (group !== -1 && !isIndex(group, groups.length)) return undefined;
  let aliases: string[] | undefined = [];
  let desc: ItemNames | undefined;
  let operator = false;
  if (extra !== undefined) {
    if (!isRecord(extra)) return undefined;
    aliases = aliasesOf(extra.a);
    if (extra.d !== undefined) {
      const d = extra.d;
      if (!Array.isArray(d) || d.length !== 2 || !isName(d[0]) || !isName(d[1])) return undefined;
      desc = { tr: d[0], en: d[1] };
    }
    if (extra.o !== undefined) {
      if (typeof extra.o !== "number") return undefined;
      operator = extra.o === 1;
    }
  }
  if (!aliases) return undefined;
  const enchantments = group === -1 ? [] : (groups[group as number] ?? []);
  return { id, stack, maxDamage, name: { tr, en }, desc, aliases, enchantments, operator };
}

/**
 * Reads an item data document (the file's JSON); undefined when it is not format 1 or any row is
 * malformed. Extras keys it does not know are read past (format 1 grows only there).
 */
export function parseItemData(doc: unknown): ItemData | undefined {
  if (!isRecord(doc) || doc.format !== ITEM_DATA_FORMAT) return undefined;
  const { mc, items: itemRows, ench: enchRows, groups: groupRows } = doc;
  if (!isName(mc) || mc.length > 32) return undefined;
  if (!Array.isArray(itemRows) || !Array.isArray(enchRows) || !Array.isArray(groupRows))
    return undefined;
  const enchantments: EnchantmentInfo[] = [];
  const enchantmentById = new Map<string, EnchantmentInfo>();
  for (const [i, row] of enchRows.entries()) {
    const e = parseEnchantment(row, enchRows.length, i);
    if (!e || enchantmentById.has(e.id)) return undefined;
    enchantments.push(e);
    enchantmentById.set(e.id, e);
  }
  const groups: number[][] = [];
  for (const g of groupRows) {
    if (!ascending(g, enchantments.length) || g.length === 0) return undefined;
    groups.push(g);
  }
  const items: ItemInfo[] = [];
  const byId = new Map<string, ItemInfo>();
  for (const row of itemRows) {
    const item = parseItem(row, groups);
    if (!item || byId.has(item.id)) return undefined;
    items.push(item);
    byId.set(item.id, item);
  }
  if (items.length === 0) return undefined;
  return { minecraft: mc, items, enchantments, byId, enchantmentById };
}

/** Reads the file's text (the launcher hands it over after checking its SHA-256). */
export function itemDataFromText(text: string): ItemData | undefined {
  try {
    return parseItemData(JSON.parse(text));
  } catch {
    return undefined;
  }
}

/** `26.1.2` → `[26, 1, 2]`; each part's leading digits (none: 0). */
function versionParts(version: string): number[] {
  return version.split(".").map((part) => Number.parseInt(part, 10) || 0);
}

function compareVersions(a: string, b: string): number {
  const [pa, pb] = [versionParts(a), versionParts(b)];
  for (let i = 0; i < Math.max(pa.length, pb.length); i++) {
    const d = (pa[i] ?? 0) - (pb[i] ?? 0);
    if (d !== 0) return d;
  }
  return 0;
}

/**
 * The version whose file to read for a game version: its own; else the newest older one (the
 * items it has); else, for a version older than every file, the oldest (the nearest newer); with
 * no version asked, the default (else the newest). The same rule as `DataManifest::pick`.
 */
export function pickItemDataVersion(
  manifest: ItemDataManifest,
  minecraft?: string,
): string | undefined {
  const versions = Object.keys(manifest.versions).sort(compareVersions);
  if (minecraft) {
    if (manifest.versions[minecraft]) return minecraft;
    const older = versions.filter((v) => compareVersions(v, minecraft) <= 0);
    return older[older.length - 1] ?? versions[0];
  }
  if (manifest.versions[manifest.default]) return manifest.default;
  return versions[versions.length - 1];
}

type GetBytes = (url: string) => Promise<Uint8Array | undefined>;
type Sha256 = (bytes: Uint8Array) => Promise<string>;

const getBytes: GetBytes = async (url) => {
  const response = await fetch(url);
  return response.ok ? new Uint8Array(await response.arrayBuffer()) : undefined;
};

const sha256Hex: Sha256 = async (bytes) => {
  const digest = await crypto.subtle.digest("SHA-256", bytes as Uint8Array<ArrayBuffer>);
  return Array.from(new Uint8Array(digest), (b) => b.toString(16).padStart(2, "0")).join("");
};

export interface LoadItemDataOptions {
  base?: string;
  get?: GetBytes;
  sha256?: Sha256;
}

const loading = new Map<string, Promise<ItemData | undefined>>();

/**
 * One game version's item data from the CDN (`pickItemDataVersion`), its bytes checked against
 * the manifest's SHA-256; undefined when the CDN cannot be reached, a check fails or the format is
 * newer (the picker then says the list could not be loaded and offers to try again). Each file is
 * asked once; a miss is asked again next time.
 */
export function loadItemData(
  minecraft?: string,
  options: LoadItemDataOptions = {},
): Promise<ItemData | undefined> {
  const { base = ITEM_DATA_BASE, get = getBytes, sha256 = sha256Hex } = options;
  const key = `${base}|${minecraft ?? ""}`;
  const hit = loading.get(key);
  if (hit) return hit;
  const load = (async () => {
    try {
      const manifestUrl = new URL("manifest.json", base);
      const manifestBytes = await get(manifestUrl.href);
      if (!manifestBytes) return undefined;
      const manifest = JSON.parse(new TextDecoder().decode(manifestBytes)) as unknown;
      if (
        !isRecord(manifest) ||
        manifest.format !== ITEM_DATA_MANIFEST_FORMAT ||
        !isRecord(manifest.versions)
      ) {
        return undefined;
      }
      const typed = manifest as unknown as ItemDataManifest;
      const version = pickItemDataVersion(typed, minecraft);
      const entry = version ? typed.versions[version] : undefined;
      // Only a file next to the manifest, named by its own hash (`<version>/items.<12 hex>.json`).
      const name = typeof entry?.file === "string" ? FILE.exec(entry.file) : null;
      if (
        !entry ||
        !name ||
        typeof entry.sha256 !== "string" ||
        !entry.sha256.startsWith(name[1] ?? "-")
      )
        return undefined;
      const bytes = await get(new URL(entry.file, manifestUrl).href);
      if (!bytes || (await sha256(bytes)) !== entry.sha256) return undefined;
      return parseItemData(JSON.parse(new TextDecoder().decode(bytes)));
    } catch {
      return undefined;
    }
  })();
  loading.set(key, load);
  void load.then((data) => {
    if (!data) loading.delete(key);
  });
  return load;
}

export function itemName(item: ItemInfo, lang: ItemLang): string {
  return item.name[lang];
}

export function enchantmentName(enchantment: EnchantmentInfo, lang: ItemLang): string {
  return enchantment.name[lang];
}

/** The enchantments that fit an item, in the file's order (by id). */
export function enchantmentsFor(data: ItemData, item: ItemInfo): EnchantmentInfo[] {
  return item.enchantments.flatMap((i) => data.enchantments[i] ?? []);
}

/** Whether two enchantments can be on one item (neither excludes the other). */
export function canCombine(data: ItemData, a: EnchantmentInfo, b: EnchantmentInfo): boolean {
  const ia = data.enchantments.indexOf(a);
  const ib = data.enchantments.indexOf(b);
  return ia !== ib && !a.excludes.includes(ib) && !b.excludes.includes(ia);
}

const ROMAN = ["I", "II", "III", "IV", "V", "VI", "VII", "VIII", "IX", "X"];

/** An enchantment level as the game writes it ("Keskinlik V"): Roman to X, digits above. */
export function levelText(level: number): string {
  return ROMAN[level - 1] ?? String(level);
}

/** Something else a picker searches with the items: one of the account's custom items. */
export interface ItemSearchExtra {
  /** `custom:<k>`. */
  readonly ref: string;
  readonly names: readonly string[];
}

export interface ItemSearchHit {
  readonly ref: string;
  readonly item?: ItemInfo;
  readonly extra?: ItemSearchExtra;
}

interface Field {
  /** Folded, single spaces. */
  whole: string;
  /** Its words. */
  words: string[];
}

interface Folded {
  /** The interface language's name and description line. */
  own: Field[];
  /** The other language's. */
  other: Field[];
  aliases: Field[];
  /** All of them. */
  all: Field[];
}

const field = (text: string): Field => {
  const whole = fold(text)
    .replace(/[’'`´ʼ]/g, "")
    .split(/[^\p{L}\p{N}]+/u)
    .filter(Boolean)
    .join(" ");
  return { whole, words: whole ? whole.split(" ") : [] };
};

const namesOf = (names: ItemNames | undefined, lang: ItemLang): Field[] =>
  names ? [field(names[lang])] : [];

const foldedOf = (own: Field[], other: Field[], aliases: Field[]): Folded => ({
  own,
  other,
  aliases,
  all: [...own, ...other, ...aliases],
});

const foldedItems = new WeakMap<ItemData, Record<ItemLang, Folded[]>>();

function foldedFor(data: ItemData, lang: ItemLang): Folded[] {
  let byLang = foldedItems.get(data);
  if (!byLang) {
    const make = (l: ItemLang): Folded[] => {
      const other: ItemLang = l === "tr" ? "en" : "tr";
      return data.items.map((item) =>
        foldedOf(
          [...namesOf(item.name, l), ...namesOf(item.desc, l)],
          [...namesOf(item.name, other), ...namesOf(item.desc, other)],
          item.aliases.map(field),
        ),
      );
    };
    byLang = { tr: make("tr"), en: make("en") };
    foldedItems.set(data, byLang);
  }
  return byLang[lang];
}

/** Three rows of the distance table, reused (a search compares thousands of short words). */
let rows = [new Int32Array(32), new Int32Array(32), new Int32Array(32)];

/** Optimal string alignment distance, or `max + 1` once it is certainly above `max`. */
function distance(a: string, b: string, max: number): number {
  const n = b.length;
  if (Math.abs(a.length - n) > max) return max + 1;
  if (n + 1 > (rows[0]?.length ?? 0)) rows = rows.map(() => new Int32Array(n + 1));
  let [before, prev, row] = rows as [Int32Array, Int32Array, Int32Array];
  for (let j = 0; j <= n; j++) prev[j] = j;
  for (let i = 1; i <= a.length; i++) {
    const ca = a.charCodeAt(i - 1);
    row[0] = i;
    let best = i;
    for (let j = 1; j <= n; j++) {
      const cb = b.charCodeAt(j - 1);
      let d = Math.min(
        (prev[j] ?? 0) + 1,
        (row[j - 1] ?? 0) + 1,
        (prev[j - 1] ?? 0) + (ca === cb ? 0 : 1),
      );
      if (i > 1 && j > 1 && ca === b.charCodeAt(j - 2) && a.charCodeAt(i - 2) === cb) {
        d = Math.min(d, (before[j - 2] ?? 0) + 1);
      }
      row[j] = d;
      if (d < best) best = d;
    }
    if (best > max) return max + 1;
    [before, prev, row] = [prev, row, before];
  }
  return prev[n] ?? max + 1;
}

/** Each token starts a word of the field (a word once; "elmas kil" in "elmas kilic"). */
function wordsStart(f: Field, tokens: readonly string[]): boolean {
  const [only] = tokens;
  if (tokens.length === 1 && only !== undefined) return f.words.some((w) => w.startsWith(only));
  const used = new Set<number>();
  return tokens.every((token) => {
    const at = f.words.findIndex((w, i) => !used.has(i) && w.startsWith(token));
    if (at < 0) return false;
    used.add(at);
    return true;
  });
}

/** Each token (4 letters or more) is a typo away from a word or the word's beginning. */
function wordsNear(f: Field, tokens: readonly string[]): boolean {
  return tokens.every((token) => {
    if (token.length < 4) return f.words.some((w) => w.startsWith(token));
    const max = token.length >= 7 ? 2 : 1;
    return f.words.some(
      (w) =>
        distance(token, w, max) <= max ||
        (w.length > token.length && distance(token, w.slice(0, token.length), max) <= max),
    );
  });
}

/** The rank of a typo match: below every other. */
const TYPO = 9;

/**
 * How well a thing's fields match the query, best first (undefined: no match): its name in the
 * interface language exactly, an alias exactly, that name's beginning, the beginnings of its
 * words (and its description line's), the other language's name the same three ways, the
 * beginnings of an alias's words, the query inside any of them (3 letters or more). A typo in
 * each word (1 letter; 2 from 7 letters) ranks `TYPO`, checked apart (`wordsNear`) and only
 * while there are fewer hits than asked.
 */
function rank(f: Folded, query: string, tokens: readonly string[]): number | undefined {
  const [name] = f.own;
  if (name && name.whole === query) return 0;
  if (f.aliases.some((a) => a.whole === query)) return 1;
  if (name?.whole.startsWith(query)) return 2;
  if (f.own.some((x) => wordsStart(x, tokens))) return 3;
  const [otherName] = f.other;
  if (otherName && otherName.whole === query) return 4;
  if (otherName?.whole.startsWith(query)) return 5;
  if (f.other.some((x) => wordsStart(x, tokens))) return 6;
  if (f.aliases.some((a) => a.whole.startsWith(query) || wordsStart(a, tokens))) return 7;
  if (query.length >= 3 && f.all.some((x) => x.whole.includes(query))) return 8;
  return undefined;
}

interface Ranked<T> {
  hit: T;
  rank: number;
  order: number;
}

/** Ranks everything, then (when short of `limit`) the typo matches; best first. */
function ranked<T>(things: readonly { folded: Folded; hit: T }[], q: Field, limit: number): T[] {
  const hits: Ranked<T>[] = [];
  const misses: number[] = [];
  for (const [order, thing] of things.entries()) {
    const r = rank(thing.folded, q.whole, q.words);
    if (r === undefined) misses.push(order);
    else hits.push({ hit: thing.hit, rank: r, order });
  }
  if (hits.length < limit) {
    for (const order of misses) {
      const thing = things[order];
      if (thing?.folded.all.some((x) => wordsNear(x, q.words))) {
        hits.push({ hit: thing.hit, rank: TYPO, order });
      }
    }
  }
  hits.sort((a, b) => a.rank - b.rank || a.order - b.order);
  return hits.slice(0, limit).map((h) => h.hit);
}

export interface SearchItemsOptions {
  lang: ItemLang;
  /** The most hits (default 20). */
  limit?: number;
  /** The account's custom items: searched by their names, first among equals. */
  extras?: readonly ItemSearchExtra[];
  /** Also operator items (command blocks, barrier); left out by default. */
  operatorItems?: boolean;
}

/**
 * The items (and custom items) a picker's query finds, best first; within a rank custom items
 * first, then the game's order. Folded like every search in the app: "elmas kilic" finds
 * "Elmas Kılıç", "dia kılıç" finds it by our alias, "nöbetçi" a smithing template by its line,
 * "elitar" the Elytra despite the typo. An empty query finds nothing.
 */
export function searchItems(
  data: ItemData | undefined,
  query: string,
  options: SearchItemsOptions,
): ItemSearchHit[] {
  const q = field(query);
  if (!q.whole) return [];
  const things: { folded: Folded; hit: ItemSearchHit }[] = (options.extras ?? []).map((extra) => ({
    folded: foldedOf(extra.names.map(field), [], []),
    hit: { ref: extra.ref, extra },
  }));
  if (data) {
    const folded = foldedFor(data, options.lang);
    for (const [i, item] of data.items.entries()) {
      const f = folded[i];
      if (f && (options.operatorItems || !item.operator)) {
        things.push({ folded: f, hit: { ref: item.id, item } });
      }
    }
  }
  return ranked(things, q, options.limit ?? 20);
}

const repeated = new WeakMap<ItemData, Record<ItemLang, Set<string>>>();

/**
 * The other language's name, when another item shows the same name and description line in
 * `lang` (Turkish "Fener" is both Beacon and Lantern): a picker writes it beside the name so the
 * two rows differ. Undefined for a name only one item has.
 */
export function nameHint(data: ItemData, item: ItemInfo, lang: ItemLang): string | undefined {
  let byLang = repeated.get(data);
  if (!byLang) {
    const find = (l: ItemLang) => {
      const seen = new Map<string, number>();
      const key = (i: ItemInfo) => `${fold(i.name[l])}\n${i.desc ? fold(i.desc[l]) : ""}`;
      for (const i of data.items) seen.set(key(i), (seen.get(key(i)) ?? 0) + 1);
      return new Set(data.items.filter((i) => (seen.get(key(i)) ?? 0) > 1).map((i) => i.id));
    };
    byLang = { tr: find("tr"), en: find("en") };
    repeated.set(data, byLang);
  }
  if (!byLang[lang].has(item.id)) return undefined;
  return item.name[lang === "tr" ? "en" : "tr"];
}

export interface SearchEnchantmentsOptions {
  lang: ItemLang;
  /** Only the enchantments that fit this item. */
  item?: ItemInfo;
  limit?: number;
}

/** The enchantments a query finds (by name in either language or alias), best first. */
export function searchEnchantments(
  data: ItemData,
  query: string,
  options: SearchEnchantmentsOptions,
): EnchantmentInfo[] {
  const q = field(query);
  const other: ItemLang = options.lang === "tr" ? "en" : "tr";
  const pool = options.item ? enchantmentsFor(data, options.item) : [...data.enchantments];
  if (!q.whole) return pool.slice(0, options.limit ?? pool.length);
  const things = pool.map((e) => ({
    folded: foldedOf([field(e.name[options.lang])], [field(e.name[other])], e.aliases.map(field)),
    hit: e,
  }));
  return ranked(things, q, options.limit ?? 20);
}
