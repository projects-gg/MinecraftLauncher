import type { HTMLAttributes, ReactNode } from "react";
import { cn } from "../lib/cn";

/** Small chamfered label for real categories (news category, loader, channel). */
export function Chip({
  tone = "neutral",
  className,
  children,
  ...props
}: HTMLAttributes<HTMLSpanElement> & { tone?: "neutral" | "gold" | "jade" | "flare" }) {
  return (
    <span
      className={cn(
        "chamfer inline-flex h-[22px] items-center gap-1 px-2 text-[11.5px] font-semibold [--cut:5px]",
        tone === "neutral" && "bg-white/9 text-bone-100",
        tone === "gold" && "bg-gold-500/28 text-gold-200",
        tone === "jade" && "bg-jade-400/15 text-jade-400",
        tone === "flare" && "bg-flare-400/15 text-flare-400",
        className,
      )}
      {...props}
    >
      {children}
    </span>
  );
}

/** Thin progress bar. Omit `value` for an indeterminate shimmer. */
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
  return (
    <div
      role="progressbar"
      aria-label={label}
      aria-valuemin={0}
      aria-valuemax={100}
      aria-valuenow={determinate ? Math.round(value) : undefined}
      className={cn("relative h-1.5 overflow-hidden rounded-full bg-white/8", className)}
    >
      <div
        className="absolute inset-y-0 left-0 rounded-full bg-[linear-gradient(90deg,var(--color-gold-600),var(--color-gold-400))] transition-[width] duration-300 ease-out"
        style={{ width: determinate ? `${Math.min(100, Math.max(0, value))}%` : "35%" }}
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

export function Kbd({ children }: { children: ReactNode }) {
  return (
    <kbd className="inline-flex h-5 min-w-5 items-center justify-center rounded border border-white/12 bg-obsidian-800 px-1 font-sans text-[11px] font-semibold text-ash-300">
      {children}
    </kbd>
  );
}

/**
 * The brand banner's pixel L-brackets. Marks selection only (selected instance, focused card).
 */
export function PixelBrackets({ className, size = 10 }: { className?: string; size?: number }) {
  const arm = `${size}px`;
  const corner = "absolute bg-gold-300";
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

/** Square pixel-art player head with a lettered fallback. */
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
        "relative inline-flex shrink-0 items-center justify-center overflow-hidden rounded-[var(--radius-head)] bg-gold-700/40 ring-1 ring-white/10",
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
          className="font-display font-bold text-gold-200"
          style={{ fontSize: Math.round(size * 0.45) }}
        >
          {name.slice(0, 1).toLocaleUpperCase("tr")}
        </span>
      )}
    </span>
  );
}

/** Empty states invite an action; illustration optional. */
export function EmptyState({
  title,
  body,
  action,
  illustration,
  className,
}: {
  title: string;
  body?: string;
  action?: ReactNode;
  illustration?: string;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col items-center px-6 py-10 text-center", className)}>
      {illustration ? (
        <img src={illustration} alt="" className="mb-4 size-36 object-contain" draggable={false} />
      ) : null}
      <p className="font-display text-lg font-semibold text-bone-50">{title}</p>
      {body ? <p className="mt-1.5 max-w-[42ch] text-ash-400">{body}</p> : null}
      {action ? <div className="mt-5">{action}</div> : null}
    </div>
  );
}
