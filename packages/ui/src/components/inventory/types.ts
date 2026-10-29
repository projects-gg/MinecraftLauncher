// The cloud inventory's data as the live socket carries it (projects-studio
// services/cloud-afk/README.md, "Features" → "Inventory"), and the view's own types.

/** A name in both interface languages, as the game's language files write it. */
export interface InvLabel {
  tr: string;
  en: string;
}

export interface InvEnchant {
  id: string;
  lvl: number;
  /** As the game writes it ("Keskinlik V"; no numeral when the enchantment's maximum is 1). */
  label: InvLabel;
}

export interface InvInsideStack {
  id: string;
  count: number;
  label: InvLabel;
}

/**
 * One part of a text's look (the contract's `nameStyle`, `loreStyle`): its length in UTF-16 code
 * units and its code, a colour (`0`–`f`, `#rrggbb`, or none for the field's own) and then the
 * flags that are on among `k l m n o` (obfuscated, bold, strikethrough, underlined, italic).
 */
export type TextSpan = [number, string];

export interface InvItem {
  /** inv: 5–8 armour (head, chest, legs, feet), 9–35 main, 36–44 hotbar, 45 off hand; win: 0 … size-1. */
  slot: number;
  /** "minecraft:diamond_sword". */
  id: string;
  count: number;
  label: InvLabel;
  /** A custom name (anvil) or a server's item name: plain text, anyone's. */
  name?: string;
  /** The name's look in the game: spans over `name` (machine 0.3.2). */
  nameStyle?: TextSpan[];
  /** At most 32 lines of at most 80 characters: plain text, anyone's. */
  lore?: string[];
  /** A look a `lore` line, or null (machine 0.3.2). */
  loreStyle?: (TextSpan[] | null)[];
  /** A player head's skin: its texture's hash on Mojang's texture server (machine 0.3.2). */
  skin?: string;
  durability?: { left: number; max: number };
  ench?: InvEnchant[];
  enchanted?: boolean;
  /**
   * A shulker box or bundle: its items' total, its first 9 stacks and, from newer machines, how
   * many stacks it holds (the game's count for "and N more...").
   */
  inside?: { count: number; top: InvInsideStack[]; stacks?: number };
  /** A potion's type, "minecraft:strong_healing". */
  potion?: string;
  /** A dye, or a potion's own colour: 0xRRGGBB. */
  color?: number;
  /** Something could not be shown. */
  partial?: boolean;
  /** The item's custom_model_data (its first float, floored), when the server set one (IA-ART). */
  cmd?: number;
  /** The `minecraft:item_model` component, when the server set one (IA-ART). */
  model?: string;
}

/** The item on the account's cursor, or nothing. */
export type InvCursor = Omit<InvItem, "slot"> | null;

/** An `inv` frame: the player inventory. */
export interface InvFrame {
  /** The selected hotbar slot, 0–8. */
  held: number;
  cursor: InvCursor;
  /** Every non-empty slot 5–45. */
  items: InvItem[];
}

export interface WinOpen {
  /** The game's menu name: `generic_9x3`, `hopper`, `shulker_box`, …, `unknown`. */
  type: string;
  /** Plain text, at most 80 characters; may carry a resource pack's private-use glyphs. */
  title: string;
  /** The window's own slots. */
  size: number;
}

/** A `win` frame with an open window. */
export interface WinFrame {
  /** The machine's number for this opening of the window; every move names the one it sees. */
  wseq: number;
  open: WinOpen;
  items: InvItem[];
}

export type InvArea = "inv" | "win";

export type InvOp =
  | {
      action: "click";
      area: InvArea;
      slot: number;
      button: 0 | 1;
      mode: "pickup" | "quick_move";
      wseq: number;
    }
  | { action: "drop"; area: InvArea; slot: number; stack: boolean; wseq: number }
  | { action: "use"; slot?: number }
  | { action: "hold"; slot: number }
  | { action: "close"; wseq: number }
  | { action: "snapshot" };

export type InvErrorCode =
  | "invalid"
  | "inventory_off"
  | "not_running"
  | "busy_trade"
  | "stale"
  | "rate_limited"
  | "not_found"
  | "host_offline";

export interface InvError {
  code: InvErrorCode | (string & {});
  reason?: string;
  field?: string;
  /** With `rate_limited`: whole seconds. */
  retryAfter?: number;
}

/** The machine's answer to a move (`invres`). `ok` means the account's client sent it on. */
export type InvResult = { ok: true } | { ok?: false; error: InvError };

/** A slot as the view names it: an area and the area's slot number. */
export interface SlotRef {
  area: InvArea;
  slot: number;
}

/**
 * - `full`: the player moves items (Studio's switch `full`);
 * - `view`: the player looks (switch `view`), with tooltips and the keyboard;
 * - `pick`: the player chooses one of the account's own items, nothing moves (AFK+'s "Envanterden
 *   seç");
 * - `static`: a picture for pages that show the feature (no input, nothing live).
 */
export type InventoryMode = "full" | "view" | "pick" | "static";

/**
 * A Projects menu's title character (IA-ART's `itemArt` glyphs, docs/content-format.md "Item
 * art"): a picture the server draws its menu with (ItemsAdder's font images), or only a move.
 */
export interface TitleGlyph {
  /** How far the title's pen moves on, in GUI pixels (negative moves back). */
  advance: number;
  /** The picture: `width` × `height` GUI pixels, its top `ascent` above the title's baseline (y + 7). */
  image?: { src: string; width: number; height: number; ascent: number };
}

/** A title as runs of private-use glyphs (their code points) and text, in order. */
export type TitleRun = { glyph: number } | { text: string };
