import { create } from "zustand";
import { api, type ImportProgress, type ImportReport, type ImportRequest } from "@/lib/api";
import { notifyWhenAway } from "@/lib/attention";
import i18n from "@/lib/i18n";
import { trackTask } from "@/stores/progress";

export type ImportStatus = "idle" | "running" | "done" | "failed";

const newJobId = () =>
  typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `import-${Date.now()}-${Math.random().toString(36).slice(2)}`;

interface ImportJobState {
  status: ImportStatus;
  jobId?: string;
  progress?: ImportProgress;
  report?: ImportReport;
  /** The import was refused or failed as a whole (a game opened meanwhile, the data folder moves). */
  error?: unknown;
  stopping: boolean;
  /** Settings → Veri's import dialog. */
  dialogOpen: boolean;
  summaryOpen: boolean;
  /** When the last run ended (screens refresh what it changed). */
  finishedAt?: number;
  start(request: ImportRequest): void;
  cancel(): void;
  setDialogOpen(open: boolean): void;
  setSummaryOpen(open: boolean): void;
  /** Forgets a finished run. */
  clear(): void;
}

/**
 * The one import that runs (the first-run step and Settings share it). It goes on in the
 * background: the welcome flow moves on, a dialog may close; the taskbar button shows its
 * progress, and a long run that ends out of sight notifies.
 */
export const useImportJob = create<ImportJobState>((set, get) => ({
  status: "idle",
  stopping: false,
  dialogOpen: false,
  summaryOpen: false,
  start(request) {
    if (get().status === "running") return;
    const jobId = newJobId();
    const task = trackTask(`import:${jobId}`);
    set({
      status: "running",
      jobId,
      progress: undefined,
      report: undefined,
      error: undefined,
      stopping: false,
      summaryOpen: false,
    });
    api.importer
      .start(request, jobId, (progress) => {
        set({ progress });
        task.report(importPercent(progress));
      })
      .then(
        (report) => {
          const ranMs = task.done();
          set({ status: "done", report, finishedAt: Date.now() });
          void notifyWhenAway(
            ranMs,
            i18n.t("import.attention.title"),
            i18n.t("import.attention.body"),
          );
        },
        (error: unknown) => {
          task.done();
          set({ status: "failed", error, finishedAt: Date.now() });
        },
      );
  },
  cancel() {
    const { jobId, status, stopping } = get();
    if (status !== "running" || !jobId || stopping) return;
    set({ stopping: true });
    void api.importer.cancel(jobId);
  },
  setDialogOpen: (dialogOpen) => set({ dialogOpen }),
  setSummaryOpen: (summaryOpen) => set({ summaryOpen }),
  clear() {
    if (get().status === "running") return;
    set({
      status: "idle",
      jobId: undefined,
      progress: undefined,
      report: undefined,
      error: undefined,
      stopping: false,
      summaryOpen: false,
    });
  },
}));

/** How far a run is, by bytes (unknown while it prepares). */
export function importPercent(progress: ImportProgress | undefined): number | undefined {
  if (!progress || progress.stage === "preparing" || progress.totalBytes <= 0) return undefined;
  return Math.min(100, (progress.doneBytes / progress.totalBytes) * 100);
}
