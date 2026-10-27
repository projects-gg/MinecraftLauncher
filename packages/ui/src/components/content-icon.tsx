// Content icons (ICON1, docs/design.md §6): the thing a tile or an empty state stands for (a
// world, a server, a pack, a set's purpose) as our own 16 x 16 pixel sprites in the launcher's
// golds, drawn on whole pixels like SignalBars. Controls and inline status marks stay Phosphor.
// Shared here (ICON2) so the launcher and Studio draw the same sprites; the Android app draws the
// same rows (`PixelSprites.java`).
import type { CSSProperties } from "react";
import { cn } from "../lib/cn";

/**
 * A sprite's colours: "h" to "o" the golds 200 to 700, "k" the deep dirt under them, "F", "f",
 * "e" flare 300, flare 400 and ember 700 (failure only), "J", "j", "q" light, base and dark jade
 * (done only).
 */
const TONES = {
  h: "var(--color-gold-200)",
  l: "var(--color-gold-300)",
  m: "var(--color-gold-400)",
  d: "var(--color-gold-500)",
  s: "var(--color-gold-600)",
  o: "var(--color-gold-700)",
  k: "color-mix(in oklab, var(--color-gold-700) 62%, var(--color-obsidian-950))",
  F: "var(--color-flare-300)",
  f: "var(--color-flare-400)",
  e: "var(--color-ember-700)",
  J: "color-mix(in oklab, var(--color-jade-400) 55%, white)",
  j: "var(--color-jade-400)",
  q: "color-mix(in oklab, var(--color-jade-400) 45%, var(--color-obsidian-950))",
} as const;

export type SpriteTone = keyof typeof TONES;

/** One gold step darker, for a quiet sprite (a set's mark at rest). Flare and jade stay. */
const QUIET: Partial<Record<SpriteTone, SpriteTone>> = {
  h: "l",
  l: "m",
  m: "d",
  d: "s",
  s: "o",
  o: "k",
};

const FILL = Object.fromEntries(
  Object.entries(TONES).map(([tone, fill]) => [tone, { fill }]),
) as Record<SpriteTone, CSSProperties>;

interface Sprite {
  /** 16 rows of 16 pixels, each a tone or "." for nothing. */
  rows: readonly string[];
  /** A one-pixel drop shadow (down and right) in this tone, as SignalBars draws its bars. */
  shadow?: SpriteTone;
}

/**
 * The sprites. They are drawn for 16 px (1x), 32 px (2x) and 48 px (3x), the only sizes where
 * every pixel stays whole. A new subject gets a new drawing here, never a stock glyph.
 */
export const CONTENT_SPRITES = {
  /** A grass block: lit grass on top and over the edges, dark dirt on the sides. */
  world: {
    rows: [
      "......lmll......",
      "....lhllllml....",
      "..llmllllhllll..",
      "lllhllmllllmhlll",
      "mmllmllhlllllldd",
      "mmmmlmllllhldddd",
      "ommmmmllmldddddk",
      "ooommmmmddddkdkk",
      "ooomommmdddkkdkk",
      "oosooomokdkkkkkk",
      "osooooookdkokkkk",
      "oooosoookkkkkkok",
      "..oooosokokkkk..",
      "....ooookkkk....",
      "......ookk......",
      "................",
    ],
  },
  /** Three rack units, each with a bay and a lit lamp. */
  server: {
    shadow: "o",
    rows: [
      "................",
      "................",
      ".lllllllllllll..",
      ".mooooommmmhmm..",
      ".sssssssssssss..",
      "................",
      ".lllllllllllll..",
      ".mooooommmmhmm..",
      ".sssssssssssss..",
      "................",
      ".lllllllllllll..",
      ".mooooommmmhmm..",
      ".sssssssssssss..",
      "................",
      "................",
      "................",
    ],
  },
  /** A controller: the empty card before the first game. */
  play: {
    shadow: "o",
    rows: [
      "................",
      "................",
      "................",
      "..llllllllll....",
      ".mmmmmmmmmmmm...",
      "mmmommmmmmmhmm..",
      "mmooommmmmhmhm..",
      "mmmommmmmmmhmm..",
      "mmmmmmmmmmmmmm..",
      "mmmmmssssmmmmm..",
      "mmmm......mmmm..",
      ".ss........ss...",
      "................",
      "................",
      "................",
      "................",
    ],
  },
  /** A chest: a pack, a mod set, a mod without its own icon. */
  pack: {
    shadow: "o",
    rows: [
      "................",
      "................",
      ".llllllllllll...",
      "lmmmmmmmmmmmmm..",
      "lmmmmmmmmmmmmm..",
      "lmmmmmhhmmmmmm..",
      "oooooolloooooo..",
      "mdddddmmdddddd..",
      "mddddddddddddd..",
      "mddddddddddddd..",
      "mddddddddddddd..",
      "mddddddddddddd..",
      "ssssssssssssss..",
      "................",
      "................",
      "................",
    ],
  },
  /** A painting: frame, sky, sun and two hills. */
  screenshots: {
    shadow: "o",
    rows: [
      "................",
      "................",
      "ssssssssssssss..",
      "soooooooooooos..",
      "sooooooooohhos..",
      "sooooooooohhos..",
      "sooooomoooooos..",
      "soooommmooooos..",
      "sooommmmmoolos..",
      "soommmmmmmllls..",
      "smmmmmmmmlllls..",
      "ssssssssssssss..",
      "................",
      "................",
      "................",
      "................",
    ],
  },
  /** Two texture tiles, planks in front of stone. */
  resourcePack: {
    shadow: "o",
    rows: [
      ".....ddddddddd..",
      ".....dssossoss..",
      ".....dosssssss..",
      ".....dsssosoos..",
      ".....dssssssss..",
      "lllllllllossos..",
      "lmmmsmmmmsssss..",
      "lmmmsmmmmsosss..",
      "lssssssssooooo..",
      "lmmmmmmsm.......",
      "lmmmmmmsm.......",
      "lssssssss.......",
      "lmmsmmmmm.......",
      "sssssssss.......",
      "................",
      "................",
    ],
  },
  /** A sun with eight short rays. */
  shaderPack: {
    shadow: "o",
    rows: [
      "......mm........",
      "......mm........",
      "..m........m....",
      "...m......m.....",
      ".....llll.......",
      "....lhhlll......",
      "mm..lhllll..mm..",
      "mm..llllld..mm..",
      "....lllldd......",
      ".....lldd.......",
      "...m......m.....",
      "..m........m....",
      "......mm........",
      "......mm........",
      "................",
      "................",
    ],
  },
  /** A scroll with three lines of writing. */
  logs: {
    shadow: "o",
    rows: [
      "................",
      "................",
      ".smmmmmmmmmms...",
      ".sddddddddddds..",
      "..llllllllll....",
      "..llssssssll....",
      "..llllllllll....",
      "..llssssslll....",
      "..llllllllll....",
      "..llssssssll....",
      "..llllllllll....",
      ".smmmmmmmmmms...",
      ".sddddddddddds..",
      "................",
      "................",
      "................",
    ],
  },
  /** A robot's head: antenna, visor, two lit eyes. */
  afk: {
    shadow: "o",
    rows: [
      "................",
      "......hh........",
      "......mm........",
      "..llllllllll....",
      ".mmmmmmmmmmmm...",
      ".moooooooooom...",
      "smohhoooohhoms..",
      "smohhoooohhoms..",
      ".moooooooooom...",
      ".mmmmmmmmmmmm...",
      ".mmmmssssmmmm...",
      ".mmmmmmmmmmmm...",
      "..ssssssssss....",
      "................",
      "................",
      "................",
    ],
  },
  /** A warning sign with the exclamation mark cut out (flare). */
  failed: {
    rows: [
      "................",
      "................",
      "......FF........",
      "......FF........",
      ".....FffF.......",
      ".....f..f.......",
      "....ff..ff......",
      "....ff..ff......",
      "...fff..fff.....",
      "...ffffffff.....",
      "..fffff.ffff....",
      "..fffffffffff...",
      ".fffffffffffff..",
      ".eeeeeeeeeeee...",
      "................",
      "................",
    ],
  },
  /** A face and a plus: an account to add. */
  account: {
    shadow: "o",
    rows: [
      "................",
      "................",
      ".ssssssss.......",
      ".ssssssss.......",
      ".sllllllls......",
      ".llllllll.......",
      ".loolloll.......",
      ".llllllll.......",
      ".lllddlll..h....",
      ".llllllll..h....",
      ".mmmmmmmm.hhhh..",
      "...........h....",
      "...........h....",
      "................",
      "................",
      "................",
    ],
  },
  /** Two smiling players, the second a step back and looking at the first: friends. */
  friends: {
    shadow: "o",
    rows: [
      "................",
      "................",
      "................",
      "........ooooooo.",
      "........ooooooo.",
      ".sssssss.ddddoo.",
      ".sssssss.dddddd.",
      ".sllllls.kmdkmd.",
      ".lllllll.dddddd.",
      ".lholohl.sdddsd.",
      ".lllllll.dsssdd.",
      ".lslllsl.ssssss.",
      ".llsssll........",
      ".mmmmmmm........",
      "................",
      "................",
    ],
  },
  /** A speech bubble with three dots: messages (an empty conversation or list of them). */
  chat: {
    shadow: "o",
    rows: [
      "................",
      "................",
      "..lllllllllll...",
      ".lhmmmmmmmmmmd..",
      ".lmmmmmmmmmmmd..",
      ".lmmmmmmmmmmmd..",
      ".lmmommommommd..",
      ".lmmmmmmmmmmmd..",
      ".lmmmmmmmmmmmd..",
      ".lmmmmmmmmmmmd..",
      "..sssssssssss...",
      "..ss............",
      "..s.............",
      "................",
      "................",
      "................",
    ],
  },
  /** A check mark (jade): something finished. */
  done: {
    rows: [
      "................",
      "................",
      ".............J..",
      "............Jj..",
      "...........Jjq..",
      "..........Jjq...",
      ".........Jjq....",
      "..J.....Jjq.....",
      "..jJ...Jjq......",
      "..qjJ.Jjq.......",
      "...qjJjq........",
      "....qjq.........",
      ".....q..........",
      "................",
      "................",
      "................",
    ],
  },
  /** A four-point sparkle and a small one: something new, the visual set. */
  sparkle: {
    shadow: "o",
    rows: [
      "................",
      "............m...",
      "......d....mhm..",
      "......m.....m...",
      "......m.........",
      ".....mlm........",
      "....mlhlm.......",
      ".dmmlhhhlmmd....",
      "....mlhlm.......",
      ".....mlm........",
      "......m.........",
      "......m.........",
      "......d.........",
      "................",
      "................",
      "................",
    ],
  },
  /** A sword, tip up right: the PvP set. */
  pvp: {
    rows: [
      "................",
      "............hhl.",
      "...........hlls.",
      "..........hlls..",
      ".........hlls...",
      "........hlls....",
      ".......hlls.....",
      "..md..hlls......",
      "..mdd.lls.......",
      "...ddmls........",
      "....dmmd........",
      "...oo.dmd.......",
      "..oso..dd.......",
      ".mmo............",
      ".mm.............",
      "................",
    ],
  },
  /** A lightning bolt: the performance set. */
  performance: {
    shadow: "o",
    rows: [
      "................",
      ".......hlld.....",
      "......lmmd......",
      ".....lmmd.......",
      "....lmmd........",
      "...lmmmmmmd.....",
      "......lmmd......",
      ".....lmmd.......",
      ".....lmd........",
      "....lmd.........",
      "....ld..........",
      "...ld...........",
      "...d............",
      "................",
      "................",
      "................",
    ],
  },
  /** A cloud, a dome between two shoulders, lit from above: 7/24 Bulut, AFK accounts on our machines. */
  cloud: {
    shadow: "o",
    rows: [
      "................",
      "................",
      "................",
      "......hhh.......",
      ".....hllllh.....",
      "....hllllllh....",
      "..hhlllllllhhh..",
      ".hllllllllllllm.",
      ".hlllllllllllmm.",
      ".lllllllllmmmmd.",
      "..mmmmmmmmmmmd..",
      "................",
      "................",
      "................",
      "................",
      "................",
    ],
  },
  /** A backpack with a buckled flap and a front pocket: a cloud account's live inventory (ITEMART). */
  inventory: {
    shadow: "o",
    rows: [
      "................",
      "................",
      "......llll......",
      ".....l....d.....",
      "....hllllllm....",
      "...lmmmmmmmmd...",
      "..lmhmmmmmmmmd..",
      "..lmmmmmmmmmmd..",
      "..ldddddddddds..",
      "..lmmmmhhmmmmd..",
      "..lmmmmssmmmmd..",
      "..lmllllllllmd..",
      "..lmlmmmmmmdmd..",
      "..lmlmmmmmmdmd..",
      "...sssssssssss..",
      "................",
    ],
  },
  /** A laced boot with a folded cuff, toe forward: a cloud account walked by hand (movement). */
  movement: {
    shadow: "o",
    rows: [
      "................",
      "....hhhhhh......",
      "....llllll......",
      "....ssssss......",
      "....lmmmmd......",
      "....lmmmhd......",
      "....lmmmmd......",
      "....lmmmhdd.....",
      "....lmmmmmmdd...",
      "...llmmmmmmmmd..",
      "...lmmmmmmmmmmd.",
      "...lmmmmmmmmhmd.",
      "..sssssssssssss.",
      "................",
      "................",
      "................",
    ],
  },
} satisfies Record<string, Sprite>;

export type ContentIconKind = keyof typeof CONTENT_SPRITES;
export type SpriteSize = 16 | 32 | 48;

interface SpritePaths {
  shadow?: { tone: SpriteTone; d: string };
  layers: { tone: SpriteTone; d: string }[];
}

const built = new Map<ContentIconKind, SpritePaths>();

/** One path per tone (runs of a row joined), built once per sprite. */
export function spritePaths(kind: ContentIconKind): SpritePaths {
  const hit = built.get(kind);
  if (hit) return hit;
  const sprite: Sprite = CONTENT_SPRITES[kind];
  const layers = new Map<SpriteTone, string>();
  let shade = "";
  sprite.rows.forEach((row, y) => {
    let x = 0;
    while (x < row.length) {
      const tone = row[x] as SpriteTone | ".";
      if (tone === ".") {
        x++;
        continue;
      }
      let w = 1;
      while (row[x + w] === tone) w++;
      layers.set(tone, `${layers.get(tone) ?? ""}M${x} ${y}h${w}v1h-${w}z`);
      x += w;
    }
    if (!sprite.shadow) return;
    x = 0;
    while (x < row.length) {
      if (row[x] === ".") {
        x++;
        continue;
      }
      let w = 1;
      while (x + w < row.length && row[x + w] !== ".") w++;
      shade += `M${x + 1} ${y + 1}h${w}v1h-${w}z`;
      x += w;
    }
  });
  const paths: SpritePaths = {
    shadow: sprite.shadow ? { tone: sprite.shadow, d: shade } : undefined,
    layers: [...layers].map(([tone, d]) => ({ tone, d })),
  };
  built.set(kind, paths);
  return paths;
}

/** The whole-pixel size that sits well in a square tile of `box` px (2x from 42 px, 3x from 68). */
export function spriteSize(box: number): SpriteSize {
  return box >= 68 ? 48 : box >= 42 ? 32 : 16;
}

/**
 * A content icon: one of our pixel sprites at 16, 32 or 48 px. Decorative, the words beside it
 * carry the meaning. `quiet` draws it one gold step darker.
 */
export function ContentIcon({
  kind,
  size = 32,
  quiet = false,
  className,
}: {
  kind: ContentIconKind;
  size?: SpriteSize;
  quiet?: boolean;
  className?: string;
}) {
  const { shadow, layers } = spritePaths(kind);
  const fill = (tone: SpriteTone) => FILL[quiet ? (QUIET[tone] ?? tone) : tone];
  return (
    <svg
      aria-hidden
      focusable="false"
      width={size}
      height={size}
      viewBox="0 0 16 16"
      shapeRendering="crispEdges"
      className={cn("shrink-0", className)}
    >
      {shadow ? <path d={shadow.d} style={fill(shadow.tone)} /> : null}
      {layers.map((layer) => (
        <path key={layer.tone} d={layer.d} style={fill(layer.tone)} />
      ))}
    </svg>
  );
}
