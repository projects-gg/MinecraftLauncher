// The room panel's own choice among the voice settings (SQOL §3: `VoiceSettings.peek`, in the
// settings backup): "Mesaj Önizlemelerini Göster", on unless the player turned it off (and while
// the settings are not read yet, or come from a launcher without it).
import type { VoiceSettings } from "@/lib/api/voice-types";
import { useVoicePrefs } from "../prefs";

export function peekOf(settings: Pick<VoiceSettings, "peek"> | null | undefined): boolean {
  return settings?.peek !== false;
}

export function usePeekSetting(): boolean {
  return useVoicePrefs((s) => peekOf(s.settings));
}
