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
 * chamfered layer (clip-path would cut an outline). Under the pointer an empty box's edge
 * brightens and a ticked one lightens, as a switch's track does. Ticked it is bone with a dark
 * mark: the screen's one gold is its main action (design.md §2, lead 29 Oct).
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
        // The box's colours: its edge, and its fill when ticked; both lighten under the pointer.
        "[--check-edge:rgb(255_255_255/0.24)] [--check-on:var(--color-bone-100)]",
        "enabled:hover:[--check-edge:rgb(255_255_255/0.4)] enabled:hover:[--check-on:var(--color-bone-50)]",
        className,
      )}
    >
      <span
        aria-hidden
        className="chamfer-ring absolute -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible/check:opacity-100"
      />
      <span
        aria-hidden
        className={cn(
          "chamfer absolute inset-0 -z-10 bg-(--check-edge) transition-colors duration-150",
          "group-data-[state=checked]/check:bg-(--check-on) group-data-[state=indeterminate]/check:bg-(--check-on)",
        )}
      />
      <span
        aria-hidden
        className={cn(
          "chamfer absolute inset-[1.5px] -z-10 bg-obsidian-900 transition-colors duration-150 [--cut:3px]",
          "group-data-[state=checked]/check:bg-(--check-on) group-data-[state=indeterminate]/check:bg-(--check-on)",
        )}
      />
      <Checkbox.Indicator forceMount className="absolute inset-0 text-obsidian-900">
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
