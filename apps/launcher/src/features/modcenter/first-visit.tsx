import { Check, CheckCircle, Plus } from "@phosphor-icons/react";
import { Button, Chip, cn, Skeleton, toast } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { newJobId, showError } from "@/features/content/jobs";
import { api, type JobProgress, type SwitchResult } from "@/lib/api";
import { MigrationPanel } from "./migrate-dialog";
import { enter, type MotionPrefs, rise, stagger, swap, useMotionPrefs } from "./motion";
import { JobLine } from "./parts";

/** One block of the result, rising in after the ones above it. */
function Section({
  prefs,
  index,
  className,
  children,
}: {
  prefs: MotionPrefs;
  index: number;
  className?: string;
  children: ReactNode;
}) {
  const motionProps = rise(prefs.reduce, 8, 0.3);
  return (
    <motion.div
      initial={motionProps.initial}
      animate={{
        ...motionProps.animate,
        transition: { ...enter(0.3), delay: 0.04 + stagger(index, 0.06) },
      }}
      className={className}
    >
      {children}
    </motion.div>
  );
}

/** Chips that settle one after another (short, capped stagger), quiet: new is not a second gold. */
function ChipRow({
  names,
  prefs,
  delay,
  children,
}: {
  names: string[];
  prefs: MotionPrefs;
  delay: number;
  children?: ReactNode;
}) {
  return (
    <div className="flex flex-wrap items-center gap-1.5">
      {names.map((name, i) => (
        <motion.span
          key={name}
          initial={{ opacity: 0, transform: prefs.reduce ? "none" : "scale(0.92)" }}
          animate={{
            opacity: 1,
            transform: prefs.reduce ? "none" : "scale(1)",
            transition: { ...enter(0.22), delay: delay + stagger(i, 0.03) },
          }}
          className="inline-flex"
        >
          <Chip>{name}</Chip>
        </motion.span>
      ))}
      {children}
    </div>
  );
}

type OwnPhase = "checking" | "found" | "none" | "fallback" | "full" | "moving" | "moved";

/**
 * The player's own mods left in the previous set: one Modrinth request finds which have a
 * version for the new one; "Taşı" moves them (chips toggle a mod off). When Modrinth cannot be
 * reached the full search stays on a button.
 */
function OwnMods({
  instanceId,
  result,
  prefs,
}: {
  instanceId: string;
  result: SwitchResult;
  prefs: MotionPrefs;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const check = useQuery({
    queryKey: ["migration-check", instanceId, result.from, result.to],
    queryFn: () => api.modCenter.migrationCheck(instanceId, result.from, result.to),
    staleTime: Number.POSITIVE_INFINITY,
    retry: false,
  });
  const [skipped, setSkipped] = useState<Set<string>>(() => new Set());
  const [progress, setProgress] = useState<JobProgress | null>(null);
  const [moved, setMoved] = useState<number | null>(null);
  const [full, setFull] = useState(false);

  const compatible = check.data?.compatible ?? [];
  const chosen = compatible.filter((m) => !skipped.has(m.versionId));
  const phase: OwnPhase =
    moved !== null
      ? "moved"
      : progress
        ? "moving"
        : full
          ? "full"
          : check.isPending
            ? "checking"
            : check.isError || check.data?.offline
              ? "fallback"
              : compatible.length > 0
                ? "found"
                : "none";

  const move = async () => {
    setProgress({ stage: "resolving", doneFiles: 0, totalFiles: 0, bytesPerSec: 0 });
    try {
      const done = await api.modCenter.migrate(
        instanceId,
        result.to,
        chosen.map((m) => m.versionId),
        newJobId(),
        (p) => setProgress(p),
      );
      // The mods the player moved (their dependencies come along without being counted).
      setMoved(chosen.length);
      if (done.unresolved > 0) toast(t("modCenter.migrate.unresolved"));
      for (const key of [
        ["instance", instanceId],
        ["instance-versions", instanceId],
        ["content-scan", instanceId, "mod"],
        ["mod-center", instanceId],
      ]) {
        void queryClient.invalidateQueries({ queryKey: key });
      }
    } catch (error) {
      showError(error);
    } finally {
      setProgress(null);
    }
  };

  const toggle = (versionId: string) =>
    setSkipped((prev) => {
      const next = new Set(prev);
      if (next.has(versionId)) next.delete(versionId);
      else next.add(versionId);
      return next;
    });

  let body: ReactNode = null;
  if (phase === "checking") {
    body = (
      <div
        role="status"
        aria-busy
        aria-label={t("modCenter.migrate.searching")}
        className="flex items-center gap-1.5"
      >
        <Skeleton className="h-6 w-20" />
        <Skeleton className="h-6 w-24" />
        <span className="flex-1" />
        <Skeleton className="h-8 w-28" />
      </div>
    );
  } else if (phase === "found") {
    body = (
      <div className="flex items-end gap-3">
        <div className="min-w-0 flex-1">
          <p className="mb-1.5 text-[12.5px] text-ash-400">{t("modCenter.switch.ownCompatible")}</p>
          <div className="flex flex-wrap gap-1.5">
            {compatible.map((mod, i) => {
              const on = !skipped.has(mod.versionId);
              return (
                <motion.button
                  key={mod.versionId}
                  type="button"
                  aria-pressed={on}
                  aria-label={t("modCenter.switch.ownToggle", { name: mod.title })}
                  onClick={() => toggle(mod.versionId)}
                  initial={{ opacity: 0, transform: prefs.reduce ? "none" : "scale(0.92)" }}
                  animate={{
                    opacity: 1,
                    transform: prefs.reduce ? "none" : "scale(1)",
                    transition: { ...enter(0.22), delay: stagger(i, 0.03) },
                  }}
                  className={cn(
                    "chamfer inline-flex h-6 cursor-pointer items-center gap-1 px-2 text-[12px] font-semibold transition-[background-color,color,scale] duration-150 [--cut:5px] active:scale-[0.97]",
                    "focus-visible:shadow-[inset_0_0_0_2px_var(--color-gold-300)] focus-visible:outline-none",
                    on
                      ? "bg-white/10 text-bone-100 hover:bg-white/14"
                      : "bg-white/[0.035] text-ash-400 hover:bg-white/7 hover:text-ash-300",
                  )}
                >
                  <span className="relative grid size-3 place-items-center">
                    <AnimatePresence initial={false} mode="popLayout">
                      <motion.span
                        key={on ? "on" : "off"}
                        initial={{ opacity: 0, transform: prefs.reduce ? "none" : "scale(0.6)" }}
                        animate={{
                          opacity: 1,
                          transform: prefs.reduce ? "none" : "scale(1)",
                          transition: enter(0.18),
                        }}
                        exit={{ opacity: 0, transition: { duration: 0.08 } }}
                        className="grid place-items-center"
                      >
                        {on ? (
                          <Check size={12} weight="bold" className="text-gold-300" />
                        ) : (
                          <Plus size={12} weight="bold" />
                        )}
                      </motion.span>
                    </AnimatePresence>
                  </span>
                  {mod.title}
                </motion.button>
              );
            })}
          </div>
        </div>
        <Button
          size="sm"
          variant="secondary"
          disabled={chosen.length === 0}
          onClick={() => void move()}
        >
          {t("modCenter.migrate.run", { count: chosen.length })}
        </Button>
      </div>
    );
  } else if (phase === "none") {
    body = <p className="text-[13px] text-ash-400">{t("modCenter.switch.ownNone")}</p>;
  } else if (phase === "fallback") {
    body = (
      <Button size="sm" variant="secondary" onClick={() => setFull(true)}>
        {t("modCenter.switch.ownFind")}
      </Button>
    );
  } else if (phase === "full") {
    body = <MigrationPanel instanceId={instanceId} from={result.from} to={result.to} intoActive />;
  } else if (phase === "moving") {
    body = (
      <div className="flex min-h-8 flex-col justify-center">
        <JobLine progress={progress} label={t("modCenter.migrate.progress")} muted />
      </div>
    );
  } else {
    body = (
      <p className="flex items-center gap-2 text-[13.5px] text-bone-100" aria-live="polite">
        <CheckCircle size={18} weight="fill" className="shrink-0 text-jade-400" />
        {t("modCenter.migrate.doneHere", { count: moved ?? 0 })}
      </p>
    );
  }

  return (
    <div className="rounded-[var(--radius-panel)] border border-white/8 bg-white/[0.03] px-4 py-3">
      <p className="mb-2.5 text-[13.5px] font-semibold text-bone-50">
        {t("modCenter.switch.ownTitle", { count: result.ownModsLeft, version: result.from })}
      </p>
      <div className="relative">
        <AnimatePresence initial={false} mode="popLayout">
          <motion.div key={phase} {...swap(0.2)}>
            {body}
          </motion.div>
        </AnimatePresence>
      </div>
    </div>
  );
}

/**
 * The page after a version switch (the version dialog's last step): the choices that came
 * along, new recommendations and, after a first visit, the player's own mods.
 */
export function FirstVisitResult({
  instanceId,
  result,
  onOpenModCenter,
}: {
  instanceId: string;
  result: SwitchResult;
  onOpenModCenter?: () => void;
}) {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  let index = 0;
  const blocks: ReactNode[] = [];
  if (result.carried.length > 0) {
    const at = index++;
    blocks.push(
      <Section key="carried" prefs={prefs} index={at}>
        <p className="mb-1.5 text-[12.5px] text-ash-400">{t("modCenter.switch.carriedList")}</p>
        <ChipRow names={result.carried} prefs={prefs} delay={0.08 + stagger(at, 0.06)} />
      </Section>,
    );
  }
  if (result.newItems.length > 0) {
    const at = index++;
    blocks.push(
      <Section key="new" prefs={prefs} index={at}>
        <p className="mb-1.5 text-[12.5px] text-ash-400">{t("modCenter.switch.newList")}</p>
        <ChipRow names={result.newItems} prefs={prefs} delay={0.08 + stagger(at, 0.06)}>
          {onOpenModCenter ? (
            <Button size="sm" variant="ghost" onClick={onOpenModCenter}>
              {t("modCenter.switch.openModCenter")}
            </Button>
          ) : null}
        </ChipRow>
      </Section>,
    );
  }
  if (!result.synced) {
    blocks.push(
      <Section key="sync" prefs={prefs} index={index++}>
        <p className="text-[12.5px] text-ash-300">{t("modCenter.switch.notSynced")}</p>
      </Section>,
    );
  }
  if (result.firstVisit && result.ownModsLeft > 0) {
    blocks.push(
      <Section key="own" prefs={prefs} index={index++}>
        <OwnMods instanceId={instanceId} result={result} prefs={prefs} />
      </Section>,
    );
  }
  return <div className="space-y-4">{blocks}</div>;
}
