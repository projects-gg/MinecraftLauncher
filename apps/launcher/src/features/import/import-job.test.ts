import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { ImportProgress, ImportReport, ImportRequest } from "@/lib/api";
import { useTaskProgress } from "@/stores/progress";

// A backend whose runs the test ends by hand.
const runs: Array<{
  request: ImportRequest;
  jobId: string;
  onProgress: (p: ImportProgress) => void;
  resolve: (report: ImportReport) => void;
  reject: (error: unknown) => void;
}> = [];
const cancelled: string[] = [];

vi.mock("@/lib/api", () => ({
  api: {
    importer: {
      start: (request: ImportRequest, jobId: string, onProgress: (p: ImportProgress) => void) =>
        new Promise<ImportReport>((resolve, reject) => {
          runs.push({ request, jobId, onProgress, resolve, reject });
        }),
      cancel: async (jobId: string) => {
        cancelled.push(jobId);
      },
    },
  },
}));
vi.mock("@/lib/attention", () => ({ notifyWhenAway: vi.fn(async () => undefined) }));

const { importPercent, useImportJob } = await import("./import-job");

const request: ImportRequest = {
  sources: [{ sourceId: "minecraft", shared: ["options", "worlds"], profiles: [] }],
};
const report: ImportReport = { outcomes: [], cancelled: false, created: [], officialId: null };
const progress = (doneBytes: number, stage: ImportProgress["stage"] = "copying") => ({
  stage,
  doneBytes,
  totalBytes: 200,
  doneItems: 0,
  totalItems: 2,
  current: "Survival",
});
const flush = () => new Promise((resolve) => setTimeout(resolve, 0));

describe("the import job", () => {
  beforeEach(() => {
    runs.length = 0;
    cancelled.length = 0;
  });
  afterEach(() => {
    useImportJob.getState().clear();
  });

  it("runs in the background with its progress on the taskbar, then keeps the report", async () => {
    useImportJob.getState().start(request);
    const state = () => useImportJob.getState();
    expect(state().status).toBe("running");
    expect(runs).toHaveLength(1);
    expect(runs[0]?.request).toEqual(request);
    // A second start while one runs changes nothing.
    state().start(request);
    expect(runs).toHaveLength(1);

    runs[0]?.onProgress(progress(50));
    expect(state().progress?.doneBytes).toBe(50);
    expect(Object.values(useTaskProgress.getState().tasks)[0]?.percent).toBe(25);

    runs[0]?.resolve(report);
    await flush();
    expect(state().status).toBe("done");
    expect(state().report).toEqual(report);
    expect(state().finishedAt).toBeTypeOf("number");
    expect(useTaskProgress.getState().tasks).toEqual({});
  });

  it("cancels once and says so while it stops", async () => {
    useImportJob.getState().start(request);
    const jobId = runs[0]?.jobId;
    useImportJob.getState().cancel();
    useImportJob.getState().cancel();
    expect(cancelled).toEqual([jobId]);
    expect(useImportJob.getState().stopping).toBe(true);
    runs[0]?.resolve({ ...report, cancelled: true });
    await flush();
    expect(useImportJob.getState().report?.cancelled).toBe(true);
  });

  it("keeps a refusal for the summary", async () => {
    useImportJob.getState().start(request);
    runs[0]?.reject(new Error("Oyun açıkken aktarılamaz."));
    await flush();
    expect(useImportJob.getState().status).toBe("failed");
    expect((useImportJob.getState().error as Error).message).toBe("Oyun açıkken aktarılamaz.");
    // A finished run can be forgotten; a new one starts clean.
    useImportJob.getState().clear();
    expect(useImportJob.getState().status).toBe("idle");
  });

  it("measures progress by bytes, not while it prepares", () => {
    expect(importPercent(undefined)).toBeUndefined();
    expect(importPercent(progress(0, "preparing"))).toBeUndefined();
    expect(importPercent(progress(100))).toBe(50);
    expect(importPercent(progress(400))).toBe(100);
  });
});
