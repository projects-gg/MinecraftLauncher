import { Archive, ArrowCounterClockwise, Broom, CheckCircle, Trash } from "@phosphor-icons/react";
import { Button, cn, Dialog, Progress, toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import {
  type Direction,
  enter,
  fade,
  pop,
  stagger,
  stepVariants,
  swap,
  useMotionPrefs,
} from "@/features/modcenter/motion";
import { api, type RemovedSet } from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { ErrorLine, type Failure, failure } from "../failure";
import { AnimatedBytes } from "./animated-bytes";
import type { CleanupPlan } from "./model";

type Step = "confirm" | "running" | "done" | "failed";
type Part = "sets" | "shared" | "leftovers";

interface Outcome {
  freedBytes: number;
  movedBytes: number;
  removed: RemovedSet[];
  skipped: number;
}

/** Everything the card shows, so lists refresh after a cleanup (or an undo). */
export const cleanupQueryKeys = [
  ["cleanup-status"],
  ["cleanup-sets"],
  ["cleanup-shared"],
  ["cleanup-leftovers"],
  ["cleanup-trash"],
  ["data-overview"],
  ["instance-versions"],
];

/**
 * "Temizle": first what goes where (into the trash, or deleted for good) with the sizes, then
 * the same lines tick off one by one while the footer turns into progress, then what was freed
 * counting up, with "Geri al" for sets that went to the trash. The panel glides between steps.
 */
export function CleanupDialog({
  open,
  plan,
  olderThanDays,
  onClose,
}: {
  open: boolean;
  plan: CleanupPlan;
  olderThanDays: number;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const prefs = useMotionPrefs();
  const [step, setStep] = useState<Step>("confirm");
  const [direction, setDirection] = useState<Direction>(1);
  // The selection behind the dialog may change; what was confirmed stays.
  const [frozen, setFrozen] = useState(plan);
  const [part, setPart] = useState<Part | null>(null);
  const [done, setDone] = useState<Part[]>([]);
  const [doneBytes, setDoneBytes] = useState(0);
  const [outcome, setOutcome] = useState<Outcome | null>(null);
  const [error, setError] = useState<Failure | null>(null);
  const [undoing, setUndoing] = useState(false);
  // Opening starts fresh with the selection of that moment (adjusted during render, so the
  // first frame already shows it).
  const [wasOpen, setWasOpen] = useState(open);
  if (open !== wasOpen) {
    setWasOpen(open);
    if (open) {
      setFrozen(plan);
      setStep("confirm");
      setDirection(1);
      setPart(null);
      setDone([]);
      setDoneBytes(0);
      setOutcome(null);
      setError(null);
      setUndoing(false);
    }
  }

  const refresh = () => {
    for (const queryKey of cleanupQueryKeys) {
      void queryClient.invalidateQueries({ queryKey });
    }
  };

  // "Temizle" fades out when pressed and still takes clicks meanwhile: the second press of a
  // double click would start the whole cleanup again (and its empty result would replace the
  // real one, "Setleri geri al" included).
  const running = useRef(false);
  const run = async () => {
    if (running.current) return;
    running.current = true;
    setDirection(1);
    setStep("running");
    const result: Outcome = { freedBytes: 0, movedBytes: 0, removed: [], skipped: 0 };
    let bytes = 0;
    const finish = (which: Part, add: number) => {
      bytes += add;
      setDoneBytes(bytes);
      setDone((list) => [...list, which]);
    };
    try {
      if (frozen.sharedIds.length > 0) {
        setPart("shared");
        const base = bytes;
        const deleted = await api.cleanup.deleteShared(frozen.sharedIds, (p) => {
          if (p.total > 0) setDoneBytes(base + (frozen.sharedBytes * p.done) / p.total);
        });
        result.freedBytes += deleted.freedBytes;
        finish("shared", frozen.sharedBytes);
      }
      if (frozen.leftoverKinds.length > 0) {
        setPart("leftovers");
        const cleaned = await api.cleanup.cleanLeftovers(frozen.leftoverKinds);
        result.freedBytes += cleaned.freedBytes;
        finish("leftovers", frozen.leftoverBytes);
      }
      // Sets go last: moving one first would make its game files look unused and widen the
      // deletion above beyond the summary the player confirmed.
      if (frozen.sets.length > 0) {
        setPart("sets");
        const removed = await api.cleanup.removeSets(
          frozen.sets.map((set) => ({ instanceId: set.instanceId, minecraft: set.minecraft })),
          olderThanDays,
        );
        result.movedBytes = removed.movedBytes;
        result.removed = removed.removed;
        result.skipped = removed.skipped;
        finish("sets", frozen.setBytes);
      }
      setPart(null);
      setOutcome(result);
      setStep("done");
    } catch (cause) {
      setPart(null);
      setOutcome(result);
      setError(failure(cause, t("dataSettings.cleanup.dialog.failed")));
      setStep("failed");
    } finally {
      running.current = false;
      refresh();
    }
  };

  const undo = async () => {
    if (!outcome || outcome.removed.length === 0) return;
    setUndoing(true);
    try {
      const back = await api.cleanup.restoreSets(outcome.removed.map((set) => set.trashId));
      if (back.restored > 0) {
        toast.success(t("dataSettings.cleanup.dialog.undone", { count: back.restored }));
      } else {
        toast.error(
          t(
            back.taken > 0
              ? "dataSettings.cleanup.dialog.undoTaken"
              : "dataSettings.cleanup.dialog.undoFailed",
          ),
        );
      }
      refresh();
      onClose();
    } catch (cause) {
      toast.error(failure(cause, t("dataSettings.cleanup.dialog.undoFailed")).message);
      setUndoing(false);
    }
  };

  const busy = step === "running" || undoing;
  const c = "dataSettings.cleanup.dialog";
  const title =
    step === "done"
      ? t(`${c}.doneTitle`)
      : step === "failed"
        ? t(`${c}.failedTitle`)
        : t(`${c}.title`, { size: formatBytes(frozen.totalBytes) });

  const lineState = (which: Part) =>
    done.includes(which) ? "done" : part === which ? "active" : "idle";
  const lines: Array<{ key: Part; icon: ReactNode; text: ReactNode; bytes: number }> = [];
  if (frozen.sharedIds.length > 0) {
    lines.push({
      key: "shared",
      icon: <Trash size={20} />,
      text: (
        <>
          <p>{t(`${c}.shared`, { count: frozen.sharedIds.length })}</p>
          <p className="text-[12.5px] text-ash-400">
            {t("dataSettings.cleanup.shared.redownload")}
          </p>
        </>
      ),
      bytes: frozen.sharedBytes,
    });
  }
  if (frozen.leftoverKinds.length > 0) {
    lines.push({
      key: "leftovers",
      icon: <Broom size={20} />,
      text: t(`${c}.leftovers`),
      bytes: frozen.leftoverBytes,
    });
  }
  if (frozen.sets.length > 0) {
    lines.push({
      key: "sets",
      icon: <Archive size={20} />,
      text: t(`${c}.sets`, { count: frozen.sets.length }),
      bytes: frozen.setBytes,
    });
  }
  let body: ReactNode;
  if (step === "confirm" || step === "running") {
    body = (
      <ul className="space-y-4 pt-1">
        {lines.map((line, index) => (
          <PlanLine
            key={line.key}
            index={index}
            icon={line.icon}
            state={lineState(line.key)}
            bytes={line.bytes}
          >
            {line.text}
          </PlanLine>
        ))}
      </ul>
    );
  } else if (step === "done" && outcome) {
    body = <Result outcome={outcome} />;
  } else {
    body = (
      <div className="space-y-4 pt-1">
        {error ? <ErrorLine error={error} /> : null}
        {outcome && (outcome.freedBytes > 0 || outcome.movedBytes > 0) ? (
          <p className="text-[13px] text-ash-300">
            {t(`${c}.partly`, { size: formatBytes(outcome.freedBytes + outcome.movedBytes) })}
          </p>
        ) : null}
      </div>
    );
  }

  const percent = frozen.totalBytes > 0 ? (doneBytes / frozen.totalBytes) * 100 : undefined;
  let footer: ReactNode;
  if (step === "confirm" || step === "running") {
    footer = (
      <div className="relative flex min-h-10 w-full items-center justify-end">
        <AnimatePresence initial={false} mode="popLayout">
          {step === "running" ? (
            <motion.div key="progress" {...swap(0.2)} className="w-full" aria-live="polite">
              <div className="flex items-baseline justify-between gap-4">
                <p className="text-[13px] font-medium text-bone-50">{t(`${c}.running`)}</p>
                <p className="text-[12.5px] text-ash-400 tabular-nums">
                  {t(`${c}.bytes`, {
                    done: formatBytes(Math.round(doneBytes)),
                    total: formatBytes(frozen.totalBytes),
                  })}
                </p>
              </div>
              <Progress className="mt-2" value={percent} label={t(`${c}.progressLabel`)} />
            </motion.div>
          ) : (
            <motion.div key="actions" {...swap(0.2)} className="flex items-center gap-2.5">
              <Button variant="ghost" onClick={onClose}>
                {t(`${c}.cancel`)}
              </Button>
              <Button
                variant="primary"
                icon={<Broom size={16} weight="bold" />}
                disabled={frozen.totalBytes === 0}
                onClick={() => void run()}
              >
                {t(`${c}.confirm`)}
              </Button>
            </motion.div>
          )}
        </AnimatePresence>
      </div>
    );
  } else {
    footer = (
      <motion.div {...fade(0.2)} className="flex w-full items-center justify-end gap-2.5">
        {step === "done" && outcome && outcome.removed.length > 0 ? (
          <Button
            variant="ghost"
            className="mr-auto"
            icon={<ArrowCounterClockwise size={16} />}
            disabled={undoing}
            onClick={() => void undo()}
          >
            {undoing ? t(`${c}.undoing`) : t(`${c}.undo`)}
          </Button>
        ) : null}
        <Button variant="primary" onClick={onClose} disabled={undoing}>
          {step === "done" ? t(`${c}.close`) : t("common.close")}
        </Button>
      </motion.div>
    );
  }

  const bodyKey = step === "running" ? "confirm" : step;
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && !busy && onClose()}
      title={
        <span className="relative block">
          <AnimatePresence mode="popLayout" initial={false}>
            <motion.span key={title} {...swap(0.18)} className="block">
              {title}
            </motion.span>
          </AnimatePresence>
        </span>
      }
      closeLabel={t("common.close")}
      width={520}
      footer={footer}
      morph={prefs.layout}
      dismissible={!busy}
      dismissOnOutsideClick={false}
    >
      <AnimatePresence mode="popLayout" initial={false} custom={direction}>
        <motion.div
          key={bodyKey}
          custom={direction}
          variants={stepVariants(prefs.reduce)}
          initial="enter"
          animate="center"
          exit="exit"
        >
          {body}
        </motion.div>
      </AnimatePresence>
    </Dialog>
  );
}

/** A line of the plan: its icon turns into a check when that part is done. */
function PlanLine({
  index,
  icon,
  state,
  bytes,
  children,
}: {
  index: number;
  icon: ReactNode;
  state: "idle" | "active" | "done";
  bytes: number;
  children: ReactNode;
}) {
  const prefs = useMotionPrefs();
  return (
    <motion.li
      initial={{ opacity: 0, transform: prefs.reduce ? "none" : "translateY(6px)" }}
      animate={{
        opacity: 1,
        transform: "translateY(0px)",
        transition: { ...enter(0.26), delay: stagger(index, 0.045) },
      }}
      className="flex items-start gap-3"
    >
      <span className="relative mt-0.5 size-5 shrink-0">
        <AnimatePresence initial={false} mode="popLayout">
          {state === "done" ? (
            <motion.span
              key="done"
              {...pop(prefs.reduce, 0.22)}
              className="absolute inset-0 text-jade-400"
            >
              <CheckCircle size={20} weight="fill" />
            </motion.span>
          ) : (
            <motion.span
              key="icon"
              {...fade(0.15)}
              className={cn(
                "absolute inset-0 transition-colors duration-200",
                state === "active" ? "text-gold-300" : "text-ash-400",
              )}
            >
              {icon}
            </motion.span>
          )}
        </AnimatePresence>
      </span>
      <div className="min-w-0 flex-1 text-[13.5px] leading-relaxed text-bone-100">{children}</div>
      <span className="shrink-0 pt-px font-display text-[14px] font-semibold text-bone-100 tabular-nums">
        {formatBytes(bytes)}
      </span>
    </motion.li>
  );
}

/** What was freed, counting up from zero; sets in the trash and skipped ones below. */
function Result({ outcome }: { outcome: Outcome }) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  const c = "dataSettings.cleanup.dialog";
  const freedFirst = outcome.freedBytes > 0 || outcome.movedBytes === 0;
  const target = freedFirst ? outcome.freedBytes : outcome.movedBytes;
  const [shown, setShown] = useState(0);
  useEffect(() => {
    setShown(target);
  }, [target]);
  const notes: string[] = [];
  if (freedFirst && outcome.movedBytes > 0) {
    notes.push(
      t(`${c}.moved`, { count: outcome.removed.length, size: formatBytes(outcome.movedBytes) }),
    );
  }
  if (outcome.skipped > 0) notes.push(t(`${c}.skipped`, { count: outcome.skipped }));
  return (
    <div className="flex flex-col items-center pt-2 pb-1 text-center" aria-live="polite">
      <motion.span {...pop(prefs.reduce, 0.3)} className="text-jade-400">
        <CheckCircle size={44} weight="duotone" />
      </motion.span>
      <AnimatedBytes
        value={shown}
        duration={700}
        className="mt-3 block font-display text-[32px] leading-none font-semibold text-bone-50 tabular-nums"
      />
      <p className="mt-2 text-[13.5px] text-ash-300">
        {freedFirst ? t(`${c}.freed`) : t(`${c}.movedOnly`, { count: outcome.removed.length })}
      </p>
      {notes.map((note, index) => (
        <motion.p
          key={note}
          initial={{ opacity: 0, transform: prefs.reduce ? "none" : "translateY(4px)" }}
          animate={{
            opacity: 1,
            transform: "translateY(0px)",
            transition: { ...enter(0.26), delay: 0.25 + stagger(index, 0.06) },
          }}
          className={cn("text-[13px] text-ash-400", index === 0 ? "mt-4" : "mt-1")}
        >
          {note}
        </motion.p>
      ))}
    </div>
  );
}
