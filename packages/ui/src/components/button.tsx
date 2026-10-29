import { type ButtonHTMLAttributes, forwardRef, type ReactNode } from "react";
import { cn } from "../lib/cn";
import { Tooltip } from "./tooltip";

type Variant = "primary" | "secondary" | "ghost" | "danger" | "bone";
type Size = "sm" | "md" | "lg";

const layers: Record<Variant, { border: string; fill: string; text: string }> = {
  primary: {
    border: "bg-gold-600",
    fill: "bg-[linear-gradient(180deg,var(--color-gold-300),var(--color-gold-500)_55%,var(--color-gold-600))] group-hover:brightness-110",
    text: "text-gold-ink font-semibold",
  },
  secondary: {
    border: "bg-white/12 group-hover:bg-white/20",
    fill: "bg-obsidian-800/85 group-hover:bg-obsidian-700/90",
    text: "text-bone-100 font-medium",
  },
  ghost: {
    border: "bg-transparent",
    fill: "bg-transparent group-hover:bg-white/6",
    text: "text-ash-300 group-hover:text-bone-100 font-medium",
  },
  // A card's action that needs the player ("Şifreyi düzelt", "Süre al") beside the screen's one gold
  // (design.md §2 and §6, lead 29 Oct): a bone plate with dark ink, never a second gold.
  bone: {
    border: "bg-bone-100 group-hover:bg-bone-50",
    fill: "bg-bone-100 group-hover:bg-bone-50",
    text: "text-obsidian-900 font-semibold",
  },
  danger: {
    border: "bg-flare-400/45",
    fill: "bg-[color-mix(in_oklab,var(--color-flare-400)_14%,var(--color-obsidian-900))] group-hover:bg-[color-mix(in_oklab,var(--color-flare-400)_22%,var(--color-obsidian-900))]",
    text: "text-flare-400 font-semibold",
  },
};

// docs/design.md §6: 28 / 32 / 38 px tall (P1; were 28 / 35 / 42).
const sizes: Record<Size, { box: string; cut: string }> = {
  sm: { box: "h-8 px-3 text-[12.5px] gap-1.5", cut: "[--cut:6px]" },
  md: { box: "h-[32px] px-3.5 text-[13px] gap-2", cut: "[--cut:6px]" },
  lg: { box: "h-[38px] px-5 text-[14px] gap-2", cut: "[--cut:8px]" },
};

export interface ButtonProps extends ButtonHTMLAttributes<HTMLButtonElement> {
  variant?: Variant;
  size?: Size;
  icon?: ReactNode;
  iconRight?: ReactNode;
}

/**
 * Chamfered action button. The focus ring is a hollow chamfered layer (`chamfer-ring`, 2px wide
 * and 2px out): an outline would stay a rectangle around the chamfered shape.
 */
export const Button = forwardRef<HTMLButtonElement, ButtonProps>(function Button(
  { variant = "secondary", size = "md", icon, iconRight, className, children, type, ...props },
  ref,
) {
  const layer = layers[variant];
  const dims = sizes[size];
  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={cn(
        "group relative isolate inline-flex shrink-0 cursor-pointer items-center justify-center whitespace-nowrap select-none",
        // Press feedback: a quick 97% squeeze (released at the same speed, interruptible). The
        // disabled look fades in and out too (a launch that starts dims "Katıl" at once).
        "transition-[transform,scale,opacity] duration-150 ease-out active:scale-[0.97] focus-visible:outline-none",
        "disabled:pointer-events-none disabled:opacity-45",
        dims.box,
        dims.cut,
        className,
      )}
      {...props}
    >
      {/* Hollow, so a ghost or tinted fill keeps its label (a filled layer turned it solid gold). */}
      <span
        aria-hidden
        className="chamfer-ring absolute -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible:opacity-100"
      />
      <span
        aria-hidden
        className={cn("chamfer absolute inset-0 -z-10 transition-colors", layer.border)}
      />
      <span
        aria-hidden
        className={cn(
          "chamfer absolute inset-px -z-10 transition-[background-color,filter]",
          layer.fill,
        )}
      />
      {/* Icons fade with the label (a ghost button's icon used to snap to bone on hover). */}
      {icon ? (
        <span className={cn("flex shrink-0 transition-colors", layer.text)}>{icon}</span>
      ) : null}
      {children ? (
        <span
          className={cn(
            "leading-none transition-colors",
            layer.text,
            // Words that change in place (`SwapLabel`) stay beside the icon.
            icon
              ? "[&_.swap-label]:justify-items-start"
              : iconRight && "[&_.swap-label]:justify-items-end",
          )}
        >
          {children}
        </span>
      ) : null}
      {iconRight ? (
        <span className={cn("flex shrink-0 transition-colors", layer.text)}>{iconRight}</span>
      ) : null}
    </button>
  );
});

export interface IconButtonProps extends Omit<ButtonHTMLAttributes<HTMLButtonElement>, "children"> {
  /** Accessible name; also shown as the tooltip. */
  label: string;
  icon: ReactNode;
  size?: "sm" | "md";
  tooltipSide?: "top" | "right" | "bottom" | "left";
  active?: boolean;
}

/** Square, quiet icon action with a required label (tooltip + aria-label). */
export const IconButton = forwardRef<HTMLButtonElement, IconButtonProps>(function IconButton(
  { label, icon, size = "md", tooltipSide = "bottom", active, className, type, ...props },
  ref,
) {
  return (
    <Tooltip content={label} side={tooltipSide}>
      <button
        ref={ref}
        type={type ?? "button"}
        aria-label={label}
        className={cn(
          // Drawn at 28 / 32 px; the hit area reaches 4 px further on every side.
          "hit-area relative inline-flex shrink-0 cursor-pointer items-center justify-center rounded-md text-ash-300",
          "transition-[background-color,color,transform,scale,opacity] duration-150 ease-out",
          "hover:bg-white/7 hover:text-bone-100 active:scale-[0.94] disabled:pointer-events-none disabled:opacity-45",
          // The "⋯" that opened a menu stays lit while it is open, so the menu reads as its own
          // even where it opens beside it over the next card (SHEET-UX).
          "data-[state=open]:bg-white/8 data-[state=open]:text-bone-100",
          size === "sm" ? "size-8" : "size-[32px]",
          // A chosen tool (a pin, a console tab) sits on the quiet plate (design.md §6): the
          // screen's gold is its main action's.
          active && "bg-[color:var(--segmented-plate,rgb(255_255_255/0.16))] text-bone-50",
          className,
        )}
        {...props}
      >
        {icon}
      </button>
    </Tooltip>
  );
});
