import {
  ArrowsClockwise,
  CheckCircle,
  MinusCircle,
  PlusCircle,
  ShieldCheck,
} from "@phosphor-icons/react";
import { Button, Chip, cn, Dialog, Skeleton, toast, useInteractionMotion } from "@pl/ui";
import { useQuery, useQueryClient } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { isCancelled, newJobId, type RunningJob, showError } from "@/features/content/jobs";
import { JobStatus, ProjectIcon } from "@/features/content/parts";
import {
  api,
  type InstanceDetail,
  type ModpackUpdate,
  type ModpackUpdatePlan,
  type PackChange,
} from "@/lib/api";
import { formatBytes } from "@/lib/format";
import { trackTask } from "@/stores/progress";
import { keptOrder, summarizeChanges } from "./modpack-changes";
import { useGameRunning } from "./use-game-running";

const ease = [0.16, 1, 0.3, 1] as const;

/** One group of the plan: a heading with a count, add-ons by name, other files as a count. */
function ChangeGroup({
  icon,
  title,
  items,
  index,
}: {
  icon: ReactNode;
  title: string;
  items: PackChange[];
  index: number;
}) {
  const { t } = useTranslation();
  const move = useInteractionMotion();
  if (items.length === 0) return null;
  const summary = summarizeChanges(items);
  return (
    <motion.section
      initial={{ opacity: 0, transform: move ? "translateY(6px)" : "none" }}
      animate={{
        opacity: 1,
        transform: "translateY(0px)",
        transition: { duration: 0.26, ease, delay: 0.04 + index * 0.05 },
      }}
    >
      <h3 className="mb-1.5 flex items-center gap-2 text-[12.5px] font-medium text-ash-300">
        {icon}
        {title}
        <span className="text-ash-500 tabular-nums">{items.length}</span>
      </h3>
      <p className="text-[13.5px] leading-relaxed text-bone-100">
        {[
          ...summary.named.map((item) => item.name),
          summary.more > 0 ? t("modpack.update.more", { count: summary.more }) : null,
          summary.files > 0 ? t("modpack.update.settingsFiles", { count: summary.files }) : null,
        ]
          .filter(Boolean)
          .join(", ")}
      </p>
    </motion.section>
  );
}

function PlanSkeleton() {
  return (
    <div aria-busy className="space-y-5">
      {["w-2/3", "w-1/2", "w-3/5"].map((width) => (
        <div key={width} className="space-y-2">
          <Skeleton className="h-3.5 w-28" />
          <Skeleton className={cn("h-4", width)} />
        </div>
      ))}
    </div>
  );
}

type Step =
  | { kind: "planning"; job: RunningJob }
  | { kind: "plan"; plan: ModpackUpdatePlan }
  | { kind: "applying"; plan: ModpackUpdatePlan; job: RunningJob }
  | { kind: "failed"; message: string };

/** "Neler değişecek?" and the update itself, in one dialog. */
function UpdateDialog({
  instance,
  update,
  title,
  open,
  onClose,
}: {
  instance: InstanceDetail;
  update: ModpackUpdate;
  title: string;
  open: boolean;
  onClose: () => void;
}) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const running = useGameRunning(instance.id);
  const [step, setStep] = useState<Step | null>(null);
  const planJob = useRef<string | null>(null);

  useEffect(() => {
    if (!open) return;
    const jobId = newJobId();
    planJob.current = jobId;
    setStep({ kind: "planning", job: { jobId } });
    api.content
      .modpackUpdatePlan(instance.id, update.versionId, jobId, (progress) => {
        if (planJob.current === jobId) setStep({ kind: "planning", job: { jobId, progress } });
      })
      .then((plan) => {
        if (planJob.current === jobId) setStep({ kind: "plan", plan });
      })
      .catch((error: unknown) => {
        if (planJob.current !== jobId || isCancelled(error)) return;
        setStep({
          kind: "failed",
          message: error instanceof Error && error.message ? error.message : String(error),
        });
      });
    return () => {
      planJob.current = null;
      void api.content.cancel(jobId);
    };
  }, [open, instance.id, update.versionId]);

  const apply = async (plan: ModpackUpdatePlan) => {
    const jobId = newJobId();
    setStep({ kind: "applying", plan, job: { jobId } });
    const task = trackTask(`modpack-update:${jobId}`);
    try {
      const result = await api.content.modpackUpdateApply(
        instance.id,
        update.versionId,
        jobId,
        (progress) => {
          setStep({ kind: "applying", plan, job: { jobId, progress } });
          task.report(progress.percent);
        },
      );
      toast.success(t("modpack.update.done", { name: title, version: result.versionNumber }));
      await Promise.all(
        [
          ["instance", instance.id],
          ["modpack-info", instance.id],
          ["modpack-update", instance.id],
          ["content-scan", instance.id],
          ["mod-center", instance.id],
        ].map((queryKey) => queryClient.invalidateQueries({ queryKey })),
      );
      onClose();
    } catch (error) {
      if (!isCancelled(error)) showError(error);
      setStep({ kind: "plan", plan });
    } finally {
      task.done();
    }
  };

  const plan = step && (step.kind === "plan" || step.kind === "applying") ? step.plan : null;
  const busy = step?.kind === "applying";
  const kept = plan ? [...plan.kept].sort((a, b) => keptOrder(a.reason) - keptOrder(b.reason)) : [];
  const nothing = plan && plan.added.length + plan.updated.length + plan.removed.length === 0;

  return (
    <Dialog
      open={open}
      onOpenChange={(next) => !next && !busy && onClose()}
      title={t("modpack.update.title", { version: update.versionNumber })}
      description={plan?.fromVersion ? `${title} ${plan.fromVersion}` : title}
      closeLabel={t("common.close")}
      width={520}
      morph
      dismissible={!busy}
      footer={
        step?.kind === "applying" ? (
          <JobStatus job={step.job} className="w-full" detailed />
        ) : step?.kind === "planning" ? (
          <>
            <JobStatus job={step.job} className="mr-auto w-56" />
            <Button variant="ghost" onClick={onClose}>
              {t("instance.cancel")}
            </Button>
          </>
        ) : (
          <>
            <Button variant="ghost" onClick={onClose}>
              {step?.kind === "failed" ? t("common.close") : t("instance.cancel")}
            </Button>
            {plan ? (
              <Button
                variant="primary"
                icon={<ArrowsClockwise size={16} weight="bold" />}
                disabled={running}
                onClick={() => void apply(plan)}
              >
                {t("modpack.update.submit")}
              </Button>
            ) : null}
          </>
        )
      }
    >
      <AnimatePresence mode="popLayout" initial={false}>
        {step?.kind === "planning" || !step ? (
          <motion.div key="planning" exit={{ opacity: 0, transition: { duration: 0.12 } }}>
            <PlanSkeleton />
          </motion.div>
        ) : step.kind === "failed" ? (
          <motion.p
            key="failed"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1 }}
            className="text-[13.5px] text-bone-100"
          >
            {step.message}
          </motion.p>
        ) : plan ? (
          <motion.div
            key="plan"
            initial={{ opacity: 0 }}
            animate={{ opacity: 1, transition: { duration: 0.2 } }}
            className="space-y-4"
          >
            {nothing ? (
              <p className="text-[13.5px] text-ash-300">{t("modpack.update.nothing")}</p>
            ) : null}
            <ChangeGroup
              index={0}
              icon={<PlusCircle size={15} weight="fill" className="text-jade-400" />}
              title={t("modpack.update.added")}
              items={plan.added}
            />
            <ChangeGroup
              index={1}
              icon={<ArrowsClockwise size={15} weight="bold" className="text-gold-300" />}
              title={t("modpack.update.updated")}
              items={plan.updated}
            />
            <ChangeGroup
              index={2}
              icon={<MinusCircle size={15} weight="fill" className="text-flare-400" />}
              title={t("modpack.update.removed")}
              items={plan.removed}
            />
            {kept.length > 0 ? (
              <motion.section
                initial={{ opacity: 0 }}
                animate={{ opacity: 1, transition: { duration: 0.26, delay: 0.2 } }}
              >
                <h3 className="mb-1.5 flex items-center gap-2 text-[12.5px] font-medium text-ash-300">
                  <ShieldCheck size={15} weight="fill" className="text-ash-300" />
                  {t("modpack.update.kept")}
                  <span className="text-ash-500 tabular-nums">{kept.length}</span>
                </h3>
                <ul className="space-y-1">
                  {kept.slice(0, 6).map(({ item, reason }) => (
                    <li key={item.path} className="flex items-baseline gap-3 text-[13px]">
                      <span className="min-w-0 truncate text-bone-100">{item.name}</span>
                      <span className="shrink-0 text-[12px] text-ash-400">
                        {t(`modpack.update.keptReason.${reason}`)}
                      </span>
                    </li>
                  ))}
                  {kept.length > 6 ? (
                    <li className="text-[12.5px] text-ash-400">
                      {t("modpack.update.more", { count: kept.length - 6 })}
                    </li>
                  ) : null}
                </ul>
              </motion.section>
            ) : null}
            <div className="flex flex-wrap gap-x-5 gap-y-1 border-t border-white/6 pt-3 text-[12.5px] text-ash-400">
              {plan.loaderTo && plan.loaderFrom ? (
                <span>
                  {t("modpack.update.loader", {
                    loader: t(`loaders.${instance.loader}`),
                    from: plan.loaderFrom,
                    to: plan.loaderTo,
                  })}
                </span>
              ) : null}
              {plan.downloadBytes > 0 ? (
                <span className="tabular-nums">
                  {t("modpack.update.download", { size: formatBytes(plan.downloadBytes) })}
                </span>
              ) : null}
              {running ? (
                <span className="text-gold-300">{t("modpack.update.gameRunning")}</span>
              ) : null}
            </div>
          </motion.div>
        ) : null}
      </AnimatePresence>
    </Dialog>
  );
}

/**
 * The Modrinth modpack an instance was made from, on its overview: name, version and, when a
 * newer version is out (asked once per visit), the update.
 */
export function ModpackCard({ instance }: { instance: InstanceDetail }) {
  const { t } = useTranslation();
  const [open, setOpen] = useState(false);
  const info = useQuery({
    queryKey: ["modpack-info", instance.id],
    queryFn: () => api.content.modpackInfo(instance.id),
  });
  const update = useQuery({
    queryKey: ["modpack-update", instance.id],
    queryFn: () => api.content.modpackUpdateCheck(instance.id),
    enabled: Boolean(info.data),
    staleTime: 30 * 60_000,
    retry: false,
  });
  const pack = info.data;
  if (!pack) return null;
  const title = pack.title ?? t("modpack.update.card");
  return (
    <motion.section
      initial={{ opacity: 0 }}
      animate={{ opacity: 1, transition: { duration: 0.22 } }}
      className="glass mt-5 flex max-w-[880px] items-center gap-4 rounded-[var(--radius-panel)] px-4 py-3.5"
    >
      <ProjectIcon src={pack.iconUrl} size={44} />
      <div className="min-w-0 flex-1">
        <p className="text-[12px] text-ash-400">{t("modpack.update.card")}</p>
        <p className="truncate font-display text-[16px] font-semibold text-bone-50">
          {title}
          {pack.versionNumber ? (
            <span className="ml-2 font-sans text-[13px] font-medium text-ash-300 tabular-nums">
              {pack.versionNumber}
            </span>
          ) : null}
        </p>
      </div>
      <div className="relative flex shrink-0 items-center justify-end gap-2.5">
        <AnimatePresence mode="popLayout" initial={false}>
          {update.isPending && update.fetchStatus !== "idle" ? (
            <motion.span
              key="checking"
              exit={{ opacity: 0, transition: { duration: 0.12 } }}
              className="text-[12.5px] text-ash-400"
            >
              {t("modpack.update.checking")}
            </motion.span>
          ) : update.isError ? (
            <motion.div key="error" initial={{ opacity: 0 }} animate={{ opacity: 1 }}>
              <Button size="sm" variant="ghost" onClick={() => void update.refetch()}>
                {t("modpack.update.retry")}
              </Button>
            </motion.div>
          ) : update.data ? (
            <motion.div
              key="available"
              initial={{ opacity: 0, transform: "translateY(4px)" }}
              animate={{
                opacity: 1,
                transform: "translateY(0px)",
                transition: { duration: 0.24, ease },
              }}
              className="flex items-center gap-2.5"
            >
              <Chip tone="gold">
                {t("modpack.update.available", { version: update.data.versionNumber })}
              </Chip>
              <Button
                size="sm"
                variant="primary"
                icon={<ArrowsClockwise size={15} weight="bold" />}
                onClick={() => setOpen(true)}
              >
                {t("modpack.update.action")}
              </Button>
            </motion.div>
          ) : update.isSuccess ? (
            <motion.span
              key="current"
              initial={{ opacity: 0 }}
              animate={{ opacity: 1 }}
              className="flex items-center gap-1.5 text-[12.5px] text-jade-400"
            >
              <CheckCircle size={15} weight="fill" />
              {t("modpack.update.upToDate")}
            </motion.span>
          ) : null}
        </AnimatePresence>
      </div>
      {update.data ? (
        <UpdateDialog
          instance={instance}
          update={update.data}
          title={title}
          open={open}
          onClose={() => setOpen(false)}
        />
      ) : null}
    </motion.section>
  );
}
