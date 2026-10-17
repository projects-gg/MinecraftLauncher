// Friend actions shared by the panel, the toasts and the home strip: one place for the call, the
// toast it may need and the screen-reader announcement. Errors become one short sentence (the
// details stay copyable through the accounts' friendly-error helper).
import { toast } from "@pl/ui";
import {
  isPermissionGranted,
  requestPermission,
  sendNotification,
} from "@tauri-apps/plugin-notification";
import { announce, friendlyError } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import type { Friend } from "@/lib/api/social-types";
import { copyText as writeClipboard } from "@/lib/clipboard";
import i18n from "@/lib/i18n";
import { isTauri } from "@/lib/platform";
import { usePrefs } from "@/stores/prefs";
import { useSocialUi } from "./store";

/** The service allows one invite per friend per 5 minutes. */
export const INVITE_COOLDOWN_MS = 5 * 60_000;

/** Runs a friends call; a failure becomes a toast. Returns undefined on failure. */
export async function attempt<T>(work: () => Promise<T>): Promise<T | undefined> {
  try {
    return await work();
  } catch (error) {
    toast.error(friendlyError(error, i18n.t("social.actions.failed")).message);
    return undefined;
  }
}

function code(error: unknown): string {
  return error instanceof Error && "code" in error && typeof error.code === "string"
    ? error.code
    : "";
}

/** "Birlikte oyna": no text, one per friend per 5 minutes (also enforced by the service). */
export async function inviteFriend(friend: Pick<Friend, "id" | "name">): Promise<void> {
  const ui = useSocialUi.getState();
  if ((ui.inviteCooldown[friend.id] ?? 0) > Date.now()) {
    toast(i18n.t("social.actions.inviteWait"));
    return;
  }
  try {
    const result = await api.social.invite(friend.id);
    ui.coolDown(friend.id, Date.now() + INVITE_COOLDOWN_MS);
    if (result === "delivered") {
      toast.success(i18n.t("social.actions.invited"));
    } else {
      toast(i18n.t("social.actions.inviteQueued", { name: friend.name }));
    }
    announce(i18n.t("social.actions.invited"));
  } catch (error) {
    if (code(error) === "SOSYAL-SINIR") {
      ui.coolDown(friend.id, Date.now() + INVITE_COOLDOWN_MS);
      toast(i18n.t("social.actions.inviteWait"));
      return;
    }
    toast.error(friendlyError(error, i18n.t("social.actions.inviteFailed")).message);
  }
}

/** True while "Birlikte oyna" to this friend has to wait. */
export function inviteCoolingDown(id: string, cooldown: Record<string, number>): boolean {
  return (cooldown[id] ?? 0) > Date.now();
}

/**
 * A system notification when the launcher is not in front (in game, in the tray), unless the
 * player turned them off (Settings → Genel → Masaüstü bildirimleri).
 */
export async function notifyOutside(title: string, body?: string): Promise<boolean> {
  if (!isTauri() || document.hasFocus() || !usePrefs.getState().desktopNotifications) {
    return false;
  }
  try {
    let granted = await isPermissionGranted();
    if (!granted) granted = (await requestPermission()) === "granted";
    if (!granted) return false;
    sendNotification(body ? { title, body } : { title });
    return true;
  } catch {
    return false;
  }
}

/** Copies text (through the app's clipboard helper); true when it worked. */
export async function copyText(text: string): Promise<boolean> {
  try {
    await writeClipboard(text);
    return true;
  } catch {
    return false;
  }
}
