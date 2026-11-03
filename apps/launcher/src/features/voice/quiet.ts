// SOC-NEXT §3.7 for voice chat (VOICE-CALL): someone the player ignores rings quietly. The page
// sends the list to the launcher (on start and whenever it changes), which keeps their rings out of
// the state: no card or tone here, no Windows notification, no card in the game; their call waits
// in the missed list without a word (the friend's row says "Cevapsız arama", the Friends entry's
// mark does not count it). Until the list has arrived, the page itself stays quiet for them.
// SOC-NEXT's switch off: nobody is ignored.
import { useEffect } from "react";
import { useSocNextFlag } from "@/features/social/next/flag";
import { ignoringNow } from "@/features/social/next/ignoring";
import { useSocNextPrefs } from "@/features/social/next/prefs";
import { voiceApi } from "@/lib/api/voice-api";
import type { VoiceMissed, VoiceRing } from "@/lib/api/voice-types";

/** Whom the player ignores now (the list the launcher gets). */
export function ignoredNow(): string[] {
  return useSocNextFlag.getState().on ? [...useSocNextPrefs.getState().ignored] : [];
}

/** The rings the player hears: everyone's but an ignored person's. */
export function heardRings(rings: readonly VoiceRing[]): VoiceRing[] {
  return rings.filter((ring) => !ignoringNow(ring.from));
}

/** The missed calls that light the Friends entry's mark: everyone's but an ignored person's. */
export function toldMissed(missed: readonly VoiceMissed[]): VoiceMissed[] {
  return missed.filter((call) => !ignoringNow(call.from));
}

/** Sends the list now and whenever it changes (the list or SOC-NEXT's switch), once per change. */
export function syncIgnored(): () => void {
  let sent: string | null = null;
  const send = () => {
    const ids = ignoredNow();
    const key = ids.join("\n");
    if (key === sent) return;
    sent = key;
    voiceApi.setIgnored(ids).catch(() => {
      // Asked again on the next change.
      sent = null;
    });
  };
  send();
  const stopPrefs = useSocNextPrefs.subscribe(send);
  const stopFlag = useSocNextFlag.subscribe(send);
  return () => {
    stopPrefs();
    stopFlag();
  };
}

export function useIgnoredSync() {
  useEffect(() => syncIgnored(), []);
}
