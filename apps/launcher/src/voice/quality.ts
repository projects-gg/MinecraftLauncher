// The connection's quality (§6.5): 0 good, 1 weak, 2 bad, from what WebRTC already counts (round
// trip, packets lost both ways, jitter). Worse shows at once; better only after two windows in a
// row, so the mark does not flicker. Windows with too few packets (silence: DTX sends little) say
// nothing about loss.

export type Quality = 0 | 1 | 2;

interface Receiver {
  received: number;
  lost: number;
  /** ms. */
  jitter: number | null;
}

/** One `getStats`, its counters as they stand. */
export interface RawStats {
  /** ms. */
  rttMs: number | null;
  /** Our packets: sent, and lost on the way to the SFU (its receiver reports). */
  upSent: number;
  upLost: number;
  /** ms, the SFU's view of ours. */
  upJitter: number | null;
  /** Every receiver's counters, by stats id. */
  down: Map<string, Receiver>;
}

/** What a stretch between two `getStats` says. */
export interface QualityWindow {
  rttMs: number | null;
  /** 0–1; null: too few packets to tell. */
  loss: number | null;
  jitterMs: number | null;
}

/** Packets a direction needs in a window before its loss counts. */
const MIN_PACKETS = 50;
const WEAK = { rttMs: 300, loss: 0.04, jitterMs: 40 };
const BAD = { rttMs: 600, loss: 0.12, jitterMs: 80 };

type Stat = { type?: string; kind?: string; [key: string]: unknown };

const num = (value: unknown): number | null =>
  typeof value === "number" && Number.isFinite(value) ? value : null;

/** The counters this needs from a stats report (any iterable of its values). */
export function readStats(values: Iterable<Stat>): RawStats {
  const raw: RawStats = { rttMs: null, upSent: 0, upLost: 0, upJitter: null, down: new Map() };
  for (const stat of values) {
    if (stat.kind !== undefined && stat.kind !== "audio") continue;
    switch (stat.type) {
      case "candidate-pair": {
        const rtt = num(stat.currentRoundTripTime);
        if (stat.nominated === true && stat.state === "succeeded" && rtt !== null) {
          raw.rttMs = rtt * 1000;
        }
        break;
      }
      case "outbound-rtp":
        raw.upSent += num(stat.packetsSent) ?? 0;
        break;
      case "remote-inbound-rtp": {
        raw.upLost += Math.max(0, num(stat.packetsLost) ?? 0);
        const jitter = num(stat.jitter);
        if (jitter !== null) raw.upJitter = Math.max(raw.upJitter ?? 0, jitter * 1000);
        if (raw.rttMs === null) {
          const rtt = num(stat.roundTripTime);
          if (rtt !== null) raw.rttMs = rtt * 1000;
        }
        break;
      }
      case "inbound-rtp": {
        const jitter = num(stat.jitter);
        raw.down.set(String(stat.id ?? ""), {
          received: num(stat.packetsReceived) ?? 0,
          lost: Math.max(0, num(stat.packetsLost) ?? 0),
          jitter: jitter === null ? null : jitter * 1000,
        });
        break;
      }
    }
  }
  return raw;
}

/** The stretch from `before` (none: since the start) to `now`. */
export function windowOf(before: RawStats | null, now: RawStats): QualityWindow {
  const losses: number[] = [];
  const upSent = now.upSent - (before?.upSent ?? 0);
  const upLost = now.upLost - (before?.upLost ?? 0);
  if (upSent >= MIN_PACKETS && upLost >= 0) losses.push(Math.min(1, upLost / upSent));
  let received = 0;
  let lost = 0;
  let jitter: number | null = now.upJitter;
  for (const [id, receiver] of now.down) {
    const old = before?.down.get(id);
    const got = receiver.received - (old?.received ?? 0);
    const missed = receiver.lost - (old?.lost ?? 0);
    if (got < 0 || missed < 0) continue;
    received += got;
    lost += missed;
    // A receiver's jitter only means something while packets come.
    if (got > 0 && receiver.jitter !== null) jitter = Math.max(jitter ?? 0, receiver.jitter);
  }
  if (received + lost >= MIN_PACKETS) losses.push(lost / (received + lost));
  return {
    rttMs: now.rttMs,
    loss: losses.length ? Math.max(...losses) : null,
    jitterMs: jitter,
  };
}

/** A window's grade. */
export function grade(window: QualityWindow): Quality {
  const over = (limit: typeof WEAK) =>
    (window.rttMs ?? 0) > limit.rttMs ||
    (window.loss ?? 0) > limit.loss ||
    (window.jitterMs ?? 0) > limit.jitterMs;
  if (over(BAD)) return 2;
  if (over(WEAK)) return 1;
  return 0;
}

/** The quality shown: worse at once, better after two windows that say so. */
export class QualityTracker {
  quality: Quality = 0;
  private better = 0;

  /** A window's grade (or the connection's own word, e.g. 2 while it is lost); true if it changed. */
  push(next: Quality): boolean {
    if (next > this.quality) {
      this.quality = next;
      this.better = 0;
      return true;
    }
    if (next === this.quality) {
      this.better = 0;
      return false;
    }
    this.better += 1;
    if (this.better < 2) return false;
    this.better = 0;
    this.quality = Math.max(next, this.quality - 1) as Quality;
    return true;
  }

  reset(): void {
    this.quality = 0;
    this.better = 0;
  }
}
