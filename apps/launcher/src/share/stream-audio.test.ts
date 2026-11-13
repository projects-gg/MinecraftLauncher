// A watched share's sound (SQOL-U1): its gain ("Yayın Sesi" × "Herkesin Sesi", the cap), the
// ducking ("Biri Konuşunca Yayın Sesini Kıs": down fast, back slowly), the output (the default when
// the chosen one is gone), and the engine: a context opened ahead of time that rests with nothing
// to play, each stream through its own gain, and the element's own sound where WebAudio is missing.
import { describe, expect, it } from "vitest";
import {
  DUCK_ATTACK_S,
  DUCK_RELEASE_S,
  duckGain,
  duckMove,
  StreamAudio,
  type StreamAudioHost,
  sinkFor,
  streamGain,
} from "./stream-audio";

describe("a stream's gain", () => {
  const env = { everyone: 100, deaf: false, volumeMax: 200 };

  it("is its volume times everyone's, up to the cap", () => {
    expect(streamGain({ volume: 100, muted: false }, env)).toBe(1);
    expect(streamGain({ volume: 160, muted: false }, env)).toBeCloseTo(1.6);
    expect(streamGain({ volume: 50, muted: false }, { ...env, everyone: 150 })).toBeCloseTo(0.75);
    // 200 % of 150 % is still at most 200 %.
    expect(streamGain({ volume: 200, muted: false }, { ...env, everyone: 150 })).toBe(2);
    // Where the cap is 100 % (no WebAudio), nothing goes over.
    expect(streamGain({ volume: 180, muted: false }, { ...env, volumeMax: 100 })).toBe(1);
  });

  it("is silent muted or deafened, and never below zero", () => {
    expect(streamGain({ volume: 120, muted: true }, env)).toBe(0);
    expect(streamGain({ volume: 120, muted: false }, { ...env, deaf: true })).toBe(0);
    expect(streamGain({ volume: -20, muted: false }, env)).toBe(0);
  });
});

describe("ducking", () => {
  const on = { duck: true, duckLevel: 60, duckSelf: false };

  it("lowers the streams while someone else speaks, by the chosen amount", () => {
    expect(duckGain(on, [], false, "100")).toBe(1);
    expect(duckGain(on, ["301"], false, "100")).toBeCloseTo(0.4);
    expect(duckGain({ ...on, duckLevel: 20 }, ["301"], false, "100")).toBeCloseTo(0.8);
    // The amount stays within %20–90.
    expect(duckGain({ ...on, duckLevel: 99 }, ["301"], false, "100")).toBeCloseTo(0.1);
    expect(duckGain({ ...on, duckLevel: 5 }, ["301"], false, "100")).toBeCloseTo(0.8);
  });

  it("never ducks while off, and for the player only with Ben Konuşunca da Kıs", () => {
    expect(duckGain({ ...on, duck: false }, ["301"], true, "100")).toBe(1);
    expect(duckGain(on, [], true, "100")).toBe(1);
    expect(duckGain(on, ["100"], false, "100")).toBe(1);
    expect(duckGain({ ...on, duckSelf: true }, [], true, "100")).toBeCloseTo(0.4);
  });

  it("goes down with the attack and comes back with the release", () => {
    expect(duckMove(1, 0.4)).toEqual({ to: 0.4, tau: DUCK_ATTACK_S / 3 });
    expect(duckMove(0.4, 1)).toEqual({ to: 1, tau: DUCK_RELEASE_S / 3 });
    expect(duckMove(0.4, 0.4)).toBeNull();
    // About 95 % of the way after three time constants: within the contract's 80 / 700 ms.
    expect(3 * (duckMove(1, 0.4)?.tau ?? 0)).toBeCloseTo(0.08);
    expect(3 * (duckMove(0.4, 1)?.tau ?? 0)).toBeCloseTo(0.7);
  });
});

describe("the output", () => {
  it("is the chosen one while the system has it, else the default", () => {
    expect(sinkFor(null, ["a"])).toBe("");
    expect(sinkFor("a", ["a", "b"])).toBe("a");
    expect(sinkFor("c", ["a", "b"])).toBe("");
    // Unknown list: try the chosen one (it falls back by itself if it fails).
    expect(sinkFor("c", null)).toBe("c");
  });
});

// --- the engine, on stand-ins ----------------------------------------------------------------------

class FakeParam {
  calls: Array<[string, ...number[]]> = [];
  constructor(public value: number) {}
  setTargetAtTime(value: number, at: number, tau: number) {
    this.calls.push(["target", value, at, tau]);
    this.value = value;
    return this;
  }
  cancelScheduledValues(at: number) {
    this.calls.push(["cancel", at]);
    return this;
  }
  setValueAtTime(value: number, at: number) {
    this.calls.push(["set", value, at]);
    return this;
  }
}

class FakeNode {
  out = new Set<FakeNode>();
  connect(node: FakeNode) {
    this.out.add(node);
    return node;
  }
  disconnect() {
    this.out.clear();
  }
}

class FakeGain extends FakeNode {
  gain = new FakeParam(1);
}

class FakeCompressor extends FakeNode {
  threshold = new FakeParam(0);
  knee = new FakeParam(0);
  ratio = new FakeParam(0);
  attack = new FakeParam(0);
  release = new FakeParam(0);
}

class FakeContext {
  state: AudioContextState = "running";
  currentTime = 0;
  destination = new FakeNode();
  gains: FakeGain[] = [];
  sinks: string[] = [];
  /** The browser lets it start (false: a page that never heard the player). */
  allowed = true;
  createGain() {
    const gain = new FakeGain();
    this.gains.push(gain);
    return gain;
  }
  createDynamicsCompressor() {
    return new FakeCompressor();
  }
  createMediaStreamSource() {
    return new FakeNode();
  }
  async suspend() {
    this.state = "suspended";
  }
  async resume() {
    if (this.allowed) this.state = "running";
  }
  async close() {
    this.state = "closed";
  }
  async setSinkId(id: string) {
    if (id === "gone") throw new Error("NotFoundError");
    this.sinks.push(id);
  }
}

class FakeElement {
  muted = false;
  volume = 1;
  srcObject: unknown = null;
  played = 0;
  paused = false;
  play() {
    this.played++;
    return Promise.resolve();
  }
  pause() {
    this.paused = true;
  }
}

class FakeTrack {
  kind = "audio";
  readyState = "live";
}

class FakeStream {
  constructor(readonly tracks: FakeTrack[]) {}
  getAudioTracks() {
    return this.tracks.filter((track) => track.kind === "audio");
  }
}

function rig(options: { webAudio?: boolean } = {}) {
  const contexts: FakeContext[] = [];
  const elements: FakeElement[] = [];
  const timers = new Map<number, () => void>();
  const inputs: Array<() => void> = [];
  let next = 1;
  const host: StreamAudioHost = {
    createContext: () => {
      if (options.webAudio === false) throw new Error("no WebAudio");
      const ctx = new FakeContext();
      contexts.push(ctx);
      return ctx as unknown as AudioContext;
    },
    createElement: () => {
      const element = new FakeElement();
      elements.push(element);
      return element as unknown as HTMLAudioElement;
    },
    createStream: (tracks) =>
      new FakeStream(tracks as unknown as FakeTrack[]) as unknown as MediaStream,
    setTimeout: (work) => {
      const id = next++;
      timers.set(id, work);
      return id;
    },
    clearTimeout: (id) => void timers.delete(id),
    onInput: (work) => {
      inputs.push(work);
      return () => {
        const at = inputs.indexOf(work);
        if (at >= 0) inputs.splice(at, 1);
      };
    },
  };
  const audio = new StreamAudio(host);
  const settle = () => new Promise((resolve) => setTimeout(resolve, 0));
  const runTimers = () => {
    for (const [id, work] of [...timers]) {
      timers.delete(id);
      work();
    }
  };
  const stream = () => new FakeStream([new FakeTrack()]) as unknown as MediaStream;
  return { audio, contexts, elements, inputs, settle, runTimers, stream };
}

describe("the stream sound engine", () => {
  it("opens ahead of time and rests until a stream comes", async () => {
    const { audio, contexts, runTimers, settle } = rig();
    audio.prepare();
    audio.prepare();
    expect(contexts).toHaveLength(1);
    runTimers();
    await settle();
    expect(contexts[0]?.state).toBe("suspended");
  });

  it("plays a stream through its own gain, muted on its element, and wakes the device", async () => {
    const { audio, contexts, elements, runTimers, settle, stream } = rig();
    audio.prepare();
    runTimers();
    await settle();
    audio.sync([{ key: "301:1", stream: stream(), level: 1.5 }], 1, "");
    await settle();
    const ctx = contexts[0];
    expect(ctx?.state).toBe("running");
    expect(elements[0]).toMatchObject({ muted: true, played: 1 });
    // The duck gain first, then the stream's: above 100 % through its own limiter.
    const gain = ctx?.gains[1];
    expect(gain?.gain.value).toBe(1.5);
    expect([...(gain?.out ?? [])][0]).toBeInstanceOf(FakeCompressor);
    // A new track (a new connection) is played anew; the old element lets go.
    audio.sync([{ key: "301:1", stream: stream(), level: 1 }], 1, "");
    expect(elements).toHaveLength(2);
    expect(elements[0]?.paused).toBe(true);
  });

  it("keeps the device awake for a stream that comes right after it opened", async () => {
    const { audio, contexts, runTimers, settle, stream } = rig();
    audio.prepare();
    audio.sync([{ key: "301:1", stream: stream(), level: 1 }], 1, "");
    runTimers();
    await settle();
    expect(contexts[0]?.state).toBe("running");
  });

  it("glides a level change and ducks down fast, back slowly", async () => {
    const { audio, contexts, settle, stream } = rig();
    audio.prepare();
    const one = stream();
    audio.sync([{ key: "301:1", stream: one, level: 1.5 }], 1, "");
    audio.sync([{ key: "301:1", stream: one, level: 0.5 }], 1, "");
    const [duck, gain] = contexts[0]?.gains ?? [];
    expect(gain?.gain.calls.at(-1)?.slice(0, 2)).toEqual(["target", 0.5]);
    expect([...(gain?.out ?? [])]).toEqual([duck]);
    audio.sync([{ key: "301:1", stream: one, level: 0.5 }], 0.4, "");
    expect(duck?.gain.calls.at(-1)).toEqual(["target", 0.4, 0, DUCK_ATTACK_S / 3]);
    audio.sync([{ key: "301:1", stream: one, level: 0.5 }], 1, "");
    expect(duck?.gain.calls.at(-1)).toEqual(["target", 1, 0, DUCK_RELEASE_S / 3]);
    await settle();
  });

  it("plays on the chosen output, and on the default when that one is gone", async () => {
    const { audio, contexts, settle, stream } = rig();
    audio.prepare();
    const one = stream();
    audio.sync([{ key: "301:1", stream: one, level: 1 }], 1, "out-1");
    await settle();
    expect(contexts[0]?.sinks).toEqual(["out-1"]);
    audio.sync([{ key: "301:1", stream: one, level: 1 }], 1, "gone");
    await settle();
    expect(contexts[0]?.sinks).toEqual(["out-1", ""]);
  });

  it("lets go of a stream no longer watched, then rests, and closes with the room", async () => {
    const { audio, contexts, elements, runTimers, settle, stream } = rig();
    audio.prepare();
    audio.sync([{ key: "301:1", stream: stream(), level: 1 }], 1, "");
    await settle();
    audio.sync([], 1, "");
    expect(elements[0]).toMatchObject({ paused: true, srcObject: null });
    runTimers();
    await settle();
    expect(contexts[0]?.state).toBe("suspended");
    audio.close();
    await settle();
    expect(contexts[0]?.state).toBe("closed");
  });

  it("wakes a context the browser kept suspended on the player's next press", async () => {
    const { audio, contexts, inputs, runTimers, settle, stream } = rig();
    audio.prepare();
    runTimers();
    await settle();
    const ctx = contexts[0];
    if (!ctx) throw new Error("no context");
    ctx.allowed = false;
    audio.sync([{ key: "301:1", stream: stream(), level: 1 }], 1, "");
    await settle();
    expect(ctx.state).toBe("suspended");
    expect(inputs).toHaveLength(1);
    ctx.allowed = true;
    inputs[0]?.();
    await settle();
    expect(ctx.state).toBe("running");
  });

  it("plays through the element without WebAudio, 100 % at most, ducked by its volume", () => {
    const { audio, elements, stream } = rig({ webAudio: false });
    audio.prepare();
    expect(audio.boosted).toBe(false);
    const one = stream();
    audio.sync([{ key: "301:1", stream: one, level: 1.5 }], 1, "");
    expect(elements[0]).toMatchObject({ muted: false, volume: 1 });
    audio.sync([{ key: "301:1", stream: one, level: 0.8 }], 0.5, "");
    expect(elements[0]?.volume).toBeCloseTo(0.4);
    audio.sync([{ key: "301:1", stream: one, level: 0 }], 1, "");
    expect(elements[0]?.muted).toBe(true);
  });
});
