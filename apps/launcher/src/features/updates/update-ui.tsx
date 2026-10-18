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
import { useQueryClient } from "@tanstack/react-query";
import { useRouter } from "@tanstack/react-router";
import { AnimatePresence, motion } from "motion/react";
import { lazy, Suspense, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { useAfkSessions } from "@/features/accounts/use-accounts";
import { AfkHead } from "@/features/afk/afk-head";
import { type AfkAccount, api } from "@/lib/api";
import i18n from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { useLaunch } from "@/stores/launch";
import { takePendingNotes, useUpdate } from "@/stores/update";
import { chooseNotes, landingRoute, loadAfterUpdate, type ReleaseNotes } from "./after-update";
import { cancelInstall, isAfkOpen, isGameBusy, requestInstall } from "./install";
import { loadNotes } from "./notes";

export { UpdatePill } from "./update-pill";

const SIX_HOURS = 6 * 60 * 60 * 1000;

/** Release notes and pack changelogs (Markdown, loaded on demand: `notes.ts`). */
const NotesMarkdown = lazy(loadNotes);

const wait = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/**
 * Pretend install for the browser preview: like the backend, open AFK accounts stop first
 * (`afkOpen`), then the installer "starts" and, as on Windows where the launcher quits, nothing
 * comes back; `fails` brings the error of an install that did not work instead.
 */
function previewApply(fails: boolean) {
  return async (options?: { afkOpen?: boolean }) => {
    if (useUpdate.getState().phase !== "ready") return;
    const afkOpen = Boolean(options?.afkOpen);
    useUpdate.setState({
      phase: "applying",
      step: afkOpen ? "stoppingAfk" : "installing",
      asking: null,
      error: null,
    });
    if (afkOpen) {
      await api.afkStopAll();
      await wait(1600);
      useUpdate.setState({ step: "installing" });
    }
    if (!fails) return new Promise<never>(() => {});
    await wait(1200);
    useUpdate.setState({
      phase: "ready",
      step: null,
      error: previewError(
        "Güncelleme kurulamadı. Tekrar dene.",
        "The update couldn't be installed. Try again.",
      ),
    });
  };
}

/** The backend's words in the preview, in the interface's language as the backend writes them. */
const previewError = (tr: string, en: string) => (i18n.language === "en" ? en : tr);

const PREVIEW_NOTES =
  "- Güncelleme artık AFK hesaplarını kendisi durdurup sonra yeniden bağlıyor\n- Oyun açıkken güncelleme oyundan sonraya kalıyor\n- Daha hızlı indirmeler";

/**
 * Browser previews (the browser has no updater): `?update=mandatory` over a downloaded update,
 * `?update=failed` after a failed download, `?update=ready` (the title bar pill),
 * `?update=installfail`, `?update=available`, `?update=downloading`, `?update=whatsnew`. With
 * `&game=running` a game is open; `&game=closing` closes it after 2.5 s.
 */
function previewUpdate() {
  const params = new URLSearchParams(window.location.search);
  const kind = params.get("update");
  const game = params.get("game");
  const base = { version: "2.5.0", notes: PREVIEW_NOTES, mandatory: false, error: null };
  switch (kind) {
    case "mandatory":
    case "failed": {
      const failed = kind === "failed";
      useUpdate.setState({
        ...base,
        mandatory: true,
        phase: failed ? "error" : "ready",
        // The backend's words for a failed download.
        error: failed
          ? previewError("Güncelleme indirilemedi.", "The update couldn't be downloaded.")
          : null,
        apply: previewApply(false),
      });
      break;
    }
    case "ready":
    case "installfail":
      useUpdate.setState({ ...base, phase: "ready", apply: previewApply(kind === "installfail") });
      break;
    case "available":
      useUpdate.setState({ ...base, phase: "available" });
      break;
    case "downloading":
      useUpdate.setState({ ...base, phase: "downloading", manual: true, percent: 42 });
      break;
  }
  if (game === "running" || game === "closing") {
    useLaunch.setState({ phase: { kind: "running", since: Date.now() } });
  }
  if (game === "closing") {
    window.setTimeout(() => useLaunch.setState({ phase: { kind: "idle" } }), 2500);
  }
}

/** Once the game closes with an update waiting for it: say so, with the restart at hand. */
function offerAfterGame() {
  toast.success(i18n.t("updates.readyAfterGame"), {
    id: "update-after-game",
    duration: 10_000,
    action: { label: i18n.t("updates.restart"), onClick: () => void requestInstall("pill") },
  });
}

/**
 * Starts the background update checks: the first 10–60 s after start, then every 6 hours plus up
 * to 30 minutes. The random part spreads thousands of launchers over time instead of having them
 * all ask the CDN in the same minute. An update that waited for a game is offered when it closes
 * (a store subscription: nothing renders again and nothing polls).
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
  useEffect(
    () =>
      useLaunch.subscribe((state, previous) => {
        const { phase, mandatory, asking } = useUpdate.getState();
        // A game starting closes the AFK question: the update waits for the game now.
        if (asking && isGameBusy(state.phase.kind)) cancelInstall();
        if (previous.phase.kind !== "running" || state.phase.kind === "running") return;
        if (phase === "ready" && !mandatory) offerAfterGame();
      }),
    [],
  );
}

/** AFK names shown on the mandatory update screen; the rest are counted. */
const ROSTER_MAX = 6;

/**
 * Blocking screen when the running version is below the published minimum. It covers the whole
 * launcher, so it must offer every way forward itself: open AFK accounts are listed and stopped by
 * "Durdur ve güncelle" (they connect again after the restart), a running game asks to be closed
 * first, and a failed download or install can be tried again (docs/tasks/q2.md §3, item 1).
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
  const step = useUpdate((s) => s.step);
  const apply = useUpdate((s) => s.apply);
  const check = useUpdate((s) => s.check);
  const gameOpen = useLaunch((s) => isGameBusy(s.phase.kind));
  const open = useAfkSessions().filter(isAfkOpen);
  // The accounts at the press: listed until the install starts, each one dimming as it stops.
  const [roster, setRoster] = useState<AfkAccount[] | null>(null);
  const panel = useRef<HTMLDivElement>(null);

  // The screen takes over: focus moves to it, never to the button (Enter would stop the AFK).
  useEffect(() => panel.current?.focus(), []);

  const busy = phase === "applying";
  const listed = roster ?? open;
  const stillOpen = new Set(open.map((account) => account.id));

  const install = async () => {
    if (busy || gameOpen) return;
    const afkOpen = open.length > 0;
    setRoster(afkOpen ? open : null);
    try {
      // The backend stops the AFK accounts first (and brings them back after the restart). On
      // Windows the installer then takes over and the launcher quits; a failure lands below.
      await apply({ afkOpen });
    } finally {
      setRoster(null);
    }
  };

  let label: string;
  let onPress: (() => void) | undefined;
  if (phase === "applying") {
    label =
      step === "stoppingAfk" ? t("afkAlerts.update.stopping") : t("afkAlerts.update.installing");
  } else if (phase === "error") {
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
      className="fixed inset-x-0 top-titlebar bottom-0 z-[60] grid place-items-center"
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
            <p className="text-[12px] font-medium text-ash-300">
              {t("updates.afkComeBack", { count: listed.length })}
            </p>
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

/** Set once the restart's leftovers were used: a later render or refetch never repeats them. */
let afterUpdateHandled = false;

/**
 * The first moments after an update restart: back on the page the player left, and "Yenilikler"
 * with the new version's notes, once.
 */
export function WhatsNew() {
  const { t } = useTranslation();
  const router = useRouter();
  const queryClient = useQueryClient();
  const [notes, setNotes] = useState<ReleaseNotes | null>(null);

  useEffect(() => {
    if (afterUpdateHandled) return;
    if (!isTauri()) {
      afterUpdateHandled = true;
      if (new URLSearchParams(window.location.search).get("update") === "whatsnew") {
        void loadNotes().finally(() => setNotes({ version: "2.5.0", notes: PREVIEW_NOTES }));
      }
      return;
    }
    // Two runs in development (StrictMode) share one backend call; the live one uses it.
    let live = true;
    void Promise.all([
      loadAfterUpdate(),
      queryClient
        .fetchQuery({ queryKey: ["app-info"], queryFn: () => api.appInfo() })
        .catch(() => null),
    ]).then(([after, info]) => {
      if (!live || afterUpdateHandled) return;
      afterUpdateHandled = true;
      const route = landingRoute(after, router.state.location.pathname);
      if (route) router.history.replace(route);
      const shown = chooseNotes(after, info ? takePendingNotes(info.version) : null);
      // The dialog opens once its Markdown renderer is there, with the notes in place.
      if (shown) void loadNotes().finally(() => setNotes(shown));
    });
    return () => {
      live = false;
    };
  }, [router, queryClient]);

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
        <Suspense fallback={null}>
          <NotesMarkdown text={notes?.notes ?? ""} />
        </Suspense>
      </div>
    </Dialog>
  );
}

/**
 * Changelog of an official pack update, opened from the "pack updated" toast (which already
 * started loading the Markdown renderer, `stores/launch.ts`).
 */
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
        <Suspense fallback={null}>
          <NotesMarkdown text={notes?.changelog ?? ""} />
        </Suspense>
      </div>
    </Dialog>
  );
}
