// Pieces of a desktop AFK account's card that a 7/24 Bulut card also shows while the same account
// runs on this computer (AFK-CLEAR: one card per account): its health, food and players, and the
// row under the state line while its client is fetched or could not be had.
import { ForkKnife, Heart, UsersThree } from "@phosphor-icons/react";
import { enter, leave, Progress, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import type { AfkAccount } from "@/lib/api";

/**
 * Under the state line while the account's client is fetched (its progress) or after it could not
 * be had (what to do). One slot: the outgoing line fades where it was while the new one takes its
 * place.
 */
export function ClientRow({ account }: { account: AfkAccount }) {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const { state } = account;
  const from = reduce ? "translateY(0px)" : "translateY(3px)";
  return (
    <AnimatePresence initial={false} mode="popLayout">
      {state.kind === "preparing" ? (
        <motion.div
          key="progress"
          className="mt-2.5"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.22) }}
          exit={{ opacity: 0, transition: leave(0.16) }}
        >
          <Progress
            value={state.percent}
            label={t(
              state.step === "downloading" ? "afkConnect.downloading" : "afkConnect.preparing",
            )}
            className="h-1"
          />
        </motion.div>
      ) : state.kind === "clientFailed" ? (
        <motion.p
          key={`hint-${state.problem}`}
          // Said once when it appears; the state line above already shows what happened.
          role="status"
          className="mt-1.5 pl-[23px] text-[12px] leading-snug text-ash-300"
          initial={{ opacity: 0, transform: from }}
          animate={{ opacity: 1, transform: "translateY(0px)", transition: enter(0.24) }}
          exit={{ opacity: 0, transition: leave(0.16) }}
        >
          <span className="sr-only">{t(`afkConnect.problem.${state.problem}.title`)}. </span>
          {t(`afkConnect.problem.${state.problem}.hint`)}
        </motion.p>
      ) : null}
    </AnimatePresence>
  );
}

/** Health, food and players of a connected account, beside its state (no extra row). */
export function Stats({ account }: { account: AfkAccount }) {
  const { t } = useTranslation();
  const stats = [
    {
      key: "health",
      label: t("afk.stats.health"),
      value: account.health,
      icon: <Heart size={14} weight="fill" className="text-flare-400" />,
    },
    {
      key: "food",
      label: t("afk.stats.food"),
      value: account.food,
      icon: <ForkKnife size={14} weight="fill" className="text-gold-400" />,
    },
    {
      key: "players",
      label: t("afk.stats.players"),
      value: account.players,
      icon: <UsersThree size={14} weight="fill" className="text-ash-300" />,
    },
  ];
  return (
    <span className="flex shrink-0 items-center gap-3 text-[12.5px] text-ash-300 tabular-nums">
      {stats.map((stat) => (
        <span key={stat.key} className="flex items-center gap-1" title={stat.label}>
          <span aria-hidden className="flex">
            {stat.icon}
          </span>
          <span className="sr-only">{stat.label}</span>
          {stat.value ?? "-"}
        </span>
      ))}
    </span>
  );
}

/** The stats of a connected account, fading in and out beside its state line. */
export function LiveStats({ account }: { account: AfkAccount }) {
  const connected = account.state.kind === "connected";
  return (
    <AnimatePresence initial={false}>
      {connected ? (
        <motion.span
          key="stats"
          className="flex"
          initial={{ opacity: 0 }}
          animate={{ opacity: 1, transition: enter(0.22) }}
          exit={{ opacity: 0, transition: leave(0.16) }}
        >
          <Stats account={account} />
        </motion.span>
      ) : null}
    </AnimatePresence>
  );
}
