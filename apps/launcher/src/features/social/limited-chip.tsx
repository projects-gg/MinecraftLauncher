// "Sınırlı hesap" (SPAM1): a quiet chip with a lock on the player's own card while their Discord
// account is new. Hovering or pressing it opens what is limited and until when ("Bu sınırlar
// kendiliğinden kalkar."). Nobody else ever sees it.
import { LockSimple } from "@phosphor-icons/react";
import { easeOutExpo } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { Popover } from "radix-ui";
import { useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useCalm } from "@/features/accounts/calm";
import type { SocialMe } from "@/lib/api/social-types";
import { dayMonth, isLimited } from "./limits";

/** Hovering opens it after a moment; leaving closes it a moment later (the pointer may be on its
 * way into it). */
const OPEN_MS = 180;
const CLOSE_MS = 220;

export function LimitedChip({ me }: { me: SocialMe }) {
  const { t } = useTranslation();
  const calm = useCalm();
  const [open, setOpen] = useState(false);
  const timer = useRef<number | null>(null);
  const limited = me.limited;
  if (!limited || !isLimited(me, Date.now())) return null;

  const later = (next: boolean, ms: number) => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = window.setTimeout(() => setOpen(next), ms);
  };
  const stay = () => {
    if (timer.current !== null) window.clearTimeout(timer.current);
    timer.current = null;
  };

  return (
    <Popover.Root open={open} onOpenChange={setOpen}>
      <Popover.Trigger asChild>
        <button
          type="button"
          onPointerEnter={(event) => {
            if (event.pointerType === "mouse") later(true, OPEN_MS);
          }}
          onPointerLeave={(event) => {
            if (event.pointerType === "mouse") later(false, CLOSE_MS);
          }}
          className="chamfer inline-flex h-[20px] shrink-0 cursor-pointer items-center gap-1 bg-white/9 px-[7px] text-[11.5px] font-semibold whitespace-nowrap text-ash-300 outline-none transition-colors duration-150 [--cut:5px] hover:bg-white/14 hover:text-bone-100 focus-visible:text-bone-100 data-[state=open]:bg-white/14 data-[state=open]:text-bone-100"
        >
          <LockSimple size={11} weight="bold" aria-hidden />
          {t("safety.limited.chip")}
        </button>
      </Popover.Trigger>
      <AnimatePresence>
        {open ? (
          <Popover.Portal forceMount>
            <Popover.Content
              forceMount
              asChild
              side="bottom"
              align="start"
              sideOffset={8}
              collisionPadding={16}
              onPointerEnter={stay}
              onPointerLeave={() => later(false, CLOSE_MS)}
              onOpenAutoFocus={(event) => event.preventDefault()}
            >
              <motion.div
                className="z-50 w-[300px] rounded-[var(--radius-panel)] border border-white/8 bg-obsidian-900/96 p-4 shadow-[inset_0_1px_0_rgb(255_255_255/0.06),0_24px_60px_rgb(34_6_6/0.55)] backdrop-blur-xl outline-none [html[data-perf=on]_&]:backdrop-blur-none"
                style={{ transformOrigin: "var(--radix-popover-content-transform-origin)" }}
                initial={calm({ opacity: 0, scale: 0.96, y: -4 })}
                animate={calm({
                  opacity: 1,
                  scale: 1,
                  y: 0,
                  transition: { duration: 0.16, ease: easeOutExpo },
                })}
                exit={calm({ opacity: 0, scale: 0.98, transition: { duration: 0.11 } })}
              >
                <p className="text-[13px] leading-snug font-semibold text-bone-50">
                  {t("safety.limited.title", { date: dayMonth(limited.until) })}
                </p>
                <ul className="mt-2.5 flex flex-col gap-1.5">
                  {limited.items.map((item) => (
                    <li key={item.key} className="flex items-start gap-2 text-[13px] text-bone-100">
                      <span
                        aria-hidden
                        className="mt-[7px] size-1 shrink-0 rounded-full bg-ash-400"
                      />
                      {item.perDay !== undefined
                        ? t(`safety.limited.items.${item.key}`, { count: item.perDay })
                        : t(`safety.limited.items.${item.key}`)}
                    </li>
                  ))}
                </ul>
                <p className="mt-3 text-[12px] text-ash-400">{t("safety.limited.lifts")}</p>
              </motion.div>
            </Popover.Content>
          </Popover.Portal>
        ) : null}
      </AnimatePresence>
    </Popover.Root>
  );
}
