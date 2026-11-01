// SOC-ID: the marks of the ways into the friends account. Microsoft's four squares (required on its
// sign-in button) and Discord's white symbol sit on a quiet square in rows and waiting cards; the
// Microsoft button is the dark surface, as tall as Discord's Blurple one beside it, and Blurple
// stays on the buttons that really open Discord (docs/design.md §6).
import { cn } from "@pl/ui";
import { type ButtonHTMLAttributes, forwardRef } from "react";
import { MicrosoftMark } from "@/features/settings/microsoft-mark";
import type { SocialProvider } from "@/lib/api/social-types";
import { DiscordMark } from "./discord-button";

/** A way's mark on a quiet square: "Bağlı hesaplar" rows and the waiting cards. */
export function ProviderTile({
  provider,
  size = 36,
  className,
}: {
  provider: SocialProvider;
  size?: number;
  className?: string;
}) {
  return (
    <span
      aria-hidden
      className={cn(
        "flex shrink-0 items-center justify-center rounded-lg bg-white/[0.06] ring-1 ring-white/8",
        className,
      )}
      style={{ width: size, height: size }}
    >
      {provider === "microsoft" ? (
        <MicrosoftMark size={Math.round(size * 0.46)} />
      ) : (
        <DiscordMark height={Math.round(size * 0.36)} />
      )}
    </span>
  );
}

/**
 * "Microsoft ile giriş yap" on the sign-in screen: the dark surface with Microsoft's mark, 48 px tall
 * like the Discord button above it, pressing at 97%.
 */
export const MicrosoftButton = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement>
>(function MicrosoftButton({ className, children, type, ...props }, ref) {
  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={cn(
        "group relative isolate inline-flex h-12 shrink-0 cursor-pointer items-center justify-center gap-3 px-6 text-[15px] font-semibold whitespace-nowrap text-bone-50 select-none [--cut:9px]",
        "transition-[transform,scale,opacity] duration-150 ease-out active:scale-[0.97] focus-visible:outline-none",
        "disabled:pointer-events-none disabled:opacity-45",
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
        className="chamfer absolute inset-0 -z-10 bg-white/14 transition-colors group-hover:bg-white/22"
      />
      <span
        aria-hidden
        className="chamfer absolute inset-px -z-10 bg-obsidian-850 transition-colors group-hover:bg-obsidian-800"
      />
      <MicrosoftMark size={18} />
      <span className="leading-none">{children}</span>
    </button>
  );
});
