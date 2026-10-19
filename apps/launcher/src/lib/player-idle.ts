// Whether somebody is using the launcher window right now, for its ambient motion (OPT2). The
// embers, the art's slow drift and the OYNA sheen used to run for as long as the window was on
// screen. In WebView2 every frame of them is a composite of the whole scene (the art, the glass
// blurs, the grain), so at 144 Hz Home cost 89 % of a CPU core with nobody looking, and 71 %
// behind another window, which WebView2 still counts as visible (E2E3). Now they play while the
// player uses the window and for IDLE_AFTER_MS after the last input, rest at once when another
// window takes the focus, and come back with the next input. main.tsx hands the answer to
// PerfProvider as `resting`.
import { useSyncExternalStore } from "react";

/** Ambient motion keeps going this long after the player's last input. */
export const IDLE_AFTER_MS = 8_000;

/** What the tracker listens to: the window, or a stand-in in tests. */
export interface IdleTarget {
  addEventListener(type: string, listener: (event: Event) => void, options?: object): void;
  removeEventListener(type: string, listener: (event: Event) => void, options?: object): void;
}

/** Everything the player does with the window. A scroll comes as a wheel, a key or a drag. */
const INPUT_EVENTS = ["pointermove", "pointerdown", "wheel", "keydown", "touchstart"];
const INPUT_OPTIONS = { capture: true, passive: true };

let idle = false;
let lastInput = 0;
let timer: ReturnType<typeof setTimeout> | undefined;
const listeners = new Set<() => void>();

function setIdle(next: boolean) {
  if (next === idle) return;
  idle = next;
  for (const listener of listeners) listener();
}

function check() {
  timer = undefined;
  const left = lastInput + IDLE_AFTER_MS - Date.now();
  if (left > 0) {
    timer = setTimeout(check, left);
    return;
  }
  setIdle(true);
}

/**
 * The player did something, or brought the window back: the motion wakes and the count starts
 * over. Cheap enough for every pointer move: one timer runs at a time, and when it fires early
 * it waits for the rest.
 */
export function playerActive() {
  lastInput = Date.now();
  setIdle(false);
  if (timer === undefined) timer = setTimeout(check, IDLE_AFTER_MS);
}

/** The player went to another window: the motion rests now. */
export function playerLeft() {
  clearTimeout(timer);
  timer = undefined;
  setIdle(true);
}

/**
 * Follows the player's input on `target` (the window) and its focus: the window losing it means
 * another window is in front. Starts awake (the window just opened). Returns a stop function.
 */
export function trackPlayerIdle(target: IdleTarget): () => void {
  const onInput = () => playerActive();
  // Only the window's own focus changes: an element's focus does not bubble up to here.
  const onFocus = (event: Event) => {
    if (event.target === target) playerActive();
  };
  const onBlur = (event: Event) => {
    if (event.target === target) playerLeft();
  };
  for (const type of INPUT_EVENTS) target.addEventListener(type, onInput, INPUT_OPTIONS);
  target.addEventListener("focus", onFocus);
  target.addEventListener("blur", onBlur);
  clearTimeout(timer);
  timer = undefined;
  playerActive();
  return () => {
    for (const type of INPUT_EVENTS) target.removeEventListener(type, onInput, INPUT_OPTIONS);
    target.removeEventListener("focus", onFocus);
    target.removeEventListener("blur", onBlur);
    clearTimeout(timer);
    timer = undefined;
  };
}

export function usePlayerIdle(): boolean {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => idle,
  );
}

/** The current answer (tests). */
export function isPlayerIdle(): boolean {
  return idle;
}
