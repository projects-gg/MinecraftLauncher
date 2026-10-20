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

/** `[GKR]` beside a crew's name: the crew colour at 24 % under bone text. */
export function TeamTag({
  tag,
  color,
  className,
}: {
  tag: string;
  color?: string;
  className?: string;
}) {
  return (
    <Chip
      className={cn(
        "h-[18px] px-1.5 font-display text-[11px] tracking-[0.06em] text-bone-50",
        className,
      )}
      style={{ background: tagTint(color ?? "gold") }}
    >
      {tag}
    </Chip>
  );
}

/** The emblem alone, large and faint, behind the crew card's corner (it carries the crew's sign). */
export function CrewWatermark({ look, size = 150 }: { look: CrewLook; size?: number }) {
  const Glyph = emblemIcon(look.emblem);
  return (
    <Glyph
      aria-hidden
      size={size}
      weight="fill"
      className="pointer-events-none absolute -top-6 -right-7 select-none"
      color={CREW_COLORS[crewColor(look.color)].edge}
      style={{ opacity: 0.07 }}
    />
  );
}
