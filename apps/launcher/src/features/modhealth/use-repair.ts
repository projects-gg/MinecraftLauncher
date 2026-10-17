// Running repairs and reinstalls from the UI: one job at a time per hook, progress as it
// arrives, stoppable, and everything that shows the mods folder refreshed at the end.
import { useQueryClient } from "@tanstack/react-query";
import { useCallback, useRef, useState } from "react";
import { newJobId } from "@/features/content/jobs";
import {
  api,
  type ReinstallProgress,
  type ReinstallResult,
  type RepairProgress,
  type RepairResult,
  type RepairScope,
} from "@/lib/api";
import { refreshAfterRepair } from "./flow";

export interface JobState<P, R> {
  running: boolean;
  progress: P | null;
  result: R | null;
  error: unknown;
}

function idle<P, R>(): JobState<P, R> {
  return { running: false, progress: null, result: null, error: null };
}

/** "Onar" for one instance. */
export function useRepairJob(instanceId: string | null) {
  const queryClient = useQueryClient();
  const [state, setState] = useState<JobState<RepairProgress, RepairResult>>(idle);
  const job = useRef<string | null>(null);

  const start = useCallback(
    async (scope: RepairScope) => {
      if (!instanceId) return undefined;
      const jobId = newJobId();
      job.current = jobId;
      setState({ running: true, progress: null, result: null, error: null });
      const mine = () => job.current === jobId;
      try {
        const result = await api.modHealth.repair(instanceId, scope, jobId, (progress) => {
          if (mine()) setState((s) => ({ ...s, progress }));
        });
        if (mine()) setState((s) => ({ ...s, running: false, result }));
        return result;
      } catch (error) {
        if (mine()) setState((s) => ({ ...s, running: false, error }));
        return undefined;
      } finally {
        refreshAfterRepair(queryClient, instanceId);
      }
    },
    [instanceId, queryClient],
  );

  const stop = useCallback(() => {
    if (job.current) void api.modHealth.cancel(job.current);
  }, []);

  const reset = useCallback(() => {
    job.current = null;
    setState(idle());
  }, []);

  return { ...state, start, stop, reset };
}

/** "Yeniden yükle" for one instance. */
export function useReinstallJob(instanceId: string | null) {
  const queryClient = useQueryClient();
  const [state, setState] = useState<JobState<ReinstallProgress, ReinstallResult>>(idle);
  const job = useRef<string | null>(null);

  const start = useCallback(async () => {
    if (!instanceId) return undefined;
    const jobId = newJobId();
    job.current = jobId;
    setState({ running: true, progress: null, result: null, error: null });
    const mine = () => job.current === jobId;
    try {
      const result = await api.modHealth.reinstall(instanceId, jobId, (progress) => {
        if (mine()) setState((s) => ({ ...s, progress }));
      });
      if (mine()) setState((s) => ({ ...s, running: false, result }));
      return result;
    } catch (error) {
      if (mine()) setState((s) => ({ ...s, running: false, error }));
      return undefined;
    } finally {
      refreshAfterRepair(queryClient, instanceId);
    }
  }, [instanceId, queryClient]);

  const stop = useCallback(() => {
    if (job.current) void api.modHealth.cancel(job.current);
  }, []);

  const reset = useCallback(() => {
    job.current = null;
    setState(idle());
  }, []);

  return { ...state, start, stop, reset };
}

/** Overall progress of a repair, 0–100: finished steps plus the running one's share. */
export function repairPercent(progress: RepairProgress | null, done = false): number {
  if (done) return 100;
  if (!progress || progress.steps.length === 0) return 0;
  const steps = progress.steps;
  const finished = steps.filter((s) => s.status !== "pending" && s.status !== "running").length;
  const running = steps.some((s) => s.status === "running");
  const share = running ? ((progress.percent ?? 30) / 100) * 0.9 : 0;
  return Math.min(99, ((finished + share) / steps.length) * 100);
}

/** Overall progress of a reinstall, 0–100. */
export function reinstallPercent(progress: ReinstallProgress | null, done = false): number {
  if (done) return 100;
  if (!progress || progress.phases.length === 0) return 0;
  const index = progress.phases.indexOf(progress.phase);
  const share = ((progress.percent ?? 20) / 100) * 0.9;
  return Math.min(99, ((Math.max(index, 0) + share) / progress.phases.length) * 100);
}
