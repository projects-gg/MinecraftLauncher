import { ArrowsClockwise, Sparkle } from "@phosphor-icons/react";
import { Button, Dialog, Progress, toast } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { motion } from "motion/react";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import Markdown, { type Components } from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import { api } from "@/lib/api";
import { canOpenLink, openLink } from "@/lib/links";
import { isTauri } from "@/lib/platform";
import { useLaunch } from "@/stores/launch";
import { takePendingNotes, useUpdate } from "@/stores/update";

const SIX_HOURS = 6 * 60 * 60 * 1000;

/**
 * Links in release notes and pack changelogs open in the system browser (or inside the launcher
 * for projects://). A plain link would load the page in the launcher's own window, with no way
 * back but a restart.
 */
const noteComponents: Components = {
  a: ({ href, children }) =>
    canOpenLink(href) ? (
      <a
        href={href}
        onClick={(e) => {
          e.preventDefault();
          openLink(href);
        }}
      >
        {children}
      </a>
    ) : (
      <span>{children}</span>
    ),
};

/**
 * Starts the background update checks: the first 10–60 s after start, then every 6 hours plus up
 * to 30 minutes. The random part spreads thousands of launchers over time instead of having them
 * all ask the CDN in the same minute.
 */
export function useUpdateChecks() {
  const check = useUpdate((s) => s.check);
  useEffect(() => {
    if (!isTauri()) return;
    const first = window.setTimeout(() => void check(), 10_000 + Math.random() * 50_000);
    const every = window.setInterval(
      () => void check(),
      SIX_HOURS + Math.random() * 30 * 60 * 1000,
    );
    return () => {
      window.clearTimeout(first);
      window.clearInterval(every);
    };
  }, [check]);
}

/** Title-bar pill once an update is downloaded. */
export function UpdatePill() {
  const { t } = useTranslation();
  const phase = useUpdate((s) => s.phase);
  const version = useUpdate((s) => s.version);
  const apply = useUpdate((s) => s.apply);
  const error = useUpdate((s) => s.error);

  useEffect(() => {
    if (error && phase === "ready") toast.error(error);
  }, [error, phase]);

  if (phase !== "ready" && phase !== "applying") return null;
  return (
    <motion.button
      type="button"
      initial={{ opacity: 0, y: -6 }}
      animate={{ opacity: 1, y: 0 }}
      onClick={() => void apply()}
      disabled={phase === "applying"}
      className="chamfer flex h-7 cursor-pointer items-center gap-2 bg-gold-500 px-3 text-[12.5px] font-semibold text-gold-ink [--cut:5px] hover:brightness-110 disabled:opacity-70"
    >
      <ArrowsClockwise
        size={14}
        weight="bold"
        className={phase === "applying" ? "animate-spin" : undefined}
      />
      {t("update.ready", { version: version ?? "" })}
    </motion.button>
  );
}

/** Blocking screen when the running version is below the published minimum. */
export function MandatoryUpdate() {
  const { t } = useTranslation();
  const { mandatory, phase, percent, apply, version } = useUpdate();
  if (!mandatory || phase === "idle" || phase === "checking") return null;
  return (
    <div className="fixed inset-0 z-[60] grid place-items-center bg-obsidian-950/92 backdrop-blur-md">
      <div className="w-[min(460px,90vw)] text-center">
        <Sparkle size={40} weight="duotone" className="mx-auto text-gold-400" />
        <h2 className="mt-3 font-display text-[22px] font-semibold text-bone-50">
          {t("update.mandatoryTitle")}
        </h2>
        <p className="mt-2 text-ash-300">{t("update.mandatoryBody", { version: version ?? "" })}</p>
        {phase === "downloading" ? (
          <Progress value={percent} label={t("update.downloading")} className="mt-6" />
        ) : (
          <Button
            className="mt-6"
            variant="primary"
            size="lg"
            onClick={() => void apply()}
            disabled={phase === "applying"}
          >
            {t("update.installNow")}
          </Button>
        )}
      </div>
    </div>
  );
}

/** "What's new" once after an update. */
export function WhatsNew() {
  const { t } = useTranslation();
  const { data: info } = useQuery({
    queryKey: ["app-info"],
    queryFn: () => api.appInfo(),
    enabled: isTauri(),
  });
  const [notes, setNotes] = useState<{ version: string; notes: string } | null>(null);
  useEffect(() => {
    if (info) setNotes(takePendingNotes(info.version));
  }, [info]);
  return (
    <Dialog
      open={Boolean(notes)}
      onOpenChange={(open) => !open && setNotes(null)}
      title={t("update.whatsNew", { version: notes?.version ?? "" })}
      closeLabel={t("common.close")}
      footer={
        <Button variant="primary" onClick={() => setNotes(null)}>
          {t("update.great")}
        </Button>
      }
    >
      <div className="prose-news">
        <Markdown rehypePlugins={[rehypeSanitize]} components={noteComponents}>
          {notes?.notes ?? ""}
        </Markdown>
      </div>
    </Dialog>
  );
}

/** Changelog of an official pack update, opened from the "pack updated" toast. */
export function PackNotesDialog() {
  const { t } = useTranslation();
  const notes = useLaunch((s) => s.packNotes);
  const show = useLaunch((s) => s.showPackNotes);
  return (
    <Dialog
      open={Boolean(notes)}
      onOpenChange={(open) => !open && show(null)}
      title={t("play.packNotesTitle", { name: notes?.name ?? "", version: notes?.version ?? "" })}
      closeLabel={t("common.close")}
      footer={
        <Button variant="primary" onClick={() => show(null)}>
          {t("update.great")}
        </Button>
      }
    >
      <div className="prose-news">
        <Markdown rehypePlugins={[rehypeSanitize]} components={noteComponents}>
          {notes?.changelog ?? ""}
        </Markdown>
      </div>
    </Dialog>
  );
}
