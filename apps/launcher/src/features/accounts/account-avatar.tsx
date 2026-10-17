import { cn, PixelBrackets, springSnappy } from "@pl/ui";
import { motion } from "motion/react";
import { type FaceAccount, SwappingFace } from "./account-face";

/**
 * The account's face (its picture, or the head cut from its own skin; a lettered tile until the
 * head is cached). The active account wears the brand's pixel brackets; with `bracketsId` they
 * travel from the old face to the new one when the selection changes.
 */
export function AccountAvatar({
  account,
  size = 40,
  selected = false,
  bracketsId,
  className,
}: {
  account: FaceAccount;
  /** Multiples of 8 keep the pixels square and sharp. */
  size?: number;
  selected?: boolean;
  bracketsId?: string;
  className?: string;
}) {
  return (
    <span
      className={cn("relative inline-flex shrink-0", className)}
      style={{ width: size, height: size }}
    >
      <SwappingFace account={account} size={size} />
      {selected ? (
        <motion.span
          layoutId={bracketsId}
          aria-hidden
          className="pointer-events-none absolute -inset-[3px]"
          transition={springSnappy}
        >
          <PixelBrackets size={Math.max(7, Math.round(size / 5))} />
        </motion.span>
      ) : null}
    </span>
  );
}
