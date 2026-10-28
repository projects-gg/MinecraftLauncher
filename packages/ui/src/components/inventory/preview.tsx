// The inventory with a stand-in machine, for a look without an account: every layout, a server's
// menu, a Projects menu with its own picture, an item on the cursor, a trade lock. Apps can mount
// it on a development page; the screenshots and the second look use it.
import { useEffect, useMemo, useState } from "react";
import {
  SAMPLE_BOOK,
  SAMPLE_CHEST,
  SAMPLE_DISPENSER,
  SAMPLE_DOUBLE_CHEST,
  SAMPLE_HEADS,
  SAMPLE_HOPPER,
  SAMPLE_INVENTORY,
  SAMPLE_KITS,
  SAMPLE_MARKET,
  SAMPLE_SHULKER,
  SAMPLE_UNKNOWN,
  withCursor,
} from "./fixtures";
import { InventoryView } from "./inventory-view";
import { createMockMachine, type MockWindowKind } from "./mock-machine";
import { textAdvance, textPath } from "./pixel-font";
import type { InventoryLang } from "./texts";
import type { InventoryMode, InvFrame, TitleGlyph, WinFrame } from "./types";

interface PreviewSetup {
  inventory: InvFrame;
  window: WinFrame | null;
  kind?: MockWindowKind;
  locked?: boolean;
}

export const PREVIEW_STATES = {
  player: { inventory: SAMPLE_INVENTORY, window: null },
  chest: { inventory: SAMPLE_INVENTORY, window: SAMPLE_CHEST },
  kits: { inventory: SAMPLE_INVENTORY, window: SAMPLE_KITS, kind: "menu" },
  doubleChest: { inventory: SAMPLE_INVENTORY, window: SAMPLE_DOUBLE_CHEST },
  hopper: { inventory: SAMPLE_INVENTORY, window: SAMPLE_HOPPER },
  shulker: { inventory: SAMPLE_INVENTORY, window: SAMPLE_SHULKER },
  dispenser: { inventory: SAMPLE_INVENTORY, window: SAMPLE_DISPENSER },
  unknown: { inventory: SAMPLE_INVENTORY, window: SAMPLE_UNKNOWN },
  market: { inventory: SAMPLE_INVENTORY, window: SAMPLE_MARKET, kind: "menu" },
  heads: { inventory: SAMPLE_INVENTORY, window: SAMPLE_HEADS, kind: "menu" },
  book: { inventory: SAMPLE_INVENTORY, window: SAMPLE_BOOK, kind: "menu" },
  cursor: { inventory: withCursor(SAMPLE_INVENTORY, 13), window: null },
  locked: { inventory: SAMPLE_INVENTORY, window: SAMPLE_CHEST, locked: true },
} satisfies Record<string, PreviewSetup>;

export type PreviewState = keyof typeof PREVIEW_STATES;

// A stand-in for a Projects menu picture, in the server's own manner: parchment in a brown frame,
// the menu's name on a plate, sunken squares where the shop's buttons are. It covers the window's
// top as a server's art does, with the window's round top corners; its straight bottom edge meets
// the window's grey. Vector, so it stays sharp at every scale.
const MARKET_ART = (() => {
  const rect = (fill: string, x: number, y: number, w: number, h: number) =>
    `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${fill}"/>`;
  const slots = [10, 12, 14, 16]
    .map((i) => {
      const x = 7 + (i % 9) * 18;
      const y = 17 + Math.floor(i / 9) * 18;
      // Sunken as the game's slots are: dark on the top and left, light on the bottom and right.
      return (
        rect("#9c6b3c", x, y, 18, 18) +
        rect("#fbe9c8", x + 1, y + 1, 17, 17) +
        rect("#d8b47f", x + 1, y + 1, 16, 16)
      );
    })
    .join("");
  const name = "MARKET";
  const ink = textAdvance(name) - 1;
  const plateWidth = ink + 12;
  const plateX = Math.floor((176 - plateWidth) / 2);
  const plate =
    rect("#3b200c", plateX, 3, plateWidth, 13) +
    rect("#7a4a24", plateX + 1, 4, plateWidth - 2, 11) +
    `<path fill="#f6e3bf" transform="translate(${plateX + 6} 6)" d="${textPath(name)}"/>`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 176 72" shape-rendering="crispEdges">${
    rect("#000", 2, 0, 172, 1) +
    rect("#000", 1, 1, 1, 1) +
    rect("#000", 174, 1, 1, 1) +
    rect("#000", 0, 2, 1, 70) +
    rect("#000", 175, 2, 1, 70) +
    rect("#7a4a24", 2, 1, 172, 1) +
    rect("#7a4a24", 1, 2, 174, 70) +
    rect("#b07a45", 2, 2, 172, 68) +
    rect("#efd3a5", 3, 3, 170, 66)
  }${slots}${plate}</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
})();

// The market title's glyphs as IA-ART would give them: a move back to the window's edge, then the
// picture (its top on the window's top: 6 + 7 - 13), wide as the window.
const MARKET_GLYPHS: ReadonlyMap<number, TitleGlyph> = new Map([
  [0xf808, { advance: -8 }],
  [0xe001, { advance: 177, image: { src: MARKET_ART, width: 176, height: 72, ascent: 13 } }],
]);
// A book menu's art in the server's manner, wider than the window (INV-HEADS): an open book whose
// cover reaches 40 GUI pixels past each side, the menu's name on a plate, sunken squares where the
// buttons are (slots 10, 12, 14, 16, so the art's x is the window's x + 40).
const BOOK_ART = (() => {
  const rect = (fill: string, x: number, y: number, w: number, h: number) =>
    `<rect x="${x}" y="${y}" width="${w}" height="${h}" fill="${fill}"/>`;
  const slots = [10, 12, 14, 16]
    .map((i) => {
      const x = 40 + 7 + (i % 9) * 18;
      const y = 17 + Math.floor(i / 9) * 18;
      return (
        rect("#5a3418", x, y, 18, 18) +
        rect("#fff4dc", x + 1, y + 1, 17, 17) +
        rect("#e2c79a", x + 1, y + 1, 16, 16)
      );
    })
    .join("");
  const name = "KİTAP";
  const ink = textAdvance(name) - 1;
  const plateWidth = ink + 12;
  const plateX = Math.floor((256 - plateWidth) / 2);
  const plate =
    rect("#2a1407", plateX, 2, plateWidth, 13) +
    rect("#6b3a1a", plateX + 1, 3, plateWidth - 2, 11) +
    `<path fill="#f6e3bf" transform="translate(${plateX + 6} 5)" d="${textPath(name)}"/>`;
  const svg = `<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 256 72" shape-rendering="crispEdges">${
    rect("#2a1407", 0, 0, 256, 72) +
    rect("#6b3a1a", 1, 1, 254, 70) +
    rect("#8a4f26", 3, 3, 250, 66) +
    rect("#e9d3a8", 8, 6, 118, 62) +
    rect("#f3e3c0", 9, 7, 116, 60) +
    rect("#e9d3a8", 130, 6, 118, 62) +
    rect("#f3e3c0", 131, 7, 116, 60) +
    rect("#4a2812", 126, 4, 4, 66)
  }${slots}${plate}</svg>`;
  return `data:image/svg+xml,${encodeURIComponent(svg)}`;
})();

// The book title's glyphs: a move back 48 from the title's x (8), so the art starts 40 left of
// the window, then the picture, its top on the window's top.
const BOOK_GLYPHS: ReadonlyMap<number, TitleGlyph> = new Map([
  [0xf80a, { advance: -48 }],
  [0xe002, { advance: 257, image: { src: BOOK_ART, width: 256, height: 72, ascent: 13 } }],
]);
const marketGlyph = (codePoint: number) =>
  MARKET_GLYPHS.get(codePoint) ?? BOOK_GLYPHS.get(codePoint);

export interface InventoryPreviewProps {
  state?: PreviewState;
  lang?: InventoryLang;
  mode?: InventoryMode;
  /** The stand-in machine's answer time (milliseconds). */
  latency?: number;
  maxScale?: number;
  align?: "center" | "start";
  fit?: "width" | "contain";
  className?: string;
}

export function InventoryPreview({
  state = "player",
  lang = "tr",
  mode = "full",
  latency = 180,
  maxScale,
  align,
  fit,
  className,
}: InventoryPreviewProps) {
  const setup: PreviewSetup = PREVIEW_STATES[state];
  const machine = useMemo(
    () =>
      createMockMachine({
        inventory: setup.inventory,
        window: setup.window,
        windowKind: setup.kind,
        trade: setup.locked,
        latency,
        mode: mode === "view" ? "view" : "full",
      }),
    [setup, latency, mode],
  );
  const [frames, setFrames] = useState(() => ({
    inventory: machine.inventory(),
    window: machine.window(),
  }));
  useEffect(() => {
    setFrames({ inventory: machine.inventory(), window: machine.window() });
    const off = machine.subscribe((inventory, window) => setFrames({ inventory, window }));
    return () => {
      off();
      machine.dispose();
    };
  }, [machine]);
  return (
    <InventoryView
      inventory={frames.inventory}
      window={frames.window}
      mode={mode}
      locked={setup.locked}
      onOp={(op) => machine.op(op)}
      lang={lang}
      titleGlyph={marketGlyph}
      maxScale={maxScale}
      align={align}
      fit={fit}
      className={className}
    />
  );
}
