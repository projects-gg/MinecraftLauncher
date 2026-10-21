// What the friends button shares in both of its places, the title bar (friends-pill.tsx) and the
// side menu (rail-entry.tsx): its name for screen readers, the action its right-click menu leads
// with, the one-time glow ("here it is") and how it moves between places.
import { UsersThree } from "@phosphor-icons/react";
import { cn, type MenuItem } from "@pl/ui";
import type { TFunction } from "i18next";
import { motion } from "motion/react";
import { useState } from "react";
import { useTranslation } from "react-i18next";
import type { SocialTeam } from "@/lib/api/social-types";
import { justMoved } from "@/stores/placement";
import { callSentenceOf } from "./crews";
import type { Waiting } from "./model";
import { enter, leave } from "./motion";

/**
 * What the gold mark stands for, in words: "Seni bekleyen istek ya da davet var" for a request or
 * an invite, "Kaan ekibi çağırıyor" for a crewmate's call (a call is neither; E2E6), "2 okunmamış
 * mesaj" for direct messages (CHAT1). Empty while nothing waits.
 */
export function waitingWords(
  t: TFunction,
  waiting: Waiting,
  crews: readonly SocialTeam[],
): string[] {
  const words: string[] = [];
  if (waiting.asks) words.push(t("social.pill.pending"));
  if (waiting.call) words.push(callSentenceOf(waiting.call, crews, t));
  if (waiting.messages > 0) words.push(t("chat.waiting", { count: waiting.messages }));
  return words;
}

/** "Arkadaşlar, 3 arkadaşın çevrimiçi, Kaan ekibi çağırıyor". */
export function entryLabel(t: TFunction, online: number, waiting: readonly string[]): string {
  return [t("social.pill.label"), online ? t("social.pill.online", { count: online }) : null]
    .concat(waiting)
    .filter(Boolean)
    .join(", ");
}

/**
 * The entry's tooltip: its name and, quieter under it, what the gold mark stands for, so a pointer
 * learns what a screen reader hears without opening the panel.
 */
export function EntryTip({ waiting }: { waiting: readonly string[] }) {
  const { t } = useTranslation();
  return (
    <span className="block">
      {t("social.pill.label")}
      {waiting.map((line) => (
        <span key={line} className="block font-normal text-ash-300">
          {line}
        </span>
      ))}
    </span>
  );
}

/** The right-click menu leads with what a click does, as on rows and tiles. */
export function openAction(t: TFunction, open: () => void): MenuItem {
  return { label: t("social.home.open"), icon: <UsersThree size={16} />, onSelect: open };
}

/**
 * "Here it is": a gold ring around the button fades in and out once, when a Home invitation is
 * closed and when the button arrives at a new place. Opacity only, so it plays under reduced
 * motion too; a new cue plays it again.
 */
export function EntryCue({
  cue,
  delay = 0.12,
  className,
}: {
  cue: number;
  delay?: number;
  className?: string;
}) {
  if (!cue) return null;
  return (
    <motion.span
      key={cue}
      aria-hidden
      className={cn(
        "pointer-events-none absolute shadow-[0_0_0_2px_var(--color-gold-300),0_0_18px_rgb(240_184_73/0.45)]",
        className,
      )}
      initial={{ opacity: 0 }}
      animate={{ opacity: [0, 1, 0] }}
      transition={{ duration: 1.5, times: [0, 0.18, 1], ease: "easeOut", delay }}
    />
  );
}

/** The button arrived here by a move (read once, when it mounts): it lights up after landing. */
export function useArrived(): boolean {
  const [arrived] = useState(() => justMoved("friends"));
  return arrived;
}

/**
 * Leaving a place: a quick fade as it shrinks a little. Arriving at the new one, a beat later:
 * a fade as it grows to full size, so the eye follows it across. Only a move animates (at
 * start-up the button is simply there); reduced motion keeps the fades.
 */
export function moveMotion(reduce: boolean, arrived: boolean) {
  // Reduced motion keeps "scale(1)", not "none": motion fills a missing scale with 0, so an exit
  // to "none" ended at scale(0) and the button vanished instead of fading (measured).
  const away = { opacity: 0, transform: reduce ? "scale(1)" : "scale(0.86)" };
  return {
    initial: arrived ? away : false,
    animate: { opacity: 1, transform: "scale(1)", transition: { ...enter(0.32), delay: 0.14 } },
    exit: { ...away, transition: leave(0.22) },
  };
}
