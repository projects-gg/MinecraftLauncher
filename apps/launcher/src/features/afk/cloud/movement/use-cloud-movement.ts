// A cloud account's movement for the "Hareket" panel: it streams only while the panel is visible and
// the window is on screen (`window-away.ts`; nothing runs otherwise). Operations go out on the live
// socket; their answers (`movres`) come back to the one that sent them.
import { useCallback, useEffect, useRef, useState } from "react";
import { useWindowAway } from "@/lib/window-away";
import { type CloudAccount, cloudApi } from "../api";
import {
  type MovementState,
  type MoveOp,
  type MoveResult,
  moveOpId,
  reduceMovement,
  WAITING,
} from "./model";

/** An answer is dropped after this long (the machine answers within 3 s, or the object says so). */
const PENDING_MS = 10_000;
const UNREACHABLE: MoveResult = { ok: false, error: { code: "host_offline" } };

/** Whether the account shows "Hareket": movement is on for it and it has a machine. */
export function movementShown(account: CloudAccount | undefined): boolean {
  const movement = account?.features?.movement;
  return Boolean(account?.host && movement?.mode === "on" && movement.on);
}

/**
 * `link`: the console's reconnect count ("Yeniden aç" after the socket was replaced), so the
 * stream is asked for again with it.
 */
export function useCloudMovement(account: CloudAccount | undefined, visible: boolean, link = 0) {
  const away = useWindowAway();
  const id = account?.id;
  const host = account?.host ?? null;
  const shown = movementShown(account);
  const [state, setState] = useState<MovementState>(WAITING);
  const pending = useRef(new Map<string, (result: MoveResult) => void>());

  // Another account starts from nothing.
  // biome-ignore lint/correctness/useExhaustiveDependencies: resets when the account changes
  useEffect(() => {
    setState(WAITING);
  }, [id]);

  useEffect(() => {
    if (!id) return;
    const waiting = pending.current;
    const stop = cloudApi.onLive((event) => {
      if (event.kind === "movementResult") {
        const resolve = waiting.get(event.id);
        if (resolve) {
          waiting.delete(event.id);
          resolve(
            event.ok ? { ok: true } : { ok: false, error: event.error ?? { code: "host_offline" } },
          );
        }
        return;
      }
      // A lost socket or machine: what it showed may be old; the next watch sends the state first.
      if (
        (event.kind === "status" && event.host === host && event.status !== "live") ||
        (event.kind === "machine" && event.host === host && !event.online)
      ) {
        setState(WAITING);
        return;
      }
      setState((current) => reduceMovement(current, event, id));
    });
    return () => {
      stop();
      for (const resolve of waiting.values()) resolve(UNREACHABLE);
      waiting.clear();
    };
  }, [id, host]);

  const streaming = Boolean(id && visible && !away && shown);
  // Watched again when the account moves to another machine or the socket was opened again.
  // biome-ignore lint/correctness/useExhaustiveDependencies: host and link restart the stream
  useEffect(() => {
    if (!streaming || !id) return;
    void cloudApi.movementWatch(id).catch(() => undefined);
    return () => {
      void cloudApi.movementUnwatch(id);
    };
  }, [streaming, id, host, link]);

  const send = useCallback(
    async (op: MoveOp): Promise<MoveResult> => {
      if (!id) return UNREACHABLE;
      const opId = moveOpId();
      const waiting = pending.current;
      const answer = new Promise<MoveResult>((resolve) => {
        waiting.set(opId, resolve);
        window.setTimeout(() => {
          if (waiting.delete(opId)) resolve(UNREACHABLE);
        }, PENDING_MS);
      });
      try {
        const refused = await cloudApi.movementOp(id, opId, op);
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
