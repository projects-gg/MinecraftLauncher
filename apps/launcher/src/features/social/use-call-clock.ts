// The time crew calls are drawn at. It moves only when something about them changes on screen: a
// call's minutes ("17 dk"), a call running out, or "Ekibi çağır" waking up after its five minutes.
// One timer to the next such moment while there is one, none otherwise (no idle work, no seconds).
import { useEffect } from "react";
import { create } from "zustand";
import { callWaitUntil, nextCallTick } from "./calls";
import { useSocialUi } from "./store";

export const useCallClock = create<{ now: number }>(() => ({ now: Date.now() }));

/** A little after the moment, so the minutes have really changed when the screen redraws. */
const LATE_MS = 40;

/** Mounted once, by SocialHost. */
export function useCallTicker() {
  const calls = useSocialUi((s) => s.view.calls);
  const refusedUntil = useSocialUi((s) => s.callRefusedUntil);
  const lastCallAt = useSocialUi((s) => s.lastCallAt);
  useEffect(() => {
    let timer: number | undefined;
    const wait = callWaitUntil(calls, refusedUntil, lastCallAt);
    const tick = () => {
      const now = Date.now();
      useCallClock.setState({ now });
      const delays = [nextCallTick(calls, now), wait > now ? wait - now : null].filter(
        (delay): delay is number => delay !== null,
      );
      timer = delays.length ? window.setTimeout(tick, Math.min(...delays) + LATE_MS) : undefined;
    };
    tick();
    return () => window.clearTimeout(timer);
  }, [calls, refusedUntil, lastCallAt]);
}
