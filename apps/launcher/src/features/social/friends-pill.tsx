// Title bar entry of the friends panel: the faces of friends who are online and how many, with a
// gold mark while a request or an invite waits. Signed out it is just the friends icon. Hidden
// entirely while the feature is off.
import { UsersThree } from "@phosphor-icons/react";
import { cn, Tooltip } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useRef } from "react";
import { useTranslation } from "react-i18next";
import { onlineFriends } from "./model";
import { enter, glide, leave, pop, swap, useSocialMotion } from "./motion";
import { SocialFace } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";

export function FriendsPill() {
  const { t } = useTranslation();
  const { reduce, layout } = useSocialMotion();
  const view = useSocialUi((s) => s.view);
  const favorites = useSocialPrefs((s) => s.favorites);
  const openPanel = useSocialUi((s) => s.openPanel);
  const trigger = useRef<HTMLButtonElement>(null);
  if (!view.enabled) return null;

  const signedIn = view.auth === "signedIn";
  const online = signedIn ? onlineFriends(view.friends, favorites) : [];
  const faces = online.slice(0, 3);
  const pending = signedIn && (view.incoming.length > 0 || view.invites.length > 0);
  const label = [
    t("social.pill.label"),
    online.length ? t("social.pill.online", { count: online.length }) : null,
    pending ? t("social.pill.pending") : null,
  ]
    .filter(Boolean)
    .join(", ");

  return (
    <Tooltip content={t("social.pill.label")}>
      <button
        ref={trigger}
        type="button"
        aria-label={label}
        onClick={() => openPanel({ from: trigger.current })}
        className="group relative flex h-titlebar cursor-pointer items-center px-0.5 outline-none after:absolute after:inset-x-0 after:top-0 after:-bottom-1"
      >
        <span
          className={cn(
            "relative flex h-8 items-center gap-2 rounded-full bg-obsidian-950/45 ring-1 ring-white/8 backdrop-blur-md transition-[background-color,box-shadow,transform,scale] duration-150",
            "group-hover:bg-obsidian-950/60 group-hover:ring-white/16 group-active:scale-[0.97] group-focus-visible:ring-2 group-focus-visible:ring-gold-300",
            faces.length ? "pr-3 pl-1" : "px-2.5",
          )}
        >
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
    </Tooltip>
  );
}
