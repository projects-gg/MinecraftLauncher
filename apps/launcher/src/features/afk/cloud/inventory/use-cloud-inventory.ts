// A cloud account's live inventory for a view on screen (the account panel's Envanter tab, and
// AFK+'s "Envanterden seç"): it streams only while the view is visible and the window is on screen
// (`window-away.ts`; nothing runs otherwise), and its moves go out on the live socket with their
// answers (`invres`) handed back to `InventoryView`.
import type { InvOp, InvResult } from "@pl/ui/inventory";
import { useCallback, useEffect, useRef, useState } from "react";
import { useWindowAway } from "@/lib/window-away";
import { type CloudAccount, cloudApi } from "../api";
import { type InventoryState, moveId, reduce, toInvResult, WAITING } from "./model";

/** A move's answer is dropped after this long (the view gives up on it after 5 s). */
const PENDING_MS = 10_000;
const UNREACHABLE: InvResult = { ok: false, error: { code: "host_offline" } };

/**
 * A watch starts the machine's frames again (`invst`, a full `inv`, then `win` only while a window
 * is open): a window that closed meanwhile must not stay drawn, so it goes until the frames say.
 */
const withoutWindow = (state: InventoryState): InventoryState =>
  state.window ? { ...state, window: null } : state;

/**
 * `link`: the console's reconnect count ("Yeniden aç" after the socket was replaced), so the
 * stream is asked for again with it.
 */
export function useCloudInventory(account: CloudAccount | undefined, visible: boolean, link = 0) {
  const away = useWindowAway();
  const id = account?.id;
  const host = account?.host ?? null;
  const mode = account?.features?.inventory?.mode ?? "off";
  const [state, setState] = useState<InventoryState>(WAITING);
  const pending = useRef(new Map<string, (result: InvResult) => void>());

  // Another account starts from nothing.
  // biome-ignore lint/correctness/useExhaustiveDependencies: resets when the account changes
  useEffect(() => {
    setState(WAITING);
  }, [id]);

  useEffect(() => {
    if (!id) return;
    const waiting = pending.current;
    const stop = cloudApi.onLive((event) => {
      if (event.kind === "invResult") {
        const resolve = waiting.get(event.id);
        if (resolve) {
          waiting.delete(event.id);
          resolve(toInvResult(event.ok, event.error));
        }
        return;
      }
      // A new connection watches again by itself.
      if (event.kind === "hello") {
        if (event.host === host) setState(withoutWindow);
        return;
      }
      setState((current) => reduce(current, event, id));
    });
    return () => {
      stop();
      for (const resolve of waiting.values()) resolve(UNREACHABLE);
      waiting.clear();
    };
  }, [id, host]);

  const streaming = Boolean(id && host && visible && !away && mode !== "off");
  // Watched again when the account moves to another machine or the socket was opened again.
  // biome-ignore lint/correctness/useExhaustiveDependencies: host and link restart the stream
  useEffect(() => {
    if (!streaming || !id) return;
    setState(withoutWindow);
    void cloudApi.inventoryWatch(id).catch(() => undefined);
    return () => {
      void cloudApi.inventoryUnwatch(id);
    };
  }, [streaming, id, host, link]);

  const onOp = useCallback(
    async (op: InvOp): Promise<InvResult> => {
      if (!id) return UNREACHABLE;
      const opId = moveId();
      const waiting = pending.current;
      const answer = new Promise<InvResult>((resolve) => {
        waiting.set(opId, resolve);
        window.setTimeout(() => {
          if (waiting.delete(opId)) resolve(UNREACHABLE);
        }, PENDING_MS);
      });
      try {
        const refused = await cloudApi.invOp(id, opId, op);
        if (refused) {
          waiting.delete(opId);
          return toInvResult(false, refused);
        }
      } catch {
        waiting.delete(opId);
        return UNREACHABLE;
      }
      return answer;
    },
    [id],
  );

  return { state, onOp, mode };
}
