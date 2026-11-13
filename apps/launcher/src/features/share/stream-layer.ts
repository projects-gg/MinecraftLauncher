// The layer a watched share's tile asks the launcher for ("Yayın Kalitesi", SQOL-U1). The tile keeps
// what it last asked for apart from what the launcher reports: a report can lag behind or not come
// at all (E2E: after "Veri Tasarrufu", "Yüksek" met a stale report that still said the full layer,
// and nothing was asked; a weak connection's hold ending met the same). While the report disagrees
// with the layer it wants, it asks once more after a short settle. A picture that changes on
// purpose (another layer, a share paused or turned to another source) stops a moment: the quality
// look is told, so that moment does not read as a weak connection.
import { useEffect, useRef, useState } from "react";
import { useVoiceUi } from "@/features/voice/store";
import { shareApi } from "@/share/api";
import { type ShareLayer, shareKey, type VoiceLiveShare, type VoiceWatch } from "@/share/types";
import { CHANGE_CALM_MS } from "@/share/watch";
import { calmShare } from "./store";

/** How long the launcher has to report the layer a tile asked for before it is asked again. */
export const LAYER_SETTLE_MS = 3_000;

/**
 * What each watch last asked for. Kept past its tile (the stage's corner and its page are two
 * tiles); a watch that ends forgets it, so watching that share again starts from the report.
 */
const lastAsked = new Map<string, ShareLayer>();

useVoiceUi.subscribe((now, before) => {
  if (now.state.share === before.state.share || lastAsked.size === 0) return;
  const watched = new Set((now.state.share?.watching ?? []).map((w) => shareKey(w.person, w.n)));
  for (const key of [...lastAsked.keys()]) if (!watched.has(key)) lastAsked.delete(key);
});

/** Tests start from nothing. */
export function forgetAskedLayers() {
  lastAsked.clear();
}

/**
 * Asks for `wanted` whenever it differs from what was last asked for this watch (before any ask:
 * from what the launcher reports), never while the watch is held or before the tile has its size.
 */
export function useWatchLayer(
  watch: Pick<VoiceWatch, "person" | "n" | "layer" | "phase">,
  wanted: ShareLayer,
  measured: boolean,
): void {
  const { person, n, layer: reported, phase } = watch;
  useEffect(() => {
    if (phase === "held" || !measured) return;
    const key = shareKey(person, n);
    const ask = () => {
      calmShare(person, n);
      void shareApi.layer(person, n, wanted).catch(() => {});
    };
    if ((lastAsked.get(key) ?? reported) !== wanted) {
      lastAsked.set(key, wanted);
      ask();
    }
    if (reported === wanted) return;
    const timer = setTimeout(ask, LAYER_SETTLE_MS);
    return () => clearTimeout(timer);
  }, [person, n, wanted, reported, phase, measured]);
}

/**
 * A share its sharer paused, minimized, brought back, turned to another kind of source or gave
 * another quality or frame rate: the quality look takes the moment its picture stops for no weak
 * connection (E2E: a step to 15 fps read as weak).
 */
export function useShareChangeCalm(
  person: string,
  n: number,
  live: Pick<VoiceLiveShare, "st" | "kind" | "fps" | "preset"> | undefined,
): void {
  const change = `${live?.st ?? "live"}:${live?.kind ?? ""}:${live?.fps ?? ""}:${live?.preset ?? ""}`;
  const last = useRef(change);
  useEffect(() => {
    if (last.current === change) return;
    last.current = change;
    calmShare(person, n);
  }, [person, n, change]);
}

/**
 * Whether a share's picture says anything of the connection now: not while its sharer paused or
 * minimized it (its frames stop by design), nor in the few seconds after it came back.
 */
export function useShareSettled(live: Pick<VoiceLiveShare, "st"> | undefined): boolean {
  const stopped = live?.st === "hold" || live?.st === "min";
  const [settled, setSettled] = useState(!stopped);
  useEffect(() => {
    if (stopped) {
      setSettled(false);
      return;
    }
    const timer = setTimeout(() => setSettled(true), CHANGE_CALM_MS);
    return () => clearTimeout(timer);
  }, [stopped]);
  return settled && !stopped;
}
