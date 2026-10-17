// Getting the player's attention while the launcher is in the background: a system
// notification (and a brief taskbar flash) when a long install finishes. Nothing happens while
// the window is in front; the in-app toast is enough then.
import { getCurrentWindow, UserAttentionType } from "@tauri-apps/api/window";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { usePrefs } from "@/stores/prefs";
import { isTauri } from "./platform";

/** Installs at least this long are worth a notification when they end in the background. */
export const LONG_TASK_MS = 20_000;

/**
 * A system notification (asks for permission the first time), unless the player turned them off
 * (Settings → Genel). Never throws.
 */
export async function notify(title: string, body: string) {
  if (!isTauri() || !usePrefs.getState().desktopNotifications) return;
  try {
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
    if (granted) sendNotification({ title, body });
  } catch {
    // Notifications are a nicety.
  }
}

/** True when the window is not in front: unfocused, minimized or in the tray. */
export async function windowAway(): Promise<boolean> {
  if (!isTauri()) return typeof document !== "undefined" && !document.hasFocus();
  try {
    const win = getCurrentWindow();
    const [focused, visible, minimized] = await Promise.all([
      win.isFocused(),
      win.isVisible(),
      win.isMinimized(),
    ]);
    return !focused || !visible || minimized;
  } catch {
    return false;
  }
}

/**
 * Tell a player who is elsewhere that something they waited for is done: only when it took at
 * least `LONG_TASK_MS` and the window is not in front.
 */
export async function notifyWhenAway(durationMs: number, title: string, body: string) {
  if (durationMs < LONG_TASK_MS || !(await windowAway())) return;
  await notify(title, body);
  if (isTauri()) {
    getCurrentWindow()
      .requestUserAttention(UserAttentionType.Informational)
      .catch(() => undefined);
  }
}
