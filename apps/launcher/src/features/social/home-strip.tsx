// Home screen: friends in game, in one quiet line over the art, with "Katıl" when some of them are
// on projects.gg. Clicking the faces or the text opens the friends panel. Nothing shows while no
// friend plays (or the feature is off).
import { cn } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { JoinButton } from "./friend-row";
import { groupFriends, isJoinable } from "./model";
import { enter, leave, useSocialMotion } from "./motion";
import { SocialFace } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";

export function FriendsInGameStrip() {
  const { t } = useTranslation();
  const { reduce } = useSocialMotion();
  const view = useSocialUi((s) => s.view);
  const favorites = useSocialPrefs((s) => s.favorites);
  const openPanel = useSocialUi((s) => s.openPanel);
  const playing =
    view.enabled && view.auth === "signedIn" ? groupFriends(view.friends, favorites).playing : [];
  const onProjects = playing.filter((f) => isJoinable(f.game));
  // projects.gg first: that is where "Katıl" leads.
  const shown = onProjects.length ? onProjects : playing;
  const first = shown[0];
  const others = shown.length - 1;
  const where = onProjects.length ? "projects" : "playing";
  const text = first
    ? others
      ? t(`social.home.${where}With`, { name: first.name, count: others })
      : t(`social.home.${where}Alone`, { name: first.name })
    : "";

  return (
    <AnimatePresence initial={false}>
      {first ? (
        <motion.div
          key="strip"
          className="glass flex w-fit max-w-[560px] items-center gap-3 rounded-full py-1.5 pr-1.5 pl-1.5"
          initial={{ opacity: 0, transform: reduce ? "none" : "translateY(8px)" }}
          animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.42) }}
          exit={{
            opacity: 0,
            transform: reduce ? "none" : "translateY(4px)",
            transition: leave(0.3),
          }}
        >
          <button
            type="button"
            aria-label={`${text}. ${t("social.home.open")}`}
            onClick={() => openPanel({ page: "list" })}
            className="flex min-w-0 cursor-pointer items-center gap-3 rounded-full py-0.5 pr-2 pl-0.5 outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
          >
            <span className="flex items-center">
              {shown.slice(0, 4).map((friend, index) => (
                <span
                  key={friend.id}
                  className="relative flex rounded-[5px] ring-2 ring-obsidian-900"
                  style={{ marginLeft: index ? -8 : 0, zIndex: 4 - index }}
                >
                  <SocialFace source={friend} size={26} />
                </span>
              ))}
            </span>
            <span className={cn("truncate text-[13.5px] font-medium text-bone-50")}>{text}</span>
          </button>
          {onProjects.length ? <JoinButton /> : null}
        </motion.div>
      ) : null}
    </AnimatePresence>
  );
}
