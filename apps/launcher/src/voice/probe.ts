// VOICE-0's call against itself in the hidden voice window (a debug launcher started with
// PROJECTS_VOICE_PROBE, src-tauri voice.rs): the microphone through the planned chain (Chromium's
// echo canceller and gain control, a noise suppressor, a speech gate), Opus to a second peer
// connection in the same page, played like a person in a room. Every few seconds the counters go
// to the launcher, which records them with the processes' CPU time and memory.
import {
  GtcrnWorkletNode,
  loadGtcrn,
  loadRnnoise,
  RnnoiseWorkletNode,
} from "@sapphi-red/web-noise-suppressor";
import gtcrnWasm from "@sapphi-red/web-noise-suppressor/gtcrn.wasm?url";
import gtcrnWorklet from "@sapphi-red/web-noise-suppressor/gtcrnWorklet.js?url";
import rnnoiseWasm from "@sapphi-red/web-noise-suppressor/rnnoise.wasm?url";
import rnnoiseSimdWasm from "@sapphi-red/web-noise-suppressor/rnnoise_simd.wasm?url";
import rnnoiseWorklet from "@sapphi-red/web-noise-suppressor/rnnoiseWorklet.js?url";
import { PILOT_HZ, PILOT_LEVEL, type PilotCounts } from "./dsp";
import { tuneOpus } from "./sdp";
import gateWorklet from "./worklets/gate?worker&url";
import meterWorklet from "./worklets/meter?worker&url";

/** What the launcher asks for (pl_voice::probe::PageConfig). */
export interface ProbePage {
  ns: "off" | "builtin" | "rnnoise" | "gtcrn";
  echo: "off" | "on" | "all" | "remoteOnly";
  agc: boolean;
  gate: boolean;
  pilot: boolean;
  bitrate: number;
  playout: "element" | "webAudio";
  volume: number;
}

export interface ProbeGo {
  page: ProbePage;
  seconds: number;
  every: number;
}

export type Report = Record<string, unknown>;

const RATE = 48000;
/** How often the page's own timer should fire: how late it is shows a slowed-down page. */
const TICK_MS = 250;

const sleep = (ms: number) => new Promise((resolve) => setTimeout(resolve, ms));

/** `getUserMedia`'s value; Chromium takes "all" and "remote-only" beside true and false. */
function echoConstraint(echo: ProbePage["echo"]): ConstrainBoolean {
  const value = { off: false, on: true, all: "all", remoteOnly: "remote-only" }[echo];
  return value as unknown as ConstrainBoolean;
}

async function suppressor(ctx: AudioContext, ns: ProbePage["ns"]): Promise<AudioNode | null> {
  if (ns === "rnnoise") {
    const wasmBinary = await loadRnnoise({ url: rnnoiseWasm, simdUrl: rnnoiseSimdWasm });
    await ctx.audioWorklet.addModule(rnnoiseWorklet);
    return new RnnoiseWorkletNode(ctx, { maxChannels: 1, wasmBinary });
  }
  if (ns === "gtcrn") {
    const wasmBinary = await loadGtcrn({ url: gtcrnWasm });
    await ctx.audioWorklet.addModule(gtcrnWorklet);
    return new GtcrnWorkletNode(ctx, { maxChannels: 1, wasmBinary });
  }
  return null;
}

/**
 * The processing graph's context. With <audio> elements playing people it plays nothing itself, so
 * it gets no output device (Chromium's sinkId "none") and the audio service runs one stream less.
 */
function audioContext(silent: boolean): AudioContext {
  const options: AudioContextOptions = { sampleRate: RATE, latencyHint: "interactive" };
  if (silent) {
    try {
      return new AudioContext({ ...options, sinkId: { type: "none" } } as AudioContextOptions);
    } catch {
      // An engine without sinkId: the context plays silence on the default device.
    }
  }
  return new AudioContext(options);
}

/** Resolves once the connection's description carries all its candidates (no trickling). */
function gathered(pc: RTCPeerConnection): Promise<void> {
  if (pc.iceGatheringState === "complete") return Promise.resolve();
  return new Promise((resolve) => {
    const done = () => {
      if (pc.iceGatheringState !== "complete") return;
      pc.removeEventListener("icegatheringstatechange", done);
      resolve();
    };
    pc.addEventListener("icegatheringstatechange", done);
  });
}

function pick(stats: RTCStatsReport, type: string): Record<string, unknown> | undefined {
  for (const value of stats.values()) {
    if (value.type === type && (value.kind === undefined || value.kind === "audio")) {
      return value as Record<string, unknown>;
    }
  }
  return undefined;
}

function only(source: Record<string, unknown> | undefined, keys: string[]): Report | null {
  if (!source) return null;
  return Object.fromEntries(keys.filter((key) => key in source).map((key) => [key, source[key]]));
}

/** Runs the call for `go.seconds` and reports every `go.every` seconds (and once at the end). */
export async function runProbe(go: ProbeGo, report: (report: Report) => void): Promise<void> {
  const { page } = go;
  const begin = performance.now();
  const t = () => Math.round(performance.now() - begin) / 1000;
  const ctx = audioContext(page.playout === "element");
  const stops: (() => void)[] = [() => void ctx.close()];
  try {
    await ctx.resume();
    const mic = await navigator.mediaDevices.getUserMedia({
      audio: {
        echoCancellation: echoConstraint(page.echo),
        autoGainControl: page.agc,
        noiseSuppression: page.ns === "builtin",
        channelCount: 1,
        sampleRate: RATE,
      },
    });
    stops.push(() => {
      for (const track of mic.getTracks()) track.stop();
    });
    const settings = mic.getAudioTracks()[0]?.getSettings();

    // Sending side: microphone → noise suppressor → speech gate → (+ pilot) → Opus.
    let chain: AudioNode = ctx.createMediaStreamSource(mic);
    const ns = await suppressor(ctx, page.ns);
    if (ns) chain = chain.connect(ns);
    const gate = { blocks: 0, openBlocks: 0 };
    if (page.gate) {
      await ctx.audioWorklet.addModule(gateWorklet);
      const node = new AudioWorkletNode(ctx, "pl-speech-gate", { outputChannelCount: [1] });
      node.port.onmessage = (event: MessageEvent<typeof gate>) => Object.assign(gate, event.data);
      chain = chain.connect(node);
    }
    const out = ctx.createMediaStreamDestination();
    out.channelCount = 1;
    chain.connect(out);
    if (page.pilot) {
      const tone = ctx.createOscillator();
      tone.frequency.value = PILOT_HZ;
      const level = ctx.createGain();
      level.gain.value = PILOT_LEVEL;
      tone.connect(level).connect(out);
      tone.start();
    }
    const sendTrack = out.stream.getAudioTracks()[0];
    if (!sendTrack) throw new Error("no track to send");

    // The other person: a second connection in this page.
    const a = new RTCPeerConnection();
    const b = new RTCPeerConnection();
    stops.push(
      () => a.close(),
      () => b.close(),
    );
    const sender = a.addTransceiver(sendTrack, { direction: "sendonly" }).sender;
    const arrived = new Promise<MediaStream>((resolve) => {
      b.ontrack = (event) => resolve(event.streams[0] ?? new MediaStream([event.track]));
    });
    await a.setLocalDescription(await a.createOffer());
    await gathered(a);
    await b.setRemoteDescription(a.localDescription as RTCSessionDescriptionInit);
    await b.setLocalDescription(await b.createAnswer());
    await gathered(b);
    const answer = b.localDescription?.sdp ?? "";
    await a.setRemoteDescription({
      type: "answer",
      sdp: tuneOpus(answer, { bitrate: page.bitrate, dtx: true, fec: true }),
    });
    const parameters = sender.getParameters();
    for (const encoding of parameters.encodings) encoding.maxBitrate = page.bitrate;
    await sender.setParameters(parameters);
    const remote = await arrived;

    // Receiving side: played like a person in a room; the pilot measured on arrival.
    const element = new Audio();
    element.srcObject = remote;
    if (page.playout === "element") {
      element.volume = Math.min(1, page.volume);
      element.muted = page.volume === 0;
    } else {
      // Chromium plays received audio only through an element; WebAudio takes a copy of it.
      element.muted = true;
      const gain = ctx.createGain();
      gain.gain.value = page.volume;
      const limiter = ctx.createDynamicsCompressor();
      limiter.threshold.value = -3;
      limiter.knee.value = 0;
      limiter.ratio.value = 20;
      limiter.attack.value = 0.003;
      limiter.release.value = 0.1;
      ctx.createMediaStreamSource(remote).connect(gain).connect(limiter).connect(ctx.destination);
    }
    await element.play();
    stops.push(() => element.pause());
    const pilot: PilotCounts = { blocks: 0, gaps: 0, gapBlocks: 0, longestGapBlocks: 0 };
    let pilotLongest = 0;
    if (page.pilot) {
      await ctx.audioWorklet.addModule(meterWorklet);
      const meter = new AudioWorkletNode(ctx, "pl-pilot-meter", { numberOfOutputs: 0 });
      meter.port.onmessage = (event: MessageEvent<PilotCounts>) => {
        Object.assign(pilot, event.data);
        pilotLongest = Math.max(pilotLongest, event.data.longestGapBlocks);
      };
      ctx.createMediaStreamSource(remote).connect(meter);
    }

    // The page's own timer: how much later than its interval it fires.
    let last = performance.now();
    let lateMax = 0;
    let ticks = 0;
    const timer = setInterval(() => {
      const now = performance.now();
      lateMax = Math.max(lateMax, now - last - TICK_MS);
      last = now;
      ticks += 1;
    }, TICK_MS);
    stops.push(() => clearInterval(timer));

    const send = async (final: boolean) => {
      const received = await b.getStats();
      const sent = await a.getStats();
      const pair = [...received.values()].find(
        (value) => value.type === "candidate-pair" && value.nominated,
      ) as { currentRoundTripTime?: number } | undefined;
      report({
        t: t(),
        final,
        vis: document.visibilityState,
        ctx: { state: ctx.state, baseLatency: ctx.baseLatency, outputLatency: ctx.outputLatency },
        timer: { lateMaxMs: Math.round(lateMax * 10) / 10, ticks },
        in: only(pick(received, "inbound-rtp"), [
          "packetsReceived",
          "packetsLost",
          "jitter",
          "totalSamplesReceived",
          "concealedSamples",
          "silentConcealedSamples",
          "concealmentEvents",
          "insertedSamplesForDeceleration",
          "removedSamplesForAcceleration",
          "jitterBufferDelay",
          "jitterBufferEmittedCount",
          "audioLevel",
        ]),
        playout: only(pick(received, "media-playout"), [
          "synthesizedSamplesDuration",
          "synthesizedSamplesEvents",
          "totalSamplesDuration",
          "totalPlayoutDelay",
          "totalSamplesCount",
        ]),
        out: only(pick(sent, "outbound-rtp"), ["packetsSent", "bytesSent", "targetBitrate"]),
        rttMs: pair?.currentRoundTripTime === undefined ? null : pair.currentRoundTripTime * 1000,
        pilot: page.pilot ? { ...pilot, longestGapBlocksMax: pilotLongest } : null,
        gate: page.gate ? { ...gate } : null,
        mic: settings ?? null,
      });
      lateMax = 0;
      pilotLongest = 0;
    };

    const end = begin + go.seconds * 1000;
    for (let next = begin + go.every * 1000; next < end; next += go.every * 1000) {
      await sleep(Math.max(0, next - performance.now()));
      await send(false);
    }
    await sleep(Math.max(0, end - performance.now()));
    await send(true);
  } catch (error) {
    report({ t: t(), final: true, error: String(error) });
  } finally {
    for (const stop of stops.reverse()) {
      try {
        stop();
      } catch {
        // Already gone.
      }
    }
  }
}
