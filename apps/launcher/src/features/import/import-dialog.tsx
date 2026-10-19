import { DownloadSimple } from "@phosphor-icons/react";
import { Button, Dialog, Progress, Skeleton, SwapLabel, toast } from "@pl/ui";
import { useQuery } from "@tanstack/react-query";
import { AnimatePresence, motion } from "motion/react";
import { type ReactNode, useState } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails } from "@/features/content/jobs";
import { stepVariants, useMotionPrefs } from "@/features/modcenter/motion";
import { api } from "@/lib/api";
import { copyText } from "@/lib/clipboard";
import { formatBytes, formatPercent } from "@/lib/format";
import { ImportPanel, ImportTotals } from "./import-choices";
import { importPercent, useImportJob } from "./import-job";
import { ImportSummary, useSummaryTitle } from "./import-status";
import { failureDetails } from "./model";
import { importKeys, useImportPicker } from "./use-import-picker";

type View = "pick" | "running" | "done";

/**
 * Settings → Veri → "Başka launcher'dan aktar": the same rows as the welcome flow's step, then
 * the progress (it may go on in the background: "Arka planda sürsün" closes the dialog, a toast
 * says when it ends), then what came over. The panel glides to each view's size while the
 * views slide in.
 */
export function ImportDialog() {
  const { t } = useTranslation();
  const prefs = useMotionPrefs();
  const open = useImportJob((s) => s.dialogOpen);
  const setOpen = useImportJob((s) => s.setDialogOpen);
  const status = useImportJob((s) => s.status);
  const jobId = useImportJob((s) => s.jobId);
  const report = useImportJob((s) => s.report);
  const error = useImportJob((s) => s.error);
  const progress = useImportJob((s) => s.progress);
  const stopping = useImportJob((s) => s.stopping);
  const start = useImportJob((s) => s.start);
  const cancel = useImportJob((s) => s.cancel);
  // The run this dialog started: its end shows here; an older one's does not.
  const [ownJob, setOwnJob] = useState<string>();
  const scan = useQuery({
    queryKey: importKeys.scan,
    queryFn: () => api.importer.scan(),
    enabled: open,
    staleTime: 0,
  });
  const picker = useImportPicker(open ? scan.data : undefined);
  const summaryTitle = useSummaryTitle();

  const view: View =
    status === "running"
      ? "running"
      : ownJob && ownJob === jobId && status !== "idle"
        ? "done"
        : "pick";

  const close = () => {
    setOpen(false);
    setOwnJob(undefined);
  };
  const begin = () => {
    start(picker.request);
    setOwnJob(useImportJob.getState().jobId);
  };
  const details = report ? failureDetails(report) : "";
  const percent = importPercent(progress);

  let body: ReactNode;
  let footer: ReactNode;
  if (view === "pick") {
    const found = scan.data && scan.data.sources.length > 0;
    body = scan.isPending ? (
      <div className="space-y-2.5 pt-1" aria-busy>
        <Skeleton className="h-5 w-1/3" />
        <Skeleton className="h-[52px] w-full" />
        <Skeleton className="h-[52px] w-full" />
      </div>
    ) : found ? (
      <div className="pt-1">
        <ImportPanel picker={picker} />
        <ImportTotals picker={picker} className="mt-2.5" />
      </div>
    ) : (
      <p className="pt-1 text-[13.5px] text-ash-300">{t("import.settings.notFound")}</p>
    );
    footer = found ? (
      <>
        <Button variant="ghost" onClick={close}>
          {t("import.settings.later")}
        </Button>
        <Button
          variant="primary"
          icon={<DownloadSimple size={16} weight="bold" />}
          disabled={picker.empty || !picker.enough || picker.pending}
          onClick={begin}
        >
          {t("import.step.import")}
        </Button>
      </>
    ) : (
      <Button variant="primary" onClick={close} disabled={scan.isPending}>
        {t("import.done.close")}
      </Button>
    );
  } else if (view === "running") {
    body = (
      <div className="pt-1" aria-live="polite">
        <div className="flex items-baseline justify-between gap-4">
          <p className="text-[14px] font-medium text-bone-50">
            {progress && progress.stage !== "preparing"
              ? t("import.progress.title")
              : t("import.progress.preparing")}
          </p>
          <p className="font-display text-[20px] font-semibold text-gold-200 tabular-nums">
            {percent !== undefined ? formatPercent(Math.floor(percent)) : ""}
          </p>
        </div>
        <Progress className="mt-3" value={percent} label={t("import.progress.label")} />
        <div className="mt-2.5 flex justify-between gap-4 text-[12.5px] text-ash-400 tabular-nums">
          <span className="min-w-0 truncate">{progress?.current ?? ""}</span>
          <span className="shrink-0">
            {progress && progress.totalBytes > 0
              ? `${formatBytes(Math.min(progress.doneBytes, progress.totalBytes))} / ${formatBytes(progress.totalBytes)}`
              : ""}
          </span>
        </div>
      </div>
    );
    footer = (
      <>
        <Button variant="ghost" className="mr-auto" onClick={close}>
          {t("import.settings.background")}
        </Button>
        <Button variant="secondary" disabled={stopping} onClick={cancel}>
          <SwapLabel
            labels={[t("import.progress.cancel"), t("import.progress.stopping")]}
            active={stopping ? 1 : 0}
          />
        </Button>
      </>
    );
  } else {
    body = report ? (
      <ImportSummary report={report} />
    ) : (
      <p className="pt-1 text-[13.5px] text-bone-100">
        {error instanceof Error && error.message ? error.message : t("import.done.titleFailed")}
      </p>
    );
    footer = (
      <>
        {details || error ? (
          <Button
            variant="ghost"
            className="mr-auto"
            onClick={() =>
              details
                ? void copyText(details).then(
                    () => toast.success(t("common.detailsCopied")),
                    () => toast.error(t("common.copyFailed")),
                  )
                : void copyErrorDetails(error)
            }
          >
            {t("common.copyDetails")}
          </Button>
        ) : null}
        <Button variant="primary" onClick={close}>
          {t("import.done.close")}
        </Button>
      </>
    );
  }

  const variants = stepVariants(prefs.reduce);
  return (
    <Dialog
      open={open}
      onOpenChange={(next) => (next ? setOpen(true) : close())}
      title={view === "done" ? summaryTitle : t("import.settings.title")}
      description={view === "pick" ? t("import.settings.dialogBody") : undefined}
      closeLabel={t("common.close")}
      width={560}
      morph
      // A pick is made as a whole: a stray click beside the dialog keeps it.
      dismissOnOutsideClick={false}
      footer={footer}
    >
      <AnimatePresence mode="popLayout" initial={false} custom={1}>
        <motion.div
          key={view}
          custom={1}
          variants={variants}
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
