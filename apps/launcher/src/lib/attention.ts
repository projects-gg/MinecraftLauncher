// Getting the player's attention while the launcher is in the background: a system
// notification (and a brief taskbar flash) when a long install finishes. Nothing happens while
// the window is in front; the in-app toast is enough then.
import { listen } from "@tauri-apps/api/event";
import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
import { commands } from "@/bindings";
import { usePrefs } from "@/stores/prefs";
import { isTauri } from "./platform";
import { isWindowAway } from "./window-away";

/** Installs at least this long are worth a notification when they end in the background. */
export const LONG_TASK_MS = 20_000;

/** What a click on a shown notification opens, by the number it was shown with. */
const clickActions = new Map<number, () => void>();
/** Only the latest notifications keep what their click opens. */
const CLICK_ACTIONS_KEPT = 32;
/**
 * Counted on from the clock (seconds, within u32), so a notification still on screen from before a
 * reload of the page never opens what a new one with the same number was about.
 */
let lastId = Math.floor(Date.now() / 1000) % 1_000_000_000;
let hearingClicks = false;

/**
 * A click on one of the launcher's notifications (Windows, `src-tauri/src/notify.rs`): the shell
 * has already brought the launcher forward; this opens what the notification was about.
 */
function hearClicks() {
  if (hearingClicks) return;
  hearingClicks = true;
  listen<number>("desktop-notification-click", (event) => {
    const action = clickActions.get(event.payload);
    clickActions.delete(event.payload);
    action?.();
  }).catch(() => {
    hearingClicks = false;
  });
}

/**
 * A system notification under the launcher's own name, whatever the player chose in Settings
 * (callers check it). `onClick` runs when the player clicks it (Windows). True when the system
 * took it; false when notifications are off for the launcher in Windows, so the caller can show
 * its in-app toast instead. Never throws.
 */
export async function showNotification(
  title: string,
  body?: string,
  onClick?: () => void,
): Promise<boolean> {
  if (!isTauri()) return false;
  lastId += 1;
  const id = lastId;
  if (onClick) {
    hearClicks();
    clickActions.set(id, onClick);
    for (const old of clickActions.keys()) {
      if (clickActions.size <= CLICK_ACTIONS_KEPT) break;
      clickActions.delete(old);
    }
  }
  try {
    const shown = await commands.desktopNotify(title, body ?? null, id);
    if (!shown) clickActions.delete(id);
    return shown;
  } catch {
    // Notifications are a nicety.
    clickActions.delete(id);
    return false;
  }
}

/**
 * A system notification, unless the player turned them off (Settings → Genel). `onClick` runs
 * when the player clicks it. Never throws.
 */
export async function notify(title: string, body: string, onClick?: () => void) {
  if (!isTauri() || !usePrefs.getState().desktopNotifications) return;
  await showNotification(title, body, onClick);
}

/**
 * True when the window is not in front: in the tray or minimized (lib/window-away.ts), or behind
 * another window (the page has lost the focus). Not Tauri's window focus: on Windows it is
 * unreliable while the page holds the keyboard focus (G6-R).
 */
export function windowAway(): boolean {
  if (typeof document === "undefined") return false;
  return isWindowAway() || !document.hasFocus();
}

/** A brief taskbar flash (informational), alongside a notification. Never throws. */
export function requestAttention() {
  if (!isTauri()) return;
  getCurrentWindow()
    .requestUserAttention(UserAttentionType.Informational)
    .catch(() => undefined);
}

/**
 * Tell a player who is elsewhere that something they waited for is done: only when it took at
 * least `LONG_TASK_MS` and the window is not in front.
 */
export async function notifyWhenAway(durationMs: number, title: string, body: string) {
  if (durationMs < LONG_TASK_MS || !windowAway()) return;
  await notify(title, body);
  requestAttention();
}
