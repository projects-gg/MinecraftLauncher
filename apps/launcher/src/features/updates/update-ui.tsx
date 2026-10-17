import { ArrowsClockwise, CircleNotch, Sparkle } from "@phosphor-icons/react";
import {
  Button,
  Chip,
  Dialog,
  EntranceGate,
  easeOutExpo,
  GatedMotion,
  Progress,
  toast,
  useMotionPrefs,
} from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import Markdown, { type Components } from "react-markdown";
import rehypeSanitize from "rehype-sanitize";
import { commands } from "@/bindings";
import { useAfkSessions } from "@/features/accounts/use-accounts";
import { AfkHead } from "@/features/afk/afk-head";
import { type AfkAccount, api } from "@/lib/api";
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
 * Browser preview of the mandatory update screen (the browser has no updater): `?update=mandatory`
 * over a downloaded update, `?update=failed` after a failed download, `&game=running` with a game
 * open. Installing only pretends, and like on Windows (where the launcher quits) never returns.
 */
function previewUpdate() {
  const params = new URLSearchParams(window.location.search);
  const kind = params.get("update");
  if (kind !== "mandatory" && kind !== "failed") return;
  const failed = kind === "failed";
  useUpdate.setState({
    mandatory: true,
    version: "2.5.0",
    phase: failed ? "error" : "ready",
    // The backend's words for a failed download.
    error: failed ? "Güncelleme indirilemedi." : null,
    apply: () => {
      useUpdate.setState({ phase: "applying" });
      return new Promise<never>(() => {});
    },
  });
  if (params.get("game") === "running") {
    useLaunch.setState({ phase: { kind: "running", since: Date.now() } });
  }
}

/**
 * Starts the background update checks: the first 10–60 s after start, then every 6 hours plus up
 * to 30 minutes. The random part spreads thousands of launchers over time instead of having them
 * all ask the CDN in the same minute.
 */
export function useUpdateChecks() {
  const check = useUpdate((s) => s.check);
  useEffect(() => {
    if (!isTauri()) {
      previewUpdate();
      return;
    }
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
  const mandatory = useUpdate((s) => s.mandatory);

  useEffect(() => {
    // The mandatory update screen says it under its button instead.
    if (error && phase === "ready" && !mandatory) toast.error(error);
  }, [error, phase, mandatory]);

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

/** An AFK account that keeps the update from installing: its client runs, connects or waits. */
const isOpen = (account: AfkAccount) =>
  account.state.kind !== "stopped" && account.state.kind !== "error";

/** How long "Durdur ve güncelle" waits for the AFK clients to exit before installing anyway. */
const AFK_STOP_WAIT_MS = 10_000;
/** AFK names shown on the mandatory update screen; the rest are counted. */
const ROSTER_MAX = 6;

/** True while an AFK client runs or a staggered "connect all" is under way. */
async function afkBusy(): Promise<boolean> {
  if (isTauri()) return commands.afkAnyActive();
  return (await api.afkAccounts()).some(isOpen);
}

/**
 * Stops every AFK account (a running "connect all" too) and waits until their clients have exited,
 * at most `AFK_STOP_WAIT_MS`: installing quits the launcher at once, and a client left behind would
 * keep the account on the server without anyone watching it.
 */
async function stopAfkForUpdate() {
  try {
    await api.afkStopAll();
  } catch {
    // The wait below decides; the install refuses if something still runs.
  }
  const deadline = Date.now() + AFK_STOP_WAIT_MS;
  while ((await afkBusy()) && Date.now() < deadline) {
    await new Promise((resolve) => setTimeout(resolve, 250));
  }
}

/**
 * Blocking screen when the running version is below the published minimum. It covers the whole
 * launcher, so it must offer every way forward itself: open AFK accounts are listed and stopped by
 * "Durdur ve güncelle", a running game asks to be closed first, and a failed download or install
 * can be tried again (docs/tasks/q2.md §3, item 1).
 */
export function MandatoryUpdate() {
  const shown = useUpdate((s) => s.mandatory && s.phase !== "idle");
  return (
    <AnimatePresence>
      {shown ? (
        <EntranceGate key="mandatory-update">
          <MandatoryScreen />
        </EntranceGate>
      ) : null}
    </AnimatePresence>
  );
}

function MandatoryScreen() {
  const { t } = useTranslation();
  const { reduce, layout } = useMotionPrefs();
  const phase = useUpdate((s) => s.phase);
  const percent = useUpdate((s) => s.percent);
  const version = useUpdate((s) => s.version);
  const error = useUpdate((s) => s.error);
  const apply = useUpdate((s) => s.apply);
  const check = useUpdate((s) => s.check);
  const gameOpen = useLaunch((s) => s.phase.kind === "launching" || s.phase.kind === "running");
  const open = useAfkSessions().filter(isOpen);
  const [stopping, setStopping] = useState(false);
  // The accounts at the press: listed until the install starts, each one dimming as it stops.
  const [roster, setRoster] = useState<AfkAccount[] | null>(null);
  const panel = useRef<HTMLDivElement>(null);

  // The screen takes over: focus moves to it, never to the button (Enter would stop the AFK).
  useEffect(() => panel.current?.focus(), []);

  const busy = stopping || phase === "applying";
  const listed = roster ?? open;
  const stillOpen = new Set(open.map((account) => account.id));

  const install = async () => {
    if (busy || gameOpen) return;
    try {
      if (open.length > 0) {
        setRoster(open);
        setStopping(true);
        await stopAfkForUpdate();
      }
      // On Windows the installer takes over and the launcher quits; a failure lands below.
      await apply();
    } finally {
      setStopping(false);
      setRoster(null);
    }
  };

  let label: string;
  let onPress: (() => void) | undefined;
  if (phase === "applying") label = t("afkAlerts.update.installing");
  else if (stopping) label = t("afkAlerts.update.stopping");
  else if (phase === "error") {
    label = t("common.retry");
    onPress = () => void check(true);
  } else if (gameOpen) label = t("afkAlerts.update.quitGame");
  else {
    label = open.length > 0 ? t("afkAlerts.update.stopAndUpdate") : t("update.installNow");
    onPress = () => void install();
  }
  const fetching = phase === "downloading" || phase === "checking";
  const problem = !busy && (phase === "ready" || phase === "error") ? error : null;

  const fade = { duration: 0.2, ease: easeOutExpo };
  const iconMotion = reduce
    ? { initial: { opacity: 0 }, animate: { opacity: 1 }, exit: { opacity: 0 } }
    : {
        initial: { opacity: 0, transform: "scale(0.6)" },
        animate: { opacity: 1, transform: "scale(1)" },
        exit: { opacity: 0, transform: "scale(0.6)" },
      };

  return (
    <motion.div
      className="fixed inset-0 z-[60] grid place-items-center"
      exit={{ opacity: 0, transition: { duration: 0.18, ease: easeOutExpo } }}
    >
      <GatedMotion.div
        aria-hidden
        className="absolute inset-0 bg-obsidian-950/92 backdrop-blur-md [html[data-perf=on]_&]:bg-obsidian-950/97 [html[data-perf=on]_&]:backdrop-blur-none"
        initial={{ opacity: 0 }}
        animate={{ opacity: 1, transition: { duration: reduce ? 0.16 : 0.24, ease: easeOutExpo } }}
      />
      <GatedMotion.div
        ref={panel}
        role="alertdialog"
        aria-modal="true"
        aria-labelledby="mandatory-update-title"
        aria-describedby="mandatory-update-body"
        tabIndex={-1}
        className="relative w-[min(460px,90vw)] text-center outline-none"
        initial={reduce ? { opacity: 0 } : { opacity: 0, transform: "translateY(12px)" }}
        animate={
          reduce
            ? { opacity: 1, transition: { duration: 0.16, ease: easeOutExpo } }
            : {
                opacity: 1,
                transform: "translateY(0px)",
                transition: { duration: 0.32, ease: easeOutExpo },
              }
        }
      >
        <Sparkle size={40} weight="duotone" className="mx-auto text-gold-400" />
        <h2
          id="mandatory-update-title"
          className="mt-3 font-display text-[22px] font-semibold text-bone-50"
        >
          {t("update.mandatoryTitle")}
        </h2>
        <p id="mandatory-update-body" className="mt-2 text-ash-300">
          {t("update.mandatoryBody", { version: version ?? "" })}
        </p>

        {listed.length > 0 ? (
          <div className="mt-5">
            <p className="text-[12px] font-medium text-ash-300">{t("afkAlerts.update.running")}</p>
            <ul className="mt-2 flex flex-wrap justify-center gap-1.5">
              <AnimatePresence initial={false}>
                {listed.slice(0, ROSTER_MAX).map((account) => (
                  <motion.li
                    key={account.id}
                    layout={layout ? "position" : false}
                    initial={{ opacity: 0 }}
                    // Stopped during "Durdur ve güncelle": it stays, dimmed, until the install.
                    animate={{ opacity: roster && !stillOpen.has(account.id) ? 0.4 : 1 }}
                    exit={{ opacity: 0 }}
                    transition={fade}
                  >
                    <Chip className="h-[24px] gap-1.5 pr-2 pl-1 text-[12px]">
                      <AfkHead nickname={account.nickname} size={16} />
                      <span className="max-w-[140px] truncate">{account.nickname}</span>
                    </Chip>
                  </motion.li>
                ))}
                {listed.length > ROSTER_MAX ? (
                  <motion.li
                    key="more"
                    layout={layout ? "position" : false}
                    initial={{ opacity: 0 }}
                    animate={{ opacity: 1 }}
                    exit={{ opacity: 0 }}
                    transition={fade}
                  >
                    <Chip className="h-[24px] text-[12px] text-ash-300">
                      {t("afkAlerts.update.more", { count: listed.length - ROSTER_MAX })}
                    </Chip>
                  </motion.li>
                ) : null}
              </AnimatePresence>
            </ul>
          </div>
        ) : null}

        {fetching ? (
          <Progress
            // Asking again after a failed download: no amount to show yet.
            value={phase === "checking" ? undefined : percent}
            label={t("update.downloading")}
            className="mt-6"
          />
        ) : (
          <Button
            className="mt-6 min-w-[272px]"
            variant="primary"
            size="lg"
            onClick={onPress}
            // Busy keeps full strength (it is working, not unavailable) but takes no second press.
            aria-disabled={busy || undefined}
            disabled={!busy && !onPress}
            icon={
              <AnimatePresence mode="popLayout" initial={false}>
                <motion.span
                  key={busy ? "busy" : "idle"}
                  className="flex"
                  {...iconMotion}
                  transition={fade}
                >
                  {busy ? (
                    <CircleNotch size={16} weight="bold" className="animate-spin" />
                  ) : (
                    <ArrowsClockwise size={16} weight="bold" />
                  )}
                </motion.span>
              </AnimatePresence>
            }
          >
            {label}
          </Button>
        )}
        {/* Only problems are spelled out; the line keeps its height so nothing moves. */}
        <p role="alert" className="mt-3 min-h-[18px] text-[12.5px] text-flare-400">
          {problem}
        </p>
      </GatedMotion.div>
    </motion.div>
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
