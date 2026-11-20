// A cloud account's auto-fishing and auto-mining for the "Balık" and "Kazma" panels: one stream for
// both (`watch` with `auto`), only while one of the two is visible and the window is on screen
// (`window-away.ts`; nothing runs otherwise). Operations go out on the live socket; their answers
// (`autores`) come back to the one that sent them.
import { useCallback, useEffect, useRef, useState } from "react";
import { useWindowAway } from "@/lib/window-away";
import { type CloudAccount, type CloudAutoError, cloudApi } from "../api";
import {
  AUTO_WAITING,
  type AutoOp,
  type AutoResult,
  type AutoState,
  autoOpId,
  autoShown,
  reduceAuto,
} from "./model";

/** An answer is dropped after this long (the machine answers within 3 s, or the object says so). */
const PENDING_MS = 10_000;
const OFFLINE: CloudAutoError = { code: "host_offline", reason: null, field: null };
const UNREACHABLE: AutoResult = { ok: false, error: OFFLINE };

/**
 * `visible`: a "Balık" or "Kazma" panel is on screen. `link`: the console's reconnect count
 * ("Yeniden aç" after the socket was replaced), so the stream is asked for again with it.
 */
export function useCloudAuto(account: CloudAccount | undefined, visible: boolean, link = 0) {
  const away = useWindowAway();
  const id = account?.id;
  const host = account?.host ?? null;
  const shown = autoShown(account, "fishing") || autoShown(account, "mining");
  const [state, setState] = useState<AutoState>(AUTO_WAITING);
  const pending = useRef(new Map<string, (result: AutoResult) => void>());

  // Another account starts from nothing.
  // biome-ignore lint/correctness/useExhaustiveDependencies: resets when the account changes
  useEffect(() => {
    setState(AUTO_WAITING);
  }, [id]);

  // Listening only for an account with a mode on: nothing else needs the frames.
  useEffect(() => {
    if (!id || !shown) return;
    const waiting = pending.current;
    const stop = cloudApi.onLive((event) => {
      if (event.kind === "autoResult") {
        const resolve = waiting.get(event.id);
        if (resolve) {
          waiting.delete(event.id);
          resolve(event.ok ? { ok: true } : { ok: false, error: event.error ?? OFFLINE });
        }
        return;
      }
      // A lost socket or machine: what it showed may be old; the next watch sends the state first.
      if (
        (event.kind === "status" && event.host === host && event.status !== "live") ||
        (event.kind === "machine" && event.host === host && !event.online)
      ) {
        setState(AUTO_WAITING);
        return;
      }
      setState((current) => reduceAuto(current, event, id));
    });
    return () => {
      stop();
      for (const resolve of waiting.values()) resolve(UNREACHABLE);
      waiting.clear();
    };
  }, [id, host, shown]);

  const streaming = Boolean(id && visible && !away && shown);
  // Watched again when the account moves to another machine or the socket was opened again.
  // biome-ignore lint/correctness/useExhaustiveDependencies: host and link restart the stream
  useEffect(() => {
    if (!streaming || !id) return;
    void cloudApi.autoWatch(id).catch(() => undefined);
    return () => {
      void cloudApi.autoUnwatch(id);
    };
  }, [streaming, id, host, link]);

  const send = useCallback(
    async (op: AutoOp): Promise<AutoResult> => {
      if (!id) return UNREACHABLE;
      const opId = autoOpId();
      const waiting = pending.current;
      const answer = new Promise<AutoResult>((resolve) => {
        waiting.set(opId, resolve);
        window.setTimeout(() => {
          if (waiting.delete(opId)) resolve(UNREACHABLE);
        }, PENDING_MS);
      });
      try {
        const refused = await cloudApi.autoOp(id, opId, op);
        if (refused) {
          waiting.delete(opId);
          return { ok: false, error: refused };
        }
      } catch {
        waiting.delete(opId);
        return UNREACHABLE;
      }
      return answer;
    },
    [id],
  );

  return { state, send, streaming };
}

export type CloudAuto = ReturnType<typeof useCloudAuto>;
