import type { LaunchEvent } from "./types";

/**
 * Preview switches for the game opening after OYNA (PLAY1): `?opening=first` (the first start
 * with a version: the window comes after 26 s, the note at 20 s), `?opening=slow` (40 s, no note),
 * `?opening=fast` (1.2 s), `?opening=hold` and `?opening=holdFirst` (the window never comes;
 * for pictures of "Açılıyor"). By default the window comes 3.4 s after the game started. Also
 * from localStorage (`pl.mock.opening`).
 */
function flag(key: string): string {
  try {
    return (
      new URLSearchParams(window.location.search).get(key) ??
      localStorage.getItem(`pl.mock.${key}`) ??
      ""
    );
  } catch {
    return "";
  }
}

export interface MockOpening {
  firstStart: boolean;
  /** After how long the game's window shows; `null`: never. */
  windowAfterMs: number | null;
}

export function mockOpening(): MockOpening {
  switch (flag("opening")) {
    case "first":
      return { firstStart: true, windowAfterMs: 26_000 };
    case "slow":
      return { firstStart: false, windowAfterMs: 40_000 };
    case "fast":
      return { firstStart: false, windowAfterMs: 1200 };
    case "hold":
      return { firstStart: false, windowAfterMs: null };
    case "holdFirst":
      return { firstStart: true, windowAfterMs: null };
    default:
      return { firstStart: false, windowAfterMs: 3400 };
  }
}

const STEP_MS = 100;

/**
 * Waits as a game that opens: `windowShown` once its time is up, unless "İptal" closed it first;
 * then `exited`, as the backend reports a game it closed. Returns whether the window showed.
 */
export async function mockWindowShown(
  opening: MockOpening,
  onEvent: (event: LaunchEvent) => void,
  job: { cancelled: boolean },
): Promise<boolean> {
  const started = Date.now();
  for (;;) {
    if (job.cancelled) {
      onEvent({
        type: "exited",
        code: null,
        runtimeSecs: Math.round((Date.now() - started) / 1000),
      });
      return false;
    }
    if (opening.windowAfterMs !== null && Date.now() - started >= opening.windowAfterMs) {
      onEvent({ type: "windowShown" });
      return true;
    }
    await new Promise((resolve) => window.setTimeout(resolve, STEP_MS));
  }
}
