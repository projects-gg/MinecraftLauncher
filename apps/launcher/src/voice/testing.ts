// Fakes for the voice engine's tests (never imported by the page).
import type { EngineSettings, EngineSound } from "./boundary";
import type { CallEvents, CallLike, CallOptions } from "./call";
import type { Graph, GraphEvents } from "./graph";
import type { RawStats } from "./quality";
import type { Timers } from "./speaking";

/** Timers that run only when told. */
export class FakeTimers implements Timers {
  time = 0;
  private jobs: { at: number; run: () => void; live: boolean }[] = [];

  now(): number {
    return this.time;
  }

  later(run: () => void, ms: number): () => void {
    const job = { at: this.time + ms, run, live: true };
    this.jobs.push(job);
    return () => {
      job.live = false;
    };
  }

  /** Jobs still waiting. */
  get pending(): number {
    return this.jobs.filter((job) => job.live).length;
  }

  advance(ms: number): void {
    const end = this.time + ms;
    for (;;) {
      const due = this.jobs
        .filter((job) => job.live && job.at <= end)
        .sort((a, b) => a.at - b.at)[0];
      if (!due) break;
      due.live = false;
      this.time = due.at;
      due.run();
    }
    this.time = end;
    this.jobs = this.jobs.filter((job) => job.live);
  }
}

export const fakeTrack = (name: string) => ({ id: name, kind: "audio" }) as MediaStreamTrack;

export class FakeGraph implements Graph {
  readonly sendTrack = fakeTrack("send");
  readonly played = new Map<string, number | null>();
  readonly attached = new Map<string, MediaStreamTrack>();
  readonly sounds: EngineSound[] = [];
  readonly configured: EngineSettings[] = [];
  talkOpen = false;
  /** The last `talk`'s tail (false: close at once). */
  talkTail = true;
  deafOn = false;
  levelsOn = false;
  testOn = false;
  closed = false;

  constructor(readonly events: GraphEvents) {}

  async configure(settings: EngineSettings): Promise<void> {
    this.configured.push(settings);
  }
  talk(open: boolean, tail = true): void {
    this.talkOpen = open;
    this.talkTail = tail;
  }
  levels(on: boolean): void {
    this.levelsOn = on;
  }
  test(on: boolean): void {
    this.testOn = on;
  }
  deaf(on: boolean): void {
    this.deafOn = on;
  }
  attach(mid: string, track: MediaStreamTrack): void {
    this.attached.set(mid, track);
  }
  play(mid: string, volume: number | null): void {
    if (this.attached.has(mid)) this.played.set(mid, volume);
  }
  detach(mid: string): void {
    this.attached.delete(mid);
    this.played.delete(mid);
  }
  sound(name: EngineSound): void {
    this.sounds.push(name);
  }
  close(): void {
    this.closed = true;
  }
}

export class FakeCall implements CallLike {
  connected = false;
  stable = true;
  closed = false;
  kbps: number;
  readonly answers: string[] = [];
  readonly offers: string[] = [];
  /** Mids the next applied offer brings (their tracks fire during it, as Chromium's do). */
  nextTracks: string[] = [];
  /** The join offer waits for this (a busy queue). */
  static offerHold: Promise<void> = Promise.resolve();
  /** The answer to the next SFU offer (default: a short one). */
  answerText: ((sdp: string) => string) | null = null;
  stats_: RawStats = { rttMs: 40, upSent: 0, upLost: 0, upJitter: null, down: new Map() };
  statsCalls = 0;

  constructor(
    readonly send: MediaStreamTrack | null,
    readonly options: CallOptions,
    readonly events: CallEvents,
  ) {
    this.kbps = options.kbps;
  }

  async offer(): Promise<{ sdp: string | null; mid: string | null }> {
    await FakeCall.offerHold;
    if (!this.send) return { sdp: null, mid: null };
    this.stable = false;
    return { sdp: "join-offer", mid: "0" };
  }
  async applyAnswer(sdp: string): Promise<void> {
    if (sdp === "broken") throw new Error("bad answer");
    this.answers.push(sdp);
    this.stable = true;
  }
  async applyOffer(sdp: string): Promise<string | null> {
    if (!this.stable) return null;
    this.offers.push(sdp);
    for (const mid of this.nextTracks) this.events.track(mid, fakeTrack(`t${mid}`));
    this.nextTracks = [];
    return this.answerText ? this.answerText(sdp) : `answer-to-${sdp}`;
  }
  setKbps(kbps: number): void {
    this.kbps = kbps;
  }
  async stats(): Promise<RawStats> {
    this.statsCalls += 1;
    return this.stats_;
  }
  close(): void {
    this.closed = true;
  }
  /** The connection comes up. */
  up(): void {
    this.connected = true;
    this.events.connected();
  }
}
