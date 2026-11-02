// SOC-ID: the marks of the ways into the friends account. Microsoft's four squares (required on its
// sign-in button) and Discord's white symbol sit on a quiet square in rows and waiting cards; the
// Microsoft button is the dark surface, in DiscordButton's sizes, and Blurple stays on the buttons
// that really open Discord (docs/design.md §6).
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

/** DiscordButton's sizes, so the two stand as equals (or Microsoft one step smaller under it). */
const sizes = {
  md: { box: "h-10 px-4 text-sm gap-2.5", cut: "[--cut:7px]", mark: 16 },
  lg: { box: "h-12 px-6 text-[15px] gap-3", cut: "[--cut:9px]", mark: 18 },
} as const;

/**
 * "Microsoft ile giriş yap" on the sign-in screen: the dark surface with Microsoft's mark, pressing
 * at 97%. `lg` beside Discord's button once Microsoft opens accounts, `md` under "Bağlı hesabınla
 * gir" while it signs in only the accounts it is linked to.
 */
export const MicrosoftButton = forwardRef<
  HTMLButtonElement,
  ButtonHTMLAttributes<HTMLButtonElement> & { size?: keyof typeof sizes }
>(function MicrosoftButton({ size = "lg", className, children, type, ...props }, ref) {
  const dims = sizes[size];
  return (
    <button
      ref={ref}
      type={type ?? "button"}
      className={cn(
        "group relative isolate inline-flex shrink-0 cursor-pointer items-center justify-center font-semibold whitespace-nowrap text-bone-50 select-none",
        dims.box,
        dims.cut,
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
      <MicrosoftMark size={dims.mark} />
      <span className="leading-none">{children}</span>
    </button>
  );
});
