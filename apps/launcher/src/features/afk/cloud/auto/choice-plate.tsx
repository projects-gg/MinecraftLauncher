// A chosen option of the auto modes (FISHMINE-L): a chamfered plate that is the quiet plate while
// chosen, as every chosen option in the launcher is (design.md §2: a Segmented's plate, never gold;
// gold is the screen's one action). Text plates (the idle moves, where it looks) and texture plates
// (the blocks to break) share it.
import { cn } from "@pl/ui";
import type { ReactNode } from "react";

export function ChoicePlate({
  label,
  pressed,
  disabled,
  onClick,
  title,
  children,
  square = false,
}: {
  /** Its words, or the accessible name of a texture plate. */
  label: string;
  pressed: boolean;
  disabled?: boolean;
  onClick: () => void;
  title?: string;
  /** A texture plate's picture (then `label` is only read out). */
  children?: ReactNode;
  square?: boolean;
}) {
  return (
    <button
      type="button"
      aria-label={children ? label : undefined}
      aria-pressed={pressed}
      title={title}
      data-on={pressed ? "" : undefined}
      disabled={disabled}
      onClick={onClick}
      className={cn(
        "group relative isolate inline-flex shrink-0 cursor-pointer items-center justify-center select-none [--cut:6px]",
        square ? "size-[40px]" : "h-8 px-3 whitespace-nowrap",
        "transition-[transform,scale,opacity] duration-150 ease-out active:scale-[0.97] focus-visible:outline-none",
        "disabled:pointer-events-none disabled:opacity-45",
      )}
    >
      <span
        aria-hidden
        className="chamfer-ring absolute -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible:opacity-100"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-0 -z-10 bg-white/12 transition-colors duration-150 group-hover:bg-white/20 group-data-[on]:bg-white/25"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-px -z-10 bg-obsidian-800/85 transition-colors duration-150 group-hover:bg-obsidian-700/90 group-data-[on]:bg-[color-mix(in_oklab,white_16%,var(--color-obsidian-800))]"
      />
      {children ? (
        <span className="opacity-65 transition-opacity duration-150 group-hover:opacity-90 group-data-[on]:opacity-100">
          {children}
        </span>
      ) : (
        <span className="text-[12.5px] leading-none font-medium text-ash-300 transition-colors duration-150 group-hover:text-bone-100 group-data-[on]:text-[var(--segmented-ink)]">
          {label}
        </span>
      )}
    </button>
  );
}
