// A crew's mark: its emblem (a bundled Phosphor icon, fill weight so it reads at 22 px too) on its
// frame, filled with the crew's colour under a faint light (an enamel badge, not a glow), and its
// tag as a chip tinted with that colour. Crews from before looks (and from a service without them)
// show today's look: gold, the cut frame. Gold stays the interface's only accent: the crew colour
// is the crew's content, like an avatar (TEAM2 §3.5).
import {
  Alien,
  Axe,
  Bird,
  Butterfly,
  Cactus,
  CastleTurret,
  Cat,
  Crown,
  Cube,
  Diamond,
  Fire,
  FishSimple,
  Flag,
  Flask,
  GameController,
  Ghost,
  Heart,
  Horse,
  type Icon,
  Leaf,
  Lightning,
  MagicWand,
  Moon,
  PawPrint,
  Planet,
  Robot,
  Rocket,
  Shield,
  Skull,
  Snowflake,
  Star,
  Sun,
  Sword,
  Target,
  Tree,
  Trophy,
  Waves,
} from "@phosphor-icons/react";
import { Chip, cn } from "@pl/ui";
import { useId } from "react";
import type { CrewLook } from "@/lib/api/social-types";
import {
  CREW_COLORS,
  crewColor,
  crewFrame,
  type Emblem,
  edgeWidth,
  FRAME_SHAPES,
  tagTint,
} from "./crew-look";

const ICONS: Record<Emblem, Icon> = {
  sword: Sword,
  shield: Shield,
  axe: Axe,
  crown: Crown,
  trophy: Trophy,
  flag: Flag,
  target: Target,
  skull: Skull,
  ghost: Ghost,
  flame: Fire,
  bolt: Lightning,
  sun: Sun,
  moon: Moon,
  star: Star,
  planet: Planet,
  snowflake: Snowflake,
  waves: Waves,
  leaf: Leaf,
  tree: Tree,
  cactus: Cactus,
  paw: PawPrint,
  cat: Cat,
  horse: Horse,
  bird: Bird,
  fish: FishSimple,
  butterfly: Butterfly,
  heart: Heart,
  diamond: Diamond,
  cube: Cube,
  rocket: Rocket,
  potion: Flask,
  wand: MagicWand,
  castle: CastleTurret,
  controller: GameController,
  alien: Alien,
  robot: Robot,
};

/** The icon of an emblem; one this version does not know is a shield. */
export function emblemIcon(emblem: string): Icon {
  return ICONS[emblem as Emblem] ?? Shield;
}

/** The mark of a crew; `size` in px (22 in lists, 64 on the crew card). */
export function CrewMark({
  look,
  size = 36,
  className,
}: {
  look: Partial<CrewLook> & Pick<CrewLook, "emblem">;
  size?: number;
  className?: string;
}) {
  const colors = CREW_COLORS[crewColor(look.color)];
  const shape = FRAME_SHAPES[crewFrame(look.frame)];
  const Glyph = emblemIcon(look.emblem);
  const glyph = Math.round(size * 0.48);
  // An SVG id must not carry React's id marks.
  const light = `crew-light-${useId().replace(/[^\w-]/g, "")}`;
  return (
    <span
      aria-hidden
      className={cn("relative inline-flex shrink-0", className)}
      style={{ width: size, height: size }}
    >
      <svg
        aria-hidden="true"
        viewBox="0 0 40 40"
        width={size}
        height={size}
        className="absolute inset-0"
      >
        <defs>
          <linearGradient id={light} x1="0" y1="0" x2="0" y2="1">
            <stop offset="0" stopColor="#fff" stopOpacity="0.14" />
            <stop offset="1" stopColor="#fff" stopOpacity="0" />
          </linearGradient>
        </defs>
        <path d={shape.path} fill={colors.fill} />
        <path d={shape.path} fill={`url(#${light})`} />
        <path
          d={shape.path}
          fill="none"
          stroke={colors.edge}
          strokeWidth={edgeWidth(size)}
          strokeLinejoin="round"
        />
      </svg>
      <Glyph
        size={glyph}
        weight="fill"
        color={colors.glyph}
        className="absolute"
        style={{ left: (size - glyph) / 2, top: (shape.glyphY / 40) * size - glyph / 2 }}
      />
    </span>
  );
}

/**
 * The tag's three sizes (TEAM6 §3.5): `sm` 16 px by chat names, `md` 18 px in lists and beside a
 * crew's name, `lg` 20 px in a person card's title. Text never under 11 px (docs/design.md §3).
 */
const TAG_SIZES = {
  sm: "h-4 px-1 text-[11px] tracking-[0.04em] [--cut:4px]",
  md: "h-[18px] px-1.5 text-[11px] tracking-[0.06em]",
  lg: "h-5 px-[7px] text-[12px] tracking-[0.06em]",
} as const;

export type TagSize = keyof typeof TAG_SIZES;

/** `[GKR]` beside a crew's name, or before a player's (TEAM6): the crew colour at 24 % under bone
 * text. The chip never shrinks; the name beside it truncates. The tag is the chip's own Figtree
 * SemiBold (docs/design.md §3, micro): Tektur's cedilla is a detached comma that drops to a
 * pixel at 11 px, so "GKŞ" read "GKS" (OVL-LANG; the in-game overlay's chip is the same). Figtree
 * sits a pixel high in the chip: `pt-px` centres the capitals and keeps Ğ's breve off the edge. */
export function TeamTag({
  tag,
  color,
  size = "md",
  className,
}: {
  tag: string;
  color?: string;
  size?: TagSize;
  className?: string;
}) {
  return (
    <Chip
      className={cn("pt-px text-bone-50", TAG_SIZES[size], className)}
      style={{ background: tagTint(color ?? "gold") }}
    >
      {tag}
    </Chip>
  );
}

/** The emblem alone, large and faint, behind the crew card's corner (it carries the crew's sign). */
export function CrewWatermark({
  look,
  size = 150,
  className = "-top-6 -right-7",
}: {
  look: CrewLook;
  size?: number;
  className?: string;
}) {
  const Glyph = emblemIcon(look.emblem);
  return (
    <Glyph
      aria-hidden
      size={size}
      weight="fill"
      className={cn("pointer-events-none absolute select-none", className)}
      color={CREW_COLORS[crewColor(look.color)].edge}
      style={{ opacity: 0.07 }}
    />
  );
}
