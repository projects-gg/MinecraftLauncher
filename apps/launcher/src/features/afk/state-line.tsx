// An AFK account's state in one line: the server bars (lit in the game, sweeping while it
// connects or its client is fetched, a cross when the connection dropped or the client could not
// be had) and the words, ticking live for uptime and the next reconnect. Used by the cards and the
// console.
import { cn, SignalBars, type SignalState, usePerf } from "@pl/ui";
import type { TFunction } from "i18next";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AfkState } from "@/lib/api";
import { formatDuration } from "@/lib/format";

/**
 * The bars for a state. They sweep only while a connection is being made (its client fetched
 * included); waiting (in the queue, for the next reconnect) and stopping show them dim, with the
 * countdown in the words.
 */
export function stateSignal(state: AfkState): SignalState {
  switch (state.kind) {
    case "connected":
      return "online";
    case "preparing":
    case "starting":
    case "connecting":
      return "pending";
    case "error":
    case "clientFailed":
      return "offline";
    default:
      return "idle";
  }
}

/** What happened, for a failed state (the card says what to do under it). */
export function failedText(state: AfkState, t: TFunction): string | null {
  switch (state.kind) {
    case "error":
      return state.standalone ? state.reason : `${t("afk.states.error")}: ${state.reason}`;
    case "clientFailed":
      return t(`afkConnect.problem.${state.problem}.title`);
    default:
      return null;
  }
}

/** Whole seconds before a reconnect attempt: counts down to 1, the attempt comes after it. */
export function reconnectSecs(nextAttemptAt: string, now: number): number {
  return Math.max(1, Math.ceil((Date.parse(nextAttemptAt) - now) / 1000));
}

/**
 * How long until the line's words change by themselves, in ms; null when they do not. The uptime
 * says seconds in its first minute and minutes after it (`formatDuration` rounds to the second),
 * the reconnect countdown changes on each whole second left. The line renders again just then and
 * not every second: an hour-long connection used to render every card's line 3600 times for 60
 * changes.
 */
export function nextChangeMs(state: AfkState, now: number): number | null {
  if (state.kind === "connected") {
    const up = now - Date.parse(state.since);
    const shownSecs = Math.round(up / 1000);
    // The words change where the rounded seconds (or minutes) do: half a second before.
    const at =
      shownSecs < 60
        ? Math.floor((up + 500) / 1000) * 1000 + 500
        : (Math.floor(shownSecs / 60) + 1) * 60_000 - 500;
    return Math.max(1, at - up);
  }
  if (state.kind === "reconnecting" && state.nextAttemptAt) {
    const left = Date.parse(state.nextAttemptAt) - now;
    if (left <= 1000) return null;
    return left % 1000 || 1000;
  }
  return null;
}

/**
 * The line's words for `state` at `now`. A stopped account that connects by itself when the
 * launcher opens (`autoStart`) says so there, where it matters (SMALL10b: a pill beside the name
 * said it before).
 */
export function stateText(state: AfkState, now: number, t: TFunction, autoStart = false): string {
  switch (state.kind) {
    case "stopped":
      return autoStart ? t("small10.afk.stoppedAutoStart") : t("afk.states.stopped");
    case "connected":
      return t("afk.states.connected", {
        uptime: formatDuration((now - Date.parse(state.since)) / 1000),
      });
    case "reconnecting":
      // Until the backend knows when (the client is still closing, or the attempt is starting)
      // the line says so without a number: a guessed one flashed "15 sn" between two reconnects.
      return state.nextAttemptAt
        ? t("afk.states.reconnecting", { secs: reconnectSecs(state.nextAttemptAt, now) })
        : t("afkReconnect.pending");
    case "queued":
      return t("afk.states.queued", { secs: state.inSecs });
    case "preparing":
      return t(state.step === "downloading" ? "afkConnect.downloading" : "afkConnect.preparing");
    case "error":
    case "clientFailed":
      return failedText(state, t) ?? "";
    default:
      return t(`afk.states.${state.kind}`);
  }
}

/** "Oyunda, 1 sa 5 dk" → "oyunda, 1 sa 5 dk", to follow a place ("Bu bilgisayarda …"). */
export function lowerFirst(text: string, lang: "tr" | "en"): string {
  return text.charAt(0).toLocaleLowerCase(lang) + text.slice(1);
}

export function StateLine({
  state,
  autoStart = false,
  where,
  className,
}: {
  state: AfkState;
  /** The account connects by itself when the launcher opens (its card says so while stopped). */
  autoStart?: boolean;
  /**
   * The line stands on the 7/24 Bulut card of the same account (one card for both, AFK-CLEAR): it
   * says where ("Bu bilgisayarda oyunda, 1 sa 5 dk").
   */
  where?: "here";
  className?: string;
}) {
  const { t, i18n } = useTranslation();
  const { unseen } = usePerf();
  const [ticks, tick] = useState(0);
  // The clock is read on every render, so a new state never shows with an old time.
  const now = Date.now();
  const words = stateText(state, now, t, autoStart);
  const lang = i18n.language === "en" ? "en" : "tr";
  const text =
    where === "here" ? t("afkClear.twin.hereState", { state: lowerFirst(words, lang) }) : words;
  // Only the uptime and the reconnect countdown tick, and only while somebody can see them.
  const wait = unseen ? null : nextChangeMs(state, now);
  useEffect(() => {
    // Armed again after every tick, also when the next wait equals the last one (two whole
    // seconds in a row: the line stood still until the state changed).
    void ticks;
    if (wait === null) return;
    const id = window.setTimeout(() => tick((n) => n + 1), wait);
    return () => window.clearTimeout(id);
  }, [wait, ticks]);
  const detail =
    state.kind === "preparing" && state.percent !== undefined
      ? t("afkConnect.percent", { value: Math.floor(state.percent) })
      : null;
  const failed = state.kind === "error" || state.kind === "clientFailed";
  return (
    <p
      className={cn("flex min-w-0 items-center gap-2 text-[12.5px]", className)}
      // A long disconnect reason is cut in the line; the whole of it stays one hover away.
      title={failed ? text : undefined}
    >
      <SignalBars state={stateSignal(state)} />
      <span className={cn("truncate", failed ? "text-flare-400" : "text-ash-300")}>{text}</span>
      {detail ? (
        <span className="shrink-0 text-ash-300 tabular-nums" aria-hidden>
          {detail}
        </span>
      ) : null}
    </p>
  );
}
