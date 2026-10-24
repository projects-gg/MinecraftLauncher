// Discord-branded actions: Discord's own logo (the official white symbol from the brand kit at
// discord.com/branding, the PNG unaltered) on Discord's Blurple, in the launcher's chamfered
// button shape. The player sees at a glance that Discord opens.
import { cn } from "@pl/ui";
import { type ButtonHTMLAttributes, forwardRef } from "react";
import discordSymbol from "@/assets/brand/discord-symbol-white.png";

const sizes = {
  sm: { box: "h-8 px-3 text-[13px] gap-2", cut: "[--cut:6px]", logo: 14 },
  md: { box: "h-10 px-4 text-sm gap-2.5", cut: "[--cut:7px]", logo: 16 },
  lg: { box: "h-12 px-6 text-[15px] gap-3", cut: "[--cut:9px]", logo: 20 },
} as const;

export const DiscordButton = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { size?: keyof typeof sizes }
>(function DiscordButton({ size = "md", className, children, type, ...props }, ref) {
  const dims = sizes[size];
  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={cn(
        "group relative isolate inline-flex shrink-0 cursor-pointer items-center justify-center font-semibold whitespace-nowrap text-white select-none",
        "transition-transform duration-100 active:translate-y-px focus-visible:outline-none disabled:pointer-events-none disabled:opacity-45",
        dims.box,
        dims.cut,
        className,
      )}
      {...props}
    >
      <span
        aria-hidden
        className="chamfer-ring absolute -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible:opacity-100"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-0 -z-10 bg-[#5865F2] transition-colors duration-150 group-hover:bg-[#4752C4] group-active:bg-[#3C45A5]"
      />
      <img
        src={discordSymbol}
        alt=""
        draggable={false}
        style={{ width: Math.round(dims.logo * 1.32), height: dims.logo }}
        className="shrink-0"
      />
      <span className="leading-none">{children}</span>
    </button>
  );
});

/**
 * The white symbol alone, uncoloured: an action that leads toward Discord without opening it yet
 * (Home's invitation opens the opt-in screen first). Blurple stays on actions that open Discord.
 */
export function DiscordMark({ height = 13 }: { height?: number }) {
  return (
    <img
      src={discordSymbol}
      alt=""
      aria-hidden
      draggable={false}
      style={{ width: Math.round(height * 1.32), height }}
      className="shrink-0"
    />
  );
}

/** The logo alone on a Blurple square (rows that link to Discord sign-in). */
export function DiscordTile({ size = 36, className }: { size?: number; className?: string }) {
  return (
    <span
      aria-hidden
      className={cn("flex shrink-0 items-center justify-center rounded-lg bg-[#5865F2]", className)}
      style={{ width: size, height: size }}
    >
      <img
        src={discordSymbol}
        alt=""
        draggable={false}
        style={{ width: Math.round(size * 0.58), height: Math.round(size * 0.44) }}
      />
    </span>
  );
}
