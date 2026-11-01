import { cn } from "../lib/cn";
import { CHECK_PATHS, type FilledGlyph, GLYPH_PATHS, SHAPE_PATHS } from "./profile-mark-drawings";
import { Tooltip } from "./tooltip";

/**
 * Profile badges (BADGES, docs/tasks/badges.md; SOC-VERIFY's marks grown into a catalogue): what
 * Projects gives an account, right of its name. Studio keeps the catalogue (name, colour, glyph,
 * shape, order, visibility); the friends service sends each card's badges by their keys and the
 * catalogue at `GET /v1/marks`. Colours, glyphs and shapes are fixed sets, each drawn for the
 * 12, 14 and 16 px grids (`profile-mark-drawings.ts`), so every badge stays sharp and keeps its
 * measured contrast in both themes. Still: no motion of its own.
 */

/** The sizes a badge is drawn for: 12 beside 12–12.5 px text, 14 beside 13–15 px, 16 by titles. */
export type MarkSize = 12 | 14 | 16;

/** The palette (`--color-mark-<name>`): white glyphs ≥ 3.5:1, ≥ 4.4:1 on obsidian-900 and ≥ 3.5:1 on white. */
export const MARK_COLORS = [
  "blue",
  "red",
  "purple",
  "green",
  "orange",
  "teal",
  "indigo",
  "pink",
  "slate",
] as const;
export type MarkColor = (typeof MARK_COLORS)[number];

/** The glyphs: the check (a stroke, SOC-VERIFY's) and the filled ones. */
export const MARK_GLYPHS = [
  "check",
  "star",
  "cloud",
  "bolt",
  "crown",
  "heart",
  "flask",
  "play",
  "trophy",
] as const satisfies readonly ("check" | FilledGlyph)[];
export type MarkGlyph = (typeof MARK_GLYPHS)[number];

/** The outlines: SOC-VERIFY's eight-lobed seal, a round badge and a shield. */
export const MARK_SHAPES = ["seal", "round", "shield"] as const;
export type MarkShape = (typeof MARK_SHAPES)[number];

/** How a badge looks, and its name in the interface's language. */
export interface MarkLook {
  color: MarkColor;
  glyph: MarkGlyph;
  shape: MarkShape;
}

/** A badge to show: its key (the service's), its name and its look. */
export interface MarkItem extends MarkLook {
  key: string;
  /** "Projects yöneticisi" / "Projects admin", in the interface's language. */
  label: string;
}

const FILL: Record<MarkColor, string> = {
  blue: "text-mark-blue",
  red: "text-mark-red",
  purple: "text-mark-purple",
  green: "text-mark-green",
  orange: "text-mark-orange",
  teal: "text-mark-teal",
  indigo: "text-mark-indigo",
  pink: "text-mark-pink",
  slate: "text-mark-slate",
};

/**
 * The paths of one badge at one size, for other renderers (the phone's vector drawables, the forum,
 * Studio's pickers): the outline (filled with the colour), and the glyph either filled white or, for
 * the check, stroked white with round caps and joins at `stroke`.
 */
export function markDrawing(
  size: MarkSize,
  shape: MarkShape,
  glyph: MarkGlyph,
): { outline: string; glyph: string; stroke: number | null } {
  const room = shape === "shield" ? "shield" : "plain";
  if (glyph === "check") {
    const check = CHECK_PATHS[size];
    return { outline: SHAPE_PATHS[size][shape], glyph: check[room], stroke: check.width };
  }
  return { outline: SHAPE_PATHS[size][shape], glyph: GLYPH_PATHS[size][room][glyph], stroke: null };
}

/** A known look, or null for values from a newer catalogue (a client draws nothing then). */
export function markLookOf(value: {
  color?: unknown;
  glyph?: unknown;
  shape?: unknown;
}): MarkLook | null {
  const { color, glyph, shape } = value;
  if (
    typeof color !== "string" ||
    typeof glyph !== "string" ||
    typeof shape !== "string" ||
    !(MARK_COLORS as readonly string[]).includes(color) ||
    !(MARK_GLYPHS as readonly string[]).includes(glyph) ||
    !(MARK_SHAPES as readonly string[]).includes(shape)
  ) {
    return null;
  }
  return { color: color as MarkColor, glyph: glyph as MarkGlyph, shape: shape as MarkShape };
}

/** The bare drawing; `label` names it for screen readers unless it is `decorative`. */
function MarkSvg({
  look,
  size,
  label,
  decorative = false,
  className,
}: {
  look: MarkLook;
  size: MarkSize;
  label?: string;
  decorative?: boolean;
  className?: string;
}) {
  const drawing = markDrawing(size, look.shape, look.glyph);
  const common = {
    focusable: "false" as const,
    width: size,
    height: size,
    viewBox: `0 0 ${size} ${size}`,
    "data-mark-color": look.color,
    className: cn("profile-mark shrink-0", FILL[look.color], className),
  };
  const paths = (
    <>
      <path d={drawing.outline} fill="currentColor" />
      {drawing.stroke === null ? (
        <path d={drawing.glyph} fill="#fff" />
      ) : (
        <path
          d={drawing.glyph}
          fill="none"
          stroke="#fff"
          strokeWidth={drawing.stroke}
          strokeLinecap="round"
          strokeLinejoin="round"
        />
      )}
    </>
  );
  return decorative ? (
    <svg aria-hidden="true" {...common}>
      {paths}
    </svg>
  ) : (
    <svg role="img" aria-label={label} {...common}>
      {paths}
    </svg>
  );
}

/**
 * One badge right of a person's name. It names itself (`role="img"` with `label`), so a screen
 * reader says "Ayşe, Doğrulanmış hesap"; the tooltip (`label`) opens under the pointer. Pass
 * `tooltip={false}` where a tooltip would sit on another (a menu row, a tooltip, a sentence).
 */
export function ProfileMark({
  color,
  glyph,
  shape = "seal",
  size = 14,
  label,
  tooltip = true,
  className,
}: Partial<Pick<MarkLook, "shape">> &
  Omit<MarkLook, "shape"> & {
    size?: MarkSize;
    label: string;
    tooltip?: boolean;
    className?: string;
  }) {
  const mark = (
    <MarkSvg look={{ color, glyph, shape }} size={size} label={label} className={className} />
  );
  if (!tooltip) return mark;
  return (
    <Tooltip content={label}>
      <span className="inline-flex shrink-0">{mark}</span>
    </Tooltip>
  );
}

/**
 * A person's badges beside their name (docs/tasks/badges.md, "BADGES-UI için"): the first `max`
 * (catalogue order, the most important first; 1 in rows, more only where a title has room), and
 * one tooltip that lists every badge with its name. Screen readers hear every name, the hidden ones
 * too. No "+2" chip: the tooltip and the person's card carry the rest. Nothing without badges.
 */
export function ProfileMarks({
  marks,
  size = 14,
  max = 1,
  tooltip = true,
  className,
}: {
  marks: readonly MarkItem[];
  size?: MarkSize;
  max?: number;
  tooltip?: boolean;
  className?: string;
}) {
  if (marks.length === 0) return null;
  const shown = marks.slice(0, Math.max(1, max));
  const names = marks.map((mark) => mark.label).join(", ");
  const icons = (
    <span
      role="img"
      aria-label={names}
      className={cn("inline-flex shrink-0 items-center gap-0.5", className)}
    >
      {shown.map((mark) => (
        <MarkSvg key={mark.key} look={mark} size={size} decorative />
      ))}
    </span>
  );
  if (!tooltip) return icons;
  return (
    <Tooltip
      content={marks.length === 1 ? names : <ProfileMarkList marks={marks} tone="inherit" />}
    >
      {icons}
    </Tooltip>
  );
}

/**
 * Every badge of a person with its name, under their name on a card (the friends panel's card, a
 * crew member's card, Studio's player header): a wrapping row, each badge 14 px beside its name in
 * the quiet secondary text. Read as a list.
 */
export function ProfileMarkList({
  marks,
  size = 14,
  tone = "quiet",
  className,
}: {
  marks: readonly MarkItem[];
  size?: MarkSize;
  /** `quiet`: the names in ash (a card); `inherit`: the surrounding text's colour (a tooltip). */
  tone?: "quiet" | "inherit";
  className?: string;
}) {
  if (marks.length === 0) return null;
  return (
    <ul
      className={cn(
        "flex items-center gap-x-3 gap-y-1",
        tone === "quiet" ? "flex-wrap" : "flex-col items-start",
        className,
      )}
    >
      {marks.map((mark) => (
        <li key={mark.key} className="inline-flex min-w-0 items-center gap-1.5">
          <MarkSvg look={mark} size={size} decorative />
          <span
            className={cn("truncate text-[12px] leading-4", tone === "quiet" && "text-ash-300")}
          >
            {mark.label}
          </span>
        </li>
      ))}
    </ul>
  );
}
