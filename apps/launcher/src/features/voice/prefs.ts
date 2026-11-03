// Ayarlar → Sesli sohbet as the rest of the page needs it (VOICE-CALL): the meters' sensitivity
// mark and the room panel's "Ses seviyesi" read the settings without the settings page open. Loaded
// once with voice chat, again whenever the launcher says they changed (`backupRev`: here, from
// another device's backup or the in-game overlay), and at once when the page saves them.
import { create } from "zustand";
import { voiceApi } from "@/lib/api/voice-api";
import type { VoiceSettings } from "@/lib/api/voice-types";

interface VoicePrefs {
  settings: VoiceSettings | null;
}

export const useVoicePrefs = create<VoicePrefs>()(() => ({ settings: null }));

/** A slider's value is saved this long after it stops moving (as the settings page's). */
const SAVE_AFTER_MS = 250;
/** The panel's volume waits to be saved (a reload meanwhile keeps what the player set). */
let saveTimer: ReturnType<typeof setTimeout> | undefined;

/** Reads the settings again (they changed somewhere); a change still being saved here wins. */
export function loadVoicePrefs(): void {
  void voiceApi
    .settings()
    .then((settings) => {
      if (saveTimer === undefined) useVoicePrefs.setState({ settings });
    })
    .catch(() => {
      // The meters go without the sensitivity mark; nothing else depends on it.
    });
}

/** The settings as the launcher kept them after a save (the settings page tells). */
export function keptVoicePrefs(settings: VoiceSettings): void {
  useVoicePrefs.setState({ settings });
}

/** The hand-set sensitivity the meters mark (0–1), in "Konuşunca" only; null otherwise. */
export function meterThreshold(settings: VoiceSettings | null): number | null {
  if (settings?.mode !== "voice" || settings.sensitivity === null) return null;
  return Math.max(0, Math.min(100, settings.sensitivity)) / 100;
}

/** "Ses seviyesi" from the room panel (percent): shown at once, saved once the drag settles. */
export function setOutputVolume(percent: number, final: boolean): void {
  const settings = useVoicePrefs.getState().settings;
  if (!settings) return;
  const next = { ...settings, outputVolume: Math.max(0, Math.min(200, Math.round(percent))) };
  useVoicePrefs.setState({ settings: next });
  clearTimeout(saveTimer);
  saveTimer = undefined;
  const save = () => {
    saveTimer = undefined;
    const latest = useVoicePrefs.getState().settings;
    if (!latest) return;
    void voiceApi
      .saveSettings(latest)
      .then(keptVoicePrefs)
      .catch(() => {
        // The next change tries again; the slider shows what it holds.
      });
  };
  if (final) save();
  else saveTimer = setTimeout(save, SAVE_AFTER_MS);
}
