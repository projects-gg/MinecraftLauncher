// A save read back, for the AFK settings editors (desktop and 7/24 Bulut): after a save the editor
// starts from the settings as stored once they come back. The save settles before its read-back
// reaches the editor (React Query tells its observers after a `setTimeout(0)`, the refetch's own
// promise settles first), so the mark stays until the stored settings change; clearing it when the
// save settled meant the read-back never ran (SMALLS-L3).
import { useEffect, useRef } from "react";

export interface ReadBack {
  /** A save was taken: the next stored settings replace the edits. */
  expect: () => void;
  /** The player edited meanwhile: the read-back leaves the draft as it is. */
  cancel: () => void;
  /** A save's read-back is on its way (nothing is held meanwhile). */
  waiting: () => boolean;
}

/** `onStored` runs once with the stored settings that came after `expect`. */
export function useReadBack<T>(baseline: T, onStored: (stored: T) => void): ReadBack {
  const waiting = useRef(false);
  const latest = useRef(onStored);
  latest.current = onStored;
  useEffect(() => {
    if (!waiting.current) return;
    waiting.current = false;
    latest.current(baseline);
  }, [baseline]);
  const api = useRef<ReadBack | null>(null);
  api.current ??= {
    expect: () => {
      waiting.current = true;
    },
    cancel: () => {
      waiting.current = false;
    },
    waiting: () => waiting.current,
  };
  return api.current;
}
