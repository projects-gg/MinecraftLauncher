import { Switch as RadixSwitch } from "radix-ui";
import { useId } from "react";
import { cn } from "../lib/cn";

/** Labeled toggle. The label is always visible and clickable. */
export function Switch({
  checked,
  onCheckedChange,
  label,
  description,
  disabled,
  hideLabel,
  className,
}: {
  checked: boolean;
  onCheckedChange: (checked: boolean) => void;
  /** Always required: becomes the accessible name even when `hideLabel` is set. */
  label: string;
  description?: string;
  disabled?: boolean;
  /** Use when a visible title already sits next to the switch (settings rows). */
  hideLabel?: boolean;
  className?: string;
}) {
  const id = useId();
  return (
    <div className={cn("flex items-center gap-3", className)}>
      <RadixSwitch.Root
        id={id}
        aria-label={hideLabel ? label : undefined}
        checked={checked}
        onCheckedChange={onCheckedChange}
        disabled={disabled}
        className={cn(
          "group relative h-[20px] w-[34px] shrink-0 cursor-pointer rounded-full bg-obsidian-700 ring-1 ring-white/10 transition-colors duration-200",
          "data-[state=checked]:bg-gold-500 data-[state=checked]:ring-gold-400/60 disabled:cursor-not-allowed disabled:opacity-50",
        )}
      >
        {/* The thumb stretches toward its travel while pressed, like a physical switch. */}
        <RadixSwitch.Thumb
          className={cn(
            "block size-[14px] origin-left translate-x-[3px] rounded-full bg-bone-100 shadow-[0_1px_3px_rgb(10_9_12/0.6)] transition-transform duration-200 ease-[cubic-bezier(0.16,1,0.3,1)]",
            "group-active:scale-x-[1.18] data-[state=checked]:origin-right data-[state=checked]:translate-x-[17px] data-[state=checked]:bg-gold-ink",
          )}
        />
      </RadixSwitch.Root>
      {hideLabel ? null : (
        <label htmlFor={id} className="cursor-pointer select-none">
          <span className="block text-[13px] font-medium text-bone-100">{label}</span>
          {description ? (
            <span className="block text-[12px] text-ash-400">{description}</span>
          ) : null}
        </label>
      )}
    </div>
  );
}
