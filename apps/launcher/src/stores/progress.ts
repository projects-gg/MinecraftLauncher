import { create } from "zustand";

/** A long job outside the launch and add-on stores (modpack imports and installs). */
export interface TaskProgress {
  /** 0–100; missing while the amount of work is unknown. */
  percent?: number;
  startedAt: number;
}

interface TaskProgressState {
  tasks: Record<string, TaskProgress>;
  report: (key: string, percent?: number) => void;
  /** Ends the task; returns how long it ran (ms). */
  finish: (key: string) => number;
}

/** Long jobs the taskbar button shows progress for (see shell/use-taskbar.ts). */
export const useTaskProgress = create<TaskProgressState>((set, get) => ({
  tasks: {},
  report: (key, percent) =>
    set((s) => ({
      tasks: {
        ...s.tasks,
        [key]: { percent, startedAt: s.tasks[key]?.startedAt ?? Date.now() },
      },
    })),
  finish: (key) => {
    const task = get().tasks[key];
    set((s) => {
      const tasks = { ...s.tasks };
      delete tasks[key];
      return { tasks };
    });
    return task ? Date.now() - task.startedAt : 0;
  },
}));

/** Track one job: `report` as it moves, `done` when it ends (returns its duration in ms). */
export function trackTask(key: string) {
  const { report, finish } = useTaskProgress.getState();
  report(key);
  return {
    report: (percent?: number) => report(key, percent),
    done: () => finish(key),
  };
}
