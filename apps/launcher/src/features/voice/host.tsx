// What voice chat (VOICE-L3) keeps mounted with the friends feature (SocialHost): the backend's state
// (event driven: nothing asks on a timer), who speaks (only while the window is on screen: hidden
// in the tray or minimized the page takes no speaking frames, so nothing renders), its notices as
// toasts with voice.md §7.10's words, the ring cards and their tone (the ringback while the player's
// call rings), and the room panel. Screen readers hear who came into or left the room (§5.1).
import { toast } from "@pl/ui";
import { useEffect } from "react";
import { announce } from "@/features/accounts/use-accounts";
import { friendListShown, useSocialUi } from "@/features/social/store";
import { voiceApi } from "@/lib/api/voice-api";
import type { VoiceNotice, VoiceState } from "@/lib/api/voice-types";
import i18n from "@/lib/i18n";
import { isWindowAway, onWindowAwayChange } from "@/lib/window-away";
import { openMicSettings } from "./actions";
import { CallConfirm } from "./call-confirm";
import { askDiscord } from "./discord-offer";
import { useLevelStream } from "./levels";
import { loadVoicePrefs } from "./prefs";
import { heardRings, useIgnoredSync } from "./quiet";
import { playTone } from "./ring-sound";
import { dismissRingToast, showRingToast } from "./ring-toast";
import { leftText, peopleChanges, toneNow } from "./rules";
import { RoomSheet } from "./sheet";
import { faceNow, useVoiceUi, voiceOn } from "./store";

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
      const text = leftText(notice.reason, t, notice.until);
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

/**
 * What changed between two states that the page says itself: ring cards, joins and leaves. An
 * ignored person's ring has no card (SOC-NEXT §3.7; the launcher keeps it out once it knows).
 */
export function onVoiceState(before: VoiceState, after: VoiceState) {
  const heard = heardRings(after.rings);
  const was = new Set(before.rings.map((r) => r.room));
  // A card goes as soon as its ring is gone or no longer heard (ignored while it rang).
  const is = new Set(heard.map((r) => r.room));
  for (const ring of heard) {
    if (was.has(ring.room)) continue;
    showRingToast(ring);
    announce(i18n.t("voice.ring.calling", { name: faceNow(ring.from).name }));
  }
  for (const room of was) if (!is.has(room)) dismissRingToast(room);
  const { joined, left } = peopleChanges(before.room, after.room);
  for (const id of joined) announce(i18n.t("voice.a11y.joined", { name: faceNow(id).name }));
  for (const id of left) announce(i18n.t("voice.a11y.left", { name: faceNow(id).name }));
  // Who shared the room, for a day's "Rapor et" from their card (voice.md §8.3).
  const ui = useVoiceUi.getState();
  const now = Date.now();
  if (before.room && left.length) ui.rememberRoom(before.room.id, left, now);
  if (before.room && after.room?.id !== before.room.id) {
    ui.rememberRoom(
      before.room.id,
      before.room.people.map((p) => p.id),
      now,
    );
  }
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

function useVoiceTones() {
  useEffect(() => {
    // The ring only (VOICE-CALL): the caller's waiting tone is the voice window's, which also
    // sounds in the tray and in the game; the page's own one doubled it while the window was in
    // front.
    const decide = () => {
      const state = useVoiceUi.getState().state;
      const tone = toneNow({ ...state, rings: heardRings(state.rings) }, isWindowAway());
      playTone(tone === "ring" ? tone : null);
    };
    decide();
    const stopStore = useVoiceUi.subscribe((now, before) => {
      if (now.state !== before.state) decide();
    });
    const stopAway = onWindowAwayChange(decide);
    return () => {
      stopStore();
      stopAway();
      playTone(null);
    };
  }, []);
}

/**
 * A missed call lights the Friends entry's mark (with the other things that wait) until the player
 * has seen the friends list in front of them: the friend's row says "Cevapsız arama" there.
 */
function useMissedSeen() {
  useEffect(() => {
    const check = () => {
      if (!isWindowAway() && friendListShown(useSocialUi.getState())) {
        useVoiceUi.getState().markMissedSeen();
      }
    };
    check();
    const stopSocial = useSocialUi.subscribe(check);
    const stopVoice = useVoiceUi.subscribe((now, before) => {
      if (now.state.missed !== before.state.missed) check();
    });
    const stopAway = onWindowAwayChange((away) => {
      if (!away) check();
    });
    return () => {
      stopSocial();
      stopVoice();
      stopAway();
    };
  }, []);
}

/**
 * Discord's keys (VOICE-KEYS), offered the first time the player is in a room with the window in
 * front (a room joined from the game waits until the window is back); the launcher answers only
 * once per computer.
 */
function useDiscordOnRoom() {
  useEffect(() => {
    const check = () => {
      if (useVoiceUi.getState().state.room && !isWindowAway()) void askDiscord("room");
    };
    check();
    const stopStore = useVoiceUi.subscribe((now, before) => {
      if (now.state.room && !before.state.room) check();
    });
    const stopAway = onWindowAwayChange((away) => {
      if (!away) check();
    });
    return () => {
      stopStore();
      stopAway();
    };
  }, []);
}

/**
 * The settings other surfaces read (VOICE-CALL: the meters' sensitivity, the panel's volume): once
 * voice chat shows, and again whenever the launcher says they changed.
 */
function useVoicePrefsSync() {
  const on = useVoiceUi((s) => voiceOn(s.state));
  const revision = useVoiceUi((s) => s.state.backupRev ?? 0);
  // biome-ignore lint/correctness/useExhaustiveDependencies: the revision says when they changed
  useEffect(() => {
    if (on) loadVoicePrefs();
  }, [on, revision]);
}

export function VoiceHost() {
  useVoiceSync();
  useVoiceTones();
  useMissedSeen();
  useDiscordOnRoom();
  useVoicePrefsSync();
  useLevelStream();
  useIgnoredSync();
  return (
    <>
      <RoomSheet />
      <CallConfirm />
    </>
  );
}
