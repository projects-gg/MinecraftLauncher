// The time crew calls (and a crew's mute) are drawn at. It moves only when something about them
// changes on screen: a call's minutes ("17 dk"), a call running out, "Ekibi çağır" waking up after
// its five minutes, or a timed mute ending ("Sessizde" goes, the bell rings again). One timer to
// the next such moment while there is one, none otherwise (no idle work, no seconds).
import { useEffect } from "react";
import { create } from "zustand";
import { callWaitUntil, nextCallTick } from "./calls";
import { nextMuteTick } from "./mute";
import { useSocialUi } from "./store";

export const useCallClock = create<{ now: number }>(() => ({ now: Date.now() }));

/** A little after the moment, so the minutes have really changed when the screen redraws. */
const LATE_MS = 40;
/** The longest a timer waits at once (browsers take at most about 24.8 days). */
const MAX_TIMER_MS = 24 * 86_400_000;

/** Mounted once, by SocialHost. */
export function useCallTicker() {
  const calls = useSocialUi((s) => s.view.calls);
  const refusedUntil = useSocialUi((s) => s.callRefusedUntil);
  const lastCallAt = useSocialUi((s) => s.lastCallAt);
  const mute = useSocialUi((s) => s.view.crewNotifications);
  useEffect(() => {
    let timer: number | undefined;
    const wait = callWaitUntil(calls, refusedUntil, lastCallAt);
    const tick = () => {
      const now = Date.now();
      useCallClock.setState({ now });
      const delays = [
        nextCallTick(calls, now),
        wait > now ? wait - now : null,
        nextMuteTick(mute, now),
      ].filter((delay): delay is number => delay !== null);
      // A mute set elsewhere may last 30 days, longer than a browser timer can wait.
      const next = Math.min(...delays, MAX_TIMER_MS);
      timer = delays.length ? window.setTimeout(tick, next + LATE_MS) : undefined;
    };
    tick();
    return () => window.clearTimeout(timer);
  }, [calls, refusedUntil, lastCallAt, mute]);
}
