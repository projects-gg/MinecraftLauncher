// The game screen's own look, drawn as whole-pixel rectangles (our drawing, not the game's
// texture): the light grey window with its two-pixel bevel and rounded black outline, the sunken
// slots, empty-armour outlines, and the frame of the held hotbar slot.
import { memo } from "react";
import type { Screen, SlotKind, SlotSpot } from "./layout";

export const GUI = {
  outline: "#000000",
  face: "#c6c6c6",
  light: "#ffffff",
  shade: "#555555",
  slot: "#8b8b8b",
  slotDark: "#373737",
  slotLight: "#ffffff",
  ghost: "#a2a2a2",
} as const;

type Rect = [x: number, y: number, w: number, h: number];

function rects(fill: string, list: Rect[], key: string) {
  const d = list.map(([x, y, w, h]) => `M${x} ${y}h${w}v${h}h${-w}z`).join("");
  return <path key={key} fill={fill} d={d} />;
}

/** The window: outline with 2-pixel round corners, white bevel top-left, grey bevel bottom-right. */
function frame(w: number, h: number) {
  return [
    rects(
      GUI.outline,
      [
        [2, 0, w - 4, 1],
        [2, h - 1, w - 4, 1],
        [0, 2, 1, h - 4],
        [w - 1, 2, 1, h - 4],
        [1, 1, 1, 1],
        [w - 2, 1, 1, 1],
        [1, h - 2, 1, 1],
        [w - 2, h - 2, 1, 1],
      ],
      "outline",
    ),
    rects(
      GUI.face,
      [
        [2, 1, w - 4, h - 2],
        [1, 2, w - 2, h - 4],
      ],
      "face",
    ),
    rects(
      GUI.light,
      [
        [2, 1, w - 5, 1],
        [1, 2, w - 5, 1],
        [1, 2, 1, h - 5],
        [2, 1, 1, h - 5],
      ],
      "light",
    ),
    rects(
      GUI.shade,
      [
        [w - 2, 3, 1, h - 5],
        [w - 3, 4, 1, h - 6],
        [3, h - 2, w - 5, 1],
        [4, h - 3, w - 6, 1],
      ],
      "shade",
    ),
  ];
}

/** One sunken slot square around an item at (x, y). */
function slotRects(spots: SlotSpot[]) {
  const base: Rect[] = [];
  const dark: Rect[] = [];
  const light: Rect[] = [];
  for (const { x, y } of spots) {
    base.push([x - 1, y - 1, 18, 18]);
    dark.push([x - 1, y - 1, 17, 1], [x - 1, y, 1, 16]);
    light.push([x, y + 16, 17, 1], [x + 16, y, 1, 16]);
  }
  return [
    rects(GUI.slot, base, "slot"),
    rects(GUI.slotDark, dark, "slot-dark"),
    rects(GUI.slotLight, light, "slot-light"),
  ];
}

// Outlines for empty armour and off-hand slots, 16 x 16 ('#' drawn), our own drawings.
const GHOSTS: Partial<Record<SlotKind, string[]>> = {
  head: [
    "................",
    "................",
    "................",
    "................",
    ".....######.....",
    "....########....",
    "...##########...",
    "...###....###...",
    "...##......##...",
    "...##......##...",
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
  ],
  chest: [
    "................",
    "................",
    "...###....###...",
    "...####..####...",
    "...##########...",
    "...##########...",
    "....########....",
    "....########....",
    "....########....",
    "....########....",
    "....########....",
    "....########....",
    "................",
    "................",
    "................",
    "................",
  ],
  legs: [
    "................",
    "................",
    "................",
    "....########....",
    "....########....",
    "....###..###....",
    "....###..###....",
    "....###..###....",
    "....###..###....",
    "....###..###....",
    "....###..###....",
    "....###..###....",
    "................",
    "................",
    "................",
    "................",
  ],
  feet: [
    "................",
    "................",
    "................",
    "................",
    "................",
    "................",
    "....###..###....",
    "....###..###....",
    "....###..###....",
    "...####..####...",
    "..#####..#####..",
    "................",
    "................",
    "................",
    "................",
    "................",
  ],
  offhand: [
    "................",
    "................",
    "....########....",
    "...##########...",
    "...##########...",
    "...##########...",
    "...##########...",
    "...##########...",
    "....########....",
    "....########....",
    ".....######.....",
    "......####......",
    ".......##.......",
    "................",
    "................",
    "................",
  ],
};

function ghostRects(spots: SlotSpot[], filled: Set<string>) {
  const list: Rect[] = [];
  for (const spot of spots) {
    const rows = GHOSTS[spot.kind];
    if (!rows || filled.has(spot.key)) continue;
    rows.forEach((row, dy) => {
      let start = -1;
      for (let dx = 0; dx <= row.length; dx++) {
        const on = row[dx] === "#";
        if (on && start < 0) start = dx;
        if (!on && start >= 0) {
          list.push([spot.x + start, spot.y + dy, dx - start, 1]);
          start = -1;
        }
      }
    });
  }
  return rects(GUI.ghost, list, "ghosts");
}

export const PanelArt = memo(function PanelArt({
  screen,
  filled,
}: {
  screen: Screen;
  /** Slot keys with an item (their empty-armour outline stays out). */
  filled: Set<string>;
}) {
  return (
    <svg
      className="pl-inv-art"
      viewBox={`0 0 ${screen.width} ${screen.height}`}
      shapeRendering="crispEdges"
      aria-hidden
      focusable="false"
    >
      {frame(screen.width, screen.height)}
      {slotRects(screen.slots)}
      {ghostRects(screen.slots, filled)}
    </svg>
  );
});

/**
 * The hotbar slot the account holds, framed as the game's hotbar marks it: a light ring and a dark
 * one around the slot (1 and 2 pixels out), the same game greys as the window. (x, y) is the item's
 * top-left. It glides to a newly held slot (`translate`, so nothing is laid out again).
 */
export function HeldFrame({ x, y }: { x: number; y: number }) {
  return (
    <svg
      className="pl-inv-held"
      viewBox="0 0 22 22"
      shapeRendering="crispEdges"
      aria-hidden
      focusable="false"
      style={{ translate: `calc(${x - 3} * var(--pl-inv-px)) calc(${y - 3} * var(--pl-inv-px))` }}
    >
      <path fill={GUI.outline} d="M0 0h22v1H0zM0 21h22v1H0zM0 1h1v20H0zM21 1h1v20h-1z" />
      <path fill={GUI.light} d="M1 1h20v1H1zM1 20h20v1H1zM1 2h1v18H1zM20 2h1v18h-1z" />
    </svg>
  );
}

/** One sunken slot of the window's, alone: the hand that holds a stack carried under a finger. */
export function SlotFrame() {
  return (
    <svg
      className="pl-inv-art"
      viewBox="0 0 18 18"
      shapeRendering="crispEdges"
      aria-hidden
      focusable="false"
    >
      <path fill={GUI.slot} d="M0 0h18v18H0z" />
      <path fill={GUI.slotDark} d="M0 0h17v1H0zM0 1h1v16H0z" />
      <path fill={GUI.slotLight} d="M1 17h17v1H1zM17 1h1v16h-1z" />
    </svg>
  );
}

/**
 * A move tried where nothing moves (looking only, a trade's lock): a small lock at the item's top
 * left for a moment, the line's own sign, the item left as it is. White with the game's drop
 * shadow (a quarter of it), as the counts are; 7 x 8 and its shadow.
 */
export function LockMark() {
  const lock = "M1 0h5v1H1zM1 1h1v2H1zM5 1h1v2H5zM0 3h7v2H0zM0 5h3v2H0zM4 5h3v2H4zM0 7h7v1H0z";
  return (
    <svg
      className="pl-inv-lock"
      viewBox="0 0 8 9"
      shapeRendering="crispEdges"
      aria-hidden
      focusable="false"
    >
      <path fill="#3f3f3f" transform="translate(1 1)" d={lock} />
      <path fill="#ffffff" d={lock} />
    </svg>
  );
}

/** The focus brackets: gold L-corners with a dark edge, readable on the grey and in the slot. */
export function FocusBrackets() {
  // 22 x 22 around the 18 x 18 slot (2 pixels out): a dark L under a gold L at each corner.
  const arm = 6;
  const dark = (x: number, y: number, sx: number, sy: number) =>
    `M${x} ${y}h${sx * arm}v${sy * 2}h${-sx * (arm - 2)}v${sy * (arm - 2)}h${-sx * 2}z`;
  const gold = (x: number, y: number, sx: number, sy: number) =>
    `M${x} ${y}h${sx * (arm - 1)}v${sy}h${-sx * (arm - 2)}v${sy * (arm - 2)}h${-sx}z`;
  const corners: Array<[number, number, number, number]> = [
    [0, 0, 1, 1],
    [22, 0, -1, 1],
    [0, 22, 1, -1],
    [22, 22, -1, -1],
  ];
  return (
    <svg
      className="pl-inv-focus"
      viewBox="0 0 22 22"
      shapeRendering="crispEdges"
      aria-hidden
      focusable="false"
    >
      <path fill="#1a0f05" d={corners.map(([x, y, sx, sy]) => dark(x, y, sx, sy)).join("")} />
      <path fill="#f6d58e" d={corners.map(([x, y, sx, sy]) => gold(x, y, sx, sy)).join("")} />
    </svg>
  );
}
