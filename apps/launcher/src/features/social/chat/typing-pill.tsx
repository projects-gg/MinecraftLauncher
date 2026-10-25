// "Ayşe yazıyor" (CHAT2 §4): a small pill floating over the bottom left of the messages, just
// above the composer. It takes no room (nothing moves when it comes or goes), carries the
// writer's face, and its three dots breathe (opacity only, 1.2 s) while it shows and the window is
// in sight; under reduced motion and in Performance mode the words end in "…" and nothing moves.
// Screen readers are not told (it would be noise).
import { useLiveMotion } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import type { SocialProfile } from "@/lib/api/social-types";
import { enter, leave, useSocialMotion } from "../motion";
import { SocialFace } from "../social-face";
import { typingWords } from "./typing";

function Dots() {
  const live = useLiveMotion();
  const dots = useRef<HTMLSpanElement>(null);
  useEffect(() => {
    if (!live || !dots.current) return;
    const animations = [...dots.current.children].map((dot, index) =>
      dot.animate([{ opacity: 0.25 }, { opacity: 1 }, { opacity: 0.25 }], {
        duration: 1200,
        delay: index * 160,
        iterations: Number.POSITIVE_INFINITY,
        easing: "ease-in-out",
      }),
    );
    return () => {
      for (const animation of animations) animation.cancel();
    };
  }, [live]);
  if (!live) return <span>…</span>;
  return (
    <span ref={dots} className="ml-0.5 inline-flex items-end gap-[3px] pb-[3px]">
      <span className="size-[3px] rounded-full bg-ash-300 opacity-60" />
      <span className="size-[3px] rounded-full bg-ash-300 opacity-60" />
      <span className="size-[3px] rounded-full bg-ash-300 opacity-60" />
    </span>
  );
}

export function TypingPill({
  writers,
  people,
}: {
  /** Who writes now, in the order they started. */
  writers: readonly string[];
  people: (id: string) => Pick<SocialProfile, "name" | "avatar" | "avatarUrl">;
}) {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const words = typingWords(
    writers.map((id) => people(id).name),
    t,
  );
  const first = writers[0];
  return (
    <AnimatePresence initial={false}>
      {words && first ? (
        <motion.div
          key="typing"
          aria-hidden
          className="pointer-events-none absolute -top-7 left-2 z-10 flex h-6 max-w-[calc(100%-16px)] items-center gap-1.5 rounded-full bg-obsidian-800/95 py-1 pr-3 pl-[3px] text-[12px] text-ash-300 shadow-[0_8px_24px_rgb(34_6_6/0.45)] ring-1 ring-white/8"
          initial={{ opacity: 0, transform: reduce ? "none" : "translateY(4px)" }}
          animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.2) }}
          exit={{ opacity: 0, transition: leave(0.16) }}
        >
          {/* Round in the round pill, as close to its end as to its top (SMALL10b). */}
          <SocialFace source={people(first)} size={16} className="overflow-hidden rounded-full" />
          <span className="truncate">{words}</span>
          <Dots />
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
