import { CaretDown, Check, MagnifyingGlass } from "@phosphor-icons/react";
import { Select as RadixSelect, Slider as RadixSlider, ToggleGroup } from "radix-ui";
import { forwardRef, type InputHTMLAttributes, type ReactNode, useId } from "react";
import { cn } from "../lib/cn";

/** Label above, helper or error below (never placeholder-as-label). */
export function Field({
  label,
  help,
  error,
  children,
  htmlFor,
  className,
}: {
  label: string;
  help?: ReactNode;
  error?: string | null;
  children: ReactNode;
  htmlFor?: string;
  className?: string;
}) {
  return (
    <div className={cn("flex flex-col gap-1.5", className)}>
      <label htmlFor={htmlFor} className="text-[13px] font-medium text-bone-100">
        {label}
      </label>
      {children}
      {error ? (
        <p role="alert" className="text-[12.5px] text-flare-400">
          {error}
        </p>
      ) : help ? (
        <p className="text-[12.5px] text-ash-400">{help}</p>
      ) : null}
    </div>
  );
}

export const Input = forwardRef<
  HTMLInputElement,
  InputHTMLAttributes<HTMLInputElement> & { invalid?: boolean }
>(function Input({ className, invalid, ...props }, ref) {
  return (
    <input
      ref={ref}
      aria-invalid={invalid || undefined}
      className={cn(
        "h-10 w-full rounded-md border border-white/10 bg-obsidian-800/80 px-3 text-[14px] text-bone-50 transition-colors placeholder:text-ash-500",
        "hover:border-white/18 focus:border-gold-400/70 focus:outline-none focus-visible:outline-none focus:ring-2 focus:ring-gold-400/25",
        invalid && "border-flare-400/70 focus:border-flare-400 focus:ring-flare-400/25",
        className,
      )}
      {...props}
    />
  );
});

export function SearchInput({
  value,
  onChange,
  placeholder,
  label,
  className,
}: {
  value: string;
  onChange: (value: string) => void;
  placeholder: string;
  label: string;
  className?: string;
}) {
  return (
    <div className={cn("relative", className)}>
      <MagnifyingGlass
        size={16}
        className="pointer-events-none absolute top-1/2 left-3 -translate-y-1/2 text-ash-400"
      />
      <Input
        aria-label={label}
        value={value}
        onChange={(e) => onChange(e.target.value)}
        placeholder={placeholder}
        className="pl-9"
      />
    </div>
  );
}

export interface SelectOption<T extends string> {
  value: T;
  label: string;
  hint?: string;
}

export function Select<T extends string>({
  value,
  onValueChange,
  options,
  label,
  className,
  disabled,
}: {
  value: T;
  onValueChange: (value: T) => void;
  options: SelectOption<T>[];
  label: string;
  className?: string;
  disabled?: boolean;
}) {
  return (
    <RadixSelect.Root
      value={value}
      onValueChange={(v) => onValueChange(v as T)}
      disabled={disabled}
    >
      <RadixSelect.Trigger
        aria-label={label}
        className={cn(
          "flex h-10 w-full cursor-pointer items-center justify-between gap-2 rounded-md border border-white/10 bg-obsidian-800/80 px-3 text-left text-[14px] text-bone-50 transition-colors",
          "hover:border-white/18 focus:border-gold-400/70 focus:ring-2 focus:ring-gold-400/25 focus:outline-none data-[disabled]:opacity-50",
          className,
        )}
      >
        <RadixSelect.Value />
        <RadixSelect.Icon>
          <CaretDown size={14} className="text-ash-400" />
        </RadixSelect.Icon>
      </RadixSelect.Trigger>
      <RadixSelect.Portal>
        <RadixSelect.Content
          position="popper"
          sideOffset={6}
          className="glass-strong z-50 max-h-[min(360px,var(--radix-select-content-available-height))] min-w-[var(--radix-select-trigger-width)] overflow-hidden rounded-[var(--radius-panel)] p-1 shadow-[0_24px_60px_rgb(34_6_6/0.55)]"
        >
          <RadixSelect.Viewport>
            {options.map((o) => (
              <RadixSelect.Item
                key={o.value}
                value={o.value}
                className="relative flex cursor-pointer items-center gap-2 rounded-md py-2 pr-8 pl-2.5 text-[13.5px] text-bone-100 outline-none select-none data-[highlighted]:bg-white/8"
              >
                <span className="min-w-0 flex-1">
                  <RadixSelect.ItemText>{o.label}</RadixSelect.ItemText>
                  {o.hint ? <span className="block text-[12px] text-ash-400">{o.hint}</span> : null}
                </span>
                <RadixSelect.ItemIndicator className="absolute right-2">
                  <Check size={15} className="text-gold-400" />
                </RadixSelect.ItemIndicator>
              </RadixSelect.Item>
            ))}
          </RadixSelect.Viewport>
        </RadixSelect.Content>
      </RadixSelect.Portal>
    </RadixSelect.Root>
  );
}

/** Mutually exclusive options shown inline (loader kind, view mode, release channel). */
export function Segmented<T extends string>({
  value,
  onValueChange,
  options,
  label,
  className,
}: {
  value: T;
  onValueChange: (value: T) => void;
  options: Array<{ value: T; label: string; icon?: ReactNode }>;
  label: string;
  className?: string;
}) {
  return (
    <ToggleGroup.Root
      type="single"
      value={value}
      onValueChange={(v) => v && onValueChange(v as T)}
      aria-label={label}
      className={cn(
        "inline-flex rounded-md border border-white/10 bg-obsidian-850/80 p-1",
        className,
      )}
    >
      {options.map((o) => (
        <ToggleGroup.Item
          key={o.value}
          value={o.value}
          className={cn(
            "chamfer flex h-8 cursor-pointer items-center gap-1.5 px-3 text-[13px] font-medium text-ash-300 transition-colors [--cut:5px]",
            "hover:text-bone-50 data-[state=on]:bg-gold-500 data-[state=on]:text-gold-ink focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300",
          )}
        >
          {o.icon}
          {o.label}
        </ToggleGroup.Item>
      ))}
    </ToggleGroup.Root>
  );
}

/** Slider with a visible label and live value. */
export function Slider({
  value,
  onValueChange,
  min,
  max,
  step = 1,
  label,
  format,
  marks,
  className,
}: {
  value: number;
  onValueChange: (value: number) => void;
  min: number;
  max: number;
  step?: number;
  label: string;
  format?: (value: number) => string;
  marks?: Array<{ value: number; label: string; tone?: "warn" }>;
  className?: string;
}) {
  const id = useId();
  return (
    <div className={cn("flex flex-col gap-2", className)}>
      <div className="flex items-baseline justify-between">
        <span id={id} className="text-[13px] font-medium text-bone-100">
          {label}
        </span>
        <span className="font-display text-[15px] font-semibold text-gold-300 tabular-nums">
          {format ? format(value) : value}
        </span>
      </div>
      <RadixSlider.Root
        value={[value]}
        min={min}
        max={max}
        step={step}
        onValueChange={([v]) => v !== undefined && onValueChange(v)}
        className="relative flex h-5 w-full cursor-pointer touch-none items-center select-none"
      >
        <RadixSlider.Track className="relative h-1.5 grow overflow-hidden rounded-full bg-white/10">
          <RadixSlider.Range className="absolute h-full bg-[linear-gradient(90deg,var(--color-gold-600),var(--color-gold-400))]" />
        </RadixSlider.Track>
        <RadixSlider.Thumb
          aria-labelledby={id}
          aria-valuetext={format ? format(value) : undefined}
          className="block size-[18px] rounded-full border-2 border-gold-300 bg-obsidian-900 shadow-[0_2px_8px_rgb(34_6_6/0.6)] transition-transform hover:scale-110 focus-visible:ring-4 focus-visible:ring-gold-400/30 focus-visible:outline-none"
        />
      </RadixSlider.Root>
      {marks ? (
        <div className="relative h-4 text-[11px] text-ash-400">
          {marks.map((m) => (
            <span
              key={m.value}
              className={cn(
                "absolute -translate-x-1/2 tabular-nums",
                m.tone === "warn" && "text-flare-400",
              )}
              style={{ left: `${((m.value - min) / (max - min)) * 100}%` }}
            >
              {m.label}
            </span>
          ))}
        </div>
      ) : null}
    </div>
  );
}
