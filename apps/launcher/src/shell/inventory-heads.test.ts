// INV-HEADS: a player head drawn with its own skin, and names and lore in the server's looks
// (packages/ui/src/items/skin.ts, packages/ui/src/components/inventory/text.ts), as the cloud
// inventory's frames carry them (machine 0.3.2).
import { readFileSync } from "node:fs";
import {
  isSkinHash,
  processSkin,
  renderHead,
  SKIN_TEXTURE_ORIGIN,
  type SkinPixels,
  skinTextureUrl,
} from "@pl/ui/items";
import { describe, expect, it } from "vitest";
import { SAMPLE_HEADS } from "../../../../packages/ui/src/components/inventory/fixtures";
import {
  scrambleText,
  textAdvance,
  wrapRuns,
} from "../../../../packages/ui/src/components/inventory/pixel-font";
import * as rules from "../../../../packages/ui/src/components/inventory/rules";
import {
  readableColor,
  styledRuns,
  TEXT_COLOR,
  tooltipLines,
} from "../../../../packages/ui/src/components/inventory/text";
import { INVENTORY_TEXTS } from "../../../../packages/ui/src/components/inventory/texts";
import type { InvItem } from "../../../../packages/ui/src/components/inventory/types";
import { toInvItem } from "../features/afk/cloud/inventory/model";

const HASH = "8a6d5e1f0c3b4a29e7d6c5b4a3928170fe6d5c4b3a2918076f5e4d3c2b1a0998";

/** A 64 x `height` skin: grey, with some areas painted. */
function skin(
  height: 64 | 32,
  paint: { x: number; y: number; w: number; h: number; rgba: number[] }[],
): SkinPixels {
  const data = new Uint8ClampedArray(64 * height * 4);
  for (let i = 0; i < data.length; i += 4) data.set([90, 90, 90, 255], i);
  for (const { x, y, w, h, rgba } of paint) {
    for (let yy = y; yy < y + h; yy++)
      for (let xx = x; xx < x + w; xx++) data.set(rgba, (yy * 64 + xx) * 4);
  }
  return { width: 64, height, data };
}

const at = (p: SkinPixels, x: number, y: number) =>
  Array.from(p.data.slice((y * p.width + x) * 4, (y * p.width + x) * 4 + 4));

describe("skin heads", () => {
  it("takes only a texture hash, on Mojang's texture server", () => {
    expect(isSkinHash(HASH)).toBe(true);
    expect(isSkinHash(HASH.toUpperCase())).toBe(false);
    expect(isSkinHash("8a6d")).toBe(false);
    expect(skinTextureUrl(HASH)).toBe(`https://textures.minecraft.net/texture/${HASH}`);
    expect(skinTextureUrl("../x")).toBeUndefined();
    expect(SKIN_TEXTURE_ORIGIN).toBe("https://textures.minecraft.net");
  });

  it("is allowed by the launcher's picture policy", () => {
    const conf = JSON.parse(
      readFileSync(new URL("../../src-tauri/tauri.conf.json", import.meta.url), "utf8"),
    ) as { app: { security: { csp: Record<string, string> } } };
    expect(conf.app.security.csp["img-src"]?.split(" ")).toContain(SKIN_TEXTURE_ORIGIN);
  });

  it("reads a skin as the game does", () => {
    expect(
      processSkin({ width: 64, height: 48, data: new Uint8ClampedArray(64 * 48 * 4) }),
    ).toBeNull();
    expect(
      processSkin({ width: 32, height: 32, data: new Uint8ClampedArray(32 * 32 * 4) }),
    ).toBeNull();
    // The head's own layer is always opaque; the hat keeps its see-through pixels.
    const modern = processSkin(
      skin(64, [
        { x: 8, y: 8, w: 1, h: 1, rgba: [10, 20, 30, 0] },
        { x: 40, y: 8, w: 8, h: 8, rgba: [0, 0, 0, 0] },
      ]),
    );
    expect(at(modern as SkinPixels, 8, 8)).toEqual([10, 20, 30, 255]);
    expect(at(modern as SkinPixels, 40, 8)[3]).toBe(0);
    // An old 64 x 32 skin whose hat area is all opaque: see-through (Notch's skins).
    const notch = processSkin(skin(32, []));
    expect(notch?.height).toBe(64);
    expect(at(notch as SkinPixels, 40, 8)[3]).toBe(0);
    const hatted = processSkin(skin(32, [{ x: 33, y: 1, w: 1, h: 1, rgba: [0, 0, 0, 0] }]));
    expect(at(hatted as SkinPixels, 40, 8)[3]).toBe(255);
  });

  it("draws the head as the atlas's heads: face right, side left, top lit", () => {
    const painted = processSkin(
      skin(64, [
        { x: 8, y: 0, w: 8, h: 8, rgba: [255, 0, 0, 255] }, // top
        { x: 8, y: 8, w: 8, h: 8, rgba: [0, 255, 0, 255] }, // face
        { x: 0, y: 8, w: 8, h: 8, rgba: [0, 0, 255, 255] }, // its side
        { x: 32, y: 0, w: 32, h: 16, rgba: [0, 0, 0, 0] }, // no hat
      ]),
    ) as SkinPixels;
    const head = renderHead(painted, 32);
    expect(head.width).toBe(32);
    const found = { red: [] as number[][], green: [] as number[][], blue: [] as number[][] };
    for (let y = 0; y < 32; y++) {
      for (let x = 0; x < 32; x++) {
        const [r = 0, g = 0, b = 0, a = 0] = at(head, x, y);
        if (a === 0) continue;
        if (r > 0 && g === 0 && b === 0) found.red.push([x, y, r]);
        if (g > 0 && r === 0 && b === 0) found.green.push([x, y, g]);
        if (b > 0 && r === 0 && g === 0) found.blue.push([x, y, b]);
      }
    }
    const mean = (list: number[][], i: number) =>
      list.reduce((s, p) => s + (p[i] ?? 0), 0) / list.length;
    expect(found.red.length).toBeGreaterThan(20);
    expect(found.green.length).toBeGreaterThan(20);
    expect(found.blue.length).toBeGreaterThan(20);
    expect(mean(found.green, 0)).toBeGreaterThan(16);
    expect(mean(found.blue, 0)).toBeLessThan(16);
    expect(mean(found.red, 1)).toBeLessThan(mean(found.green, 1));
    // The game's slot lights: the top at full light, the sides darker.
    expect(mean(found.red, 2)).toBe(255);
    expect(mean(found.green, 2)).toBeLessThan(255);
    expect(mean(found.blue, 2)).toBeLessThan(255);
    // The same skin draws the same head.
    expect(Array.from(renderHead(painted, 32).data)).toEqual(Array.from(head.data));

    // A hat over the face covers it, a quarter pixel out.
    const hatted = processSkin(
      skin(64, [
        { x: 8, y: 8, w: 8, h: 8, rgba: [0, 255, 0, 255] },
        { x: 32, y: 0, w: 32, h: 16, rgba: [0, 0, 0, 0] },
        { x: 40, y: 8, w: 8, h: 8, rgba: [255, 0, 255, 255] },
      ]),
    ) as SkinPixels;
    const covered = renderHead(hatted, 48);
    let green = 0;
    let magenta = 0;
    for (let i = 0; i < covered.data.length; i += 4) {
      const [r = 0, g = 0, b = 0] = covered.data.slice(i, i + 3);
      if (g > 0 && r === 0 && b === 0) green++;
      if (r > 0 && b > 0 && g === 0) magenta++;
    }
    expect(green).toBe(0);
    expect(magenta).toBeGreaterThan(40);
  });
});

describe("the server's looks", () => {
  const lore = TEXT_COLOR.lore;

  it("splits a text by its spans and keeps cleanText's text", () => {
    expect(
      styledRuns(
        "Hasar +12",
        [
          [6, "7"],
          [3, "c"],
        ],
        lore,
      ),
    ).toEqual([
      { text: "Hasar ", color: "#aaaaaa" },
      { text: "+12", color: "#ff5555" },
    ]);
    // No colour: the field's own; flags in the code's order.
    expect(styledRuns("Kılıç", [[5, "lo"]], "#55ffff")).toEqual([
      { text: "Kılıç", color: "#55ffff", bold: true, italic: true },
    ]);
    // Runs of space fold into one in the look they start in; none at the ends; glyphs go.
    expect(
      styledRuns(
        "  a\u00a0 \ue001b  ",
        [
          [4, "c"],
          [1, "9"],
          [1, "6"],
          [2, "6"],
          [1, "e"],
        ],
        lore,
      ),
    ).toEqual([
      { text: "a ", color: "#ff5555" },
      { text: "b", color: "#ffaa00" },
    ]);
    // Spans that do not cover the text, codes that are not the contract's, a broken character.
    expect(styledRuns("abc", [[2, "c"]], lore)).toBeNull();
    expect(styledRuns("abc", [[3, "z"]], lore)).toBeNull();
    expect(styledRuns("abc", [[3, "ol"]], lore)).toBeNull();
    expect(
      styledRuns(
        "😀a",
        [
          [1, "c"],
          [2, "9"],
        ],
        lore,
      ),
    ).toBeNull();
    expect(styledRuns("abc", undefined, lore)).toBeNull();
  });

  it("lightens only colours that do not read on the tooltip, in their own hue", () => {
    const lum = (hex: string) => {
      const c = [1, 3, 5].map((i) => Number.parseInt(hex.slice(i, i + 2), 16) / 255);
      const [r, g, b] = c.map((v) => (v <= 0.03928 ? v / 12.92 : ((v + 0.055) / 1.055) ** 2.4));
      return 0.2126 * (r ?? 0) + 0.7152 * (g ?? 0) + 0.0722 * (b ?? 0);
    };
    const background = lum("#1e0f1e");
    const palette = "0123456789abcdef"
      .split("")
      .map((code) => styledRuns("x", [[1, code]], lore)?.[0]?.color);
    for (const color of [...palette, readableColor("#101010"), readableColor("#2a0a4a")]) {
      expect(color).toMatch(/^#[0-9a-f]{6}$/);
      expect((lum(color ?? "") + 0.05) / (background + 0.05)).toBeGreaterThanOrEqual(4.5);
    }
    expect(palette[5]).toBe(TEXT_COLOR.lore);
    expect(palette[6]).toBe("#ffaa00");
    expect(palette[15]).toBe("#ffffff");
    expect(readableColor("#E0115F")).toBe(readableColor("#e0115f"));
    // Dark blue stays blue: its blue the largest part.
    const blue = palette[1] ?? "";
    expect(Number.parseInt(blue.slice(5, 7), 16)).toBeGreaterThan(
      Number.parseInt(blue.slice(1, 3), 16),
    );
  });

  it("draws the tooltip's name and lore in their looks, spacer lines too", () => {
    const head = SAMPLE_HEADS.items.find((i) => i.slot === 13) as InvItem;
    const lines = tooltipLines(head, "tr", INVENTORY_TEXTS.tr);
    expect(lines[0]?.text).toBe("Steve Kafası");
    expect(lines[0]?.runs).toEqual([{ text: "Steve Kafası", color: "#55ffff" }]);
    expect(lines.map((l) => l.text)).toEqual([
      "Steve Kafası",
      "Nadir",
      "gizli ödül",
      "",
      "Fiyat: 900 jeton",
    ]);
    expect(lines[2]?.runs).toEqual([
      { text: "gizli", color: TEXT_COLOR.lore, obfuscated: true },
      { text: " ödül", color: "#aaaaaa" },
    ]);
    expect(lines[3]?.runs).toBeUndefined();
    // Without looks the lines stay as before.
    const plain = { ...head, nameStyle: undefined, loreStyle: undefined };
    expect(tooltipLines(plain, "tr", INVENTORY_TEXTS.tr)[1]).toEqual({
      text: "Nadir",
      color: TEXT_COLOR.lore,
    });
    // A head's skin and looks tell stacks apart, as the game's components do.
    expect(rules.sameItem(head, { ...head })).toBe(true);
    expect(rules.sameItem(head, { ...head, skin: HASH })).toBe(false);
    expect(rules.sameItem(head, plain)).toBe(false);
  });

  it("wraps a line of parts at the same spaces, bold letters wider", () => {
    const runs = [
      { text: "Uzun bir ", color: "#ffffff" },
      { text: "satır", color: "#ff5555", bold: true },
    ];
    expect(wrapRuns(runs, 500)).toEqual([runs]);
    const lines = wrapRuns(runs, textAdvance("Uzun bir") + 1);
    expect(lines).toEqual([
      [{ text: "Uzun bir", color: "#ffffff" }],
      [{ text: "satır", color: "#ff5555", bold: true }],
    ]);
    expect(textAdvance("ab", true)).toBe(textAdvance("ab") + 2);
    // Obfuscated: a still frame, each letter one of the same width, spaces kept.
    const scrambled = scrambleText("gizli ödül");
    expect(scrambled).toBe(scrambleText("gizli ödül"));
    expect(scrambled[5]).toBe(" ");
    expect(textAdvance(scrambled)).toBe(textAdvance("gizli ödül"));
  });

  it("maps the machine's looks and skin into the view's item", () => {
    const item = toInvItem({
      slot: 3,
      id: "minecraft:player_head",
      count: 1,
      label: { tr: "Oyuncu Kafası", en: "Player Head" },
      name: "Kılıç",
      nameStyle: [[5, "6lo"]],
      lore: ["Hasar +12", ""],
      loreStyle: [
        [
          [6, "7"],
          [3, "c"],
        ],
        null,
      ],
      skin: HASH,
    });
    expect(item.skin).toBe(HASH);
    expect(item.nameStyle).toEqual([[5, "6lo"]]);
    expect(item.loreStyle).toEqual([
      [
        [6, "7"],
        [3, "c"],
      ],
      null,
    ]);
    const old = toInvItem({
      slot: 3,
      id: "minecraft:paper",
      count: 1,
      lore: ["a"],
      loreStyle: [null],
    });
    expect(old.loreStyle).toBeUndefined();
    expect(old.skin).toBeUndefined();
  });
});
