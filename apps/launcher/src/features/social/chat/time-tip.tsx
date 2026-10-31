// The full time of the message under the pointer (CHAT-UX: "28 Ekim 2026 Çarşamba 14:05"), one tip
// for the whole list rather than one per message. The pointer resting on a message as long as the
// launcher's tooltips wait shows it over the message, on its sender's side (under it at the list's
// top); the next message the pointer moves onto shows its own at once. It goes when the pointer
// leaves the message or reaches something with a tip or press of its own (a link, a quote, the
// toolbar), at a press, the wheel, a scroll, a key, and when the list changes. Nothing runs while
// nothing is under the pointer: one short wait for each message the pointer comes onto. Messages
// carry their time as `data-at` (message-row.tsx); a touch shows nothing.
import { AnimatePresence, motion } from "motion/react";
import { type RefObject, useEffect, useRef, useState } from "react";
import { createPortal } from "react-dom";
import { enter, leave } from "../motion";
import { fullTime } from "./chat-time";

/** As long as the launcher's tooltips wait (`TooltipProvider`); within this of one closing, at once. */
const WAIT_MS = 350;
const SKIP_MS = 150;
/** Room the tip needs over a message inside the list (px); with less it shows under the message. */
const ROOM = 34;
/** What has its own tip or press: the time does not show over it. */
const OWN = "a, button, [role='toolbar']";

interface Tip {
  at: number;
  mine: boolean;
  /** The message's edge on its sender's side and the tip's edge toward it (window px). */
  x: number;
  y: number;
  below: boolean;
}

export function TimeTip({ list, cue }: { list: RefObject<HTMLElement | null>; cue: unknown }) {
  const [tip, setTip] = useState<Tip | null>(null);
  const current = useRef<HTMLElement | null>(null);
  const hideNow = useRef<() => void>(() => {});

  useEffect(() => {
    const element = list.current;
    if (!element) return;
    let wait = 0;
    let closedAt = Number.NEGATIVE_INFINITY;
    const hide = () => {
      window.clearTimeout(wait);
      if (current.current) closedAt = performance.now();
      current.current = null;
      setTip(null);
    };
    hideNow.current = hide;
    const show = (message: HTMLElement) => {
      const at = Number(message.dataset.at);
      if (!Number.isFinite(at) || !message.isConnected) return;
      const box = message.getBoundingClientRect();
      const below = box.top - element.getBoundingClientRect().top < ROOM;
      const mine = message.dataset.mine !== undefined;
      setTip({
        at,
        mine,
        x: mine ? box.right : box.left,
        y: below ? box.bottom + 6 : box.top - 6,
        below,
      });
    };
    const onOver = (event: PointerEvent) => {
      if (event.pointerType !== "mouse") return;
      const target = event.target instanceof Element ? event.target : null;
      const message = target?.closest<HTMLElement>("[data-at]") ?? null;
      if (!message || target?.closest(OWN)) {
        if (current.current) hide();
        return;
      }
      if (message === current.current) return;
      const quick = current.current !== null || performance.now() - closedAt < SKIP_MS;
      window.clearTimeout(wait);
      current.current = message;
      setTip(null);
      wait = window.setTimeout(() => show(message), quick ? 0 : WAIT_MS);
    };
    const onLeave = () => {
      if (current.current) hide();
    };
    element.addEventListener("pointerover", onOver);
    element.addEventListener("pointerleave", onLeave);
    element.addEventListener("pointerdown", hide);
    element.addEventListener("wheel", hide, { passive: true });
    element.addEventListener("scroll", hide, { passive: true });
    window.addEventListener("keydown", hide);
    window.addEventListener("blur", hide);
    return () => {
      window.clearTimeout(wait);
      element.removeEventListener("pointerover", onOver);
      element.removeEventListener("pointerleave", onLeave);
      element.removeEventListener("pointerdown", hide);
      element.removeEventListener("wheel", hide);
      element.removeEventListener("scroll", hide);
      window.removeEventListener("keydown", hide);
      window.removeEventListener("blur", hide);
    };
  }, [list]);

  // A message came, went or grew: the rows may have moved under the tip.
  // biome-ignore lint/correctness/useExhaustiveDependencies: `cue` changes with the list
  useEffect(() => hideNow.current(), [cue]);

  return createPortal(
    <AnimatePresence>
      {tip ? (
        <motion.div
          key={`${tip.at}:${tip.x}:${tip.y}`}
          aria-hidden
          className="glass-strong pointer-events-none fixed z-50 rounded-md px-2 py-1 text-[12px] font-medium whitespace-nowrap text-bone-100 tabular-nums shadow-[0_12px_32px_rgb(34_6_6/0.45)]"
          style={{
            left: tip.x,
            top: tip.y,
            translate: `${tip.mine ? "-100%" : "0"} ${tip.below ? "0" : "-100%"}`,
          }}
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.14) }}
          exit={{ opacity: 0, transition: leave(0.1) }}
        >
          {fullTime(tip.at)}
        </motion.div>
      ) : null}
    </AnimatePresence>,
    document.body,
  );
}
