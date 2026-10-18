import { toast } from "@pl/ui";
import { useQueryClient } from "@tanstack/react-query";
import { useTranslation } from "react-i18next";
import { create } from "zustand";
import { DISK_FULL } from "@/features/disk/launch-error";
import { openMakeSpace } from "@/features/disk/make-space";
import {
  api,
  type ContentInstallResult,
  type ContentKind,
  type InstalledContent,
  type JobProgress,
} from "@/lib/api";
import { notifyWhenAway } from "@/lib/attention";
import { copyText } from "@/lib/clipboard";
import i18n from "@/lib/i18n";

/** A running install/update, keyed by `instanceId:projectId` (or `instanceId:kind:all`). */
export interface RunningJob {
  jobId: string;
  progress?: JobProgress;
}

interface JobsState {
  jobs: Record<string, RunningJob>;
  setJob: (key: string, job: RunningJob | undefined) => void;
}

export const useContentJobs = create<JobsState>((set) => ({
  jobs: {},
  setJob: (key, job) =>
    set((state) => {
      const jobs = { ...state.jobs };
      if (job) jobs[key] = job;
      else delete jobs[key];
      return { jobs };
    }),
}));

/**
 * Add-ons removed with "Geri al" still waiting for their toast to end, keyed
 * `instanceId:kind:fileName`: their rows stay hidden (also across tab switches) meanwhile.
 */
const usePendingRemovals = create<{
  keys: Record<string, true>;
  set: (key: string, pending: boolean) => void;
}>((set) => ({
  keys: {},
  set: (key, pending) =>
    set((state) => {
      const keys = { ...state.keys };
      if (pending) keys[key] = true;
      else delete keys[key];
      return { keys };
    }),
}));

/** How long "Geri al" is offered before a removed add-on really goes. */
const UNDO_MS = 6000;

/**
 * Removing an add-on with "Geri al": the row hides at once and the file goes (to the system's
 * trash) only when the toast ends, so a misclick costs nothing.
 */
export function useUndoableRemoval(instanceId: string, kind: ContentKind) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const keys = usePendingRemovals((s) => s.keys);
  const prefix = `${instanceId}:${kind}:`;
  const isHidden = (fileName: string) => Boolean(keys[prefix + fileName]);

  const remove = (fileName: string, name: string) => {
    const key = prefix + fileName;
    const pending = usePendingRemovals.getState();
    if (pending.keys[key]) return;
    pending.set(key, true);
    let settled = false;
    const commit = async () => {
      if (settled) return;
      settled = true;
      try {
        await api.content.remove(instanceId, kind, fileName);
        await queryClient.invalidateQueries({ queryKey: ["instance", instanceId] });
      } catch (error) {
        showError(error);
      } finally {
        usePendingRemovals.getState().set(key, false);
      }
    };
    toast(t("content.toasts.removed", { name }), {
      duration: UNDO_MS,
      action: {
        label: t("instance.trash.undo"),
        onClick: () => {
          settled = true;
          usePendingRemovals.getState().set(key, false);
        },
      },
      onAutoClose: () => void commit(),
      onDismiss: () => void commit(),
    });
  };

  return { isHidden, remove };
}

/** How long a finished job waits for the refreshed list before it lets go anyway. */
const REFRESH_WAIT_MS = 2500;

export const projectJobKey = (instanceId: string, projectId: string) =>
  `${instanceId}:${projectId}`;
export const bulkJobKey = (instanceId: string, kind: ContentKind) => `${instanceId}:${kind}:all`;

export function newJobId(): string {
  return typeof crypto !== "undefined" && "randomUUID" in crypto
    ? crypto.randomUUID()
    : `${Date.now()}-${Math.random().toString(36).slice(2)}`;
}

export function errorCode(error: unknown): string | undefined {
  return error && typeof error === "object" && "code" in error && typeof error.code === "string"
    ? error.code
    : undefined;
}

export const isCancelled = (error: unknown) => errorCode(error) === "IPTAL";
export const isOffline = (error: unknown) => errorCode(error) === "MODRINTH-AG";

/** The technical text behind a failed command (its code and details), for support. */
export function errorDetails(error: unknown): string {
  return error &&
    typeof error === "object" &&
    "details" in error &&
    typeof error.details === "string"
    ? `${errorCode(error) ?? ""}\n${error.details}`.trim()
    : String(error);
}

/** "Ayrıntıları kopyala": puts an error's technical details on the clipboard. */
export async function copyErrorDetails(error: unknown) {
  try {
    await copyText(errorDetails(error));
    toast.success(i18n.t("common.detailsCopied"));
  } catch {
    toast.error(i18n.t("common.copyFailed"));
  }
}

/**
 * Toast for a failed command; cancellations stay quiet. Details go to "copy details"; a disk
 * that cannot hold the job (`DISK-DOLU`, before anything was downloaded or on the way) offers
 * "Yer aç" instead, like OYNA does.
 */
export function showError(error: unknown) {
  if (isCancelled(error)) return;
  const message =
    error instanceof Error && error.message ? error.message : i18n.t("content.browse.errorTitle");
  if (errorCode(error) === DISK_FULL) {
    toast.error(message, {
      id: "content-disk",
      duration: 10_000,
      action: { label: i18n.t("disk.makeSpace"), onClick: openMakeSpace },
    });
    return;
  }
  toast.error(message, {
    action: {
      label: i18n.t("common.copyDetails"),
      onClick: () => void copyErrorDetails(error),
    },
  });
}

/** Label of a job stage ("İndiriliyor", "Dosyalar çıkarılıyor", ...). */
export function stageLabel(stage: string): string {
  return i18n.t(`content.stages.${stage}`, { defaultValue: i18n.t("content.stages.resolving") });
}

function names(list: Array<{ title?: string; fileName?: string; projectId?: string }>) {
  return list
    .map((x) => x.title ?? x.fileName ?? x.projectId ?? "")
    .filter(Boolean)
    .join(", ");
}

function announce(result: ContentInstallResult, title: string) {
  const dependencies = result.installed.filter((i) => i.dependency).length;
  if (result.installed.length === 0 && result.alreadyInstalled.length > 0) {
    toast(i18n.t("content.toasts.alreadyInstalled", { name: title }));
  } else if (dependencies > 0) {
    toast.success(i18n.t("content.toasts.installedWithDeps", { name: title, count: dependencies }));
  } else {
    toast.success(i18n.t("content.toasts.installed", { name: title }));
  }
  if (result.unresolved.length > 0) {
    toast(i18n.t("content.toasts.unresolved", { names: names(result.unresolved) }));
  }
  if (result.conflicts.length > 0) {
    toast(i18n.t("content.toasts.conflicts", { names: names(result.conflicts) }));
  }
}

/** Install, update and remove add-ons of one instance, with progress, toasts and refreshes. */
export function useContentActions(instanceId: string, kind: ContentKind) {
  const { t } = useTranslation();
  const queryClient = useQueryClient();
  const setJob = useContentJobs((s) => s.setJob);

  /** Resolves once the installed list is fresh (at most `REFRESH_WAIT_MS` later). */
  const refresh = (): Promise<void> => {
    void queryClient.invalidateQueries({ queryKey: ["instance", instanceId] });
    // Fetched directly when known: the mods tab keeps its scan query disabled (updates are
    // checked on demand), and an invalidated disabled query would keep stale update chips.
    const scanKey = ["content-scan", instanceId, kind];
    const pending: Promise<unknown>[] = [
      queryClient.invalidateQueries({ queryKey: scanKey }).catch(() => undefined),
    ];
    if (queryClient.getQueryData(scanKey) !== undefined) {
      pending.push(
        queryClient
          .fetchQuery({
            queryKey: scanKey,
            queryFn: () => api.content.scan(instanceId, kind, true),
            staleTime: 0,
          })
          .catch(() => undefined),
      );
    }
    const timeout = new Promise<void>((resolve) => setTimeout(resolve, REFRESH_WAIT_MS));
    return Promise.race([Promise.all(pending).then(() => undefined), timeout]);
  };

  /** `done`: what a player who went elsewhere is told when a long job ends. */
  const run = async (
    key: string,
    work: (jobId: string) => Promise<void>,
    done?: { title: string; body: string },
  ) => {
    if (useContentJobs.getState().jobs[key]) return;
    const jobId = newJobId();
    const startedAt = Date.now();
    setJob(key, { jobId });
    try {
      await work(jobId);
      if (done) void notifyWhenAway(Date.now() - startedAt, done.title, done.body);
    } catch (error) {
      showError(error);
    } finally {
      setJob(key, undefined);
    }
  };

  const install = (projectId: string, title: string, versionId?: string) =>
    run(
      projectJobKey(instanceId, projectId),
      async (jobId) => {
        const key = projectJobKey(instanceId, projectId);
        const result = await api.content.install(
          { instanceId, kind, projectId, versionId, jobId },
          (progress) => setJob(key, { jobId, progress }),
        );
        announce(result, title);
        // The row keeps its progress until the list knows the new file, so its button goes
        // straight to "Yüklü" instead of flashing "Yükle" again in between.
        await refresh();
      },
      {
        title: t("attention.content.title"),
        body: t("attention.content.installed", { name: title }),
      },
    );

  const update = (entries: InstalledContent[]) => {
    const single = entries.length === 1 ? entries[0] : undefined;
    const key =
      single?.projectId !== undefined
        ? projectJobKey(instanceId, single.projectId)
        : bulkJobKey(instanceId, kind);
    return run(
      key,
      async (jobId) => {
        const result = await api.content.update(
          { instanceId, kind, fileNames: entries.map((e) => e.fileName), jobId },
          (progress) => setJob(key, { jobId, progress }),
        );
        await refresh();
        const updated = result.installed.filter((i) => !i.dependency);
        const only = updated[0];
        if (updated.length === 1 && only) {
          toast.success(t("content.toasts.updated", { name: only.title ?? only.fileName }));
        } else if (updated.length > 1) {
          toast.success(t("content.toasts.updatedMany", { count: updated.length }));
        } else {
          toast(t("content.toasts.noUpdates"));
        }
        if (result.unresolved.length > 0) {
          toast(t("content.toasts.unresolved", { names: names(result.unresolved) }));
        }
      },
      { title: t("attention.content.title"), body: t("attention.content.updated") },
    );
  };

  const remove = async (fileName: string, name: string) => {
    try {
      await api.content.remove(instanceId, kind, fileName);
      void refresh();
      toast(t("content.toasts.removed", { name }));
    } catch (error) {
      showError(error);
    }
  };

  const addFiles = async (paths?: string[]) => {
    try {
      const chosen = paths ?? (await api.content.pickFiles(kind, true));
      if (!chosen || chosen.length === 0) return;
      const result = await api.content.addFiles(instanceId, kind, chosen);
      void refresh();
      if (result.added.length > 0) {
        toast.success(t("content.toasts.added", { count: result.added.length }));
      }
      if (result.skipped.length > 0) {
        toast(t("content.toasts.skipped", { count: result.skipped.length }));
      }
    } catch (error) {
      showError(error);
    }
  };

  const cancel = (key: string) => {
    const job = useContentJobs.getState().jobs[key];
    if (job) void api.content.cancel(job.jobId);
  };

  return { install, update, remove, addFiles, cancel };
}
