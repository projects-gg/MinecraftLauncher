// The friends button in the title bar (its home; the player can move it to the side menu from its
// right-click menu or Settings → Görünüm, see rail-entry.tsx). Signed in: the faces of friends who
// are online and how many, with a gold mark while a request, an invite or a crewmate's call waits
// (its tooltip and name say which; the icon alone while nobody is on). Before sign-in it also
// carries its name, so the feature is found at a glance. Hidden entirely while the feature is off.
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
import { enter, glide, leave, pop, swap, useSocialMotion } from "./motion";
import { SocialFace } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";
import { useCallClock } from "./use-call-clock";

export function FriendsPill() {
  const enabled = useSocialUi((s) => s.view.enabled);
  const place = usePlace("friends");
  return (
    <AnimatePresence initial={false}>
      {enabled && place === "titlebar" ? <Pill key="pill" /> : null}
    </AnimatePresence>
  );
}

function Pill() {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const view = useSocialUi((s) => s.view);
  const favorites = useSocialPrefs((s) => s.favorites);
  const openPanel = useSocialUi((s) => s.openPanel);
  const cue = useSocialUi((s) => s.entryCue);
  const arrived = useArrived();
  const trigger = useRef<HTMLButtonElement>(null);

  const signedIn = view.auth === "signedIn";
  // Not signed in yet (a saved session being checked is not: it is about to be signed in).
  const named = view.auth === "signedOut" || view.auth === "signingIn";
  const online = signedIn ? onlineFriends(view.friends, favorites) : [];
  const faces = online.slice(0, 3);
  const now = useCallClock((s) => s.now);
  const crewCalls = useSocialPrefs((s) => s.notifyCrewCalls);
  // A request, an invite or a crewmate's call waits for the player: the gold mark, named in the
  // label and the tooltip.
  const waiting = signedIn ? waitingWords(t, whatWaits(view, crewCalls, now), view.crews) : [];
  const pending = waiting.length > 0;
  const open = () => openPanel({ from: trigger.current });

  return (
    <motion.div className="flex" {...moveMotion(reduce, arrived)}>
      <MovableMenu
        id="friends"
        actions={[openAction(t, open)]}
        // The name is on the pill while signed out: no tooltip repeating it.
        wrapTrigger={(menuTrigger) => (
          <Tooltip content={<EntryTip waiting={waiting} />} disabled={named}>
            {menuTrigger}
          </Tooltip>
        )}
      >
        <button
          ref={trigger}
          type="button"
          aria-label={entryLabel(t, online.length, waiting)}
          aria-haspopup="dialog"
          data-movable="friends"
          data-place="titlebar"
          onClick={open}
          onKeyDown={(event) => openMenuFromKeyboard(event, "titlebar")}
          className="group relative flex h-titlebar cursor-pointer items-center px-0.5 outline-none after:absolute after:inset-x-0 after:top-0 after:-bottom-1"
        >
          <span
            className={cn(
              "relative flex h-8 items-center gap-2 rounded-full bg-obsidian-950/45 ring-1 ring-white/8 backdrop-blur-md transition-[background-color,box-shadow,transform,scale] duration-150",
              "group-hover:bg-obsidian-950/60 group-hover:ring-white/16 group-active:scale-[0.97] group-focus-visible:ring-2 group-focus-visible:ring-gold-300",
              faces.length ? "pr-3 pl-1" : named ? "pr-3 pl-2.5" : "px-2.5",
            )}
          >
            <EntryCue cue={cue} className="-inset-px rounded-full" />
            {/* Moved here: it lights up once it has landed. */}
            <EntryCue cue={arrived ? 1 : 0} delay={0.42} className="-inset-px rounded-full" />
            {faces.length ? (
              <span className="flex items-center">
                <AnimatePresence initial={false} mode="popLayout">
                  {faces.map((friend, index) => (
                    <motion.span
                      key={friend.id}
                      layout={layout ? "position" : false}
                      transition={{ layout: glide }}
                      className="relative flex rounded-[5px] ring-2 ring-obsidian-950"
                      style={{ marginLeft: index ? -7 : 0, zIndex: faces.length - index }}
                      // Layout glides compose with `scale` (not a transform string).
                      initial={{ opacity: 0, scale: reduce ? 1 : 0.85 }}
                      animate={{ opacity: 1, scale: 1, transition: enter(0.22) }}
                      exit={{ opacity: 0, scale: reduce ? 1 : 0.85, transition: leave(0.22) }}
                    >
                      <SocialFace source={friend} size={22} />
                    </motion.span>
                  ))}
                </AnimatePresence>
              </span>
            ) : (
              <UsersThree
                size={17}
                className="text-ash-300 transition-colors group-hover:text-bone-50"
                aria-hidden
              />
            )}
            {named ? (
              <span className="text-[13px] font-medium whitespace-nowrap text-bone-100 transition-colors group-hover:text-bone-50">
                {t("social.pill.label")}
              </span>
            ) : null}
            {online.length ? (
              <span className="relative inline-flex font-display text-[13px] font-semibold text-bone-50 tabular-nums">
                <AnimatePresence mode="popLayout" initial={false}>
                  <motion.span key={online.length} {...swap(reduce, 0.18)}>
                    {online.length}
                  </motion.span>
                </AnimatePresence>
              </span>
            ) : null}
            <AnimatePresence initial={false}>
              {pending ? (
                <motion.span
                  key="pending"
                  aria-hidden
                  className="absolute -top-0.5 -right-0.5 size-2.5 rounded-full bg-gold-400 ring-2 ring-obsidian-950"
                  {...pop(reduce)}
                />
              ) : null}
            </AnimatePresence>
          </span>
        </button>
      </MovableMenu>
    </motion.div>
  );
}
