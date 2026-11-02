// The voice engine behind the launcher's commands (pl_voice::engine; design §6). Nothing runs until
// the launcher asks: the sound opens for a stay, the settings page or "Mikrofonu dene" and closes
// when none of them needs it (no context, microphone, timer or listener left; after a stay it waits
// 1.5 s, since a room move leaves and joins at once); the peer connection lives from a join to the
// leave. Opening and closing the sound and every negotiation run one at a time in the launcher's
// order; mute, deafen, volumes, settings, the level bar and the self-test apply at once.
//
// The SFU flow (§7.5) from here: `join` → a peer connection with the one track the capture worklet
// sends (silence without a microphone: plugging one in later needs no new join) → `offer`; the
// room's `answer`; its `offer`s name new receivers (mid → person) → `answer`; a receiver whose
// person left, or came back on a new receiver, is dead: it stops playing at once and the room
// closes such receivers in batches. TURN: no connection 5 s after the answer → `needTurn`, the
// launcher fetches credentials (`ice`) and the page joins again through them (none within 8 s:
// `lost`); lost 3 s → `lost`, the launcher sends a new `join` (or `sync` from its side when the
// connection held).
import type {
  EngineCommand,
  EngineIce,
  EnginePerson,
  EngineReport,
  EngineSettings,
  EngineSound,
  Playout,
} from "./boundary";
import { Call, type CallEvents, type CallLike, type CallOptions } from "./call";
import { AudioGraph, type Graph, type GraphEvents } from "./graph";
import { MidMap } from "./mids";
import { grade, type Quality, QualityTracker, type RawStats, windowOf } from "./quality";
import { SerialQueue } from "./queue";
import { realTimers, SpeakingReporter, type Timers } from "./speaking";

export interface Engine {
  handle(command: EngineCommand): void;
}

export interface EngineDeps {
  openGraph(settings: EngineSettings, events: GraphEvents): Promise<Graph>;
  openCall(send: MediaStreamTrack | null, options: CallOptions, events: CallEvents): CallLike;
  timers: Timers;
}

const realDeps: EngineDeps = {
  openGraph: (settings, events) => AudioGraph.open(settings, events),
  openCall: (send, options, events) => new Call(send, options, events),
  timers: realTimers,
};

/** Until the launcher's first `configure` (it always sends one first). */
export const DEFAULT_SETTINGS: EngineSettings = {
  inputDevice: null,
  outputDevice: null,
  noiseSuppression: true,
  echoCancellation: true,
  autoGain: true,
  sensitivity: null,
  micVolume: 100,
  pushToTalk: false,
  kbps: 24,
  sounds: true,
};

/** The quality check's pace in a call with others: rare while good, close while not. */
export const QUALITY_GOOD_MS = 10_000;
export const QUALITY_POOR_MS = 2_000;
/** After `needTurn`, no TURN credentials by then (refused, or none to give): the connection is lost. */
export const TURN_WAIT_MS = 8_000;
/** After a stay the sound waits this long for the next one before it closes. */
export const RELEASE_GRACE_MS = 1_500;
/**
 * Descriptions grow with every receiver the SFU closed without a renegotiation; past this (the
 * room takes 15,000 characters) the connection starts over with only the people in the room.
 */
export const SDP_MAX_CHARS = 14_000;

export function createEngine(
  report: (value: EngineReport) => void,
  deps: EngineDeps = realDeps,
): Engine {
  return new VoiceEngine(report, deps);
}

class VoiceEngine implements Engine {
  private settings: EngineSettings = DEFAULT_SETTINGS;
  private stay = false;
  private setup = false;
  private testing = false;
  private deaf = false;
  private talkOpen = false;
  private people: EnginePerson[] = [];
  private ice: EngineIce | null = null;
  private graph: Graph | null = null;
  private opening: Promise<Graph | null> | null = null;
  private call: CallLike | null = null;
  /** Bumped by every join, TURN rejoin and leave: older steps stop where they are. */
  private generation = 0;
  private readonly queue = new SerialQueue();
  private readonly mids = new MidMap();
  /** The current call's receivers. */
  private readonly receivers = new Map<string, MediaStreamTrack>();
  /** Receivers whose sound is speech now. */
  private readonly lit = new Set<string>();
  private me = false;
  private readonly speaking: SpeakingReporter;
  private readonly quality = new QualityTracker();
  private qualityTimer: (() => void) | null = null;
  private qualityBefore: RawStats | null = null;
  private statsBefore: RawStats | null = null;
  /** The playout path last told (once per sound, and on change). */
  private toldPlayout: Playout | null = null;
  private turnWait: (() => void) | null = null;
  private releaseTimer: (() => void) | null = null;

  constructor(
    private readonly report: (value: EngineReport) => void,
    private readonly deps: EngineDeps,
  ) {
    this.speaking = new SpeakingReporter(
      (ids, me) => this.report({ t: "speaking", ids, me }),
      deps.timers,
    );
  }

  handle(command: EngineCommand): void {
    switch (command.t) {
      case "configure":
        this.configure(command.settings);
        break;
      case "join":
        this.join();
        break;
      case "answer":
        this.step("answer", (call) => call.applyAnswer(command.sdp));
        break;
      case "offer":
        this.offer(command.sdp, command.mids);
        break;
      case "talk":
        // `tail` only when push-to-talk was let go; a mute, a deafen or a moderator cuts at once.
        this.talkOpen = command.open;
        this.graph?.talk(command.open, command.tail);
        break;
      case "deaf":
        this.deaf = command.on;
        this.graph?.deaf(command.on);
        if (command.on) this.graph?.sound("stop");
        break;
      case "people":
        this.people = command.people;
        this.replay();
        this.watchQuality();
        break;
      case "ice":
        this.ice = command.ice;
        if (this.stay) this.connect();
        break;
      case "sound":
        this.sound(command.sound);
        break;
      case "stats":
        void this.stats();
        break;
      case "setup":
        this.setup = command.on;
        this.refresh();
        break;
      case "selfTest":
        this.testing = command.on;
        this.refresh();
        break;
      case "leave":
        this.leave();
        break;
    }
  }

  // -----------------------------------------------------------------------------------------
  // The sound

  /** At once: the sound queues its own microphone work, so nothing waits behind a negotiation. */
  private configure(settings: EngineSettings): void {
    this.settings = settings;
    this.call?.setKbps(settings.kbps);
    void this.graph?.configure(settings).catch(() => undefined);
  }

  /** Opens the sound once (shared by whoever asks meanwhile); null if it cannot open. */
  private ensureGraph(): Promise<Graph | null> {
    this.cancelRelease();
    if (this.graph) return Promise.resolve(this.graph);
    this.opening ??= this.deps
      .openGraph(this.settings, this.graphEvents())
      .then(
        (graph) => {
          this.graph = graph;
          // Settings that changed while it opened; the test before the way out opens.
          void graph.configure(this.settings).catch(() => undefined);
          graph.levels(this.setup || this.testing);
          graph.test(this.testing);
          graph.deaf(this.deaf);
          graph.talk(this.talkOpen);
          for (const [mid, track] of this.receivers) graph.attach(mid, track);
          this.replay();
          return graph;
        },
        () => {
          this.report({ t: "failed", code: "audio" });
          this.report({ t: "mic", mic: "missing", changed: false });
          return null;
        },
      )
      .finally(() => {
        this.opening = null;
      });
    return this.opening;
  }

  private graphEvents(): GraphEvents {
    return {
      me: (on) => {
        this.me = on;
        this.pushSpeaking();
      },
      level: (level, open) => {
        if (this.setup || this.testing) this.report({ t: "level", level, open });
      },
      speaking: (mid, on) => {
        if (on) this.lit.add(mid);
        else this.lit.delete(mid);
        this.pushSpeaking();
      },
      mic: (mic, changed) => this.report({ t: "mic", mic, changed }),
      devices: (devices) => this.report({ t: "devices", devices }),
      playout: (playout) => {
        if (playout === this.toldPlayout) return;
        this.toldPlayout = playout;
        this.report({ t: "playout", playout });
      },
      failed: (code) => this.report({ t: "failed", code }),
    };
  }

  /** The settings page or the self-test changed: adjust at once, open or close in turn. */
  private refresh(): void {
    // A test never waits behind a negotiation: nothing may go out while it runs.
    this.graph?.levels(this.setup || this.testing);
    this.graph?.test(this.testing);
    void this.queue.run(async () => {
      if (this.stay || this.setup || this.testing) {
        const graph = await this.ensureGraph();
        graph?.levels(this.setup || this.testing);
        graph?.test(this.testing);
      } else {
        this.release();
      }
    });
  }

  /** Nothing needs the sound: close it (no context, microphone or listener stays). */
  private release(): void {
    this.cancelRelease();
    if (this.stay || this.setup || this.testing || !this.graph) return;
    this.graph.close();
    this.graph = null;
    this.lit.clear();
    this.me = false;
    this.toldPlayout = null;
  }

  /** After a stay: closes the sound unless something needs it within the grace. */
  private releaseSoon(): void {
    this.cancelRelease();
    if (this.setup || this.testing || !this.graph) return;
    this.releaseTimer = this.deps.timers.later(() => {
      this.releaseTimer = null;
      void this.queue.run(async () => this.release());
    }, RELEASE_GRACE_MS);
  }

  private cancelRelease(): void {
    this.releaseTimer?.();
    this.releaseTimer = null;
  }

  private sound(name: EngineSound): void {
    // Deafened, nothing plays but the shortcut's own tick.
    if (this.deaf && name !== "tick" && name !== "stop") return;
    this.graph?.sound(name);
  }

  // -----------------------------------------------------------------------------------------
  // The call

  private join(): void {
    this.stay = true;
    this.connect();
  }

  /** A new peer connection (a join, a lost one, or TURN): offered to the launcher. */
  private connect(): void {
    const generation = ++this.generation;
    this.cancelTurnWait();
    this.cancelRelease();
    void this.queue.run(async () => {
      if (generation !== this.generation || !this.stay) return;
      const graph = await this.ensureGraph();
      if (generation !== this.generation || !this.stay) return;
      this.dropCall();
      try {
        const call = this.deps.openCall(
          graph?.sendTrack ?? null,
          { ice: this.ice, kbps: this.settings.kbps, timers: this.deps.timers },
          this.callEvents(generation),
        );
        this.call = call;
        const { sdp, mid } = await call.offer();
        if (generation !== this.generation) return;
        this.report({ t: "offer", sdp, mid });
      } catch {
        if (generation !== this.generation) return;
        this.report({ t: "failed", code: "offer" });
        this.dropCall();
        this.report({ t: "lost" });
      }
    });
  }

  private callEvents(generation: number): CallEvents {
    const current = () => generation === this.generation;
    return {
      connected: () => {
        if (!current()) return;
        this.report({ t: "connected" });
        this.qualityBefore = null;
        if (this.quality.quality !== 0) {
          this.quality.reset();
          this.report({ t: "quality", q: 0 });
        }
        this.watchQuality();
      },
      interrupted: () => {
        if (current()) this.setQuality(2);
      },
      resumed: () => {
        if (!current()) return;
        this.quality.reset();
        this.report({ t: "quality", q: 0 });
        this.watchQuality();
      },
      lost: () => {
        if (!current()) return;
        this.setQuality(2);
        this.dropCall();
        this.report({ t: "lost" });
      },
      needTurn: () => {
        if (!current()) return;
        this.dropCall();
        this.report({ t: "needTurn" });
        // A refused TURN request gets no `ice`: then the connection is lost and the launcher
        // joins anew (or gives up), never a stay without a connection.
        this.turnWait = this.deps.timers.later(() => {
          this.turnWait = null;
          if (current() && this.stay && !this.call) this.report({ t: "lost" });
        }, TURN_WAIT_MS);
      },
      track: (mid, track) => {
        if (!current()) return;
        this.receivers.set(mid, track);
        this.graph?.attach(mid, track);
        this.replay();
      },
      ended: (mid) => {
        if (!current()) return;
        this.receivers.delete(mid);
        this.lit.delete(mid);
        this.graph?.detach(mid);
        this.pushSpeaking();
      },
    };
  }

  private cancelTurnWait(): void {
    this.turnWait?.();
    this.turnWait = null;
  }

  /** A negotiation step on the current call, in order; a failure loses the call. */
  private step(code: string, run: (call: CallLike) => Promise<unknown>): void {
    const generation = this.generation;
    void this.queue.run(async () => {
      const call = this.call;
      if (!call || generation !== this.generation) return;
      try {
        await run(call);
      } catch {
        if (generation !== this.generation || call !== this.call) return;
        this.report({ t: "failed", code });
        this.dropCall();
        this.report({ t: "lost" });
      }
    });
  }

  /**
   * The SFU's offer for the current connection. One that comes while a join waits for its answer
   * belongs to the session the join replaces: it is left unanswered (the room forgets it).
   */
  private offer(sdp: string, mids: { [mid in string]: string }): void {
    if (!this.stay) return;
    this.step("remote_offer", async (call) => {
      if (!call.stable) return;
      if (sdp.length > SDP_MAX_CHARS) {
        this.restart(call);
        return;
      }
      // Who each new receiver plays is known before its track arrives.
      this.mids.assign(mids);
      const answer = await call.applyOffer(sdp);
      if (answer === null || call !== this.call) return;
      if (answer.length > SDP_MAX_CHARS) {
        this.restart(call);
        return;
      }
      this.report({ t: "answer", sdp: answer });
      this.replay();
    });
  }

  /** The descriptions outgrew the room's limit: a new connection carries only the living. */
  private restart(call: CallLike): void {
    if (call !== this.call) return;
    this.report({ t: "failed", code: "sdp_size" });
    this.dropCall();
    this.report({ t: "lost" });
  }

  private leave(): void {
    this.stay = false;
    this.generation += 1;
    this.ice = null;
    this.people = [];
    this.talkOpen = false;
    this.cancelTurnWait();
    this.graph?.talk(false, false);
    void this.queue.run(async () => {
      this.dropCall();
      this.speaking.clear();
      this.quality.reset();
      this.graph?.sound("stop");
      // A room move leaves and joins at once: the sound waits a moment for the next stay.
      if (!this.stay) this.releaseSoon();
    });
  }

  /** Closes the call and forgets its receivers (they stop playing at once). */
  private dropCall(): void {
    this.qualityTimer?.();
    this.qualityTimer = null;
    this.qualityBefore = null;
    this.statsBefore = null;
    const call = this.call;
    this.call = null;
    call?.close();
    for (const mid of this.receivers.keys()) this.graph?.detach(mid);
    this.receivers.clear();
    this.lit.clear();
    this.mids.clear();
    this.pushSpeaking();
  }

  // -----------------------------------------------------------------------------------------
  // People

  /** Whom a receiver plays now: its person while in the room, not silenced, and its newest. */
  private personOf(mid: string): EnginePerson | null {
    const id = this.mids.personOf(mid);
    if (id === undefined || !this.mids.current(mid)) return null;
    const person = this.people.find((p) => p.id === id);
    return person && !person.silent ? person : null;
  }

  /** Every receiver as the people say: played at its volume, or released (dead or silent). */
  private replay(): void {
    const graph = this.graph;
    if (graph) {
      for (const mid of this.receivers.keys()) {
        const person = this.personOf(mid);
        graph.play(mid, person ? Math.min(200, Math.max(0, person.volume)) : null);
      }
    }
    this.pushSpeaking();
  }

  private pushSpeaking(): void {
    if (!this.stay) return;
    const ids = new Set<string>();
    for (const mid of this.lit) {
      const person = this.personOf(mid);
      if (person && this.receivers.has(mid)) ids.add(person.id);
    }
    this.speaking.set(ids, this.me);
  }

  // -----------------------------------------------------------------------------------------
  // Quality

  private setQuality(q: Quality): void {
    if (this.quality.push(q)) this.report({ t: "quality", q: this.quality.quality });
  }

  /** In a connected call with someone else: one `getStats` now and then (often while poor). */
  private watchQuality(): void {
    if (this.qualityTimer) return;
    if (!this.stay || !this.call?.connected || this.people.length === 0) return;
    const wait = this.quality.quality === 0 ? QUALITY_GOOD_MS : QUALITY_POOR_MS;
    this.qualityTimer = this.deps.timers.later(() => {
      this.qualityTimer = null;
      void this.sampleQuality();
    }, wait);
  }

  private async sampleQuality(): Promise<void> {
    const call = this.call;
    if (!call) return;
    try {
      const now = await call.stats();
      if (call !== this.call) return;
      const window = windowOf(this.qualityBefore, now);
      this.qualityBefore = now;
      this.setQuality(grade(window));
    } catch {
      // The call closed meanwhile.
    }
    this.watchQuality();
  }

  /** The quality mark's numbers, once (the launcher asks every 2 s while it shows). */
  private async stats(): Promise<void> {
    const call = this.call;
    if (!call) {
      this.report({ t: "stats", rttMs: null, loss: null, jitterMs: null });
      return;
    }
    try {
      const now = await call.stats();
      const window = windowOf(this.statsBefore, now);
      this.statsBefore = now;
      this.report({
        t: "stats",
        rttMs: window.rttMs === null ? null : Math.round(window.rttMs),
        loss: window.loss,
        jitterMs: window.jitterMs === null ? null : Math.round(window.jitterMs),
      });
    } catch {
      this.report({ t: "stats", rttMs: null, loss: null, jitterMs: null });
    }
  }
}
