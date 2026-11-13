// The player's own share as it really goes out (SQOL, sqol.md §1.2/§3): its small live picture
// (`share_preview`, at most twice a second, asked only while a card or the stage's own tile shows
// it) and what it sends (`share-stats`: size, frame rate, encoder). One listener for the page
// while anything reads them, none otherwise; a picture is decoded before it shows (no half-drawn
// frame), and an older picture that decodes late never covers a newer one. The numbers draw again
// only when what they show changes (not at every few seconds' report).
import { useEffect, useSyncExternalStore } from "react";
import i18n from "@/lib/i18n";
import { shareApi } from "@/share/api";
import type { SharePreviewFrame, ShareStats } from "@/share/types";

// --- the small live picture -------------------------------------------------------------------

const asking = new Map<number, number>();
const frames = new Map<number, string>();
const frameSeq = new Map<number, number>();
const frameListeners = new Set<() => void>();
let stopFrames: (() => void) | null = null;

function tellFrames() {
  for (const listener of frameListeners) listener();
}

async function decodes(dataUrl: string): Promise<boolean> {
  if (typeof Image === "undefined") return true;
  const image = new Image();
  if (typeof image.decode !== "function") return true;
  image.src = dataUrl;
  try {
    await image.decode();
    return true;
  } catch {
    return false;
  }
}

function onFrame(frame: SharePreviewFrame) {
  if (!asking.has(frame.n)) return;
  const mine = (frameSeq.get(frame.n) ?? 0) + 1;
  frameSeq.set(frame.n, mine);
  void decodes(frame.dataUrl).then((ok) => {
    // A newer picture came meanwhile, or nobody shows it any more.
    if (!ok || frameSeq.get(frame.n) !== mine || !asking.has(frame.n)) return;
    frames.set(frame.n, frame.dataUrl);
    tellFrames();
  });
}

/** Asks for share `n`'s live picture; the returned function lets go (the last one stops it). */
function askPreview(n: number): () => void {
  const count = (asking.get(n) ?? 0) + 1;
  asking.set(n, count);
  if (!stopFrames) stopFrames = shareApi.onPreview(onFrame);
  if (count === 1) void shareApi.preview(n, true).catch(() => {});
  let done = false;
  return () => {
    if (done) return;
    done = true;
    const left = (asking.get(n) ?? 1) - 1;
    if (left > 0) {
      asking.set(n, left);
      return;
    }
    asking.delete(n);
    void shareApi.preview(n, false).catch(() => {});
    if (asking.size === 0) {
      stopFrames?.();
      stopFrames = null;
    }
  };
}

/** The share is gone: its last picture goes with it. */
export function forgetOwnPreview(n: number) {
  frameSeq.delete(n);
  if (frames.delete(n)) tellFrames();
}

function subscribeFrames(listener: () => void): () => void {
  frameListeners.add(listener);
  return () => frameListeners.delete(listener);
}

/**
 * Share `n`'s small live picture while `on` (the card or the tile is on screen); the last one
 * stays while it is off (null until the first came).
 */
export function useOwnSharePreview(n: number, on: boolean): string | null {
  useEffect(() => (on ? askPreview(n) : undefined), [n, on]);
  return useSyncExternalStore(
    subscribeFrames,
    () => frames.get(n) ?? null,
    () => null,
  );
}

// --- what it sends -------------------------------------------------------------------------------

const stats = new Map<number, ShareStats>();
const statsListeners = new Set<() => void>();
let stopStats: (() => void) | null = null;

/** What the card shows of a report: its size, its whole frame rate, its encoder. */
const shown = (s: ShareStats) => `${s.w}x${s.h}@${Math.round(s.fps)}:${s.hw ? 1 : 0}`;

function onStats(next: ShareStats) {
  const last = stats.get(next.n);
  if (last && shown(last) === shown(next)) return;
  stats.set(next.n, next);
  for (const listener of statsListeners) listener();
}

function subscribeStats(listener: () => void): () => void {
  statsListeners.add(listener);
  if (!stopStats) stopStats = shareApi.onStats(onStats);
  return () => {
    statsListeners.delete(listener);
    if (statsListeners.size === 0) {
      stopStats?.();
      stopStats = null;
    }
  };
}

/** The share is gone: its numbers go with it. */
export function forgetOwnStats(n: number) {
  if (stats.delete(n)) for (const listener of statsListeners) listener();
}

/** What share `n` really sends (null until the launcher said). */
export function useOwnShareStats(n: number): ShareStats | null {
  return useSyncExternalStore(
    subscribeStats,
    () => stats.get(n) ?? null,
    () => null,
  );
}

/** "1280×720 · 30 fps · Donanım": what a share really sends, for the card and the own tile. */
export function statsLine(s: ShareStats): string {
  const t = i18n.t.bind(i18n);
  return t("share.own.stats", {
    w: s.w,
    h: s.h,
    fps: Math.round(s.fps),
    encoder: s.hw ? t("share.own.hardware") : t("share.own.software"),
  });
}

/** Tests start from nothing. */
export function resetOwnLive() {
  asking.clear();
  frames.clear();
  frameSeq.clear();
  stats.clear();
  stopFrames?.();
  stopFrames = null;
  stopStats?.();
  stopStats = null;
}
