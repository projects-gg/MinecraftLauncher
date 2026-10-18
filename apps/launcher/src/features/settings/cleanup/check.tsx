import { Check as CheckIcon, Minus } from "@phosphor-icons/react";
import { cn } from "@pl/ui";
import { Checkbox } from "radix-ui";

/** Shape of each mark: grows in from 60% (never from nothing) and fades with its state. */
const mark =
  "absolute inset-0 m-auto transition-[transform,scale,opacity] duration-150 ease-[cubic-bezier(0.23,1,0.32,1)] scale-[0.6] opacity-0";

/**
 * Small chamfered checkbox (the hextech cut of actions, docs/design.md §1) with a third
 * "partly" state for a group's own box. The tick and the bar crossfade, so a partly box never
 * flashes a tick on its way out; a press dips the box a little. The focus ring is a larger
 * chamfered layer (clip-path would cut an outline).
 */
export function Check({
  id,
  checked,
  onCheckedChange,
  label,
  disabled,
  className,
}: {
  id?: string;
  checked: boolean | "indeterminate";
  onCheckedChange: (next: boolean) => void;
  /** Accessible name when no `<label>` points at it. */
  label?: string;
  disabled?: boolean;
  className?: string;
}) {
  return (
    <Checkbox.Root
      id={id}
      checked={checked}
      onCheckedChange={(next) => onCheckedChange(next === true)}
      aria-label={label}
      disabled={disabled}
      className={cn(
        "group/check relative isolate inline-flex size-[18px] shrink-0 cursor-pointer items-center justify-center [--cut:4px]",
        "transition-transform duration-150 ease-[cubic-bezier(0.23,1,0.32,1)] active:scale-[0.9] focus-visible:outline-none",
        "disabled:cursor-default disabled:opacity-45",
        className,
      )}
    >
      <span
        aria-hidden
        className="chamfer absolute -inset-[3px] -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible/check:opacity-100"
      />
      <span
        aria-hidden
        className={cn(
          "chamfer absolute inset-0 -z-10 bg-white/24 transition-colors duration-150",
          "group-hover/check:bg-white/40 group-data-[state=checked]/check:bg-gold-500 group-data-[state=indeterminate]/check:bg-gold-500",
        )}
      />
      <span
        aria-hidden
        className={cn(
          "chamfer absolute inset-[1.5px] -z-10 bg-obsidian-900 transition-colors duration-150 [--cut:3px]",
          "group-data-[state=checked]/check:bg-gold-500 group-data-[state=indeterminate]/check:bg-gold-500",
        )}
      />
      <Checkbox.Indicator forceMount className="absolute inset-0 text-gold-ink">
        <CheckIcon
          size={12}
          weight="bold"
          className={cn(
            mark,
            "group-data-[state=checked]/check:scale-100 group-data-[state=checked]/check:opacity-100",
          )}
        />
        <Minus
          size={12}
          weight="bold"
          className={cn(
            mark,
            "group-data-[state=indeterminate]/check:scale-100 group-data-[state=indeterminate]/check:opacity-100",
          )}
        />
      </Checkbox.Indicator>
    </Checkbox.Root>
  );
}
