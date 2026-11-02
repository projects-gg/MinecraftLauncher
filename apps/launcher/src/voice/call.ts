// The page's side of the SFU flow (§7.5): one peer connection per SFU session, bundled, with one
// send-only audio track; the room's answer to the join offer, then the SFU's offers for the people
// to receive (answered here). Connection watch: not connected 5 s after the first answer → ask for
// TURN (or, already with TURN, report it lost); connected, then gone for 3 s (or failed) → lost.
// The launcher answers both with a new join (a new connection here); this never restarts ICE
// itself, since the room takes no offer from the page but a join.
import type { EngineIce } from "./boundary";
import { type RawStats, readStats } from "./quality";
import { tuneOpus } from "./sdp";
import { realTimers, type Timers } from "./speaking";

export interface CallEvents {
  /** Audio flows (once per connection). */
  connected(): void;
  /** Connected before, gone now (brief: it may come back within 3 s). */
  interrupted(): void;
  /** Back after an interruption. */
  resumed(): void;
  /** Gone for good: the launcher joins anew. */
  lost(): void;
  /** Never connected: TURN may help. */
  needTurn(): void;
  /** A receiver's track (new, or a mid the SFU gave again). */
  track(mid: string, track: MediaStreamTrack): void;
  /** A receiver the SFU stopped. */
  ended(mid: string): void;
}

export interface CallOptions {
  /** TURN credentials (only after ICE failed). */
  ice: EngineIce | null;
  kbps: number;
  timers?: Timers;
}

/** What the engine needs of a call (tests give it a fake). */
export interface CallLike {
  readonly connected: boolean;
  /** No negotiation half done (an SFU offer may apply now). */
  readonly stable: boolean;
  /** The join offer: one send-only audio m-line (sdp null: nothing to send, listen only). */
  offer(): Promise<{ sdp: string | null; mid: string | null }>;
  /** The room's answer to the join offer. */
  applyAnswer(sdp: string): Promise<void>;
  /** The SFU's offer; returns the answer. null: not now (a join waits for its answer). */
  applyOffer(sdp: string): Promise<string | null>;
  setKbps(kbps: number): void;
  stats(): Promise<RawStats>;
  close(): void;
}

/** Cloudflare's free STUN (design §7.5). */
const STUN = "stun:stun.cloudflare.com:3478";
const CONNECT_MS = 5000;
const LOST_MS = 3000;

export class Call implements CallLike {
  private readonly pc: RTCPeerConnection;
  private readonly timers: Timers;
  private sender: RTCRtpSender | null = null;
  private kbps: number;
  private answered = false;
  private wasConnected = false;
  private interrupted = false;
  private done = false;
  private connectTimer: (() => void) | null = null;
  private lostTimer: (() => void) | null = null;

  constructor(
    send: MediaStreamTrack | null,
    private readonly options: CallOptions,
    private readonly events: CallEvents,
  ) {
    this.timers = options.timers ?? realTimers;
    this.kbps = options.kbps;
    const iceServers: RTCIceServer[] = [{ urls: STUN }];
    if (options.ice) {
      iceServers.push({
        urls: options.ice.urls,
        username: options.ice.username,
        credential: options.ice.credential,
      });
    }
    this.pc = new RTCPeerConnection({
      iceServers,
      bundlePolicy: "max-bundle",
      rtcpMuxPolicy: "require",
    });
    this.pc.addEventListener("connectionstatechange", () => this.onState());
    const watched = new WeakSet<MediaStreamTrack>();
    this.pc.addEventListener("track", (event) => {
      const mid = event.transceiver.mid;
      if (mid === null || this.done) return;
      const track = event.track;
      if (!watched.has(track)) {
        watched.add(track);
        track.addEventListener("ended", () => {
          if (!this.done) this.events.ended(mid);
        });
      }
      this.events.track(mid, track);
    });
    if (send) this.sender = this.pc.addTransceiver(send, { direction: "sendonly" }).sender;
  }

  get connected(): boolean {
    return this.pc.connectionState === "connected";
  }

  get stable(): boolean {
    return !this.done && this.pc.signalingState === "stable";
  }

  /** Whether TURN was in this connection's servers. */
  get withTurn(): boolean {
    return this.options.ice !== null;
  }

  async offer(): Promise<{ sdp: string | null; mid: string | null }> {
    if (!this.sender) return { sdp: null, mid: null };
    await this.pc.setLocalDescription(await this.pc.createOffer());
    const mid = this.pc.getTransceivers().find((t) => t.sender === this.sender)?.mid ?? null;
    return { sdp: this.pc.localDescription?.sdp ?? null, mid };
  }

  async applyAnswer(sdp: string): Promise<void> {
    if (this.done || this.pc.signalingState !== "have-local-offer") return;
    await this.pc.setRemoteDescription({ type: "answer", sdp: this.tune(sdp) });
    // The description's own cap holds even if the encoder's cannot be set: never a lost call.
    await this.capBitrate().catch(() => undefined);
    this.watch();
  }

  async applyOffer(sdp: string): Promise<string | null> {
    // A join waits for its answer; an offer now belongs to the session it replaced.
    if (this.done || this.pc.signalingState !== "stable") return null;
    await this.pc.setRemoteDescription({ type: "offer", sdp: this.tune(sdp) });
    await this.pc.setLocalDescription(await this.pc.createAnswer());
    this.watch();
    return this.pc.localDescription?.sdp ?? null;
  }

  setKbps(kbps: number): void {
    if (kbps === this.kbps) return;
    this.kbps = kbps;
    void this.capBitrate().catch(() => undefined);
  }

  async stats(): Promise<RawStats> {
    const report = await this.pc.getStats();
    return readStats(report.values() as Iterable<{ type?: string; kind?: string }>);
  }

  close(): void {
    if (this.done) return;
    this.done = true;
    this.connectTimer?.();
    this.lostTimer?.();
    this.connectTimer = null;
    this.lostTimer = null;
    this.pc.close();
  }

  private tune(sdp: string): string {
    return tuneOpus(sdp, { bitrate: this.kbps * 1000, dtx: true, fec: true });
  }

  private async capBitrate(): Promise<void> {
    const sender = this.sender;
    if (!sender || this.done) return;
    const parameters = sender.getParameters();
    if (parameters.encodings.length === 0) return;
    for (const encoding of parameters.encodings) encoding.maxBitrate = this.kbps * 1000;
    await sender.setParameters(parameters);
  }

  /** From the first answer on: connected within 5 s, or TURN (or lost). */
  private watch(): void {
    if (this.answered || this.done) return;
    this.answered = true;
    if (this.connected) {
      this.onState();
      return;
    }
    this.connectTimer = this.timers.later(() => {
      this.connectTimer = null;
      if (this.done || this.wasConnected) return;
      this.finish(this.withTurn ? "lost" : "needTurn");
    }, CONNECT_MS);
  }

  private onState(): void {
    if (this.done) return;
    const state = this.pc.connectionState;
    if (state === "connected") {
      this.connectTimer?.();
      this.connectTimer = null;
      this.lostTimer?.();
      this.lostTimer = null;
      if (!this.wasConnected) {
        this.wasConnected = true;
        this.events.connected();
      } else if (this.interrupted) {
        this.interrupted = false;
        this.events.resumed();
      }
      return;
    }
    if (state === "failed") {
      this.finish(this.wasConnected || this.withTurn ? "lost" : "needTurn");
      return;
    }
    if (state === "disconnected" && this.wasConnected && !this.lostTimer) {
      this.interrupted = true;
      this.events.interrupted();
      this.lostTimer = this.timers.later(() => {
        this.lostTimer = null;
        if (!this.done && !this.connected) this.finish("lost");
      }, LOST_MS);
    }
  }

  /** The connection is over: one word to the launcher, nothing after it. */
  private finish(why: "lost" | "needTurn"): void {
    if (this.done) return;
    this.close();
    if (why === "lost") this.events.lost();
    else this.events.needTurn();
  }
}
