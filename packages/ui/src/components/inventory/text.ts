// Text that reaches the view from anyone (item names, lore, window titles) and the game-styled
// lines built from it. Plain text only: never markup, never styled as our own interface; a
// server's colours and flags only as the game draws them (the frame's looks, INV-HEADS).
import { fill, type InventoryLang, type InventoryTexts } from "./texts";
import type { InvCursor, InvItem, TextSpan, TitleRun } from "./types";

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

/** A part of a line in one look: the game's colour (made readable) and its flags. */
export interface TextRun {
  text: string;
  color: string;
  bold?: boolean;
  italic?: boolean;
  underlined?: boolean;
  strikethrough?: boolean;
  obfuscated?: boolean;
}

export interface TooltipLine {
  text: string;
  color: string;
  /** The game's italic, which only a container's "and N more..." takes. */
  italic?: boolean;
  /** The line in the server's looks (the frame's `nameStyle`, `loreStyle`); `text` is all of it. */
  runs?: TextRun[];
}

/** The game's 16 text colours, by their formatting code (`§0`–`§f`). */
const LEGACY_COLORS = [
  "#000000",
  "#0000aa",
  "#00aa00",
  "#00aaaa",
  "#aa0000",
  "#aa00aa",
  "#ffaa00",
  "#aaaaaa",
  "#555555",
  "#5555ff",
  "#55ff55",
  "#55ffff",
  "#ff5555",
  "#ff55ff",
  "#ffff55",
  "#ffffff",
] as const;

/** The tooltip's darkest look: 94 % over a white page (the tooltip colour test's worst case). */
const TOOLTIP_WORST = [0x1e, 0x0f, 0x1e] as const;

function luminance(rgb: readonly number[]): number {
  const [r = 0, g = 0, b = 0] = rgb.map((v) => {
    const c = v / 255;
    return c <= 0.03928 ? c / 12.92 : ((c + 0.055) / 1.055) ** 2.4;
  });
  return 0.2126 * r + 0.7152 * g + 0.0722 * b;
}

const BACKGROUND = luminance(TOOLTIP_WORST);
const readsAt = (rgb: readonly number[]) => (luminance(rgb) + 0.05) / (BACKGROUND + 0.05) >= 4.5;
const readable = new Map<string, string>();

/**
 * A server's text colour as the tooltip draws it: as it is when it reads at 4.5:1 on the tooltip,
 * else the least step towards white that does (its own hue, a little paler). The game's dark
 * purple is lore's own (`TEXT_COLOR.lore`).
 */
export function readableColor(hex: string): string {
  const key = hex.toLowerCase();
  if (key === LEGACY_COLORS[5]) return TEXT_COLOR.lore;
  const hit = readable.get(key);
  if (hit) return hit;
  const value = Number.parseInt(key.slice(1), 16);
  const rgb = [(value >> 16) & 255, (value >> 8) & 255, value & 255];
  const toward = (t: number) => rgb.map((c) => Math.round(c + (255 - c) * t));
  let step = 0;
  while (step < 255 && !readsAt(toward(step / 255))) step++;
  const out = `#${toward(step / 255)
    .map((c) => c.toString(16).padStart(2, "0"))
    .join("")}`;
  if (readable.size > 512) readable.clear();
  readable.set(key, out);
  return out;
}

/** A part's code (the frame's spans, a console line's `st`): colour, then the flags in order. */
export const STYLE_CODE = /^(?:[0-9a-f]|#[0-9a-f]{6})?k?l?m?n?o?$/;

/**
 * A part code's look: its colour made readable by `colorOf` (the tooltip's rule unless a caller
 * draws on another ground; none when the code has none) and its flags.
 */
export function codeLook(
  code: string,
  colorOf: (hex: string) => string = readableColor,
): Omit<TextRun, "text" | "color"> & { color?: string } {
  let color: string | undefined;
  let flags = code;
  if (code.startsWith("#")) {
    color = code.slice(0, 7);
    flags = code.slice(7);
  } else if (/^[0-9a-f]/.test(code)) {
    color = LEGACY_COLORS[Number.parseInt(code.slice(0, 1), 16)];
    flags = code.slice(1);
  }
  const look: Omit<TextRun, "text" | "color"> & { color?: string } = {};
  if (color) look.color = colorOf(color);
  if (flags.includes("k")) look.obfuscated = true;
  if (flags.includes("l")) look.bold = true;
  if (flags.includes("m")) look.strikethrough = true;
  if (flags.includes("n")) look.underlined = true;
  if (flags.includes("o")) look.italic = true;
  return look;
}

function runLook(code: string, base: string): Omit<TextRun, "text"> {
  const look = codeLook(code);
  return { ...look, color: look.color ?? base };
}

const sameLook = (a: Omit<TextRun, "text">, b: Omit<TextRun, "text">) =>
  a.color === b.color &&
  Boolean(a.bold) === Boolean(b.bold) &&
  Boolean(a.italic) === Boolean(b.italic) &&
  Boolean(a.underlined) === Boolean(b.underlined) &&
  Boolean(a.strikethrough) === Boolean(b.strikethrough) &&
  Boolean(a.obfuscated) === Boolean(b.obfuscated);

/**
 * A text's parts in their looks (a frame's spans over it), cleaned as `cleanText` cleans the text:
 * the same characters left out, a run of space one space in the look it starts in, none at the
 * ends. `base` is the colour of a part without one (the field's own). Null when the spans are not
 * the contract's or do not cover the text exactly: the text then shows as it is.
 */
export function styledRuns(
  text: string,
  spans: readonly TextSpan[] | null | undefined,
  base: string,
): TextRun[] | null {
  if (!Array.isArray(spans) || spans.length === 0) return null;
  const chars: { char: string; look: Omit<TextRun, "text"> }[] = [];
  let at = 0;
  for (const span of spans) {
    if (!Array.isArray(span) || span.length !== 2) return null;
    const [length, code] = span;
    if (!Number.isInteger(length) || length <= 0 || typeof code !== "string") return null;
    if (!STYLE_CODE.test(code)) return null;
    const part = text.slice(at, at + length);
    // Never between the halves of a character.
    const first = part.charCodeAt(0);
    const last = part.charCodeAt(part.length - 1);
    if (
      part.length !== length ||
      (first >= 0xdc00 && first <= 0xdfff) ||
      (last >= 0xd800 && last <= 0xdbff)
    ) {
      return null;
    }
    const look = runLook(code, base);
    for (const char of plainChars(part)) {
      if (!isPrivateUse(char.codePointAt(0) ?? 0)) chars.push({ char, look });
    }
    at += length;
  }
  if (at !== text.length) return null;
  const runs: TextRun[] = [];
  let space: Omit<TextRun, "text"> | undefined;
  for (const { char, look } of chars) {
    if (/\s/.test(char)) {
      if (runs.length > 0) space ??= look;
      continue;
    }
    const pieces: [string, Omit<TextRun, "text">][] = space
      ? [
          [" ", space],
          [char, look],
        ]
      : [[char, look]];
    for (const [piece, pieceLook] of pieces) {
      const tail = runs[runs.length - 1];
      if (tail && sameLook(tail, pieceLook)) tail.text += piece;
      else runs.push({ ...pieceLook, text: piece });
    }
    space = undefined;
  }
  return runs.map((run) => run.text).join("") === cleanText(text) && runs.length > 0 ? runs : null;
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
  const nameColor = RARITY_COLOR[rarityOf(item)];
  const name: TooltipLine = { text: itemName(item, lang), color: nameColor };
  // The custom name in its look (the name shown is the custom one whenever it has text).
  const nameRuns = item.name ? styledRuns(item.name, item.nameStyle, nameColor) : null;
  if (nameRuns && nameRuns.map((run) => run.text).join("") === name.text) name.runs = nameRuns;
  const lines: TooltipLine[] = [name];
  const inside = item.inside;
  if (inside && inside.top.length > 0) {
    const shown = inside.top.slice(0, 5);
    for (const stack of shown) {
      const text = fill(texts.insideStack, { item: labelOf(stack.label, lang), n: stack.count });
      lines.push({ text, color: TEXT_COLOR.white });
    }
    // The game counts the stacks it did not list. Newer machines say how many stacks there are.
    // Without that, the frame's first 9 tell it when they hold every item; otherwise there are
    // more than the frame tells. Both in the game's style for that line: white italic.
    const listed = inside.top.reduce((sum, stack) => sum + stack.count, 0);
    const stacks = inside.stacks;
    const counted =
      typeof stacks === "number" && Number.isInteger(stacks) && stacks >= inside.top.length
        ? stacks
        : null;
    if (counted === null && listed < inside.count) {
      lines.push({ text: texts.insideMoreUnknown, color: TEXT_COLOR.white, italic: true });
    } else {
      const n = (counted ?? inside.top.length) - shown.length;
      if (n > 0) {
        lines.push({ text: fill(texts.insideMore, { n }), color: TEXT_COLOR.white, italic: true });
      }
    }
  }
  for (const enchant of item.ench ?? []) {
    const text = labelOf(enchant.label, lang);
    if (text) {
      const curse = CURSES.has(bareId(enchant.id));
      lines.push({ text, color: curse ? TEXT_COLOR.red : TEXT_COLOR.gray });
    }
  }
  // Every line, the blank ones too: a server's spacer is a line of its own in the game.
  (item.lore ?? []).forEach((line, i) => {
    const shown: TooltipLine = { text: cleanText(line), color: TEXT_COLOR.lore };
    const runs = styledRuns(line, item.loreStyle?.[i], TEXT_COLOR.lore);
    if (runs) shown.runs = runs;
    lines.push(shown);
  });
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
