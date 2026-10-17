// Whether the launcher window is out of sight: hidden in the tray or minimized. WebView2 keeps
// reporting the page as visible then (`document.hidden` stays false; measured in the release app,
// G6-R), so decorative motion (embers, Ken Burns, the OYNA sheen) kept drawing at the screen's
// refresh rate behind the tray icon, at about 70 % of a CPU core. main.tsx pauses ambient motion
// and polling while this is true.
import { useSyncExternalStore } from "react";

/** The window calls the tracker needs: a subset of Tauri's `Window`, so tests can stand in. */
export interface AwayWindow {
  isVisible(): Promise<boolean>;
  isMinimized(): Promise<boolean>;
  onFocusChanged(handler: (event: { payload: boolean }) => void): Promise<() => void>;
  onResized(handler: () => void): Promise<() => void>;
}

/** A resize drag fires many events; the window is asked once they settle. */
export const RESIZE_SETTLE_MS = 120;

let away = false;
let check: (() => Promise<void>) | undefined;
const listeners = new Set<() => void>();

function setAway(next: boolean) {
  if (next === away) return;
  away = next;
  for (const listener of listeners) listener();
}

/**
 * Follows the window. Gaining focus means it is in front. Minimizing (title bar, taskbar, Win+D)
 * resizes it, so a resize asks whether it is visible and not minimized; so does losing focus.
 * Hiding sends no event at all, and on Windows a blur also arrives right after the window was
 * shown (WebView2 takes the keyboard focus), so the launcher asks with `refreshWindowAway` after
 * it shows or hides the window itself. An answer that arrives after the window came back to the
 * front is dropped. Returns a stop function.
 */
export function trackWindowAway(win: AwayWindow): () => void {
  let asked = 0;
  let settle: ReturnType<typeof setTimeout> | undefined;
  check = async () => {
    const question = ++asked;
    try {
      const [visible, minimized] = await Promise.all([win.isVisible(), win.isMinimized()]);
      if (question === asked) setAway(!visible || minimized);
    } catch {
      // The last answer stands.
    }
  };
  const stops = [
    win.onFocusChanged(({ payload }) => {
      if (payload) {
        asked++;
        setAway(false);
      } else {
        void check?.();
      }
    }),
    win.onResized(() => {
      clearTimeout(settle);
      settle = setTimeout(() => void check?.(), RESIZE_SETTLE_MS);
    }),
  ];
  return () => {
    clearTimeout(settle);
    check = undefined;
    for (const stop of stops) void stop.then((fn) => fn());
  };
}

/** Asks the window again (after the launcher showed, hid or minimized it). */
export function refreshWindowAway(): Promise<void> {
  return check?.() ?? Promise.resolve();
}

/** Calls `listener` with the new answer each time it changes. Returns the unsubscribe function. */
export function onWindowAwayChange(listener: (away: boolean) => void): () => void {
  const notify = () => listener(away);
  listeners.add(notify);
  return () => {
    listeners.delete(notify);
  };
}

export function useWindowAway(): boolean {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => away,
  );
}

/** The current answer (tests). */
export function isWindowAway(): boolean {
  return away;
}
