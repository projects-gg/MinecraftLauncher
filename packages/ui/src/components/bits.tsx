import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../lib/cn";

/**
 * Small chamfered label for real categories (news category, loader, channel). `onArt`: the gold
 * label over key art, on a dark base so it reads on any season's picture.
 */
export function Chip({
  tone = "neutral",
  className,
  children,
  ...props
}: HTMLAttributes<HTMLSpanElement> & { tone?: "neutral" | "gold" | "jade" | "flare" | "onArt" }) {
  return (
    <span
      className={cn(
        "chamfer inline-flex h-[20px] shrink-0 items-center gap-1 px-[7px] text-[11.5px] font-semibold whitespace-nowrap [--cut:5px]",
        tone === "neutral" && "bg-white/9 text-bone-100",
        tone === "gold" && "bg-gold-500/28 text-gold-200",
        tone === "jade" && "bg-jade-400/15 text-jade-400",
        tone === "flare" && "bg-flare-400/15 text-flare-300",
        tone === "onArt" &&
          "bg-[linear-gradient(rgb(232_154_44/0.3),rgb(232_154_44/0.3)),linear-gradient(rgb(10_9_12/0.78),rgb(10_9_12/0.78))] text-gold-200",
        className,
      )}
      {...props}
    >
      {children}
    </span>
  );
}

/**
 * Thin progress bar. Omit `value` for an indeterminate sweep. The fill moves with `transform`
 * only (no layout work per update), easing between the steps a job reports.
 */
export function Progress({
  value,
  className,
  label,
}: {
  value?: number;
  className?: string;
  label: string;
}) {
  const determinate = typeof value === "number";
  const percent = determinate ? Math.min(100, Math.max(0, value)) : 0;
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={determinate ? Math.round(percent) : undefined}
      className={cn("relative h-1.5 overflow-hidden rounded-full bg-white/8", className)}
    >
      <div
        className={cn(
          "absolute inset-y-0 left-0 rounded-full bg-[linear-gradient(90deg,var(--color-gold-600),var(--color-gold-400))]",
          determinate
            ? "w-full transition-transform duration-500 ease-[cubic-bezier(0.16,1,0.3,1)]"
            : "w-[35%] animate-indeterminate",
        )}
        style={determinate ? { transform: `translateX(${percent - 100}%)` } : undefined}
      >
        <div className="absolute inset-0 overflow-hidden rounded-full">
          <div className="h-full w-full animate-shimmer bg-[linear-gradient(90deg,transparent,rgb(255_255_255/0.35),transparent)]" />
        </div>
      </div>
    </div>
  );
}

/** Content-shaped loading placeholder. */
export function Skeleton({ className }: { className?: string }) {
  return (
    <div
      aria-hidden
      className={cn("relative overflow-hidden rounded-md bg-obsidian-800/80", className)}
    >
      <div className="absolute inset-0 animate-shimmer bg-[linear-gradient(90deg,transparent,rgb(255_255_255/0.05),transparent)]" />
    </div>
  );
}

/**
 * Words that change in place on a control ("Güncellemeleri denetle" → "Denetleniyor"): every
 * wording sits in one grid cell, so the control keeps the width of the longest and nothing beside
 * it moves; the outgoing words fade first, the new ones follow (POLISH2: the buttons next to
 * "Güncellemeleri denetle" slid 60 px and back). Only the shown words are in the accessible name.
 * Centred on their own; a `Button` with an icon sets them beside it (`swap-label`, REVIEW2: a
 * short wording floated 37 px away from its icon). A wording may carry its own mark (Home's
 * "Katıl" with its gold play glyph, SMALL10): the mark comes and goes with its words.
 */
export function SwapLabel({ labels, active }: { labels: ReactNode[]; active: number }) {
  return (
    <span className="swap-label inline-grid justify-items-center">
      {labels.map((label, i) => (
        <span
          // biome-ignore lint/suspicious/noArrayIndexKey: the wordings are fixed, in order
          key={i}
          aria-hidden={i === active ? undefined : true}
          className={cn(
            "[grid-area:1/1] transition-opacity",
            i === active ? "opacity-100 delay-[60ms] duration-150" : "opacity-0 duration-100",
          )}
        >
          {label}
        </span>
      ))}
    </span>
  );
}

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded border border-white/12 bg-obsidian-800 px-1 font-sans text-[11px] font-semibold text-ash-300">
      {children}
    </kbd>
  );
}

/**
 * The brand banner's pixel L-brackets. Marks selection only (selected instance, focused card), in
 * bone: the shape says chosen, the screen's gold is its main action's (design.md §1.3, §2).
 */
export function PixelBrackets({ className, size = 10 }: { className?: string; size?: number }) {
  const arm = `${size}px`;
  const corner = "absolute bg-bone-100";
  return (
    <span aria-hidden className={cn("pointer-events-none absolute -inset-1", className)}>
      <span className={cn(corner, "top-0 left-0 h-[2px]")} style={{ width: arm }} />
      <span className={cn(corner, "top-0 left-0 w-[2px]")} style={{ height: arm }} />
      <span className={cn(corner, "top-0 right-0 h-[2px]")} style={{ width: arm }} />
      <span className={cn(corner, "top-0 right-0 w-[2px]")} style={{ height: arm }} />
      <span className={cn(corner, "bottom-0 left-0 h-[2px]")} style={{ width: arm }} />
      <span className={cn(corner, "bottom-0 left-0 w-[2px]")} style={{ height: arm }} />
      <span className={cn(corner, "right-0 bottom-0 h-[2px]")} style={{ width: arm }} />
      <span className={cn(corner, "right-0 bottom-0 w-[2px]")} style={{ height: arm }} />
    </span>
  );
}

/**
 * Square pixel-art player head. Without a picture, its first letter on the quiet plate in bone,
 * not gold: a face is not an action, and the screen's one gold is its main action (design.md §2).
 */
export function PlayerHead({
  src,
  name,
  size = 32,
  className,
}: {
  src?: string | null;
  name: string;
  size?: number;
  className?: string;
}) {
  return (
    <span
      className={cn(
        "relative inline-flex shrink-0 items-center justify-center overflow-hidden rounded-[var(--radius-head)] bg-white/[0.07] ring-1 ring-white/10",
        className,
      )}
      style={{ width: size, height: size }}
    >
      {src ? (
        <img
          src={src}
          alt=""
          width={size}
          height={size}
          draggable={false}
          className="size-full [image-rendering:pixelated]"
        />
      ) : (
        <span
          className="font-display font-bold text-bone-100"
          style={{ fontSize: Math.round(size * 0.45) }}
        >
          {name.slice(0, 1).toLocaleUpperCase("tr")}
        </span>
      )}
    </span>
  );
}

/**
 * Empty states invite an action. A picture (`illustration`) for page-sized states, or the small
 * chamfered mark (`icon`) the instance tabs use for their own.
 */
export function EmptyState({
  title,
  body,
  action,
  illustration,
  icon,
  className,
}: {
  title: string;
  body?: string;
  action?: ReactNode;
  illustration?: string;
  icon?: ReactNode;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-center px-6 py-8 text-center", className)}>
      {illustration ? (
        <img src={illustration} alt="" className="mb-3 size-28 object-contain" draggable={false} />
      ) : icon ? (
        // `data-empty-icon`: an app's own look for it (Studio draws the icon alone, without the
        // tile: docs/design.md §9).
        <span
          aria-hidden
          data-empty-icon=""
          className="chamfer mb-4 flex size-14 items-center justify-center bg-obsidian-800 text-gold-400/85 [--cut:8px]"
        >
          {icon}
        </span>
      ) : null}
      <p className="font-display text-[16px] font-semibold text-bone-50">{title}</p>
      {body ? <p className="mt-1.5 max-w-[42ch] text-[13px] text-ash-300">{body}</p> : null}
      {action ? <div className="mt-4">{action}</div> : null}
    </div>
  );
}
