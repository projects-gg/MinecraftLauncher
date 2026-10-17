import { cn } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef } from "react";
import { enter, glide, leave, stagger, useMotionPrefs } from "@/features/modcenter/motion";
import { Percent } from "@/features/modcenter/parts";
import type { StepStatus } from "@/lib/api";
import { StatusGlyph } from "./status-glyph";

export interface ChecklistRow {
  key: string;
  status: StepStatus;
  text: string;
  note?: string;
  /** Download progress of the running row. */
  percent?: number;
}

const tone: Record<StepStatus, string> = {
  pending: "text-ash-400",
  running: "text-bone-50",
  done: "text-bone-100",
  failed: "text-bone-100",
  skipped: "text-ash-400",
};

/**
 * Steps of a repair or reinstall. The first rows cascade in (40 ms apart); rows added later
 * fade in where they belong and the rows below glide. A row's state shows in its glyph and the
 * tone of its text; the running row carries its percentage.
 */
export function Checklist({ rows, label }: { rows: ChecklistRow[]; label: string }) {
  const prefs = useMotionPrefs();
  const first = useRef(true);
  useEffect(() => {
    first.current = false;
  }, []);
  const shift = (px: number) => (prefs.reduce ? "none" : `translateY(${px}px)`);
  return (
    <ul aria-label={label} className="space-y-px">
      <AnimatePresence>
        {rows.map((row, index) => (
          <motion.li
            key={row.key}
            layout={prefs.layout ? "position" : false}
            transition={{ layout: glide }}
            initial={{ opacity: 0, transform: shift(6) }}
            animate={{
              opacity: 1,
              transform: shift(0),
              transition: { ...enter(0.26), delay: first.current ? stagger(index) : 0 },
            }}
            exit={{ opacity: 0, transition: leave(0.2) }}
            className="flex items-start gap-3 py-[7px]"
          >
            <StatusGlyph status={row.status} />
            <div className="min-w-0 flex-1">
              <p
                className={cn(
                  "text-[13.5px] leading-5 transition-colors duration-200",
                  tone[row.status],
                )}
              >
                {row.text}
              </p>
              <AnimatePresence initial={false}>
                {row.note ? (
                  <motion.p
                    key="note"
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1, transition: enter(0.2) }}
                    exit={{ opacity: 0, transition: leave(0.16) }}
                    className="text-[12.5px] leading-[18px] text-ash-400"
                  >
                    {row.note}
                  </motion.p>
                ) : null}
              </AnimatePresence>
            </div>
            <AnimatePresence initial={false}>
              {row.status === "running" && typeof row.percent === "number" ? (
                <motion.span
                  key="percent"
                  initial={{ opacity: 0 }}
                  animate={{ opacity: 1, transition: enter(0.18) }}
                  exit={{ opacity: 0, transition: leave(0.14) }}
                  className="pt-px text-[13px]"
                >
                  <Percent value={row.percent} />
                </motion.span>
              ) : null}
            </AnimatePresence>
          </motion.li>
        ))}
      </AnimatePresence>
    </ul>
  );
}
