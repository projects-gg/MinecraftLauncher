// What the Windows taskbar button shows while the launcher downloads or installs.

export type TaskbarState =
  | { status: "none" }
  | { status: "normal"; percent: number }
  | { status: "indeterminate" }
  | { status: "error" };

const clamp = (value: number) => Math.round(Math.min(100, Math.max(0, value)));

/**
 * The launch (OYNA) wins: it is what the player waits for. Otherwise other jobs (add-ons,
 * modpacks) are averaged; jobs that cannot tell how far they are make the bar indeterminate
 * only when no job can.
 */
export function taskbarState(input: {
  /** Percent of the launch's install while it prepares files, else null. */
  launch: number | null;
  /** Other running jobs (percent, or undefined while unknown). */
  jobs: Array<number | undefined>;
}): TaskbarState {
  if (input.launch !== null && Number.isFinite(input.launch)) {
    return { status: "normal", percent: clamp(input.launch) };
  }
  if (input.jobs.length === 0) return { status: "none" };
  const known = input.jobs.filter((p): p is number => typeof p === "number" && Number.isFinite(p));
  if (known.length === 0) return { status: "indeterminate" };
  return { status: "normal", percent: clamp(known.reduce((a, b) => a + b, 0) / known.length) };
}
