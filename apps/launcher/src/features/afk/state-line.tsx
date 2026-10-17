// An AFK account's state in one line: a dot (gold pulses while something is under way) and the
// words, ticking live for uptime and the next reconnect. Used by the cards and the console.
import { cn } from "@pl/ui";
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

function stateTone(state: AfkState): "jade" | "gold" | "flare" | "ash" {
  switch (state.kind) {
    case "connected":
      return "jade";
    case "error":
      return "flare";
    case "stopped":
      return "ash";
    default:
      return "gold";
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
  const tone = stateTone(state);
  return (
    <p
      className={cn("flex min-w-0 items-center gap-2 text-[12.5px]", className)}
      // A long disconnect reason is cut in the line; the whole of it stays one hover away.
      title={state.kind === "error" ? text : undefined}
    >
      <span
        aria-hidden
        className={cn(
          "size-2 shrink-0 rounded-full",
          tone === "jade" && "bg-jade-400 shadow-[0_0_8px_var(--color-jade-400)]",
          tone === "gold" &&
            "bg-gold-400 motion-safe:animate-pulse [html[data-perf=on]_&]:animate-none",
          tone === "flare" && "bg-flare-400",
          tone === "ash" && "bg-ash-500",
        )}
      />
      <span className={cn("truncate", tone === "flare" ? "text-flare-400" : "text-ash-300")}>
        {text}
      </span>
    </p>
  );
}
