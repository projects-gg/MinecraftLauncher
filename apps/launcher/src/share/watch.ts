// The watch engine (SHARE; the lead's contract share.md §2, §6): one receive-only peer connection
// per watching page (the main window, or the "Yayın" window) with the room's watch session on the
// SFU. The launcher brokers its SDP: the room's offer comes as `offer` (a new connection when the
// session is new, the same one when it only grew or shrank), the answer goes back as `answer`.
// Chromium decodes in hardware where the GPU can; the picture goes to a `<video>`, the share's
// sound with it (its own volume, `stream.ts`).
//
// Nothing runs without a share on the page: no connection, no timer. Its only timers are the
// connection's own waits (ICE 5 s, a dropped connection 3 s) and, while a share shows, a quality
// look every 2 s (`getStats` on this computer, nothing goes to a server), for a quiet "Bağlantı
// zayıf" on its tile.
import { shareKey, type WatchCmd, type WatchMid, type WatchReport } from "./types";

/** ICE that has not connected by then asks for TURN (the voice page's rule). */
export const ICE_WAIT_MS = 5000;
/** A connection that dropped and did not come back by then is lost. */
export const DROP_WAIT_MS = 3000;
/** How often a showing share's quality is looked at. */
export const STATS_MS = 2000;

export type ShareQuality = "good" | "weak";

export interface WatchEngine {
  handle(cmd: WatchCmd): void;
  /** The picture and sound of a share on this page, when it came. */
  stream(person: string, n: number): MediaStream | null;
  quality(person: string, n: number): ShareQuality;
  /** Called on every change of the streams or their quality. */
  subscribe(listener: () => void): () => void;
  /** Only these shares (their `shareKey`) are watched now: the others' streams go. */
  retain(keys: readonly string[]): void;
  /** Quality looks run only while the page shows a share. */
  setShowing(showing: boolean): void;
  dispose(): void;
}

/** What the engine needs of the browser (tests stand in). */
export interface WatchHost {
  createPeer(config: RTCConfiguration): RTCPeerConnection;
  setTimeout(work: () => void, ms: number): number;
  clearTimeout(id: number): void;
  setInterval(work: () => void, ms: number): number;
  clearInterval(id: number): void;
  createStream(): MediaStream;
}

const browserHost: WatchHost = {
  createPeer: (config) => new RTCPeerConnection(config),
  setTimeout: (work, ms) => window.setTimeout(work, ms),
  clearTimeout: (id) => window.clearTimeout(id),
  setInterval: (work, ms) => window.setInterval(work, ms),
  clearInterval: (id) => window.clearInterval(id),
  createStream: () => new MediaStream(),
};

/** The ICE ufrag an SDP carries (a new watch session has a new one). */
export function ufragOf(sdp: string): string | null {
  return /^a=ice-ufrag:(\S+)/m.exec(sdp)?.[1] ?? null;
}

/** A loss above this share of packets reads as weak (as a frozen picture does). */
const LOSS_WEAK = 0.05;

export function createWatchEngine(
  report: (report: WatchReport) => void,
  host: WatchHost = browserHost,
): WatchEngine {
  let pc: RTCPeerConnection | null = null;
  let ufrag: string | null = null;
  let iceServers: RTCIceServer[] = [];
  let mids = new Map<string, WatchMid>();
  const streams = new Map<string, MediaStream>();
  const qualities = new Map<string, ShareQuality>();
  const listeners = new Set<() => void>();
  let iceTimer: number | null = null;
  let dropTimer: number | null = null;
  let statsTimer: number | null = null;
  let showing = false;
  let connectedOnce = false;
  let askedTurn = false;
  /** Offers are taken one after another (the room may send two quickly). */
  let queue: Promise<void> = Promise.resolve();
  /** Each connection's own number: a late event of a closed one changes nothing. */
  let generation = 0;
  const lastStats = new Map<string, { lost: number; received: number; freezes: number }>();

  const notify = () => {
    for (const listener of listeners) listener();
  };

  const clearTimers = () => {
    if (iceTimer !== null) host.clearTimeout(iceTimer);
    if (dropTimer !== null) host.clearTimeout(dropTimer);
    iceTimer = null;
    dropTimer = null;
  };

  const syncStats = () => {
    const wanted = showing && pc !== null && streams.size > 0;
    if (wanted && statsTimer === null) {
      statsTimer = host.setInterval(() => void look(), STATS_MS);
    } else if (!wanted && statsTimer !== null) {
      host.clearInterval(statsTimer);
      statsTimer = null;
    }
  };

  const close = () => {
    generation++;
    clearTimers();
    if (pc) {
      pc.ontrack = null;
      pc.onconnectionstatechange = null;
      pc.close();
    }
    pc = null;
    ufrag = null;
    connectedOnce = false;
    askedTurn = false;
    mids = new Map();
    const had = streams.size > 0;
    streams.clear();
    qualities.clear();
    lastStats.clear();
    syncStats();
    if (had) notify();
  };

  const lost = () => {
    close();
    report({ t: "lost" });
  };

  const open = (): RTCPeerConnection => {
    close();
    const mine = generation;
    const peer = host.createPeer({ iceServers, bundlePolicy: "max-bundle" });
    peer.ontrack = (event) => {
      if (mine !== generation) return;
      const mid = event.transceiver.mid;
      const entry = mid ? mids.get(mid) : undefined;
      if (!entry) return;
      const key = shareKey(entry.person, entry.n);
      let stream = streams.get(key);
      if (!stream) {
        stream = host.createStream();
        streams.set(key, stream);
      }
      for (const old of stream.getTracks().filter((t) => t.kind === event.track.kind)) {
        stream.removeTrack(old);
      }
      stream.addTrack(event.track);
      syncStats();
      notify();
    };
    peer.onconnectionstatechange = () => {
      if (mine !== generation) return;
      switch (peer.connectionState) {
        case "connected":
          clearTimers();
          if (!connectedOnce) {
            connectedOnce = true;
            report({ t: "connected" });
          }
          break;
        case "disconnected":
          if (dropTimer === null) {
            dropTimer = host.setTimeout(() => {
              dropTimer = null;
              if (mine === generation && peer.connectionState !== "connected") lost();
            }, DROP_WAIT_MS);
          }
          break;
        case "failed":
          lost();
          break;
        default:
          break;
      }
    };
    pc = peer;
    // Not connected in 5 s: TURN once, then lost.
    iceTimer = host.setTimeout(() => {
      iceTimer = null;
      if (mine !== generation || peer.connectionState === "connected") return;
      if (!askedTurn && iceServers.length === 0) {
        askedTurn = true;
        report({ t: "needTurn" });
      } else {
        lost();
      }
    }, ICE_WAIT_MS);
    return peer;
  };

  const take = async (sdp: string, list: WatchMid[], fresh: boolean) => {
    const offered = ufragOf(sdp);
    const same = !fresh && pc !== null && pc.signalingState !== "closed" && offered === ufrag;
    const peer = same && pc ? pc : open();
    ufrag = offered;
    const mine = generation;
    // An offer names the m-lines it adds; the session's earlier ones stay.
    for (const m of list) mids.set(m.mid, m);
    try {
      await peer.setRemoteDescription({ type: "offer", sdp });
      const answer = await peer.createAnswer();
      await peer.setLocalDescription(answer);
      if (mine !== generation) return;
      report({ t: "answer", sdp: peer.localDescription?.sdp ?? answer.sdp ?? "" });
    } catch {
      if (mine === generation) lost();
      return;
    }
    syncStats();
  };

  const look = async () => {
    const peer = pc;
    if (!peer) return;
    const mine = generation;
    let stats: RTCStatsReport;
    try {
      stats = await peer.getStats();
    } catch {
      return;
    }
    if (mine !== generation) return;
    let changed = false;
    stats.forEach((entry: RTCStats & Record<string, unknown>) => {
      if (entry.type !== "inbound-rtp" || entry.kind !== "video") return;
      const mid = typeof entry.mid === "string" ? entry.mid : null;
      const share = mid ? mids.get(mid) : undefined;
      if (!share) return;
      const key = shareKey(share.person, share.n);
      const num = (value: unknown) => (typeof value === "number" ? value : 0);
      const lostNow = num(entry.packetsLost);
      const receivedNow = num(entry.packetsReceived);
      const freezesNow = num(entry.freezeCount);
      const before = lastStats.get(key);
      lastStats.set(key, { lost: lostNow, received: receivedNow, freezes: freezesNow });
      const lostDelta = before ? Math.max(0, lostNow - before.lost) : 0;
      const receivedDelta = before ? Math.max(0, receivedNow - before.received) : 0;
      const froze = before ? freezesNow > before.freezes : false;
      const loss = lostDelta + receivedDelta > 0 ? lostDelta / (lostDelta + receivedDelta) : 0;
      // Few frames are no sign (a still window of text sends few); a picture that froze or lost
      // packets is.
      const weak = froze || loss > LOSS_WEAK;
      const quality: ShareQuality = weak ? "weak" : "good";
      if (qualities.get(key) !== quality) {
        qualities.set(key, quality);
        changed = true;
      }
    });
    if (changed) notify();
  };

  return {
    handle(cmd) {
      switch (cmd.t) {
        case "offer":
          queue = queue.then(() => take(cmd.sdp, cmd.mids, cmd.fresh));
          return;
        case "ice":
          iceServers = [
            { urls: cmd.ice.urls, username: cmd.ice.username, credential: cmd.ice.credential },
          ];
          // The next connection goes through TURN: this one ends, the launcher asks anew.
          queue = queue.then(lost);
          return;
        case "reset":
          // Nothing watched: TURN credentials expire, the next session asks again if it must.
          iceServers = [];
          queue = queue.then(close);
          return;
      }
    },
    stream: (person, n) => streams.get(shareKey(person, n)) ?? null,
    quality: (person, n) => qualities.get(shareKey(person, n)) ?? "good",
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    retain(keys) {
      const kept = new Set(keys);
      let changed = false;
      for (const key of [...streams.keys()]) {
        if (!kept.has(key)) {
          streams.delete(key);
          qualities.delete(key);
          lastStats.delete(key);
          changed = true;
        }
      }
      syncStats();
      if (changed) notify();
    },
    setShowing(next) {
      showing = next;
      syncStats();
    },
    dispose() {
      close();
      listeners.clear();
    },
  };
}
