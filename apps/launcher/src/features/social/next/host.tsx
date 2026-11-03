// What SOC-NEXT runs in the background, mounted once with the friends feature (SocialHost): its
// switch from the signed config, the snapshot shown again when a nickname changes, the timed
// status's one timer, and "Yayıncı modu"'s look for a streaming program (when the launcher opens,
// when its window comes back and when a game starts; never on a timer).
import { useQuery } from "@tanstack/react-query";
import { useEffect, useRef, useState } from "react";
import { useTranslation } from "react-i18next";
import { announce } from "@/features/accounts/use-accounts";
import { api } from "@/lib/api";
import { useWindowAway } from "@/lib/window-away";
import { useLaunch } from "@/stores/launch";
import { SocNoteHost } from "../note/host";
import { changeStatus } from "../status-menu";
import { useSocialUi } from "../store";
import { socNextOn, useSocNext, useSocNextFlag } from "./flag";
import { pruneMentions } from "./mention-marks";
import { NicknameDialog } from "./nickname-dialog";
import { useSocNextPrefs } from "./prefs";
import { timerDelay, timerVerdict } from "./status-timer";
import { useStreamer } from "./streamer";

/** The signed config's `features.socNext`, from the home content (the same query Home reads). */
function useFlagSync() {
  const { data } = useQuery({ queryKey: ["home-content"], queryFn: () => api.homeContent() });
  const on = Boolean(data?.socNext);
  useEffect(() => {
    useSocNextFlag.setState({ on });
  }, [on]);
}

/** A nickname given or taken away, or the switch turned: the snapshot shows again. */
function useNamesSync() {
  useEffect(() => {
    const again = () => useSocialUi.getState().reshow();
    const stopPrefs = useSocNextPrefs.subscribe((now, before) => {
      if (now.nicknames !== before.nicknames && socNextOn()) again();
    });
    const stopFlag = useSocNextFlag.subscribe((now, before) => {
      if (now.on !== before.on) again();
    });
    return () => {
      stopPrefs();
      stopFlag();
    };
  }, []);
}

/** "Rahatsız etmeyin" or "Görünmez" for a while: back to "Çevrimiçi" at its end (status-timer.ts). */
function useStatusTimer() {
  const { t } = useTranslation();
  const on = useSocNext();
  const timer = useSocNextPrefs((s) => s.statusTimer);
  const status = useSocialUi((s) =>
    s.view.auth === "signedIn" ? (s.statusPending ?? s.view.me?.status) : undefined,
  );
  // The clock is read again at the timer's end and when the window comes to the front (a computer
  // that slept meanwhile wakes the timeout late).
  const [tick, setTick] = useState(0);
  useEffect(() => {
    const again = () => setTick((n) => n + 1);
    window.addEventListener("focus", again);
    return () => window.removeEventListener("focus", again);
  }, []);
  // biome-ignore lint/correctness/useExhaustiveDependencies: `tick` reads the clock again
  useEffect(() => {
    if (!on || !timer) return;
    const prefs = useSocNextPrefs.getState();
    const verdict = timerVerdict(timer, status, Date.now());
    if (verdict === "drop") {
      prefs.setStatusTimer(null);
    } else if (verdict === "end") {
      prefs.setStatusTimer(null);
      changeStatus("online");
      announce(t("socNext.status.back"));
    } else if (verdict === "wait" && status !== undefined) {
      const id = window.setTimeout(() => setTick((n) => n + 1), timerDelay(timer, Date.now()));
      return () => window.clearTimeout(id);
    }
  }, [on, timer, status, tick, t]);
}

/** "Yayın programı açıkken": one look at the start, when the window shows again and when a game
 * starts (the stream usually starts with it). */
function useStreamerLook() {
  const on = useSocNext();
  const mode = useSocNextPrefs((s) => s.streamer);
  const away = useWindowAway();
  const phase = useLaunch((s) => s.phase.kind);
  const looking = useRef(false);
  const watching = on && mode === "auto";
  const starting = phase === "launching";
  const look = useRef(() => {
    if (looking.current) return;
    looking.current = true;
    void api.social
      .streamingApp()
      .then((running) => useStreamer.getState().setDetected(running))
      .finally(() => {
        looking.current = false;
      });
  });
  useEffect(() => {
    if (watching && !away) look.current();
  }, [watching, away]);
  useEffect(() => {
    if (watching && starting) look.current();
  }, [watching, starting]);
}

/** A crew conversation read (nothing unread any more) loses its "@". */
function useMentionPrune() {
  useEffect(
    () =>
      useSocialUi.subscribe((now, before) => {
        if (now.view.chat !== before.view.chat) {
          pruneMentions(now.view.chat?.conversations ?? []);
        }
      }),
    [],
  );
}

export function SocNextHost() {
  useFlagSync();
  useNamesSync();
  useMentionPrune();
  useStatusTimer();
  useStreamerLook();
  return (
    <>
      <NicknameDialog />
      {/* SOC-NOTE: the notes' clock, "Not ekle" and "Sabitler dolu". */}
      <SocNoteHost />
    </>
  );
}
