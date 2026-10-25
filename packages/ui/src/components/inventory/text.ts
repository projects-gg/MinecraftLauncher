// Text that reaches the view from anyone (item names, lore, window titles) and the game-styled
// lines built from it. Plain text only: never markup, never styled as our own interface.
import { fill, type InventoryLang, type InventoryTexts } from "./texts";
import type { InvCursor, InvItem, TitleRun } from "./types";

/** Controls and direction marks (embeddings, overrides, isolates and the plain marks). */
function isControl(codePoint: number): boolean {
  return (
    codePoint <= 0x1f ||
    (codePoint >= 0x7f && codePoint <= 0x9f) ||
    codePoint === 0x61c ||
    codePoint === 0x200e ||
    codePoint === 0x200f ||
    (codePoint >= 0x202a && codePoint <= 0x202e) ||
    (codePoint >= 0x2066 && codePoint <= 0x2069)
  );
}

/** A resource pack's private-use glyphs (ItemsAdder's menu pictures and spacers, icons in lore). */
export function isPrivateUse(codePoint: number): boolean {
  return (
    (codePoint >= 0xe000 && codePoint <= 0xf8ff) || (codePoint >= 0xf0000 && codePoint <= 0x10ffff)
  );
}

const LEGACY_FORMAT = "0123456789abcdefklmnorx";

/** The characters of a text without controls and the game's legacy formatting codes ("§a"). */
function* plainChars(text: string): Generator<string> {
  let skipNext = false;
  for (const char of text) {
    if (skipNext) {
      skipNext = false;
      if (LEGACY_FORMAT.includes(char.toLowerCase())) continue;
    }
    if (char === "§") {
      skipNext = true;
      continue;
    }
    if (isControl(char.codePointAt(0) ?? 0)) continue;
    yield char;
  }
}

/**
 * Anyone's text as the view shows it: no controls, direction marks, formatting codes or glyphs,
 * runs of space folded. The machine already masks and cuts it; this is the view's own guard.
 */
export function cleanText(text: string): string {
  let out = "";
  for (const char of plainChars(text)) {
    if (!isPrivateUse(char.codePointAt(0) ?? 0)) out += char;
  }
  return out.replace(/\s+/g, " ").trim();
}

/** A title as runs of private-use glyphs and visible text, in order. */
export function titleRuns(title: string): TitleRun[] {
  const runs: TitleRun[] = [];
  let text = "";
  for (const char of plainChars(title)) {
    const codePoint = char.codePointAt(0) ?? 0;
    if (isPrivateUse(codePoint)) {
      if (text) runs.push({ text });
      text = "";
      runs.push({ glyph: codePoint });
    } else {
      text += char;
    }
  }
  if (text) runs.push({ text });
  return runs;
}

/** The private-use code points of a title, in order. */
export function titleGlyphs(title: string): number[] {
  return titleRuns(title).flatMap((run) => ("glyph" in run ? [run.glyph] : []));
}

type Shown = InvItem | NonNullable<InvCursor>;

/** "minecraft:diamond_sword" → "diamond_sword". */
export function bareId(id: string): string {
  const colon = id.indexOf(":");
  return (colon === -1 ? id : id.slice(colon + 1)).toLowerCase();
}

function labelOf(label: { tr: string; en: string }, lang: InventoryLang): string {
  return cleanText(label[lang] || label.tr || label.en || "");
}

/** The name the game shows: the custom name, else the item type's. */
export function itemName(item: Shown, lang: InventoryLang): string {
  const custom = item.name ? cleanText(item.name) : "";
  return custom || labelOf(item.label, lang) || bareId(item.id);
}

export type Rarity = "common" | "uncommon" | "rare" | "epic";

// The game's rarities for the items a player is likely to carry (1.21+); everything else is common.
const EPIC = new Set([
  "enchanted_golden_apple",
  "dragon_egg",
  "dragon_head",
  "elytra",
  "mace",
  "heavy_core",
  "command_block",
  "chain_command_block",
  "repeating_command_block",
  "command_block_minecart",
  "structure_block",
  "jigsaw",
  "debug_stick",
  "knowledge_book",
]);
const RARE = new Set([
  "beacon",
  "conduit",
  "end_crystal",
  "golden_apple",
  "trident",
  "nether_star",
  "ominous_trial_key",
]);
const UNCOMMON = new Set([
  "experience_bottle",
  "dragon_breath",
  "enchanted_book",
  "player_head",
  "zombie_head",
  "creeper_head",
  "skeleton_skull",
  "wither_skeleton_skull",
  "piglin_head",
  "heart_of_the_sea",
  "totem_of_undying",
  "ominous_bottle",
  "trial_key",
  "sniffer_egg",
  "wind_charge",
  "breeze_rod",
]);

function baseRarity(path: string): Rarity {
  if (EPIC.has(path)) return "epic";
  if (RARE.has(path) || path.startsWith("music_disc_")) return "rare";
  if (UNCOMMON.has(path) || path.endsWith("_armor_trim_smithing_template")) return "uncommon";
  return "common";
}

function isEnchanted(item: Shown): boolean {
  return Boolean(item.enchanted) || (item.ench?.length ?? 0) > 0;
}

/** The name's colour class: an enchanted common or uncommon item reads rare, a rare one epic. */
export function rarityOf(item: Shown): Rarity {
  const path = bareId(item.id);
  const rarity = baseRarity(path);
  if (!isEnchanted(item) || path === "enchanted_book") return rarity;
  return rarity === "rare" || rarity === "epic" ? "epic" : "rare";
}

/** The game's text colours. Lore's dark purple is lifted to read at 4.5:1 on the tooltip. */
export const TEXT_COLOR = {
  white: "#ffffff",
  yellow: "#ffff55",
  aqua: "#55ffff",
  lightPurple: "#ff55ff",
  gray: "#aaaaaa",
  red: "#ff5555",
  lore: "#bc55bc",
  title: "#404040",
} as const;

export const RARITY_COLOR: Record<Rarity, string> = {
  common: TEXT_COLOR.white,
  uncommon: TEXT_COLOR.yellow,
  rare: TEXT_COLOR.aqua,
  epic: TEXT_COLOR.lightPurple,
};

/** The game's drop shadow for a text colour: a quarter of it. */
export function shadowOf(hex: string): string {
  const value = Number.parseInt(hex.slice(1), 16);
  const shadow = (value & 0xfcfcfc) >> 2;
  return `#${shadow.toString(16).padStart(6, "0")}`;
}

const CURSES = new Set(["binding_curse", "vanishing_curse"]);

export interface TooltipLine {
  text: string;
  color: string;
}

/**
 * The lines of an item's tooltip, top to bottom, in the game's order (ItemStack's details: a
 * container's contents, then enchantments, then lore; the wear line last) and colours.
 */
export function tooltipLines(
  item: Shown,
  lang: InventoryLang,
  texts: InventoryTexts,
): TooltipLine[] {
  const lines: TooltipLine[] = [
    { text: itemName(item, lang), color: RARITY_COLOR[rarityOf(item)] },
  ];
  const inside = item.inside;
  if (inside && inside.top.length > 0) {
    const shown = inside.top.slice(0, 5);
    for (const stack of shown) {
      const text = fill(texts.insideStack, { item: labelOf(stack.label, lang), n: stack.count });
      lines.push({ text, color: TEXT_COLOR.white });
    }
    // The game counts the stacks it did not list. The frame lists the first 9: when they hold
    // every item, the rest are known; otherwise there are more than the frame tells.
    const listed = inside.top.reduce((sum, stack) => sum + stack.count, 0);
    if (listed < inside.count) {
      lines.push({ text: texts.insideMoreUnknown, color: TEXT_COLOR.gray });
    } else if (inside.top.length > shown.length) {
      const n = inside.top.length - shown.length;
      lines.push({ text: fill(texts.insideMore, { n }), color: TEXT_COLOR.gray });
    }
  }
  for (const enchant of item.ench ?? []) {
    const text = labelOf(enchant.label, lang);
    if (text) {
      const curse = CURSES.has(bareId(enchant.id));
      lines.push({ text, color: curse ? TEXT_COLOR.red : TEXT_COLOR.gray });
    }
  }
  for (const line of item.lore ?? []) {
    const text = cleanText(line);
    if (text) lines.push({ text, color: TEXT_COLOR.lore });
  }
  const durability = item.durability;
  if (durability && durability.left < durability.max) {
    const text = fill(texts.durability, { left: durability.left, max: durability.max });
    lines.push({ text, color: TEXT_COLOR.white });
  }
  if (item.partial) lines.push({ text: texts.partial, color: TEXT_COLOR.gray });
  return lines;
}

/** Whole percent of durability left, or null for items that do not wear. */
export function durabilityPercent(item: Shown): number | null {
  const durability = item.durability;
  if (!durability || durability.max <= 0) return null;
  return Math.max(0, Math.min(100, Math.round((durability.left / durability.max) * 100)));
}

/** What a screen reader says for an item: name, count, wear, enchantment. */
export function itemSpeech(item: Shown, lang: InventoryLang, texts: InventoryTexts): string {
  const parts = [itemName(item, lang)];
  if (item.count > 1) parts.push(fill(texts.count, { n: item.count }));
  const durability = item.durability;
  const percent = durabilityPercent(item);
  if (percent !== null && durability && durability.left < durability.max) {
    parts.push(fill(texts.durabilityPercent, { p: percent }));
  }
  if (isEnchanted(item)) parts.push(texts.enchanted);
  return parts.join(", ");
}

/** The game's bar width (of 13 pixels) for what is left: Math.round(13 - damage * 13 / max). */
export function durabilityWidth(left: number, max: number): number {
  if (max <= 0) return 0;
  return Math.max(0, Math.min(13, Math.round(13 - ((max - left) * 13) / max)));
}

/** The game's bar colour, red to green by what is left: Mth.hsvToRgb(fraction / 3, 1, 1). */
export function durabilityColor(left: number, max: number): string {
  const fraction = max > 0 ? Math.max(0, Math.min(1, left / max)) : 0;
  const scaled = (fraction / 3) * 6;
  const sector = Math.floor(scaled) % 6;
  const f = scaled - Math.floor(scaled);
  const rise = Math.trunc(f * 255);
  const fall = Math.trunc((1 - f) * 255);
  const rgb: Record<number, [number, number, number]> = {
    0: [255, rise, 0],
    1: [fall, 255, 0],
    2: [0, 255, rise],
    3: [0, fall, 255],
    4: [rise, 0, 255],
    5: [255, 0, fall],
  };
  return `#${(rgb[sector] ?? [0, 255, 0]).map((v) => v.toString(16).padStart(2, "0")).join("")}`;
}
