// What an account looks like everywhere it appears (title bar, menu, center, palette, rail,
// command bar, first-run flow, toasts): its chosen picture, or its player head.
import { cn, easeOutExpo, PlayerHead } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { PresetFace } from "@/features/avatars/preset-face";
import { presetAvatar } from "@/features/avatars/presets";
import type { AccountSummary } from "@/lib/api";
import { useCalm } from "./calm";

export type FaceAccount = Pick<AccountSummary, "username" | "headUrl" | "kind" | "avatar">;

/** The bundled picture of an offline account, if it chose one that still exists. */
function pictureOf(account: FaceAccount) {
  return account.kind === "offline" ? presetAvatar(account.avatar) : null;
}

/** Changes when the face itself changes (another picture, a new head, the head arriving). */
export function faceKey(account: FaceAccount): string {
  return pictureOf(account)?.id ?? account.headUrl ?? `letter:${account.username.slice(0, 1)}`;
}

export function AccountFace({
  account,
  size,
  className,
}: {
  account: FaceAccount;
  /** Multiples of 8 keep head pixels square and sharp. */
  size: number;
  className?: string;
}) {
  const picture = pictureOf(account);
  return picture ? (
    <PresetFace avatar={picture} size={size} className={className} />
  ) : (
    <PlayerHead name={account.username} src={account.headUrl} size={size} className={className} />
  );
}

/**
 * The face with a soft cross-fade when it changes: the new one settles in from a touch smaller
 * and blurred while the old one fades out on top of it. Opacity only under reduced motion.
 */
export function SwappingFace({
  account,
  size,
  className,
}: {
  account: FaceAccount;
  size: number;
  className?: string;
}) {
  const calm = useCalm();
  return (
    <span
      className={cn("relative inline-flex shrink-0", className)}
      style={{ width: size, height: size }}
    >
      <AnimatePresence initial={false}>
        <motion.span
          key={faceKey(account)}
          className="absolute inset-0 flex"
          initial={calm({ opacity: 0, scale: 0.88, filter: "blur(3px)" })}
          animate={calm({
            opacity: 1,
            scale: 1,
            filter: "blur(0px)",
            transition: { duration: 0.32, ease: easeOutExpo },
          })}
          exit={calm({ opacity: 0, filter: "blur(2px)", transition: { duration: 0.2 } })}
        >
          <AccountFace account={account} size={size} />
        </motion.span>
      </AnimatePresence>
    </span>
  );
}
