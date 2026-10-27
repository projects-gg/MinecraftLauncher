// The status strip above the smart console (CAFK-FEAT §3.2): what the server keeps on screen (the
// action bar, a title, boss bars) in place, never as log lines. Updated by the machine's `bars`
// frames; a text leaves when its time is up (one timer for the next expiry, none while nothing
// shows). Motion: fades and the bar's fill (transform), nothing idle.
import { cn, enter, leave } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { CloudBars } from "@/bindings";

/** The game's boss bar colours, a step quieter on our dark panel. */
const BOSS: Record<string, string> = {
  pink: "#e58fb8",
  blue: "#6fa8e8",
  red: "#e06a5a",
  green: "#6cc48a",
  yellow: "#e8c25a",
  purple: "#a983e0",
  white: "#d9d4de",
};

const fade = {
  initial: { opacity: 0 },
  animate: { opacity: 1, transition: enter(0.18) },
  exit: { opacity: 0, transition: leave(0.14) },
};

export function ConsoleStrip({
  bars,
  skewMs,
}: {
  bars: CloudBars | null;
  /** The service's clock minus this computer's: the machine's times are the service's. */
  skewMs: number;
}) {
  const { t } = useTranslation();
  const [now, setNow] = useState(() => Date.now() + skewMs);
  const actionbar = bars?.actionbar && (bars.actionbar.untilMs ?? 0) > now ? bars.actionbar : null;
  const title = bars?.title && (bars.title.untilMs ?? 0) > now ? bars.title : null;
  const boss = bars?.boss ?? [];

  // Wake once, when the next text's time is up.
  useEffect(() => {
    const at = Date.now() + skewMs;
    const ends = [actionbar?.untilMs, title?.untilMs].filter(
      (until): until is number => typeof until === "number" && until > at,
    );
    if (!ends.length) return;
    const id = window.setTimeout(() => setNow(Date.now() + skewMs), Math.min(...ends) - at + 20);
    return () => window.clearTimeout(id);
  }, [actionbar?.untilMs, title?.untilMs, skewMs]);

  // A new strip may bring texts that are already due: look at the clock again.
  // biome-ignore lint/correctness/useExhaustiveDependencies: re-reads the clock for each strip
  useEffect(() => {
    setNow(Date.now() + skewMs);
  }, [bars, skewMs]);

  const empty = !actionbar && !title && boss.length === 0;
  return (
    <AnimatePresence initial={false}>
      {empty ? null : (
        <motion.section
          key="strip"
          aria-label={t("cloudAfkFeat.console.strip")}
          className="mx-5 mb-1 flex flex-col gap-2 rounded-md bg-white/[0.035] px-3 py-2.5 ring-1 ring-white/6"
          {...fade}
        >
          <AnimatePresence initial={false}>
            {title ? (
              <motion.p key={`t:${title.text}`} className="min-w-0" {...fade}>
                <span className="block truncate font-display text-[14px] font-semibold text-bone-50">
                  {title.text}
                </span>
                {title.subtitle ? (
                  <span className="block truncate text-[12.5px] text-ash-300">
                    {title.subtitle}
                  </span>
                ) : null}
              </motion.p>
            ) : null}
            {actionbar ? (
              <motion.p
                key="actionbar"
                className="truncate text-[12.5px] text-bone-100"
                title={actionbar.text}
                {...fade}
              >
                {actionbar.text}
              </motion.p>
            ) : null}
            {boss.map((bar) => {
              const progress = Math.min(1, Math.max(0, bar.progress ?? 0));
              return (
                <motion.div key={bar.id} className="flex flex-col gap-1" {...fade}>
                  <span className="truncate text-[12.5px] text-bone-100" title={bar.text}>
                    {bar.text}
                  </span>
                  <span
                    aria-hidden
                    className="relative h-[3px] overflow-hidden rounded-full bg-white/8"
                  >
                    <span
                      className={cn(
                        "absolute inset-0 origin-left rounded-full opacity-80",
                        "transition-transform duration-300 ease-out",
                      )}
                      style={{
                        background: BOSS[bar.color ?? ""] ?? BOSS.white,
                        transform: `scaleX(${progress})`,
                      }}
                    />
                  </span>
                </motion.div>
              );
            })}
          </AnimatePresence>
        </motion.section>
      )}
    </AnimatePresence>
  );
}
