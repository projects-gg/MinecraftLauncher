import {
  ArrowsClockwise,
  CircleNotch,
  DownloadSimple,
  GameController,
} from "@phosphor-icons/react";
import { cn, easeOutExpo, toast } from "@pl/ui";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect } from "react";
import { useTranslation } from "react-i18next";
import { useLaunch } from "@/stores/launch";
import { useUpdate } from "@/stores/update";
import {
  cancelInstall,
  checkNow,
  isGameBusy,
  type PillView,
  pillView,
  requestInstall,
} from "./install";
import { InstallConfirm } from "./install-confirm";

type ShownView = Exclude<PillView, { kind: "hidden" }>;

/**
 * Title bar pill: the launcher update's one place in sight. It offers an update that is not
 * downloaded yet, shows a download the player asked for, restarts into a downloaded one (asking
 * first when AFK accounts are open) and, while a game is open, waits for the game to close.
 */
export function UpdatePill() {
  const phase = useUpdate((s) => s.phase);
  const version = useUpdate((s) => s.version);
  const mandatory = useUpdate((s) => s.mandatory);
  const manual = useUpdate((s) => s.manual);
  // Whole percents: the pill draws again at most a hundred times per download.
  const percent = useUpdate((s) => Math.round(s.percent));
  const step = useUpdate((s) => s.step);
  const error = useUpdate((s) => s.error);
  const gameBusy = useLaunch((s) => isGameBusy(s.phase.kind));

  useEffect(() => {
    // An install that failed (the mandatory update screen says it under its button instead).
    if (error && phase === "ready" && !mandatory) toast.error(error);
  }, [error, phase, mandatory]);

  const view = pillView({ phase, version, mandatory, manual, percent, step, gameBusy });
  return (
    <AnimatePresence initial={false}>
      {view.kind === "hidden" ? null : <Pill key="update-pill" view={view} />}
    </AnimatePresence>
  );
}

function Pill({ view }: { view: ShownView }) {
  const { t } = useTranslation();
  const version = useUpdate((s) => s.version);
  const asking = useUpdate((s) => s.asking?.from === "pill");

  const gold = view.kind === "ready" || view.kind === "installing";
  const passive =
    view.kind === "afterGame" || view.kind === "installing" || view.kind === "downloading";
  // Next to "ready" the pill keeps its width, so nothing in the title bar moves when a game
  // starts or ends, or while it installs: the ready label sizes it, unseen.
  const sized = view.kind === "afterGame" || view.kind === "installing";
  const readyLabel = t("update.ready", { version: version ?? "" });

  const press = () => {
    if (view.kind === "available") {
      void checkNow("pill");
      return;
    }
    if (view.kind !== "ready") return;
    if (asking) cancelInstall();
    else void requestInstall("pill");
  };

  let icon: ReactNode;
  let label: string;
  const spinner = (
    <CircleNotch size={14} weight="bold" className="animate-spin motion-reduce:animate-none" />
  );
  switch (view.kind) {
    case "available":
      icon = <DownloadSimple size={14} weight="bold" className="text-gold-300" />;
      label = t("updates.available");
      break;
    case "downloading":
      icon = spinner;
      label = t("updates.downloading", { percent: view.percent });
      break;
    case "ready":
      icon = <ArrowsClockwise size={14} weight="bold" />;
      label = readyLabel;
      break;
    case "afterGame":
      icon = <GameController size={14} weight="bold" className="text-ash-300" />;
      label = t("updates.afterGame");
      break;
    case "installing":
      icon = spinner;
      label =
        view.step === "stoppingAfk"
          ? t("afkAlerts.update.stopping")
          : t("afkAlerts.update.installing");
      break;
  }
  const contentKey = view.kind === "installing" ? `installing-${view.step}` : view.kind;

  return (
    <InstallConfirm from="pill">
      <motion.button
        type="button"
        onClick={passive ? undefined : press}
        aria-disabled={passive || undefined}
        aria-expanded={view.kind === "ready" ? asking : undefined}
        initial={{ opacity: 0, transform: "translateY(-6px)" }}
        animate={{
          opacity: 1,
          transform: "translateY(0px)",
          transition: { duration: 0.24, ease: easeOutExpo },
        }}
        exit={{ opacity: 0, transition: { duration: 0.14, ease: easeOutExpo } }}
        className={cn(
          "group relative isolate flex h-7 items-center justify-center px-3 text-[12.5px] font-semibold whitespace-nowrap [--cut:5px] focus-visible:outline-none",
          gold ? "text-gold-ink" : "text-bone-100",
          // A download in progress keeps one width while its percent grows.
          view.kind === "downloading" && "min-w-[148px]",
          passive
            ? "cursor-default"
            : "cursor-pointer transition-[filter,transform] duration-150 ease-out hover:brightness-110 active:scale-[0.97]",
        )}
      >
        {/* The focus ring is a larger chamfer (an outline would be cut off by the shape). */}
        <span
          aria-hidden
          className="chamfer absolute -inset-[3px] -z-20 bg-gold-300 opacity-0 transition-opacity group-focus-visible:opacity-100"
        />
        {/* Two surfaces cross-fade when the pill changes meaning (a game starts or ends). */}
        <span
          aria-hidden
          className={cn(
            "chamfer absolute inset-0 -z-10 bg-gold-500 transition-opacity duration-200 ease-out",
            gold ? "opacity-100" : "opacity-0",
          )}
        />
        <span
          aria-hidden
          className={cn(
            "chamfer absolute inset-0 -z-10 bg-white/10 transition-opacity duration-200 ease-out",
            gold ? "opacity-0" : "opacity-100",
          )}
        />
        <span className="grid">
          <motion.span
            key={contentKey}
            className="col-start-1 row-start-1 flex items-center justify-center gap-2"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            transition={{ duration: 0.16, ease: easeOutExpo }}
          >
            {icon}
            <span className="tabular-nums">{label}</span>
          </motion.span>
          {sized ? (
            <span aria-hidden className="invisible col-start-1 row-start-1 flex items-center gap-2">
              <ArrowsClockwise size={14} weight="bold" />
              <span className="tabular-nums">{readyLabel}</span>
            </span>
          ) : null}
        </span>
      </motion.button>
    </InstallConfirm>
  );
}
