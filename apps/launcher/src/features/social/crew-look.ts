// A crew's look (TEAM2 §4.1): 12 colours, 4 frames and 36 emblems, as pure data (crew-look.test.ts).
// The crew colour is the crew's content, like an avatar: it fills the emblem's frame and tints
// the tag chip. It is never an interface colour: buttons, focus rings, links and states stay gold
// and the semantic colours (docs/design.md §2).

/** Every emblem in the picker's order: heraldry, nature, creatures, things (9 × 4). */
export const EMBLEMS = [
  "sword",
  "shield",
  "axe",
  "crown",
  "trophy",
  "flag",
  "target",
  "skull",
  "ghost",
  "flame",
  "bolt",
  "sun",
  "moon",
  "star",
  "planet",
  "snowflake",
  "waves",
  "leaf",
  "tree",
  "cactus",
  "paw",
  "cat",
  "horse",
  "bird",
  "fish",
  "butterfly",
  "heart",
  "diamond",
  "cube",
  "rocket",
  "potion",
  "wand",
  "castle",
  "controller",
  "alien",
  "robot",
] as const;

export type Emblem = (typeof EMBLEMS)[number];

/** The emblems every service takes; the other 16 came with crew looks (`teamFeatures: look`). */
export const FIRST_EMBLEMS: readonly Emblem[] = [
  "sword",
  "shield",
  "crown",
  "flame",
  "bolt",
  "star",
  "skull",
  "diamond",
  "heart",
  "rocket",
  "tree",
  "paw",
  "ghost",
  "cube",
  "axe",
  "trophy",
  "moon",
  "flag",
  "target",
  "planet",
];

/** The picker's emblems: all 36 where the service offers looks, else the first 20 (same order). */
export function emblemsFor(look: boolean): readonly Emblem[] {
  return look ? EMBLEMS : EMBLEMS.filter((emblem) => FIRST_EMBLEMS.includes(emblem));
}

export function isEmblem(value: string): value is Emblem {
  return (EMBLEMS as readonly string[]).includes(value);
}

/** gold-300 and bone-100 (docs/design.md §2): the emblem's colour on its frame. */
const GOLD_GLYPH = "#F6D58E";
const BONE_GLYPH = "#F3EBDD";

/**
 * Frame fill, frame edge and emblem colour (contrast checked on the panel `#16131A`: emblem on
 * fill ≥ 5.6:1, edge on the panel ≥ 3:1). Green is a deep forest green on purpose, apart from
 * jade-400 (presence); there is no flare tone (errors), crimson is darker.
 */
export const CREW_COLORS = {
  gold: { fill: "#6E4A12", edge: "#E89A2C", glyph: GOLD_GLYPH },
  copper: { fill: "#74361A", edge: "#D9824E", glyph: GOLD_GLYPH },
  crimson: { fill: "#6E1C22", edge: "#D9574D", glyph: GOLD_GLYPH },
  rose: { fill: "#6C2141", edge: "#DB6E99", glyph: GOLD_GLYPH },
  purple: { fill: "#452868", edge: "#A57BE0", glyph: GOLD_GLYPH },
  navy: { fill: "#1C2B57", edge: "#6F8EE0", glyph: GOLD_GLYPH },
  blue: { fill: "#17466E", edge: "#5AA6E0", glyph: BONE_GLYPH },
  teal: { fill: "#12514F", edge: "#46BDB4", glyph: BONE_GLYPH },
  green: { fill: "#1C4F31", edge: "#5DB47E", glyph: GOLD_GLYPH },
  silver: { fill: "#3A3D48", edge: "#B3B9C8", glyph: BONE_GLYPH },
  ivory: { fill: "#D6CAB4", edge: "#F3EBDD", glyph: "#2A1D10" },
  obsidian: { fill: "#141118", edge: "#6B6172", glyph: GOLD_GLYPH },
} as const;

export type CrewColor = keyof typeof CREW_COLORS;
export const COLORS = Object.keys(CREW_COLORS) as CrewColor[];

export const FRAMES = ["cut", "shield", "banner", "hex"] as const;
export type CrewFrame = (typeof FRAMES)[number];

/** Today's look, and what a crew from before looks (or an unknown value) shows. */
export const DEFAULT_COLOR: CrewColor = "gold";
export const DEFAULT_FRAME: CrewFrame = "cut";

export function crewColor(value: string | undefined): CrewColor {
  return value && value in CREW_COLORS ? (value as CrewColor) : DEFAULT_COLOR;
}

export function crewFrame(value: string | undefined): CrewFrame {
  return value && (FRAMES as readonly string[]).includes(value)
    ? (value as CrewFrame)
    : DEFAULT_FRAME;
}

/**
 * The frames' outlines in a 40 × 40 box: the brand's cut (top-left and bottom-right corners), a
 * shield, a Minecraft-like banner with a swallowtail, a hexagon. `glyphY` moves the emblem to where
 * the shape has its weight (a banner's tail and a shield's point sit low).
 */
export const FRAME_SHAPES: Record<CrewFrame, { path: string; glyphY: number }> = {
  cut: { path: "M9 1.5H38.5V31L31 38.5H1.5V9Z", glyphY: 20 },
  shield: {
    path: "M20 1.5L36.5 6.5V19C36.5 28.6 29.6 35.2 20 38.5C10.4 35.2 3.5 28.6 3.5 19V6.5Z",
    glyphY: 18.5,
  },
  banner: { path: "M4.5 1.5H35.5V38.5L20 31L4.5 38.5Z", glyphY: 16.5 },
  hex: { path: "M20 1.5L36.5 10.75V29.25L20 38.5L3.5 29.25V10.75Z", glyphY: 20 },
};

/** The edge thins as the mark grows: 2.2 units at 22 px, 1.4 at 64 px. */
export function edgeWidth(size: number): number {
  const t = Math.min(1, Math.max(0, (size - 22) / (64 - 22)));
  return Math.round((2.2 - 0.8 * t) * 100) / 100;
}

/** `#RRGGBB` with an alpha, as `rgb(r g b / a)`. */
export function withAlpha(hex: string, alpha: number): string {
  const value = Number.parseInt(hex.slice(1), 16);
  return `rgb(${(value >> 16) & 255} ${(value >> 8) & 255} ${value & 255} / ${alpha})`;
}

/** The tag chip: the crew colour's edge at 24 % under bone text. */
export function tagTint(color: string): string {
  return withAlpha(CREW_COLORS[crewColor(color)].edge, 0.24);
}

/** "Rastgele": a colour, a frame and an emblem, never all three as they are. */
export function shuffleLook(
  current: { color: CrewColor; frame: CrewFrame; emblem: Emblem },
  emblems: readonly Emblem[],
  random: () => number = Math.random,
): { color: CrewColor; frame: CrewFrame; emblem: Emblem } {
  const pick = <T>(list: readonly T[], not: T): T => {
    const others = list.filter((item) => item !== not);
    return others[Math.floor(random() * others.length)] ?? not;
  };
  return {
    color: pick(COLORS, current.color),
    frame: pick(FRAMES, current.frame),
    emblem: pick(emblems, current.emblem),
  };
}
