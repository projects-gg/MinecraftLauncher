// The account center's link to the friends feature: while signed out it says so, "Discord ile giriş
// yapmadın" (a state, as its signed-in lines are: an action there read as a third way to add a game
// account, SMALL12), the player's friends profile and who is online once signed in. Opens the
// friends panel (the account center closes first). Hidden while the feature is off.
import { CaretRight } from "@phosphor-icons/react";
import { springSnappy } from "@pl/ui";
import { motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { DiscordTile } from "./discord-button";
import { onlineFriends } from "./model";
import { SocialAvatar } from "./social-face";
import { useSocialPrefs, useSocialUi } from "./store";

export function SocialAccountRow({ onOpen }: { onOpen: () => void }) {
  const { t } = useTranslation();
  const view = useSocialUi((s) => s.view);
  const favorites = useSocialPrefs((s) => s.favorites);
  const openPanel = useSocialUi((s) => s.openPanel);
  if (!view.enabled) return null;
  const me = view.auth === "signedIn" ? view.me : undefined;
  const online = me ? onlineFriends(view.friends, favorites).length : 0;
  return (
    <motion.div layout="position" transition={springSnappy}>
      <button
        type="button"
        onClick={() => {
          onOpen();
          // The account center slides out first; the friends panel follows.
          window.setTimeout(() => openPanel({ page: "list" }), 160);
        }}
        className="group flex min-h-14 w-full cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-3.5 py-2.5 text-left ring-1 ring-white/[0.07] transition-[background-color,box-shadow] duration-200 hover:bg-white/[0.05] hover:ring-white/12 focus-visible:ring-2 focus-visible:ring-gold-300 focus-visible:outline-none"
      >
        {me ? (
          <SocialAvatar
            source={me}
            size={36}
            state={me.status === "invisible" ? undefined : "launcher"}
            dnd={me.status === "dnd"}
          />
        ) : (
          <DiscordTile size={36} />
        )}
        <span className="min-w-0 flex-1">
          <span className="block text-[14px] font-medium text-bone-50">
            {t("social.account.row")}
          </span>
          <span className="block truncate text-[12.5px] text-ash-400">
            {me ? (
              online ? (
                <span className="text-jade-400">
                  {t("social.account.connected", { count: online })}
                </span>
              ) : (
                t("social.account.noneOnline")
              )
            ) : (
              t("social.account.signedOut")
            )}
          </span>
        </span>
        <CaretRight
          size={16}
          className="shrink-0 text-ash-400 transition-transform duration-150 group-hover:translate-x-0.5"
          aria-hidden
        />
      </button>
    </motion.div>
  );
}
