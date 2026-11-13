// Screen sharing on the room panel (SHARE; share.md §3): above the panel's three buttons,
// "Ekranını paylaş" while the player shares nothing; while they share, each share's card (SQOL:
// own-card.tsx: its live picture, what it shows, its quality, who watches, "Ekranı Değiştir", "⋯"
// and "Durdur"), calm (no red, no light). A friend room's leader picks "Kim paylaşabilir" in the
// panel's header menu (`useShareRuleItems`; the footer keeps only the player's own sharing).
// Beside each person who shares, "İzle" (their "YAYINDA" is live-card.tsx's).
import { Monitor } from "@phosphor-icons/react";
import { Button, cn, type MenuItem, toast, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useTranslation } from "react-i18next";
import { shareApi } from "@/share/api";
import type { ShareBy, VoiceLiveShare } from "@/share/types";
import { watchShare } from "./actions";
import { OwnShareCard } from "./own-card";
import { openNewShare } from "./own-store";
import { useShare } from "./store";

/**
 * Above the panel's buttons: the player's shares, and "Ekranını paylaş" (no props: the room
 * panel's footer renders it as it is).
 */
export function ShareFooterRow() {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const share = useShare();
  if (!share.on || (!share.canShare && !share.mine.length)) return null;
  const more = share.canShare && share.mine.length < share.mineMax;
  // Why "Ekranını paylaş" waits, in one sentence (none: it does not).
  const why = !share.allowed
    ? t("share.notice.notAllowed")
    : !share.open
      ? t("share.notice.off")
      : null;
  if (!share.mine.length && !more) return null;
  return (
    <div className="mb-2 flex flex-col gap-1.5">
      <AnimatePresence initial={false}>
        {share.mine.map((mine) => (
          <motion.div
            key={mine.n}
            initial={{ opacity: 0, transform: `translateY(${reduce ? 0 : 4}px)` }}
            animate={{
              opacity: 1,
              transform: "translateY(0px)",
              transition: { duration: 0.22, ease: [0.16, 1, 0.3, 1] },
            }}
            exit={{ opacity: 0, transition: { duration: 0.14 } }}
          >
            <OwnShareCard mine={mine} />
          </motion.div>
        ))}
      </AnimatePresence>
      {more ? (
        <>
          <Button
            size={share.mine.length ? "sm" : "lg"}
            variant={share.mine.length ? "ghost" : "secondary"}
            icon={<Monitor size={17} weight="bold" />}
            aria-disabled={why !== null || undefined}
            className={cn("w-full", why !== null && "opacity-45")}
            onClick={why === null ? openNewShare : undefined}
          >
            {t("share.action")}
          </Button>
          {/* Why it waits, where the eye already is (a tooltip only reaches a pointer). */}
          {why ? <p className="px-1 text-center text-[12px] text-ash-400">{why}</p> : null}
        </>
      ) : null}
    </div>
  );
}

/** The same row under the name the SQOL notes give it (the room panel's footer, SQOL-U3). */
export const SharePanel = ShareFooterRow;

/**
 * "Kim Paylaşabilir" › "Herkes" / "Lider ve Moderatörler" for the room panel's header menu
 * (SQOL-U3 places it): a friend room's leader only; nothing elsewhere. The room checks the role.
 */
export function useShareRuleItems(): MenuItem[] {
  const { t } = useTranslation();
  const share = useShare();
  if (!share.on || !share.canSetShareBy) return [];
  const set = (who: ShareBy) => {
    if (who === share.shareBy) return;
    void shareApi
      .setSharers(who)
      .then((done) => {
        if (!done) toast(t("share.notice.failed"));
      })
      .catch(() => toast(t("share.notice.failed")));
  };
  return [
    {
      label: t("share.who.label"),
      icon: <Monitor size={16} />,
      onSelect: () => {},
      items: (["all", "mods"] as const).map((who) => ({
        label: t(`share.who.${who}`),
        radio: true,
        checked: share.shareBy === who,
        onSelect: () => set(who),
      })),
    },
  ];
}

/** A person's first share the player does not watch yet. */
export function unwatched(shares: VoiceLiveShare[], watching: { person: string; n: number }[]) {
  return shares.find((s) => !watching.some((w) => w.person === s.person && w.n === s.n)) ?? null;
}

/** "İzle" beside a person who shares something the player does not watch. (Who shares is said by
 * "YAYINDA" after their name: live-card.tsx; SHARE-LIVE merged the small mark into it.) */
export function WatchButton({
  person,
  name,
  className,
}: {
  person: string;
  name: string;
  className?: string;
}) {
  const { t } = useTranslation();
  const share = useShare();
  const next = unwatched(
    share.live.filter((l) => l.person === person),
    share.watching,
  );
  if (!share.on || !next) return null;
  return (
    <Button
      size="sm"
      variant="secondary"
      aria-label={t("share.row.watchLabel", { name })}
      className={className}
      onClick={() => void watchShare(next.person, next.n)}
    >
      {t("share.row.watch")}
    </Button>
  );
}
