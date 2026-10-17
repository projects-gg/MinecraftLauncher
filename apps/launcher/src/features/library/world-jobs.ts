import { create } from "zustand";
import type { BackupProgress } from "@/lib/api";

/** A running backup or restore. Kept outside the tab so progress survives switching tabs. */
export interface WorldJob {
  jobId: string;
  progress?: BackupProgress;
}

interface WorldJobsState {
  jobs: Record<string, WorldJob>;
  setJob: (key: string, job: WorldJob | undefined) => void;
}

export const useWorldJobs = create<WorldJobsState>((set) => ({
  jobs: {},
  setJob: (key, job) =>
    set((state) => {
      const jobs = { ...state.jobs };
      if (job) jobs[key] = job;
      else delete jobs[key];
      return { jobs };
    }),
}));

export const backupJobKey = (instanceId: string, folder: string) =>
  `${instanceId}:backup:${folder}`;
export const restoreJobKey = (instanceId: string, fileName: string) =>
  `${instanceId}:restore:${fileName}`;

/** Percent done, or `undefined` while the amount of work is unknown. */
export function jobPercent(progress: BackupProgress | undefined): number | undefined {
  if (!progress) return undefined;
  if (progress.bytesTotal > 0) return (progress.bytesDone / progress.bytesTotal) * 100;
  if (progress.filesTotal > 0) return (progress.filesDone / progress.filesTotal) * 100;
  return undefined;
}
