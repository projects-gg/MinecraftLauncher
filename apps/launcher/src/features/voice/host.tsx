// What voice chat (VOICE-L3) keeps mounted with the friends feature (SocialHost): the backend's state
// (event driven: nothing asks on a timer), who speaks (only while the window is on screen: hidden
// in the tray or minimized the page takes no speaking frames, so nothing renders), its notices as
// toasts with voice.md §7.10's words, the ring cards, and the room panel. Screen readers hear who
// came into or left the room (voice.md §5.1).
import { toast } from "@pl/ui";
import { useEffect } from "react";
import { announce } from "@/features/accounts/use-accounts";
import { voiceApi } from "@/lib/api/voice-api";
import type { VoiceNotice, VoiceState } from "@/lib/api/voice-types";
import i18n from "@/lib/i18n";
import { isWindowAway, onWindowAwayChange } from "@/lib/window-away";
import { openMicSettings } from "./actions";
import { dismissRingToast, showRingToast } from "./ring-toast";
import { leftText, peopleChanges } from "./rules";
import { RoomSheet } from "./sheet";
import { faceNow, useVoiceUi } from "./store";

/** Toasts with a way out stay a little longer (they ask for an answer). */
const ACTION_TOAST_MS = 9000;

export function handleVoiceNotice(notice: VoiceNotice) {
  const t = i18n.t.bind(i18n);
  const ui = useVoiceUi.getState();
  switch (notice.kind) {
    case "left": {
      // The two the pill keeps, with their own way back ("Tekrar ara", "Tekrar dene").
      if (notice.reason === "noAnswer" && notice.friend) {
        ui.setEnded({ kind: "noAnswer", friend: notice.friend });
        announce(t("voice.call.noAnswer"));
        return;
      }
      if (notice.reason === "failed") {
        ui.setEnded({ kind: "failed" });
        announce(t("voice.left.failed"));
        return;
      }
      const text = leftText(notice.reason, t);
      announce(text);
      toast(text);
      return;
    }
    case "moved":
      announce(t("voice.moved"));
      toast(t("voice.moved"));
      return;
    case "mutedByModerator":
      announce(t("voice.mutedByMod"));
      toast(t("voice.mutedByMod"));
      return;
    case "micChanged":
      toast(t("voice.mic.changed"));
      return;
    case "mic":
      if (notice.mic === "denied") {
        toast(t("voice.mic.denied"), {
          duration: ACTION_TOAST_MS,
          action: { label: t("voice.mic.openSettings"), onClick: () => void openMicSettings() },
        });
      } else if (notice.mic === "missing") {
        toast(t("voice.mic.missing"));
      }
      return;
    case "missed":
      // The Windows notification and the friend's row say it; the page adds nothing.
      return;
  }
}

/** What changed between two states that the page says itself: ring cards, joins and leaves. */
export function onVoiceState(before: VoiceState, after: VoiceState) {
  const was = new Set(before.rings.map((r) => r.room));
  const is = new Set(after.rings.map((r) => r.room));
  for (const ring of after.rings) {
    if (was.has(ring.room)) continue;
    showRingToast(ring);
    announce(i18n.t("voice.ring.calling", { name: faceNow(ring.from).name }));
  }
  for (const room of was) if (!is.has(room)) dismissRingToast(room);
  const { joined, left } = peopleChanges(before.room, after.room);
  for (const id of joined) announce(i18n.t("voice.a11y.joined", { name: faceNow(id).name }));
  for (const id of left) announce(i18n.t("voice.a11y.left", { name: faceNow(id).name }));
}

function useVoiceSync() {
  useEffect(() => {
    let alive = true;
    // A pushed state wins over the first answer, which may have left before it.
    let heard = false;
    const apply = (state: VoiceState) => {
      const before = useVoiceUi.getState().state;
      useVoiceUi.getState().setState(state);
      onVoiceState(before, state);
    };
    const stopState = voiceApi.onState((state) => {
      heard = true;
      apply(state);
    });
    const stopNotice = voiceApi.onNotice(handleVoiceNotice);

    // Who speaks, only while the window is on screen (the backend sends nothing meanwhile too).
    let stopSpeaking = () => {};
    const listen = () => {
      stopSpeaking();
      stopSpeaking = voiceApi.onSpeaking((speaking) => useVoiceUi.getState().setSpeaking(speaking));
    };
    const rest = () => {
      stopSpeaking();
      stopSpeaking = () => {};
      useVoiceUi.getState().setSpeaking({ ids: [], me: false });
    };
    if (isWindowAway()) rest();
    else listen();
    const stopAway = onWindowAwayChange((away) => (away ? rest() : listen()));

    void voiceApi
      .state()
      .then((state) => {
        if (alive && !heard) apply(state);
      })
      .catch(() => {
        // Voice chat stays hidden; nothing else depends on it.
      });
    return () => {
      alive = false;
      stopState();
      stopNotice();
      stopSpeaking();
      stopAway();
    };
  }, []);
}

export function VoiceHost() {
  useVoiceSync();
  return <RoomSheet />;
}
