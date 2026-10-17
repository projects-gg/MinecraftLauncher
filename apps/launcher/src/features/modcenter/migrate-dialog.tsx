import { ArrowRight, CheckCircle, CheckSquare, Square } from "@phosphor-icons/react";
import { Button, Chip, cn, Dialog, EmptyState, Skeleton, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { newJobId, showError } from "@/features/content/jobs";
import { api, type InstanceSummary, type JobProgress, type MigrationMod } from "@/lib/api";
import { formatRelative } from "@/lib/format";
import { usePrefs } from "@/stores/prefs";
import { type Direction, enter, fade, stagger, stepVariants, swap, useMotionPrefs } from "./motion";
import { JobLine } from "./parts";

function statusText(t: (key: string) => string, mod: MigrationMod): string {
  return t(`modCenter.migrate.status.${mod.status}`);
}

const rowWidths = ["w-28", "w-36", "w-24"];

/** Shaped like the plan rows: a checkbox, a title and a version line. */
function PlanSkeleton({ label }: { label: string }) {
  return (
    <div role="status" aria-busy aria-label={label} className="space-y-0.5">
      <Skeleton className="mb-3 h-3.5 w-44" />
      {rowWidths.map((width) => (
        <div key={width} className="flex items-center gap-3 px-3 py-2">
          <Skeleton className="size-[19px] shrink-0" />
          <div className="flex-1 space-y-1.5">
            <Skeleton className={`h-3.5 ${width}`} />
            <Skeleton className="h-3 w-20" />
          </div>
        </div>
      ))}
    </div>
  );
}

/** A checkbox mark that swaps with a small scale, so choosing feels physical. */
function Mark({ checked, reduce }: { checked: boolean; reduce: boolean }) {
  return (
    <span className="relative grid size-[19px] shrink-0 place-items-center">
      <AnimatePresence initial={false} mode="popLayout">
        <motion.span
          key={checked ? "on" : "off"}
          initial={{ opacity: 0, transform: reduce ? "none" : "scale(0.7)" }}
          animate={{ opacity: 1, transform: reduce ? "none" : "scale(1)", transition: enter(0.18) }}
          exit={{ opacity: 0, transition: { duration: 0.08 } }}
          className="grid place-items-center"
        >
          {checked ? (
            <CheckSquare size={19} weight="fill" className="text-gold-400" />
          ) : (
            <Square size={19} className="text-ash-400" />
          )}
        </motion.span>
      </AnimatePresence>
    </span>
  );
}

/**
 * The player's own mods of `from`'s set (the active one when omitted) with a Modrinth version
 * for `to`, and moving the chosen ones there with one click.
 */
export function MigrationPanel({
  instanceId,
  from,
  to,
  intoActive = false,
  onDone,
}: {
  instanceId: string;
  from?: string;
  to: string;
  /** `to` is the instance's current version (the mods land in its mods folder). */
  intoActive?: boolean;
  onDone?: (installed: number) => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const prefs = useMotionPrefs();
  const plan = useQuery({
    queryKey: ["migration-plan", instanceId, from ?? "", to],
    queryFn: () => api.modCenter.migrationPlan(instanceId, from, to),
    staleTime: 60_000,
    retry: false,
  });
  const [chosen, setChosen] = useState<Set<string>>(() => new Set());
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [moved, setMoved] = useState<number | null>(null);

  useEffect(() => {
    const ids = (plan.data?.mods ?? [])
      .filter((m) => m.status === "compatible" && m.targetVersionId)
      .map((m) => m.targetVersionId as string);
    setChosen(new Set(ids));
  }, [plan.data]);

  const mods = plan.data?.mods ?? [];
  const compatible = mods.filter((m) => m.status === "compatible");

  // The button fades out when pressed and still takes the second press of a double click.
  const running = useRef(false);
  const run = async () => {
    if (running.current) return;
    running.current = true;
    setProgress({ stage: "resolving", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
    try {
      const result = await api.modCenter.migrate(instanceId, to, [...chosen], newJobId(), (p) =>
        setProgress(p),
      );
      // The mods the player moved (their dependencies come along without being counted).
      setMoved(chosen.size);
      if (result.unresolved > 0) toast(t("modCenter.migrate.unresolved"));
      void queryClient.invalidateQueries({ queryKey: ["instance", instanceId] });
      void queryClient.invalidateQueries({ queryKey: ["instance-versions", instanceId] });
      void queryClient.invalidateQueries({ queryKey: ["content-scan", instanceId, "mod"] });
      onDone?.(chosen.size);
    } catch (error) {
      showError(error);
    } finally {
      running.current = false;
      setProgress(null);
    }
  };

  let state: string;
  let content: ReactNode;
  if (plan.isPending) {
    state = "loading";
    content = <PlanSkeleton label={t("modCenter.migrate.searching")} />;
  } else if (plan.isError || !plan.data) {
    state = "error";
    content = (
      <EmptyState
        title={t("modCenter.migrate.failed")}
        action={
          <Button variant="secondary" onClick={() => void plan.refetch()}>
            {t("common.retry")}
          </Button>
        }
      />
    );
  } else if (plan.data.offline) {
    state = "offline";
    content = <p className="text-[13px] text-ash-300">{t("modCenter.migrate.offline")}</p>;
  } else if (mods.length === 0) {
    state = "none";
    content = <p className="text-[13px] text-ash-300">{t("modCenter.migrate.none")}</p>;
  } else if (moved !== null) {
    state = "moved";
    content = (
      <p className="flex items-center gap-2 text-[13.5px] text-bone-100" aria-live="polite">
        <CheckCircle size={18} weight="fill" className="shrink-0 text-jade-400" />
        {t(intoActive ? "modCenter.migrate.doneHere" : "modCenter.migrate.done", {
          count: moved,
          version: to,
        })}
      </p>
    );
  } else {
    state = "plan";
    content = (
      <div>
        <p className="mb-2 text-[13px] text-ash-300">
          {t("modCenter.migrate.summary", {
            compatible: compatible.length,
            total: mods.length,
            version: to,
          })}
        </p>
        <ul className="space-y-0.5">
          {mods.map((mod, i) => {
            const id = mod.targetVersionId;
            const selectable = mod.status === "compatible" && Boolean(id);
            const checked = Boolean(id && chosen.has(id));
            return (
              <motion.li
                key={mod.fileName}
                initial={{ opacity: 0, transform: prefs.reduce ? "none" : "translateY(4px)" }}
                animate={{
                  opacity: 1,
                  transform: prefs.reduce ? "none" : "translateY(0px)",
                  transition: { ...enter(0.24), delay: stagger(i, 0.03) },
                }}
              >
                <button
                  type="button"
                  disabled={!selectable || progress !== null}
                  aria-pressed={selectable ? checked : undefined}
                  onClick={() =>
                    id &&
                    setChosen((prev) => {
                      const next = new Set(prev);
                      if (next.has(id)) next.delete(id);
                      else next.add(id);
                      return next;
                    })
                  }
                  className={cn(
                    "group/mod relative flex w-full items-center gap-3 rounded-[var(--radius-panel)] px-3 py-2 text-left",
                    selectable
                      ? "cursor-pointer focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
                      : "cursor-default",
                  )}
                >
                  {selectable ? (
                    <span
                      aria-hidden
                      className="pointer-events-none absolute inset-0 rounded-[inherit] bg-white/5 opacity-0 transition-opacity duration-150 group-hover/mod:opacity-100"
                    />
                  ) : null}
                  {selectable ? (
                    <Mark checked={checked} reduce={prefs.reduce} />
                  ) : (
                    <span aria-hidden className="size-[19px] shrink-0" />
                  )}
                  <span className="relative min-w-0 flex-1">
                    <span
                      className={cn(
                        "block truncate text-[13.5px] font-medium transition-colors duration-150",
                        selectable ? "text-bone-50" : "text-ash-300",
                      )}
                    >
                      {mod.title}
                    </span>
                    <span className="block truncate text-[12px] text-ash-400">
                      {mod.status === "compatible" && mod.targetVersionNumber
                        ? mod.currentVersion
                          ? t("modCenter.migrate.versionChange", {
                              from: mod.currentVersion,
                              to: mod.targetVersionNumber,
                            })
                          : t("modCenter.migrate.versionTo", { to: mod.targetVersionNumber })
                        : statusText(t, mod)}
                    </span>
                  </span>
                  {mod.status === "alreadyThere" ? (
                    <Chip className="relative">{t("modCenter.migrate.status.alreadyThere")}</Chip>
                  ) : null}
                </button>
              </motion.li>
            );
          })}
        </ul>
        <div className="relative mt-3 flex min-h-10 items-center justify-end">
          <AnimatePresence initial={false} mode="popLayout">
            {progress ? (
              <motion.div key="progress" {...swap(0.2)} className="w-full">
                <JobLine progress={progress} label={t("modCenter.migrate.progress")} muted />
              </motion.div>
            ) : (
              <motion.div key="run" {...swap(0.2)}>
                <Button
                  variant={intoActive ? "secondary" : "primary"}
                  size={intoActive ? "sm" : "md"}
                  disabled={chosen.size === 0}
                  icon={<ArrowRight size={16} />}
                  onClick={() => void run()}
                >
                  {t("modCenter.migrate.run", { count: chosen.size })}
                </Button>
              </motion.div>
            )}
          </AnimatePresence>
        </div>
      </div>
    );
  }

  return (
    <div className="relative">
      <AnimatePresence initial={false} mode="popLayout">
        <motion.div key={state} {...swap(0.2)}>
          {content}
        </motion.div>
      </AnimatePresence>
    </div>
  );
}

/** "Başka sürüme taşı": pick a version, then move compatible own mods into its set. */
export function MigrateDialog({
  instance,
  open,
  onClose,
}: {
  instance: InstanceSummary;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  const showSnapshots = usePrefs((s) => s.showSnapshots);
  const [target, setTarget] = useState<string | null>(null);
  const [direction, setDirection] = useState<Direction>(1);
  const versions = useQuery({
    queryKey: ["instance-versions", instance.id, showSnapshots],
    queryFn: () => api.modCenter.versions(instance.id, showSnapshots),
    enabled: open,
    staleTime: 60_000,
  });
  useEffect(() => {
    if (!open) setTarget(null);
  }, [open]);

  const pick = (version: string | null) => {
    setDirection(version ? 1 : -1);
    setTarget(version);
  };
  const options = (versions.data?.options ?? []).filter((o) => !o.current && o.available);
  const title = target
    ? t("modCenter.migrate.titleTo", { version: target })
    : t("modCenter.migrate.title");

  let body: ReactNode;
  if (target) {
    body = <MigrationPanel instanceId={instance.id} to={target} />;
  } else if (versions.isPending) {
    body = (
      <div aria-busy className="space-y-0.5">
        {["w-24", "w-32", "w-20", "w-28"].map((width) => (
          <div key={width} className="flex h-11 items-center gap-3 px-3">
            <Skeleton className="h-5 w-14" />
            <Skeleton className="h-[22px] w-14" />
            <span className="flex-1" />
            <Skeleton className={`h-3 ${width}`} />
          </div>
        ))}
      </div>
    );
  } else if (options.length === 0) {
    body = <p className="text-[13px] text-ash-300">{t("modCenter.migrate.noVersions")}</p>;
  } else {
    body = (
      <ul className="max-h-[52vh] space-y-0.5 overflow-y-auto">
        {options.map((option) => (
          <li key={option.minecraft}>
            <button
              type="button"
              onClick={() => pick(option.minecraft)}
              className="group/opt relative flex w-full cursor-pointer items-center gap-3 rounded-[var(--radius-panel)] px-3 py-2.5 text-left focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-gold-300"
            >
              <span
                aria-hidden
                className="pointer-events-none absolute inset-0 rounded-[inherit] bg-white/5 opacity-0 transition-opacity duration-150 group-hover/opt:opacity-100"
              />
              <span className="relative font-display text-[16px] font-semibold text-bone-50 tabular-nums">
                {option.minecraft}
              </span>
              {option.recommended ? (
                <Chip className="relative">{t("modCenter.versions.recommended")}</Chip>
              ) : null}
              {option.snapshot ? (
                <Chip className="relative">{t("modCenter.versions.snapshot")}</Chip>
              ) : null}
              {option.hasSet ? (
                <Chip className="relative">{t("modCenter.versions.hasSet")}</Chip>
              ) : null}
              <span className="flex-1" />
              {option.lastPlayedAt ? (
                <span className="relative text-[12px] text-ash-400">
                  {t("modCenter.versions.lastPlayed", {
                    when: formatRelative(option.lastPlayedAt),
                  })}
                </span>
              ) : null}
            </button>
          </li>
        ))}
      </ul>
    );
  }

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && onClose()}
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
      width={600}
      morph={prefs.layout}
      // A list of choices applied as a whole: a stray click beside it keeps them.
      dismissOnOutsideClick={false}
      footer={
        target ? (
          <motion.div {...fade(0.2)}>
            <Button variant="ghost" onClick={() => pick(null)}>
              {t("modCenter.migrate.otherVersion")}
            </Button>
          </motion.div>
        ) : undefined
      }
    >
      <AnimatePresence mode="popLayout" initial={false} custom={direction}>
        <motion.div
          key={target ?? "versions"}
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
