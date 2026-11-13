// The room chat's previews over the stream's stage (SQOL §1.1, §1.3; Discord's "Text Chat in
// Voice" shows the same while its panel is closed): while the stage is up and the room panel is not
// beside it (or the stage fills the window), others' new messages show at its top right, at most
// three, the newest at the bottom; each fades 6 s after it came, the pointer resting on them holds
// them, Esc takes them all away, and they never lie over the control that has the focus (WCAG
// 2.4.11: they fade out of the way). A press opens the panel at its chat. Screen readers hear the
// same from the room chat's host, gathered, so the previews themselves are out of their reading
// and out of the Tab order. "Mesaj Önizlemelerini Göster" (Ayarlar → Sesli Sohbet) turns them off.
//
// SQOL-U1's stage places `<RoomChatPeek />` in its top right corner. `PeekFallback` (voice chat's
// host) places them itself while no stage of the page has one.
import { cn } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { messageLine } from "@/features/social/chat/preview";
import { enter, glide, leave, useSocialMotion } from "@/features/social/motion";
import { SocialFace } from "@/features/social/social-face";
import { useWindowAway } from "@/lib/window-away";
import { useStageMode } from "../stage-mode";
import { useVoiceUi } from "../store";
import { usePanelChat } from "./panel-chat";
import { useRoomPeople } from "./people";
import { heldPeeks, livePeeks, nextPeekEnd } from "./rules";
import { usePeekSetting } from "./settings";
import { useRoomChat } from "./store";

/** How many stages place the previews now (the fallback stays away while one does). */
const usePeekHosts = create<{ hosts: number }>()(() => ({ hosts: 0 }));

/** The previews show now: over the stage, the panel not beside it, the setting on. */
function usePeekShown(): boolean {
  const room = useVoiceUi((s) => s.state.room);
  const panel = useVoiceUi((s) => s.panel);
  const stage = useStageMode();
  const on = usePeekSetting();
  const away = useWindowAway();
  const choice = usePanelChat(room);
  const over = stage === "page" || stage === "full";
  return on && over && !away && choice.kind === "room" && (!panel || stage === "full");
}

function overlaps(a: DOMRect, b: DOMRect): boolean {
  return a.left < b.right && b.left < a.right && a.top < b.bottom && b.top < a.bottom;
}

function PeekList() {
  const { t, i18n } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const peeks = useRoomChat((s) => s.peeks);
  const apply = useRoomChat((s) => s.apply);
  const shown = usePeekShown();
  const people = useRoomPeople();
  const list = useRef<HTMLDivElement>(null);
  const [resting, setResting] = useState<number | null>(null);
  const [inTheWay, setInTheWay] = useState(false);
  const live = shown && peeks.length > 0;

  // Each fades on its own time: one timer, for the next to go (none while the pointer rests).
  useEffect(() => {
    if (!peeks.length || resting !== null) return;
    const now = Date.now();
    const next = nextPeekEnd(peeks, now);
    const prune = () => apply((d) => ({ ...d, peeks: livePeeks(d.peeks, Date.now()) }));
    if (next === null) {
      prune();
      return;
    }
    const timer = window.setTimeout(prune, next - now + 16);
    return () => window.clearTimeout(timer);
  }, [peeks, resting, apply]);

  // Esc takes them away (it still does what it does elsewhere: nothing is stopped here).
  useEffect(() => {
    if (!live) return;
    const onKey = (event: KeyboardEvent) => {
      if (event.key !== "Escape" || event.defaultPrevented) return;
      apply((d) => (d.peeks.length ? { ...d, peeks: [] } : d));
    };
    document.addEventListener("keydown", onKey);
    return () => document.removeEventListener("keydown", onKey);
  }, [live, apply]);

  // Never over the control that has the focus: they fade out of its way while it is there (looked
  // at again when the focus moves and when the list changes its size).
  // biome-ignore lint/correctness/useExhaustiveDependencies: the list's length changes its box
  useEffect(() => {
    if (!live) {
      setInTheWay(false);
      return;
    }
    const check = () => {
      const box = list.current?.getBoundingClientRect();
      const focused = document.activeElement;
      if (!box || !(focused instanceof HTMLElement) || focused === document.body) {
        setInTheWay(false);
        return;
      }
      setInTheWay(overlaps(box, focused.getBoundingClientRect()));
    };
    check();
    document.addEventListener("focusin", check);
    document.addEventListener("focusout", check);
    return () => {
      document.removeEventListener("focusin", check);
      document.removeEventListener("focusout", check);
    };
  }, [live, peeks.length]);

  if (!shown) return null;
  return (
    <div
      ref={list}
      aria-hidden
      className={cn(
        "flex w-[300px] max-w-full flex-col items-stretch gap-1.5 transition-opacity duration-150",
        inTheWay && "pointer-events-none opacity-0",
      )}
      onPointerEnter={() => setResting(Date.now())}
      onPointerLeave={() => {
        const since = resting;
        setResting(null);
        if (since !== null) apply((d) => ({ ...d, peeks: heldPeeks(d.peeks, Date.now() - since) }));
      }}
    >
      <AnimatePresence initial={false}>
        {peeks.map((peek) => {
          const sender = people(peek.from);
          const line = messageLine(
            { text: peek.text, ...(peek.sticker ? { sticker: peek.sticker } : {}) },
            t,
            i18n.language,
          );
          return (
            <motion.button
              key={peek.key}
              type="button"
              tabIndex={-1}
              layout={layout ? "position" : false}
              transition={{ layout: glide }}
              initial={{ opacity: 0, transform: reduce ? "translateY(0px)" : "translateY(6px)" }}
              animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.22) }}
              exit={{ opacity: 0, transition: leave(0.16) }}
              onClick={() => useVoiceUi.getState().openPanel({ focus: "chat" })}
              className="pointer-events-auto flex w-full cursor-pointer items-start gap-2.5 rounded-[var(--radius-panel)] bg-obsidian-900/94 px-3 py-2 text-left shadow-[0_14px_36px_rgb(34_6_6/0.5)] ring-1 ring-white/8 transition-[background-color,transform,scale] duration-150 ease-out hover:bg-obsidian-800 active:scale-[0.98]"
            >
              <SocialFace
                source={sender}
                size={24}
                className="mt-0.5 shrink-0 overflow-hidden rounded-[var(--radius-head)]"
              />
              <span className="min-w-0 flex-1">
                <span className="block truncate text-[12.5px] leading-[1.3] font-semibold text-bone-50">
                  {sender.name}
                </span>
                <span className="block truncate text-[13px] leading-[1.4] text-bone-100">
                  {line}
                </span>
              </span>
            </motion.button>
          );
        })}
      </AnimatePresence>
    </div>
  );
}

/** The previews, for the stage's top right corner (SQOL §4: SQOL-U1 places it). */
export function RoomChatPeek() {
  useEffect(() => {
    usePeekHosts.setState((s) => ({ hosts: s.hosts + 1 }));
    return () => usePeekHosts.setState((s) => ({ hosts: s.hosts - 1 }));
  }, []);
  return <PeekList />;
}

/**
 * The previews while no stage places them (voice chat's host): at the stage's top right, under the
 * title bar (over the whole window when the stage fills it).
 */
export function PeekFallback() {
  const hosts = usePeekHosts((s) => s.hosts);
  const stage = useStageMode();
  if (hosts > 0 || (stage !== "page" && stage !== "full")) return null;
  return (
    <div
      className={cn(
        "pointer-events-none fixed right-5",
        stage === "full" ? "top-16 z-[61]" : "top-[calc(var(--spacing-titlebar)+64px)] z-[25]",
      )}
    >
      <PeekList />
    </div>
  );
}
