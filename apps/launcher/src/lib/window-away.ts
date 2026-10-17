// Whether the launcher window is out of sight: hidden in the tray or minimized. WebView2 keeps
// reporting the page as visible then (`document.hidden` stays false; measured in the release app,
// G6-R), so decorative motion (embers, Ken Burns, the OYNA sheen) kept drawing at the screen's
// refresh rate behind the tray icon, at about 70 % of a CPU core. main.tsx pauses ambient motion
// while this is true.
import { useSyncExternalStore } from "react";

/** The window calls the tracker needs: a subset of Tauri's `Window`, so tests can stand in. */
export interface AwayWindow {
  isVisible(): Promise<boolean>;
  isMinimized(): Promise<boolean>;
  onFocusChanged(handler: (event: { payload: boolean }) => void): Promise<() => void>;
  onResized(handler: () => void): Promise<() => void>;
}

let away = false;
let check: (() => Promise<void>) | undefined;
const listeners = new Set<() => void>();

function setAway(next: boolean) {
  if (next === away) return;
  away = next;
  for (const listener of listeners) listener();
}

/**
 * Follows the window: gaining focus means it is in front; losing focus, or a resize while it is not
 * focused (minimized from the taskbar or with Win+D), asks whether it is still visible and not
 * minimized. An answer that arrives after the window came back to the front is dropped. The
 * launcher's own show at start-up asks with `refreshWindowAway`. Returns a stop function.
 */
export function trackWindowAway(win: AwayWindow): () => void {
  let focused = false;
  let asked = 0;
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
      focused = payload;
      if (payload) {
        asked++;
        setAway(false);
      } else {
        void check?.();
      }
    }),
    win.onResized(() => {
      if (!focused) void check?.();
    }),
  ];
  return () => {
    check = undefined;
    for (const stop of stops) void stop.then((fn) => fn());
  };
}

/** Asks the window again (after the launcher showed it, or chose to stay in the tray). */
export function refreshWindowAway(): Promise<void> {
  return check?.() ?? Promise.resolve();
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
