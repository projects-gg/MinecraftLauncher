import { toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useEffect, useRef } from "react";
import { useTranslation } from "react-i18next";
import { copyErrorDetails } from "@/features/content/jobs";
import { ImportDialog } from "./import-dialog";
import { useImportJob } from "./import-job";
import { failureMessage, ImportSummaryDialog } from "./import-status";
import { summaryTitle } from "./model";

/**
 * Lives in the app shell: Settings' import dialog, the summary dialog, and after the welcome
 * flow a toast when a run ends (with "Özet"). Whatever a run changed is read again when it ends.
 */
export function ImportHost({ onboarding }: { onboarding: boolean }) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const finishedAt = useImportJob((s) => s.finishedAt);
  const seen = useRef(finishedAt);
  useEffect(() => {
    if (finishedAt === undefined || finishedAt === seen.current) return;
    seen.current = finishedAt;
    void queryClient.invalidateQueries();
    const job = useImportJob.getState();
    // The welcome flow's card and Settings' dialog say it themselves.
    if (onboarding || job.dialogOpen) return;
    if (job.status === "failed") {
      toast.error(failureMessage(job.error, t), {
        action: { label: t("common.copyDetails"), onClick: () => void copyErrorDetails(job.error) },
      });
      return;
    }
    if (!job.report) return;
    const key = summaryTitle(job.report);
    const show = { label: t("import.done.summary"), onClick: () => job.setSummaryOpen(true) };
    if (key === "title") toast.success(t("import.done.title"), { action: show });
    else toast(t(`import.done.${key}`), { action: show });
  }, [finishedAt, onboarding, queryClient, t]);
  return (
    <>
      <ImportDialog />
      <ImportSummaryDialog />
    </>
  );
}
