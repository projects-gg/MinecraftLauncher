import { CheckCircle, MinusCircle, XCircle } from "@phosphor-icons/react";
import { AnimatePresence, motion } from "motion/react";
import { pop, useMotionPrefs } from "@/features/modcenter/motion";
import type { StepStatus } from "@/lib/api";

/**
 * The state of one step: a quiet ring while waiting, a softly breathing gold core while it runs
 * (a status light, not a spinner), then a check, a cross or a dash that grows in from 90%.
 * The box never changes size, so the text beside it never moves.
 */
export function StatusGlyph({ status }: { status: StepStatus }) {
  const prefs = useMotionPrefs();
  return (
    <span aria-hidden className="relative grid size-5 shrink-0 place-items-center">
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span
          key={status}
          {...pop(prefs.reduce, status === "pending" ? 0.16 : 0.24)}
          className="absolute inset-0 grid place-items-center"
        >
          {status === "pending" ? (
            <span className="size-[9px] rounded-full ring-[1.5px] ring-ash-500/80" />
          ) : status === "running" ? (
            <span className="grid size-[15px] place-items-center rounded-full ring-[1.5px] ring-gold-400/70">
              <span className="size-[7px] rounded-full bg-gold-300 animate-pulse motion-reduce:animate-none" />
            </span>
          ) : status === "done" ? (
            <CheckCircle size={19} weight="fill" className="text-jade-400" />
          ) : status === "failed" ? (
            <XCircle size={19} weight="fill" className="text-flare-400" />
          ) : (
            <MinusCircle size={18} className="text-ash-400" />
          )}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}
