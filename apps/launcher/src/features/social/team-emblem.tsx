// A team's emblem: one of the service's emblem names, drawn with a bundled Phosphor icon in gold
// on a dark tile (the same tile as the panel's empty states), and its tag as a quiet chip.
import {
  Axe,
  Crown,
  Cube,
  Diamond,
  Fire,
  Flag,
  Ghost,
  Heart,
  type Icon,
  Lightning,
  Moon,
  PawPrint,
  Planet,
  Rocket,
  Shield,
  Skull,
  Star,
  Sword,
  Target,
  Tree,
  Trophy,
} from "@phosphor-icons/react";
import { Chip, cn } from "@pl/ui";
import type { Emblem } from "./teams";

const ICONS: Record<Emblem, Icon> = {
  sword: Sword,
  shield: Shield,
  crown: Crown,
  flame: Fire,
  bolt: Lightning,
  star: Star,
  skull: Skull,
  diamond: Diamond,
  heart: Heart,
  rocket: Rocket,
  tree: Tree,
  paw: PawPrint,
  ghost: Ghost,
  cube: Cube,
  axe: Axe,
  trophy: Trophy,
  moon: Moon,
  flag: Flag,
  target: Target,
  planet: Planet,
};

export function emblemIcon(emblem: string): Icon {
  return ICONS[emblem as Emblem] ?? Shield;
}

/** The emblem tile; `size` in px (the icon takes a bit over half of it). */
export function TeamEmblem({
  emblem,
  size = 36,
  className,
}: {
  emblem: string;
  size?: number;
  className?: string;
}) {
  const Glyph = emblemIcon(emblem);
  return (
    <span
      aria-hidden
      className={cn(
        "flex shrink-0 items-center justify-center rounded-[var(--radius-head)] bg-gold-700/25 text-gold-300 ring-1 ring-gold-400/25",
        className,
      )}
      style={{ width: size, height: size }}
    >
      <Glyph size={Math.round(size * 0.56)} weight="duotone" />
    </span>
  );
}

/** `[GKR]` beside a team's name. */
export function TeamTag({ tag, className }: { tag: string; className?: string }) {
  return (
    <Chip className={cn("h-[18px] px-1.5 font-display text-[11px] tracking-[0.06em]", className)}>
      {tag}
    </Chip>
  );
}
