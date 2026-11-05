// Screen sharing on the room panel (SHARE; share.md §3): above the panel's three buttons,
// "Ekranını paylaş" while the player shares nothing; while they share, each share's line says
// "Yayında" calmly (no red, no light), what it shows, who watches (their faces; their names on the
// pointer) and "Durdur". Beside each person who shares, their mark and "İzle".
import { CaretDown, Check, Monitor, Screencast } from "@phosphor-icons/react";
import { Button, cn, Menu, type MenuItem, Tooltip, toast, useMotionPrefs } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { useEffect } from "react";
import { useTranslation } from "react-i18next";
import { SpeakingFace } from "@/features/voice/faces";
import { useVoicePrefs } from "@/features/voice/prefs";
import { useFaces, useVoiceUi } from "@/features/voice/store";
import { shareApi } from "@/share/api";
import type { ShareBy, VoiceLiveShare, VoiceMyShare } from "@/share/types";
import { stopShare, unwatchShare, watchShare } from "./actions";
import { useShare, useShareUi } from "./store";
import { ShareVideo } from "./video";

/** The player's own picture, small, while "Kendi önizlememi göster" is on. */
function SelfPreview({ me, n }: { me: string; n: number }) {
  const { t } = useTranslation();
  useEffect(() => {
    void watchShare(me, n);
    return () => void unwatchShare(me, n);
  }, [me, n]);
  return (
    <div
      className="aspect-video w-[96px] shrink-0 overflow-hidden rounded-[6px] bg-obsidian-950 ring-1 ring-white/10"
      aria-label={t("share.live.preview")}
      role="img"
    >
      <ShareVideo person={me} n={n} volume={0} muted fit="cover" />
    </div>
  );
}

function Watchers({ ids }: { ids: string[] }) {
  const { t } = useTranslation();
  const faces = useFaces(ids);
  if (!ids.length) {
    return <span className="text-[12.5px] text-ash-400">{t("share.live.resting")}</span>;
  }
  const names = faces.map((f) => f.name).join(", ");
  return (
    <Tooltip content={t("share.live.watchedBy", { names })}>
      <span className="flex items-center gap-2">
        <span className="sr-only">{t("share.live.watchedBy", { names })}</span>
        <span className="flex -space-x-1.5">
          {faces.slice(0, 3).map((face) => (
            <SpeakingFace
              key={face.id}
              face={face}
              size={20}
              round
              speaking={false}
              className="ring-2 ring-obsidian-900"
            />
          ))}
        </span>
        <span className="text-[12.5px] whitespace-nowrap text-ash-300">
          {t("share.live.watchers", { count: ids.length })}
        </span>
      </span>
    </Tooltip>
  );
}

/** One of the player's shares. */
function LiveLine({ mine, me, preview }: { mine: VoiceMyShare; me: string; preview: boolean }) {
  const { t } = useTranslation();
  const live = mine.phase !== "starting";
  const what =
    mine.kind === "game" ? t("share.kind.game") : mine.label || t(`share.kind.${mine.kind}`);
  return (
    <div className="flex min-h-[44px] items-center gap-2.5 rounded-[var(--radius-panel)] bg-white/[0.045] py-1.5 pr-1.5 pl-3">
      {preview && live ? <SelfPreview me={me} n={mine.n} /> : null}
      <Screencast
        size={17}
        weight="bold"
        aria-hidden
        className={cn("shrink-0", live ? "text-jade-400" : "text-ash-400")}
      />
      <span className="flex min-w-0 flex-1 flex-col">
        <span className="text-[13px] leading-[1.3] font-semibold text-bone-50">
          {live ? t("share.live.on") : t("share.live.starting")}
        </span>
        <span className="truncate text-[12px] leading-[1.35] text-ash-400">{what}</span>
      </span>
      {live ? <Watchers ids={mine.watchers} /> : null}
      <Button
        size="sm"
        variant="secondary"
        aria-label={t("share.live.stopLabel")}
        onClick={() => void stopShare(mine.n)}
      >
        {t("share.live.stop")}
      </Button>
    </div>
  );
}

/** Above the panel's buttons: the player's shares, and "Ekranını paylaş". */
export function ShareFooterRow() {
  const { t } = useTranslation();
  const { reduce } = useMotionPrefs();
  const share = useShare();
  const openPicker = useShareUi((s) => s.openPicker);
  const me = useVoiceUi((s) => s.state.room?.me.id ?? "");
  const preview = useVoicePrefs((s) => s.settings?.share?.selfPreview ?? false);
  if (!share.on || (!share.canShare && !share.mine.length)) return null;
  const more = share.canShare && share.mine.length < share.mineMax;
  // Why "Ekranını paylaş" waits, in one sentence (none: it does not).
  const why = !share.allowed
    ? t("share.notice.notAllowed")
    : !share.open
      ? t("share.notice.off")
      : null;
  return (
    <div className="mb-2 flex flex-col gap-1.5">
      <AnimatePresence initial={false}>
        {share.mine.map((mine) => (
          <motion.div
            key={mine.n}
            initial={{ opacity: 0, y: reduce ? 0 : 4 }}
            animate={{ opacity: 1, y: 0, transition: { duration: 0.22, ease: [0.16, 1, 0.3, 1] } }}
            exit={{ opacity: 0, transition: { duration: 0.14 } }}
          >
            <LiveLine mine={mine} me={me} preview={preview} />
          </motion.div>
        ))}
      </AnimatePresence>
      {more ? (
        <Tooltip content={why ?? ""} disabled={why === null}>
          <Button
            size={share.mine.length ? "sm" : "lg"}
            variant={share.mine.length ? "ghost" : "secondary"}
            icon={<Monitor size={17} weight="bold" />}
            aria-disabled={why !== null || undefined}
            aria-description={why ?? undefined}
            className={cn("w-full", why !== null && "opacity-45")}
            onClick={why === null ? openPicker : undefined}
          >
            {t("share.action")}
          </Button>
        </Tooltip>
      ) : null}
      {share.canSetShareBy ? <WhoCanShare /> : null}
    </div>
  );
}

/** "Kim paylaşabilir" for a friend room's leader: one quiet line, its choice in a menu. */
function WhoCanShare() {
  const { t } = useTranslation();
  const share = useShare();
  const set = (who: ShareBy) => {
    if (who === share.shareBy) return;
    void shareApi
      .setSharers(who)
      .then((done) => {
        if (!done) toast(t("share.notice.failed"));
      })
      .catch(() => toast(t("share.notice.failed")));
  };
  const items: MenuItem[] = (["all", "mods"] as const).map((who) => ({
    label: t(`share.who.${who}`),
    icon:
      who === share.shareBy ? <Check size={15} weight="bold" /> : <span className="size-[15px]" />,
    onSelect: () => set(who),
  }));
  return (
    <div className="flex items-center justify-center gap-1.5 text-[12.5px] text-ash-400">
      <span>{t("share.who.label")}</span>
      <Menu
        items={items}
        align="center"
        trigger={
          <button
            type="button"
            aria-label={`${t("share.who.label")}: ${t(`share.who.${share.shareBy}`)}`}
            className="flex h-7 cursor-pointer items-center gap-1 rounded-[6px] px-1.5 font-medium text-bone-100 outline-none transition-colors duration-150 hover:bg-white/6 focus-visible:ring-2 focus-visible:ring-gold-300"
          >
            {t(`share.who.${share.shareBy}`)}
            <CaretDown size={12} weight="bold" aria-hidden />
          </button>
        }
      />
    </div>
  );
}

/** A person's first share the player does not watch yet. */
function unwatched(shares: VoiceLiveShare[], watching: { person: string; n: number }[]) {
  return shares.find((s) => !watching.some((w) => w.person === s.person && w.n === s.n)) ?? null;
}

/** Beside a person who shares: the mark (its kind on the pointer). */
export function ShareMark({ person }: { person: string }) {
  const { t } = useTranslation();
  const share = useShare();
  const theirs = share.live.filter((l) => l.person === person);
  const first = theirs[0];
  if (!first) return null;
  const label = t("share.row.mark", { kind: t(`share.kind.${first.kind}`) });
  return (
    <Tooltip content={label}>
      <span className="flex shrink-0" role="img" aria-label={label}>
        <Screencast size={15} weight="bold" className="text-jade-400" />
      </span>
    </Tooltip>
  );
}

/** "İzle" beside a person who shares something the player does not watch. */
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
      icon={<Screencast size={15} weight="bold" />}
      aria-label={t("share.row.watchLabel", { name })}
      className={className}
      onClick={() => void watchShare(next.person, next.n)}
    >
      {t("share.row.watch")}
    </Button>
  );
}
