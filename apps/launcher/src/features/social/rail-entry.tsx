// The friends button in the side menu (the rail), for players who moved it there from the title
// bar pill's right-click menu or Settings → Görünüm. It sits above Settings like the rail's links:
// the friends icon, how many friends are online (a small jade count) and a gold mark while a
// request, an invite or a crewmate's call waits (its tooltip and name say which). Its right-click
// menu moves it back to the title bar.
import { UsersThree } from "@phosphor-icons/react";
import { cn, Tooltip } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { MovableMenu, openMenuFromKeyboard } from "@/shell/movable-menu";
import { usePlace } from "@/stores/placement";
import {
  EntryCue,
  EntryTip,
  entryLabel,
  moveMotion,
  openAction,
  useArrived,
  waitingWords,
} from "./entry";
import { onlineFriends, whatWaits } from "./model";
import { pop, swap, useSocialMotion } from "./motion";
import { useSocialPrefs, useSocialUi } from "./store";
import { useCallClock } from "./use-call-clock";

export function FriendsRailEntry() {
  const enabled = useSocialUi((s) => s.view.enabled);
  const place = usePlace("friends");
  return (
    <AnimatePresence initial={false}>
      {enabled && place === "rail" ? <RailEntry key="friends" /> : null}
    </AnimatePresence>
  );
}

function RailEntry() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const view = useSocialUi((s) => s.view);
  const favorites = useSocialPrefs((s) => s.favorites);
  const openPanel = useSocialUi((s) => s.openPanel);
  const panelOpen = useSocialUi((s) => s.open);
  const cue = useSocialUi((s) => s.entryCue);
  const arrived = useArrived();
  const trigger = useRef<HTMLButtonElement>(null);

  const signedIn = view.auth === "signedIn";
  const online = signedIn ? onlineFriends(view.friends, favorites).length : 0;
  const now = useCallClock((s) => s.now);
  const crewCalls = useSocialPrefs((s) => s.notifyCrewCalls);
  const chatMutes = useSocialPrefs((s) => s.chatMutes);
  // A request, an invite, a crewmate's call or an unread message waits for the player: the gold
  // mark, named in the label and the tooltip.
  const waiting = signedIn
    ? waitingWords(t, whatWaits(view, crewCalls, now, chatMutes), view.crews)
    : [];
  const pending = waiting.length > 0;
  const open = () => openPanel({ from: trigger.current });

  return (
    <motion.div className="flex" {...moveMotion(reduce, arrived)}>
      <MovableMenu
        id="friends"
        actions={[openAction(t, open)]}
        wrapTrigger={(menuTrigger) => (
          <Tooltip content={<EntryTip waiting={waiting} />} side="right">
            {menuTrigger}
          </Tooltip>
        )}
      >
        <button
          ref={trigger}
          type="button"
          aria-label={entryLabel(t, online, waiting)}
          aria-haspopup="dialog"
          data-movable="friends"
          data-place="rail"
          onClick={open}
          onKeyDown={(event) => openMenuFromKeyboard(event, "rail")}
          className="group relative flex size-[40px] cursor-pointer items-center justify-center rounded-lg outline-none transition-transform duration-150 ease-out focus-visible:ring-2 focus-visible:ring-gold-300 active:scale-[0.94]"
        >
          <span
            className={cn(
              "absolute inset-[3px] rounded-lg transition-colors duration-150",
              panelOpen ? "bg-white/8" : "group-hover:bg-white/6",
            )}
          />
          <EntryCue cue={cue} className="inset-[3px] rounded-lg" />
          {/* Moved here: it lights up once it has landed. */}
          <EntryCue cue={arrived ? 1 : 0} delay={0.42} className="inset-[3px] rounded-lg" />
          <UsersThree
            size={22}
            weight={panelOpen ? "duotone" : "regular"}
            className={cn(
              "relative transition-colors duration-150",
              panelOpen ? "text-gold-300" : "text-ash-400 group-hover:text-bone-100",
            )}
          />
          {/* Like the news link's mark: still, gold, fading in and out. */}
          <span
            aria-hidden
            className={cn(
              "absolute top-[6px] right-[6px] size-1.5 rounded-full bg-gold-400 transition-opacity duration-200 ease-out",
              pending ? "opacity-100" : "opacity-0",
            )}
          />
          <AnimatePresence initial={false}>
            {online ? (
              <motion.span
                key="online"
                aria-hidden
                className="absolute right-0 bottom-0.5 flex h-4 min-w-4 items-center justify-center overflow-hidden rounded-full bg-obsidian-950 px-1 font-display text-[11px] leading-none font-semibold text-jade-400 tabular-nums ring-1 ring-jade-400/45"
                {...pop(reduce)}
              >
                <AnimatePresence mode="popLayout" initial={false}>
                  <motion.span key={Math.min(online, 10)} {...swap(reduce, 0.18)}>
                    {online > 9 ? "9+" : online}
                  </motion.span>
                </AnimatePresence>
              </motion.span>
            ) : null}
          </AnimatePresence>
        </button>
      </MovableMenu>
    </motion.div>
  );
}
