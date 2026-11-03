// How long a call has been on (VOICE-CALL): "04:12", "1:04:12" past an hour, in the title bar's
// pill and on the room panel's call, for a call with one friend (a group or a crew room has
// none). It counts from when someone else first shared the room (`togetherSince`, kept by the
// launcher, so a reload or a reconnect never restarts it). One timer for every clock on the page,
// aligned to the second, and only while the window is on screen: hidden in the tray or minimized
// nothing ticks, and the clock is right again the moment it comes back.
import { cn } from "@pl/ui";
import { useSyncExternalStore } from "react";
import { useTranslation } from "react-i18next";
import type { VoiceState } from "@/lib/api/voice-types";
import { isWindowAway, onWindowAwayChange } from "@/lib/window-away";

/** A call's length as clocks say it: minutes and seconds, hours in front past the first one. */
export function elapsedText(seconds: number): string {
  const total = Math.max(0, Math.floor(seconds));
  const hours = Math.floor(total / 3600);
  const minutes = Math.floor((total % 3600) / 60);
  const rest = total % 60;
  const two = (n: number) => String(n).padStart(2, "0");
  return hours > 0 ? `${hours}:${two(minutes)}:${two(rest)}` : `${two(minutes)}:${two(rest)}`;
}

/** When the call the player is in started, if it is a call with one friend (else null). */
export function callStart(
  state: Pick<VoiceState, "room" | "togetherSince" | "calling">,
): number | null {
  const room = state.room;
  if (room?.kind !== "friend" || room.people.length !== 1 || state.calling) return null;
  return state.togetherSince ?? null;
}

// --- the page's one clock -------------------------------------------------------------------------

const listeners = new Set<() => void>();
let now = Math.floor(Date.now() / 1000);
let timer: ReturnType<typeof setTimeout> | undefined;
let stopAway: (() => void) | undefined;

function notify(): void {
  now = Math.floor(Date.now() / 1000);
  for (const listener of listeners) listener();
}

function schedule(): void {
  clearTimeout(timer);
  timer = undefined;
  if (!listeners.size || isWindowAway()) return;
  // Just past the next whole second, so the digits change together with the system clock.
  timer = setTimeout(
    () => {
      notify();
      schedule();
    },
    1000 - (Date.now() % 1000) + 5,
  );
}

function subscribe(listener: () => void): () => void {
  listeners.add(listener);
  if (!stopAway) {
    stopAway = onWindowAwayChange((away) => {
      if (!away) notify();
      schedule();
    });
  }
  if (listeners.size === 1) {
    now = Math.floor(Date.now() / 1000);
    schedule();
  }
  return () => {
    listeners.delete(listener);
    if (listeners.size) return;
    clearTimeout(timer);
    timer = undefined;
    stopAway?.();
    stopAway = undefined;
  };
}

/** Unix seconds now, ticking once a second while a clock shows and the window is on screen. */
export function useNowSeconds(): number {
  return useSyncExternalStore(subscribe, () => now);
}

/** A call's clock (`since`: Unix seconds). A timer to screen readers, read when they reach it. */
export function CallTime({ since, className }: { since: number; className?: string }) {
  const { t } = useTranslation();
  const seconds = useNowSeconds() - since;
  return (
    <span
      role="timer"
      aria-label={t("voice.call.time", { time: elapsedText(seconds) })}
      className={cn("font-display tabular-nums", className)}
    >
      {elapsedText(seconds)}
    </span>
  );
}
