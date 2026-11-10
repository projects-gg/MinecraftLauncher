// A conversation's unread count, the same wherever it leads into the conversation ("Mesajlar"'s
// rows, the crew page's "Sohbete Git", CREW-UI): the row's one gold (design.md §2), "@" before the
// number when the player is mentioned (SOC-NEXT §3.1), "99+" past 99. It grows in from 80% and
// leaves faster; screen readers hear the count in the row's own name, not here.
import { At } from "@phosphor-icons/react";
import { AnimatePresence, motion } from "motion/react";
import { enter, leave } from "../motion";

export function UnreadChip({ count, mentioned }: { count: number; mentioned: boolean }) {
  return (
    <AnimatePresence initial={false}>
      {count > 0 ? (
        <motion.span
          key="unread"
          aria-hidden
          className="chamfer flex h-[18px] min-w-[18px] items-center justify-center gap-px bg-gold-500 px-1 font-display text-[11px] font-semibold text-gold-ink tabular-nums [--cut:4px]"
          initial={{ opacity: 0, transform: "scale(0.8)" }}
          animate={{ opacity: 1, transform: "scale(1)", transition: enter(0.2) }}
          exit={{ opacity: 0, transform: "scale(0.8)", transition: leave(0.14) }}
        >
          {mentioned ? <At size={11} weight="bold" aria-hidden /> : null}
          {count > 99 ? "99+" : count}
        </motion.span>
      ) : null}
    </AnimatePresence>
  );
}
