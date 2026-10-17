// Performance mode on weak computers (docs/tasks/q2.md, proposal 2). The launcher looks at the
// machine once: a very weak one gets the mode at once (its toast offers "Geri al"), a weak one is
// measured on Home and only offered the mode when its frames really are slow, a capable one is
// left alone. Whatever the player chooses afterwards stands (prefs `perfTouched`).

export interface Hardware {
  /** Memory Windows reports in MB (unknown in the browser preview). */
  memoryMb?: number;
  /** Logical processors (`navigator.hardwareConcurrency`). */
  cores?: number;
}

export type Tier = "veryWeak" | "weak" | "capable";

/** What the launcher does about this computer, decided once. */
export type FirstStep = "switch" | "measure" | "none";

/**
 * Windows reports what is left after the graphics share: a 4 GB laptop shows 3.4 to 4 GB, an 8 GB
 * one 5.9 to 8 GB. Each limit sits half a gigabyte above the installed size it stands for.
 */
export const VERY_WEAK_MEMORY_MB = 4608;
export const WEAK_MEMORY_MB = 8704;
export const VERY_WEAK_CORES = 2;
export const WEAK_CORES = 4;

const known = (value: number | undefined): value is number =>
  typeof value === "number" && Number.isFinite(value) && value > 0;

/** Either sign of weakness is enough; an unknown figure counts for nothing. */
export function hardwareTier({ memoryMb, cores }: Hardware): Tier {
  const memory = known(memoryMb) ? memoryMb : Number.POSITIVE_INFINITY;
  const threads = known(cores) ? cores : Number.POSITIVE_INFINITY;
  if (memory <= VERY_WEAK_MEMORY_MB || threads <= VERY_WEAK_CORES) return "veryWeak";
  if (memory <= WEAK_MEMORY_MB || threads <= WEAK_CORES) return "weak";
  return "capable";
}

export function firstStep(tier: Tier, performanceMode: boolean): FirstStep {
  // Already on: nothing to switch, nothing to suggest.
  if (performanceMode) return "none";
  if (tier === "veryWeak") return "switch";
  if (tier === "weak") return "measure";
  return "none";
}

/** Most frames slower than this means under about 45 frames a second. */
export const SLOW_MEDIAN_MS = 22;
/** A frame that missed at least one refresh of a 60 Hz screen. */
export const LONG_FRAME_MS = 34;
/** This share of long frames reads as a stutter even when the median looks fine. */
export const LONG_FRAME_SHARE = 0.2;

export interface FrameStats {
  frames: number;
  medianMs: number;
  /** Share of the frames longer than `LONG_FRAME_MS` (0–1). */
  longShare: number;
}

/** `deltas`: the time between consecutive animation frames, in ms. */
export function frameStats(deltas: readonly number[]): FrameStats {
  const frames = deltas.length;
  if (frames === 0) return { frames: 0, medianMs: 0, longShare: 0 };
  const sorted = [...deltas].sort((a, b) => a - b);
  const middle = Math.floor(frames / 2);
  const medianMs =
    frames % 2 === 1
      ? (sorted[middle] ?? 0)
      : ((sorted[middle - 1] ?? 0) + (sorted[middle] ?? 0)) / 2;
  const longShare = deltas.filter((delta) => delta > LONG_FRAME_MS).length / frames;
  return { frames, medianMs, longShare };
}

export function framesLookSlow({ frames, medianMs, longShare }: FrameStats): boolean {
  return frames > 0 && (medianMs > SLOW_MEDIAN_MS || longShare >= LONG_FRAME_SHARE);
}

/**
 * Browser preview only (`?perf=veryweak` or `?perf=weak`): pretend hardware and, for "weak",
 * busy work in every measured frame, so the real measurement sees a struggling computer.
 */
export interface PerfPreview {
  hardware: Hardware;
  frameLoadMs: number;
}

export function parsePerfPreview(search: string): PerfPreview | null {
  const value = new URLSearchParams(search).get("perf");
  if (value === "veryweak") return { hardware: { memoryMb: 4096, cores: 2 }, frameLoadMs: 0 };
  if (value === "weak") return { hardware: { memoryMb: 8192, cores: 4 }, frameLoadMs: 30 };
  return null;
}
