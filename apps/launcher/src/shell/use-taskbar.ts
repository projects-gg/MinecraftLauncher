// Download and install progress on the Windows taskbar button (macOS: the Dock icon, Linux:
// launchers that support it). The bar follows the launch (OYNA), else add-on and modpack jobs;
// a failed launch turns it red for a moment.
import { getCurrentWindow, ProgressBarStatus } from "@tauri-apps/api/window";
import { useEffect, useMemo, useRef, useState } from "react";
import { useContentJobs } from "@/features/content/jobs";
import { isTauri } from "@/lib/platform";
import { isOpening, useLaunch } from "@/stores/launch";
import { useTaskProgress } from "@/stores/progress";
import { type TaskbarState, taskbarState } from "./taskbar-model";

const ERROR_FLASH_MS = 2600;

function toTauri(state: TaskbarState) {
  switch (state.status) {
    case "normal":
      return { status: ProgressBarStatus.Normal, progress: state.percent };
    case "indeterminate":
      return { status: ProgressBarStatus.Indeterminate };
    case "error":
      return { status: ProgressBarStatus.Error, progress: 100 };
    default:
      return { status: ProgressBarStatus.None };
  }
}

export function useTaskbarProgress() {
  const launch = useLaunch((s) => (s.phase.kind === "working" ? s.phase.percent : null));
  const opening = useLaunch((s) => isOpening(s.phase));
  const failed = useLaunch((s) => s.phase.kind === "error");
  const contentJobs = useContentJobs((s) => s.jobs);
  const tasks = useTaskProgress((s) => s.tasks);
  const [flash, setFlash] = useState(false);

  useEffect(() => {
    if (!failed) return;
    setFlash(true);
    const timer = setTimeout(() => setFlash(false), ERROR_FLASH_MS);
    return () => clearTimeout(timer);
  }, [failed]);

  const state = useMemo<TaskbarState>(
    () =>
      flash
        ? { status: "error" }
        : taskbarState({
            launch,
            opening,
            jobs: [
              ...Object.values(contentJobs).map((job) => job.progress?.percent),
              ...Object.values(tasks).map((task) => task.percent),
            ],
          }),
    [flash, launch, opening, contentJobs, tasks],
  );

  // Only real changes reach the window (the bar moves in whole percents).
  const applied = useRef("");
  useEffect(() => {
    if (!isTauri()) return;
    const key = JSON.stringify(state);
    if (key === applied.current) return;
    applied.current = key;
    getCurrentWindow()
      .setProgressBar(toTauri(state))
      .catch(() => undefined);
  }, [state]);

  useEffect(
    () => () => {
      if (isTauri()) {
        getCurrentWindow()
          .setProgressBar({ status: ProgressBarStatus.None })
          .catch(() => undefined);
      }
    },
    [],
  );
}
