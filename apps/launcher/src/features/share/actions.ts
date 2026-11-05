// What the share buttons do (SHARE): each asks the launcher and says a refusal in one toast; what
// worked shows in the state (the panel's "Yayında", the stage).
import { toast } from "@pl/ui";
import { keptVoicePrefs, useVoicePrefs } from "@/features/voice/prefs";
import { voiceApi } from "@/lib/api/voice-api";
import i18n from "@/lib/i18n";
import { shareApi } from "@/share/api";
import {
  DEFAULT_SHARE_SETTINGS,
  type ShareClosedWhy,
  type ShareNotice,
  type ShareRefusal,
  type ShareSource,
  type VoiceShareSettings,
} from "@/share/types";

/** A refusal in the player's words. */
export function refusalText(refusal: ShareRefusal, max = 2): string {
  const t = i18n.t.bind(i18n);
  switch (refusal) {
    case "off":
    case "budget":
      return t("share.notice.off");
    case "unsupported":
      return t("share.notice.unsupported");
    case "notAllowed":
      return t("share.notice.notAllowed");
    case "full":
      return t("share.notice.full", { max });
    case "roomFull":
      return t("share.notice.roomFull");
    case "watchFull":
      return t("share.notice.watchFull");
    case "watchMax":
      return t("share.notice.watchMax");
    case "gone":
      return t("share.notice.gone");
    case "failed":
      return t("share.notice.failed");
  }
}

/** Why one of the player's shares ended, in their words (none when they left the room). */
export function closedText(why: ShareClosedWhy): string | null {
  const t = i18n.t.bind(i18n);
  switch (why) {
    case "mod":
      return t("share.notice.mod");
    case "budget":
    case "limit":
      return t("share.notice.off");
    case "idle":
      return t("share.notice.idle");
    case "sourceGone":
      return t("share.notice.sourceGone");
    case "left":
      return null;
    case "error":
      return t("share.notice.stopped");
  }
}

/** A `share-notice` as a toast. */
export function noticeText(notice: ShareNotice): string | null {
  const t = i18n.t.bind(i18n);
  switch (notice.kind) {
    case "closed":
      return closedText(notice.why);
    case "refused":
      return refusalText(notice.refusal);
    case "audioOff":
      return t("share.notice.audioOff");
    case "watchStopped":
      return t("share.notice.watchStopped");
    case "watchFailed":
      return t("share.notice.watchFailed");
  }
}

/** "Paylaş" in the picker. */
export async function startShare(source: ShareSource, label: string, max: number) {
  try {
    const refused = await shareApi.start(source, label);
    if (refused) toast(refusalText(refused, max));
  } catch {
    toast(i18n.t("share.notice.failed"));
  }
}

export async function stopShare(n: number) {
  try {
    await shareApi.stop(n);
  } catch {
    // The share ends with the room anyway.
  }
}

/** "İzle". */
export async function watchShare(person: string, n: number) {
  try {
    const refused = await shareApi.watch(person, n);
    if (refused) toast(refusalText(refused));
  } catch {
    toast(i18n.t("share.notice.failed"));
  }
}

export async function unwatchShare(person: string, n: number) {
  try {
    await shareApi.unwatch(person, n);
  } catch {
    // Watching ends with the room anyway.
  }
}

/** The player's share choices as voice chat keeps them. */
export function shareSettings(): VoiceShareSettings {
  return { ...DEFAULT_SHARE_SETTINGS, ...useVoicePrefs.getState().settings?.share };
}

/** Saves a share choice (the picker's and the settings page's). */
export function setShareSetting<K extends keyof VoiceShareSettings>(
  key: K,
  value: VoiceShareSettings[K],
) {
  const settings = useVoicePrefs.getState().settings;
  if (!settings) return;
  const next = { ...settings, share: { ...shareSettings(), [key]: value } };
  // Shown at once; the launcher's kept copy follows.
  keptVoicePrefs(next);
  void voiceApi
    .saveSettings(next)
    .then(keptVoicePrefs)
    .catch(() => {
      // The next change tries again.
    });
}
