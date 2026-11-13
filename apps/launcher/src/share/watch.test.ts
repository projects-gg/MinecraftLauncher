import { describe, expect, it } from "vitest";
import type { WatchReport } from "./types";
import { createWatchEngine, DROP_WAIT_MS, ICE_WAIT_MS, ufragOf, type WatchHost } from "./watch";

class FakeTrack {
  constructor(readonly kind: string) {}
}

class FakeStream {
  tracks: FakeTrack[] = [];
  getTracks() {
    return this.tracks;
  }
  addTrack(track: FakeTrack) {
    this.tracks.push(track);
  }
  removeTrack(track: FakeTrack) {
    this.tracks = this.tracks.filter((t) => t !== track);
  }
}

class FakePeer {
  connectionState: RTCPeerConnectionState = "new";
  signalingState: RTCSignalingState = "stable";
  localDescription: { sdp: string } | null = null;
  remote: string | null = null;
  closed = false;
  ontrack: ((event: unknown) => void) | null = null;
  onconnectionstatechange: (() => void) | null = null;
  constructor(readonly config: RTCConfiguration) {}
  async setRemoteDescription(description: { sdp: string }) {
    this.remote = description.sdp;
  }
  async createAnswer() {
    return { type: "answer", sdp: `answer to ${this.remote}` };
  }
  async setLocalDescription(answer: { sdp: string }) {
    this.localDescription = answer;
  }
  stats = new Map<string, Record<string, unknown>>();
  async getStats() {
    return this.stats;
  }
  close() {
    this.closed = true;
    this.signalingState = "closed";
  }
  state(next: RTCPeerConnectionState) {
    this.connectionState = next;
    this.onconnectionstatechange?.();
  }
  track(mid: string, kind: string) {
    this.ontrack?.({ transceiver: { mid }, track: new FakeTrack(kind) });
  }
}

function rig() {
  const peers: FakePeer[] = [];
  const timers = new Map<number, { at: number; work: () => void }>();
  const intervals = new Map<number, () => void>();
  let now = 0;
  let next = 1;
  const host: WatchHost = {
    createPeer: (config) => {
      const peer = new FakePeer(config);
      peers.push(peer);
      return peer as unknown as RTCPeerConnection;
    },
    setTimeout: (work, ms) => {
      const id = next++;
      timers.set(id, { at: now + ms, work });
      return id;
    },
    clearTimeout: (id) => void timers.delete(id),
    setInterval: (work) => {
      const id = next++;
      intervals.set(id, work);
      return id;
    },
    clearInterval: (id) => void intervals.delete(id),
    createStream: () => new FakeStream() as unknown as MediaStream,
  };
  const reports: WatchReport[] = [];
  const engine = createWatchEngine((r) => reports.push(r), host);
  const advance = (ms: number) => {
    now += ms;
    for (const [id, timer] of [...timers]) {
      if (timer.at <= now) {
        timers.delete(id);
        timer.work();
      }
    }
  };
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
  /** The quality look's interval fires once. */
  const look = () => {
    for (const work of intervals.values()) work();
  };
  return { engine, peers, reports, advance, settle, look };
}

const offer = (ufrag: string) => `v=0\r\na=ice-ufrag:${ufrag}\r\n`;
const mids = [
  { mid: "0", person: "301", n: 1, video: true },
  { mid: "1", person: "301", n: 1, video: false },
];

describe("the watch engine", () => {
  it("answers the room's offer on one connection while its session lasts", async () => {
    const { engine, peers, reports, settle } = rig();
    engine.handle({ t: "offer", sdp: offer("abc"), mids, fresh: false });
    await settle();
    expect(peers).toHaveLength(1);
    expect(reports).toEqual([{ t: "answer", sdp: `answer to ${offer("abc")}` }]);
    // The session grew: the same connection.
    engine.handle({ t: "offer", sdp: offer("abc"), mids, fresh: false });
    await settle();
    expect(peers).toHaveLength(1);
    // A new session: a new connection, the old one closed (the room says so; a new ufrag too).
    engine.handle({ t: "offer", sdp: offer("xyz"), mids, fresh: false });
    await settle();
    expect(peers).toHaveLength(2);
    expect(peers[0]?.closed).toBe(true);
    engine.handle({ t: "offer", sdp: offer("xyz"), mids, fresh: true });
    await settle();
    expect(peers).toHaveLength(3);
    expect(ufragOf(offer("xyz"))).toBe("xyz");
  });

  it("puts each share's tracks in its stream and drops the ones that leave", async () => {
    const { engine, peers, settle } = rig();
    let changes = 0;
    engine.subscribe(() => changes++);
    engine.handle({ t: "offer", sdp: offer("abc"), mids, fresh: false });
    await settle();
    peers[0]?.track("0", "video");
    peers[0]?.track("1", "audio");
    peers[0]?.track("9", "video");
    const stream = engine.stream("301", 1);
    expect(stream?.getTracks().map((t) => t.kind)).toEqual(["video", "audio"]);
    expect(changes).toBe(2);
    // A later offer of the same session keeps them; the page lets go of what is not watched.
    engine.handle({ t: "offer", sdp: offer("abc"), mids: [], fresh: false });
    await settle();
    expect(engine.stream("301", 1)).not.toBeNull();
    engine.retain([]);
    expect(engine.stream("301", 1)).toBeNull();
  });

  it("asks for TURN once when ICE does not connect, then gives up", async () => {
    const { engine, reports, advance, settle, peers } = rig();
    engine.handle({ t: "offer", sdp: offer("abc"), mids, fresh: false });
    await settle();
    advance(ICE_WAIT_MS);
    expect(reports.at(-1)).toEqual({ t: "needTurn" });
    // TURN came: this connection ends, the launcher asks anew; the next one goes through TURN.
    engine.handle({ t: "ice", ice: { urls: ["turn:x"], username: "u", credential: "c" } });
    await settle();
    expect(reports.at(-1)).toEqual({ t: "lost" });
    engine.handle({ t: "offer", sdp: offer("def"), mids, fresh: false });
    await settle();
    expect(peers[1]?.config.iceServers).toEqual([
      { urls: ["turn:x"], username: "u", credential: "c" },
    ]);
    advance(ICE_WAIT_MS);
    expect(reports.at(-1)).toEqual({ t: "lost" });
  });

  it("says connected once and lost after a drop that lasts", async () => {
    const { engine, reports, advance, settle, peers } = rig();
    engine.handle({ t: "offer", sdp: offer("abc"), mids, fresh: false });
    await settle();
    peers[0]?.state("connected");
    peers[0]?.state("connected");
    expect(reports.filter((r) => r.t === "connected")).toHaveLength(1);
    // A short drop costs nothing.
    peers[0]?.state("disconnected");
    peers[0]?.state("connected");
    advance(DROP_WAIT_MS);
    expect(reports.at(-1)).toEqual({ t: "connected" });
    peers[0]?.state("disconnected");
    advance(DROP_WAIT_MS);
    expect(reports.at(-1)).toEqual({ t: "lost" });
    expect(peers[0]?.closed).toBe(true);
  });

  it("keeps what arrives of a share (Yayın Bilgileri) only while someone asks for it", async () => {
    const { engine, peers, settle, look } = rig();
    engine.handle({ t: "offer", sdp: offer("abc"), mids, fresh: false });
    await settle();
    peers[0]?.track("0", "video");
    engine.setShowing(true);
    const peer = peers[0];
    if (!peer) throw new Error("no peer");
    const video = {
      type: "inbound-rtp",
      kind: "video",
      mid: "0",
      frameWidth: 1920,
      frameHeight: 1080,
      framesPerSecond: 59.6,
      packetsLost: 0,
      packetsReceived: 100,
      freezeCount: 0,
    };
    peer.stats = new Map([["v", video]]);
    look();
    await settle();
    expect(engine.info("301", 1)).toBeNull();
    const release = engine.watchInfo();
    let changes = 0;
    engine.subscribe(() => changes++);
    look();
    await settle();
    expect(engine.info("301", 1)).toEqual({ w: 1920, h: 1080, fps: 60 });
    expect(changes).toBe(1);
    // The same figures: nothing new for the page to draw.
    look();
    await settle();
    expect(changes).toBe(1);
    release();
    expect(engine.info("301", 1)).toBeNull();
  });

  it("closes everything on reset, and a closed connection's late events change nothing", async () => {
    const { engine, reports, settle, peers } = rig();
    engine.handle({ t: "offer", sdp: offer("abc"), mids, fresh: false });
    await settle();
    peers[0]?.track("0", "video");
    engine.handle({ t: "reset" });
    await settle();
    expect(peers[0]?.closed).toBe(true);
    expect(engine.stream("301", 1)).toBeNull();
    const before = reports.length;
    peers[0]?.state("failed");
    expect(reports.length).toBe(before);
  });
});
