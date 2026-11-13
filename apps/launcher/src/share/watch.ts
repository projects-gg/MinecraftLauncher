// The watch engine (SHARE; the lead's contract share.md §2, §6): one receive-only peer connection
// per watching page (the main window, or the "Yayın" window) with the room's watch session on the
// SFU. The launcher brokers its SDP: the room's offer comes as `offer` (a new connection when the
// session is new, the same one when it only grew or shrank), the answer goes back as `answer`.
// Chromium decodes in hardware where the GPU can; the picture goes to a `<video>`, the sound to
// WebAudio (share/stream-audio.ts: its own volume, ducking).
//
// Nothing runs without a share on the page: no connection, no timer. Its only timers are the
// connection's own waits (ICE 5 s, a dropped connection 3 s) and, while a share shows, a quality
// look every 2 s (`getStats` on this computer, nothing goes to a server), for a quiet "Düşük
// Kalite" on its tile and, while it shows, "Yayın Bilgileri".
import { shareKey, type WatchCmd, type WatchMid, type WatchReport } from "./types";

/** ICE that has not connected by then asks for TURN (the voice page's rule). */
export const ICE_WAIT_MS = 5000;
/** A connection that dropped and did not come back by then is lost. */
export const DROP_WAIT_MS = 3000;
/** How often a showing share's quality is looked at. */
export const STATS_MS = 2000;
/**
 * A picture that changed on purpose (another source, layer or frame rate, a pause or a hold
 * ending) stops a moment: a freeze this soon after a new size, a key frame the link did not ask
 * for or `calm` is no weak connection even when the link stumbles then. Lost packets beyond a few
 * percent always are.
 */
export const CHANGE_CALM_MS = 4000;

export type ShareQuality = "good" | "weak";

/** SQOL "Yayın Bilgileri": what arrives of a share now (its picture's size and frames a second). */
export interface ShareInfo {
  w: number;
  h: number;
  fps: number;
}

export interface WatchEngine {
  handle(cmd: WatchCmd): void;
  /** The picture and sound of a share on this page, when it came. */
  stream(person: string, n: number): MediaStream | null;
  quality(person: string, n: number): ShareQuality;
  /**
   * What arrives of a share, as the last quality look saw it (null: nothing seen yet). Kept only
   * while someone asks for it (`watchInfo`).
   */
  info(person: string, n: number): ShareInfo | null;
  /** "Yayın Bilgileri" shows: the quality looks keep `info` too. Returns its release. */
  watchInfo(): () => void;
  /**
   * The share changed on purpose (its layer was asked for, its sharer paused it or changed its
   * source): a freeze in the next few seconds says nothing of the connection.
   */
  calm(person: string, n: number): void;
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
  /** Milliseconds on a steady clock. */
  now(): number;
}

const browserHost: WatchHost = {
  createPeer: (config) => new RTCPeerConnection(config),
  setTimeout: (work, ms) => window.setTimeout(work, ms),
  clearTimeout: (id) => window.clearTimeout(id),
  setInterval: (work, ms) => window.setInterval(work, ms),
  clearInterval: (id) => window.clearInterval(id),
  createStream: () => new MediaStream(),
  now: () => performance.now(),
};

/** The ICE ufrag an SDP carries (a new watch session has a new one). */
export function ufragOf(sdp: string): string | null {
  return /^a=ice-ufrag:(\S+)/m.exec(sdp)?.[1] ?? null;
}

/** A loss above this share of packets reads as weak (as a picture frozen by a stumbling link does). */
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
  const infos = new Map<string, ShareInfo>();
  /** How many ask for `info` now. */
  let infoAsked = 0;
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
  /**
   * Each share's counters at the last look: key frames and size (did its picture change?), the
   * page's asks for lost packets and key frames (`asks`), and whether the link had trouble then.
   */
  const lastStats = new Map<
    string,
    {
      lost: number;
      received: number;
      freezes: number;
      keys: number;
      asks: number;
      w: number;
      h: number;
      trouble: boolean;
    }
  >();
  /** Until when a share's freezes say nothing (its picture changed on purpose). */
  const calmUntil = new Map<string, number>();

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
      // Looks that start again compare with nothing older: what froze while nobody looked (a
      // hidden window, a held watch) says nothing of the connection now.
      lastStats.clear();
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
    infos.clear();
    lastStats.clear();
    calmUntil.clear();
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
      const keysNow = num(entry.keyFramesDecoded);
      const asksNow = num(entry.nackCount) + num(entry.pliCount) + num(entry.firCount);
      const w = num(entry.frameWidth);
      const h = num(entry.frameHeight);
      const before = lastStats.get(key);
      const lostDelta = before ? Math.max(0, lostNow - before.lost) : 0;
      const receivedDelta = before ? Math.max(0, receivedNow - before.received) : 0;
      // The link's own trouble: packets lost, or this page asked for them again or for a key frame.
      const trouble = !!before && (lostDelta > 0 || asksNow > before.asks);
      lastStats.set(key, {
        lost: lostNow,
        received: receivedNow,
        freezes: freezesNow,
        keys: keysNow,
        asks: asksNow,
        w,
        h,
        trouble,
      });
      // A new size, or a key frame the link did not ask for: the picture changed on purpose
      // (another source, layer or frame rate, a pause ending, a new watcher).
      const now = host.now();
      const resized = !!before && w > 0 && before.w > 0 && (w !== before.w || h !== before.h);
      const keyed = !!before && keysNow > before.keys && !trouble;
      if (resized || keyed) calmUntil.set(key, now + CHANGE_CALM_MS);
      const calm = (calmUntil.get(key) ?? 0) > now;
      const froze = before ? freezesNow > before.freezes : false;
      const loss = lostDelta + receivedDelta > 0 ? lostDelta / (lostDelta + receivedDelta) : 0;
      // A freeze is weak only with the link's trouble (in this look or the one before): without
      // it the sharer simply sent fewer frames (a still window sends one a second, and Chromium
      // counts the gaps after motion as freezes for seconds; a switch, another frame rate, a
      // pause). Packets lost beyond a few percent are weak in any case.
      const frozeWeak = froze && !calm && (trouble || before?.trouble === true);
      const weak = frozeWeak || loss > LOSS_WEAK;
      const quality: ShareQuality = weak ? "weak" : "good";
      if (qualities.get(key) !== quality) {
        qualities.set(key, quality);
        changed = true;
      }
      // "Yayın Bilgileri", only while it shows: a new object only when a figure changed.
      if (infoAsked > 0) {
        const fps = Math.round(num(entry.framesPerSecond));
        const known = infos.get(key);
        if (w > 0 && h > 0 && (!known || known.w !== w || known.h !== h || known.fps !== fps)) {
          infos.set(key, { w, h, fps });
          changed = true;
        }
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
          // The next connection goes through TURN: this one ends, the launcher asks anew. One that
          // came up meanwhile stays (TURN waits for the next one).
          queue = queue.then(() => {
            if (pc?.connectionState !== "connected") lost();
          });
          return;
        case "reset":
          // A session asked anew keeps TURN (that is what it was asked for, MOB-WATCH). Nothing
          // watched, or TURN ran out: it goes, the next session asks again if it must.
          if (!cmd.keepTurn) iceServers = [];
          queue = queue.then(close);
          return;
      }
    },
    stream: (person, n) => streams.get(shareKey(person, n)) ?? null,
    quality: (person, n) => qualities.get(shareKey(person, n)) ?? "good",
    info: (person, n) => infos.get(shareKey(person, n)) ?? null,
    calm(person, n) {
      calmUntil.set(shareKey(person, n), host.now() + CHANGE_CALM_MS);
    },
    watchInfo() {
      infoAsked++;
      let released = false;
      return () => {
        if (released) return;
        released = true;
        infoAsked--;
        if (infoAsked === 0) infos.clear();
      };
    },
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
          infos.delete(key);
          lastStats.delete(key);
          calmUntil.delete(key);
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
