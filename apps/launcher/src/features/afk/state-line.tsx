// An AFK account's state in one line: the server bars (lit in the game, sweeping while it
// connects, a cross when the connection dropped) and the words, ticking live for uptime and the
// next reconnect. Used by the cards and the console.
import { cn, SignalBars, type SignalState } from "@pl/ui";
import { useEffect, useState } from "react";
import { useTranslation } from "react-i18next";
import type { AfkState } from "@/lib/api";
import { formatDuration } from "@/lib/format";

function useNow(intervalMs = 1000) {
  const [now, setNow] = useState(() => Date.now());
  useEffect(() => {
    const id = window.setInterval(() => setNow(Date.now()), intervalMs);
    return () => window.clearInterval(id);
  }, [intervalMs]);
  return now;
}

/**
 * The bars for a state. They sweep only while a connection is being made; waiting (in the queue,
 * for the next reconnect) and stopping show them dim, with the countdown in the words.
 */
export function stateSignal(state: AfkState): SignalState {
  switch (state.kind) {
    case "connected":
      return "online";
    case "starting":
    case "connecting":
      return "pending";
    case "error":
      return "offline";
    default:
      return "idle";
  }
}

export function StateLine({ state, className }: { state: AfkState; className?: string }) {
  const { t } = useTranslation();
  const now = useNow();
  let text: string;
  switch (state.kind) {
    case "connected":
      text = t("afk.states.connected", {
        uptime: formatDuration((now - new Date(state.since).getTime()) / 1000),
      });
      break;
    case "reconnecting":
      text = t("afk.states.reconnecting", {
        secs: Math.max(0, Math.round((new Date(state.nextAttemptAt).getTime() - now) / 1000)),
      });
      break;
    case "queued":
      text = t("afk.states.queued", { secs: state.inSecs });
      break;
    case "error":
      text = `${t("afk.states.error")}: ${state.reason}`;
      break;
    default:
      text = t(`afk.states.${state.kind}`);
  }
  const failed = state.kind === "error";
  return (
    <p
      className={cn("flex min-w-0 items-center gap-2 text-[12.5px]", className)}
      // A long disconnect reason is cut in the line; the whole of it stays one hover away.
      title={failed ? text : undefined}
    >
      <SignalBars state={stateSignal(state)} />
      <span className={cn("truncate", failed ? "text-flare-400" : "text-ash-300")}>{text}</span>
    </p>
  );
}
