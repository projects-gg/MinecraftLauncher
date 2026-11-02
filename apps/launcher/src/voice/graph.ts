// The voice window's sound (design §6.2, §6.4; VOICE-0's decisions): one AudioContext does it all
// (one audio thread, one output stream). The microphone comes through Chromium's echo canceller
// (asked for "all": it hears WebAudio's output too) and automatic gain, then the capture worklet
// (gate, RNNoise, push-to-talk), whose output is the one track the peer connection sends; it never
// changes, so a new microphone never touches the call. People play through a gain each (0–200 %;
// above 100 % behind their own limiter), the mix through one more limiter. An engine whose echo
// canceller cannot hear WebAudio ("all" refused) plays people through <audio> elements, 100 % at
// most. Who speaks is measured on arrival (the speakers worklet). Chromium only feeds a remote
// track to WebAudio while a media element plays it, so each person has a muted element.
import rnnoiseWasmUrl from "@sapphi-red/web-noise-suppressor/rnnoise_simd.wasm?url";
import type {
  EngineSettings,
  EngineSound,
  Playout,
  VoiceDevice,
  VoiceDevices,
  VoiceMic,
} from "./boundary";
import type { CaptureConfig } from "./capture-core";
import { listDevices } from "./devices";
import { loops, renderSound, type SoundName } from "./sounds";
import { SPEAKER_SLOTS } from "./speakers-core";
import type { CaptureMessage, CaptureOptions, CapturePost } from "./worklets/capture";
import captureWorkletUrl from "./worklets/capture?worker&url";
import type { SpeakersMessage, SpeakersPost } from "./worklets/speakers";
import speakersWorkletUrl from "./worklets/speakers?worker&url";

export interface GraphEvents {
  /** The microphone sends speech now (or stopped). */
  me(on: boolean): void;
  /** The settings page's bar. */
  level(level: number, open: boolean): void;
  /** A receiver's sound is speech now (or stopped). */
  speaking(mid: string, on: boolean): void;
  /** The microphone's state; `changed`: the one in use went and another took its place. */
  mic(mic: VoiceMic, changed: boolean): void;
  devices(devices: VoiceDevices): void;
  /** How people play now (once the sound is open, then on every change). */
  playout(playout: Playout): void;
  /** Something the launcher should log (a short code). */
  failed(code: string): void;
}

/** What the engine needs of the sound (tests give it a fake). */
export interface Graph {
  /** What the peer connection sends (silence while the gate is shut, or without a microphone). */
  readonly sendTrack: MediaStreamTrack;
  configure(settings: EngineSettings): Promise<void>;
  /** The way out opens or closes; `tail` false: close at once even in push-to-talk. */
  talk(open: boolean, tail?: boolean): void;
  levels(on: boolean): void;
  test(on: boolean): void;
  deaf(on: boolean): void;
  /** A receiver of the call (not played until `play`). */
  attach(mid: string, track: MediaStreamTrack): void;
  /** Plays a receiver at `volume` percent and measures it; null: neither. */
  play(mid: string, volume: number | null): void;
  detach(mid: string): void;
  sound(name: EngineSound): void;
  close(): void;
}

interface Entry {
  track: MediaStreamTrack;
  stream: MediaStream;
  /** Only while played (Chromium feeds WebAudio a remote track only while an element plays it). */
  element: HTMLAudioElement | null;
  source: MediaStreamAudioSourceNode | null;
  gain: GainNode | null;
  boost: DynamicsCompressorNode | null;
  slot: number | null;
  volume: number | null;
}

interface Mic {
  stream: MediaStream;
  track: MediaStreamTrack;
  source: MediaStreamAudioSourceNode;
}

const RATE = 48000;
const MIC_TIMEOUT_MS = 8000;
const RESUME_TIMEOUT_MS = 2000;
/** Volume changes glide (no clicks). */
const GLIDE_S = 0.015;

let wasmBytes: Promise<ArrayBuffer | null> | null = null;

/** RNNoise's module, fetched once per page (each worklet gets its own copy). */
function rnnoiseBytes(): Promise<ArrayBuffer | null> {
  wasmBytes ??= fetch(rnnoiseWasmUrl)
    .then((response) => (response.ok ? response.arrayBuffer() : null))
    .catch(() => null);
  return wasmBytes;
}

function wait(ms: number): Promise<void> {
  return new Promise((resolve) => setTimeout(resolve, ms));
}

type Sinkable = { setSinkId?: (id: string) => Promise<void> };

async function setSink(target: unknown, id: string): Promise<void> {
  const sinkable = target as Sinkable;
  if (typeof sinkable.setSinkId === "function") await sinkable.setSinkId(id);
}

function limiterOf(ctx: AudioContext): DynamicsCompressorNode {
  return new DynamicsCompressorNode(ctx, {
    threshold: -3,
    knee: 0,
    ratio: 20,
    attack: 0.003,
    release: 0.1,
  });
}

/** What a refused `getUserMedia` means for the player. */
function micStateOf(error: unknown): VoiceMic {
  const name = error instanceof Error ? error.name : "";
  return name === "NotAllowedError" || name === "SecurityError" ? "denied" : "missing";
}

/** A chosen device that is not there (worth trying the default instead). */
function gone(error: unknown): boolean {
  const name = error instanceof Error ? error.name : "";
  return name === "NotFoundError" || name === "OverconstrainedError";
}

export class AudioGraph implements Graph {
  readonly sendTrack: MediaStreamTrack;
  private readonly capture: AudioWorkletNode;
  private readonly limiter: DynamicsCompressorNode;
  private readonly mix: GainNode;
  private readonly monitor: GainNode;
  private readonly speakers: AudioWorkletNode[] = [];
  /** Which receiver each speakers input measures, its generation, and whether it is lit. */
  private readonly slots: (string | null)[] = [];
  private readonly slotGenerations: number[] = [];
  private generations = 0;
  private readonly lit = new Set<number>();
  private readonly entries = new Map<string, Entry>();
  private readonly buffers = new Map<SoundName, AudioBuffer>();
  private settings: EngineSettings;
  private mic: Mic | null = null;
  private micState: VoiceMic = "unknown";
  private micWork: Promise<void> = Promise.resolve();
  private path: Playout = "webAudio";
  private started = false;
  private ns: "loading" | "ready" | "failed" | "slow" = "loading";
  private deafOn = false;
  private testOn = false;
  private sinkId = "";
  private inputs: VoiceDevice[] | null = null;
  private outputs: VoiceDevice[] | null = null;
  private sentDevices = "";
  private waiting: AudioBufferSourceNode | null = null;
  private closed = false;
  private readonly onDeviceChange = () => {
    this.micWork = this.micWork.then(() => this.devicesChanged()).catch(() => undefined);
  };

  /** Opens the sound: the context, its worklets, the microphone (if it can), the devices. */
  static async open(settings: EngineSettings, events: GraphEvents): Promise<AudioGraph> {
    const ctx = new AudioContext({ sampleRate: RATE, latencyHint: "interactive" });
    try {
      const [wasm] = await Promise.all([
        rnnoiseBytes(),
        ctx.audioWorklet.addModule(captureWorkletUrl),
        ctx.audioWorklet.addModule(speakersWorkletUrl),
      ]);
      const graph = new AudioGraph(ctx, settings, events, wasm);
      await graph.start();
      return graph;
    } catch (error) {
      void ctx.close().catch(() => undefined);
      throw error;
    }
  }

  private constructor(
    private readonly ctx: AudioContext,
    settings: EngineSettings,
    private readonly events: GraphEvents,
    wasm: ArrayBuffer | null,
  ) {
    this.settings = settings;
    if (!wasm) {
      this.ns = "failed";
      events.failed("rnnoise_load");
    }
    const options: CaptureOptions = { wasm, config: this.captureConfig() };
    this.capture = new AudioWorkletNode(ctx, "pl-voice-capture", {
      numberOfInputs: 1,
      numberOfOutputs: 2,
      outputChannelCount: [1, 1],
      channelCount: 1,
      channelCountMode: "explicit",
      channelInterpretation: "speakers",
      processorOptions: options,
    });
    this.capture.port.onmessage = (event: MessageEvent<CapturePost>) => this.onCapture(event.data);
    const send = ctx.createMediaStreamDestination();
    send.channelCount = 1;
    this.capture.connect(send, 0);
    const track = send.stream.getAudioTracks()[0];
    if (!track) throw new Error("no send track");
    this.sendTrack = track;
    this.limiter = limiterOf(ctx);
    this.limiter.connect(ctx.destination);
    this.mix = new GainNode(ctx, { gain: 1 });
    this.mix.connect(this.limiter);
    this.monitor = new GainNode(ctx, { gain: 1 });
    this.monitor.connect(this.limiter);
  }

  private async start(): Promise<void> {
    // The microphone first: a page that captures may play sound without a click.
    await this.acquire(false);
    await this.resume();
    await this.applySink();
    await this.listDevices();
    navigator.mediaDevices.addEventListener("devicechange", this.onDeviceChange);
    this.started = true;
    this.events.playout(this.path);
  }

  private async resume(): Promise<void> {
    if (this.ctx.state === "running") return;
    const running = await Promise.race([
      this.ctx.resume().then(
        () => true,
        () => false,
      ),
      wait(RESUME_TIMEOUT_MS).then(() => false),
    ]);
    if (!running || (this.ctx.state as AudioContextState) !== "running") {
      this.events.failed("audio_blocked");
    }
  }

  private post(message: CaptureMessage): void {
    this.capture.port.postMessage(message);
  }

  private nsUsable(): boolean {
    return this.ns === "loading" || this.ns === "ready";
  }

  private captureConfig(): CaptureConfig {
    const s = this.settings;
    return {
      gain: s.autoGain ? 1 : Math.min(200, Math.max(0, s.micVolume)) / 100,
      ns: s.noiseSuppression && this.nsUsable(),
      threshold: s.sensitivity === null ? null : Math.min(100, Math.max(0, s.sensitivity)),
      ptt: s.pushToTalk,
    };
  }

  private onCapture(message: CapturePost): void {
    if (this.closed) return;
    switch (message.t) {
      case "me":
        this.events.me(message.on);
        break;
      case "level":
        this.events.level(message.level, message.open);
        break;
      case "ns":
        if (message.ok) {
          this.ns = "ready";
        } else if (this.ns !== "failed") {
          this.ns = "failed";
          this.events.failed("rnnoise_load");
          this.fallBackToBuiltIn();
        }
        break;
      case "slow":
        this.ns = "slow";
        this.events.failed("rnnoise_slow");
        this.fallBackToBuiltIn();
        break;
    }
  }

  /** RNNoise is out: the browser's own suppressor takes over (it needs the microphone anew). */
  private fallBackToBuiltIn(): void {
    this.post({ t: "config", config: this.captureConfig() });
    if (this.settings.noiseSuppression) this.reacquire(false);
  }

  async configure(settings: EngineSettings): Promise<void> {
    const before = this.settings;
    this.settings = settings;
    this.post({ t: "config", config: this.captureConfig() });
    const builtIn = (s: EngineSettings) => s.noiseSuppression && !this.nsUsable();
    if (
      before.inputDevice !== settings.inputDevice ||
      before.echoCancellation !== settings.echoCancellation ||
      before.autoGain !== settings.autoGain ||
      builtIn(before) !== builtIn(settings)
    ) {
      this.reacquire(false);
      await this.micWork;
    }
    if (before.outputDevice !== settings.outputDevice) await this.applySink();
  }

  talk(open: boolean, tail = true): void {
    this.post({ t: "talk", open, tail });
  }

  levels(on: boolean): void {
    this.post({ t: "levels", on });
  }

  test(on: boolean): void {
    if (on === this.testOn) return;
    this.testOn = on;
    this.post({ t: "test", on });
    if (on) this.capture.connect(this.monitor, 1);
    else this.capture.disconnect(this.monitor, 1);
  }

  deaf(on: boolean): void {
    this.deafOn = on;
    this.mix.gain.setTargetAtTime(on ? 0 : 1, this.ctx.currentTime, GLIDE_S / 2);
    if (this.path === "element") {
      for (const entry of this.entries.values()) {
        if (entry.element) entry.element.muted = on || entry.volume === null;
      }
    }
  }

  // -----------------------------------------------------------------------------------------
  // The microphone

  private reacquire(changed: boolean): void {
    this.micWork = this.micWork.then(() => this.acquire(changed)).catch(() => undefined);
  }

  /** Opens the chosen microphone (or the default when it is gone); `changed`: say it changed. */
  private async acquire(changed: boolean): Promise<void> {
    if (this.closed) return;
    const chosen = this.settings.inputDevice;
    let stream: MediaStream | null = null;
    let state: VoiceMic = "ok";
    let moved = changed;
    try {
      stream = await this.openMic(chosen);
    } catch (error) {
      if (chosen && gone(error)) {
        moved = true;
        try {
          stream = await this.openMic(null);
        } catch (second) {
          state = this.refused(second);
        }
      } else {
        state = this.refused(error);
      }
    }
    if (this.closed) {
      for (const track of stream?.getTracks() ?? []) track.stop();
      return;
    }
    this.takeMic(stream);
    if (state !== this.micState || (moved && state === "ok")) {
      this.micState = state;
      this.events.mic(state, moved && state === "ok");
    }
  }

  private refused(error: unknown): VoiceMic {
    const name = error instanceof Error ? error.name : "";
    if (name === "NotReadableError") this.events.failed("mic_busy");
    if (name === "TimeoutError") this.events.failed("mic_timeout");
    return micStateOf(error);
  }

  private async openMic(deviceId: string | null): Promise<MediaStream> {
    const s = this.settings;
    const base: MediaTrackConstraints = {
      autoGainControl: s.autoGain,
      noiseSuppression: s.noiseSuppression && !this.nsUsable(),
      channelCount: { ideal: 1 },
      sampleRate: { ideal: RATE },
    };
    if (deviceId) base.deviceId = { exact: deviceId };
    if (!s.echoCancellation) return this.getMic({ ...base, echoCancellation: false });
    try {
      // Chromium takes "all" beside true/false: the canceller hears all output, WebAudio's too.
      return await this.getMic({ ...base, echoCancellation: "all" as unknown as boolean });
    } catch (error) {
      const name = error instanceof Error ? error.name : "";
      if (name !== "TypeError" && name !== "OverconstrainedError") throw error;
      return this.getMic({ ...base, echoCancellation: true });
    }
  }

  /** `getUserMedia`, given up after a while (a late stream is stopped). */
  private getMic(audio: MediaTrackConstraints): Promise<MediaStream> {
    return new Promise((resolve, reject) => {
      let settled = false;
      const timer = setTimeout(() => {
        settled = true;
        const error = new Error("microphone timed out");
        error.name = "TimeoutError";
        reject(error);
      }, MIC_TIMEOUT_MS);
      navigator.mediaDevices.getUserMedia({ audio }).then(
        (stream) => {
          clearTimeout(timer);
          if (settled) {
            for (const track of stream.getTracks()) track.stop();
            return;
          }
          settled = true;
          resolve(stream);
        },
        (error: unknown) => {
          clearTimeout(timer);
          if (settled) return;
          settled = true;
          reject(error);
        },
      );
    });
  }

  private takeMic(stream: MediaStream | null): void {
    const old = this.mic;
    this.mic = null;
    if (old) {
      old.source.disconnect();
      for (const track of old.stream.getTracks()) track.stop();
    }
    const track = stream?.getAudioTracks()[0];
    if (!stream || !track) {
      this.setPath("webAudio");
      return;
    }
    const source = this.ctx.createMediaStreamSource(stream);
    source.connect(this.capture);
    this.mic = { stream, track, source };
    track.addEventListener("ended", () => {
      // Unplugged (or taken away): the default takes its place.
      if (this.mic?.track === track) this.reacquire(true);
    });
    this.post({ t: "mic" });
    const echo: unknown = track.getSettings().echoCancellation;
    this.setPath(this.settings.echoCancellation && echo !== "all" ? "element" : "webAudio");
  }

  // -----------------------------------------------------------------------------------------
  // Devices

  private async listDevices(): Promise<void> {
    let all: MediaDeviceInfo[];
    try {
      all = await navigator.mediaDevices.enumerateDevices();
    } catch {
      return;
    }
    // Without the microphone's permission ids and names are hidden: nothing to offer then.
    const known = all.some((device) => device.deviceId !== "" && device.label !== "");
    const devices = listDevices(known ? all : []);
    this.inputs = known ? devices.inputs : null;
    this.outputs = known ? devices.outputs : null;
    const key = JSON.stringify(devices);
    if (key !== this.sentDevices) {
      this.sentDevices = key;
      this.events.devices(devices);
    }
  }

  private async devicesChanged(): Promise<void> {
    if (this.closed) return;
    await this.listDevices();
    const chosen = this.settings.inputDevice;
    const inUse = this.mic?.track.getSettings().deviceId;
    if (this.micState !== "ok") {
      // A microphone may be there now.
      if (this.inputs === null || this.inputs.length > 0) await this.acquire(false);
    } else if (chosen && inUse !== chosen && this.inputs?.some((d) => d.id === chosen)) {
      // The chosen one is back.
      await this.acquire(false);
    } else if (
      this.inputs &&
      inUse &&
      inUse !== "default" &&
      !this.inputs.some((d) => d.id === inUse)
    ) {
      await this.acquire(true);
    }
    await this.applySink();
  }

  /** The chosen output while it is there, else the system's default. */
  private async applySink(): Promise<void> {
    const chosen = this.settings.outputDevice ?? "";
    let wanted = chosen;
    if (chosen && this.outputs && !this.outputs.some((d) => d.id === chosen)) wanted = "";
    if (wanted === this.sinkId) return;
    try {
      await setSink(this.ctx, wanted);
      this.sinkId = wanted;
    } catch {
      if (wanted !== "") {
        await setSink(this.ctx, "").catch(() => undefined);
      }
      this.sinkId = "";
    }
    for (const entry of this.entries.values()) {
      if (entry.element) void setSink(entry.element, this.sinkId).catch(() => undefined);
    }
  }

  // -----------------------------------------------------------------------------------------
  // People

  attach(mid: string, track: MediaStreamTrack): void {
    const old = this.entries.get(mid);
    if (old?.track === track || this.closed) return;
    if (old) this.detach(mid);
    this.entries.set(mid, {
      track,
      stream: new MediaStream([track]),
      element: null,
      source: null,
      gain: null,
      boost: null,
      slot: null,
      volume: null,
    });
  }

  play(mid: string, volume: number | null): void {
    const entry = this.entries.get(mid);
    if (!entry) return;
    entry.volume = volume === null ? null : Math.max(0, volume);
    this.wire(mid, entry);
  }

  detach(mid: string): void {
    const entry = this.entries.get(mid);
    if (!entry) return;
    this.entries.delete(mid);
    this.silence(mid, entry);
  }

  private setPath(path: Playout): void {
    if (path === this.path) return;
    this.path = path;
    for (const [mid, entry] of this.entries) this.wire(mid, entry);
    if (this.started) this.events.playout(path);
  }

  private sourceOf(entry: Entry): MediaStreamAudioSourceNode {
    entry.source ??= this.ctx.createMediaStreamSource(entry.stream);
    return entry.source;
  }

  /** The element that plays (or, muted, feeds WebAudio) a receiver, made when first needed. */
  private elementOf(entry: Entry): HTMLAudioElement {
    if (entry.element) return entry.element;
    const element = new Audio();
    element.srcObject = entry.stream;
    element.muted = true;
    void setSink(element, this.sinkId).catch(() => undefined);
    void element.play().catch(() => undefined);
    entry.element = element;
    return element;
  }

  /** A receiver nobody plays (silenced, left, or dead): nothing of it stays connected or playing. */
  private silence(mid: string, entry: Entry): void {
    this.unmeter(mid, entry);
    entry.gain?.disconnect();
    entry.boost?.disconnect();
    entry.source?.disconnect();
    entry.gain = null;
    entry.boost = null;
    entry.source = null;
    if (entry.element) {
      entry.element.pause();
      entry.element.srcObject = null;
      entry.element = null;
    }
  }

  private wire(mid: string, entry: Entry): void {
    const volume = entry.volume;
    if (volume === null) {
      this.silence(mid, entry);
      return;
    }
    const element = this.elementOf(entry);
    this.meter(mid, entry);
    if (this.path === "element") {
      entry.gain?.disconnect();
      entry.boost?.disconnect();
      entry.gain = null;
      entry.boost = null;
      element.volume = Math.min(1, volume / 100);
      element.muted = this.deafOn;
      return;
    }
    element.muted = true;
    const level = Math.min(2, volume / 100);
    // Above 100 % a person's own limiter keeps their peaks from pressing everyone else down.
    const boosted = level > 1;
    let gain = entry.gain;
    if (!gain) {
      gain = new GainNode(this.ctx, { gain: level });
      this.sourceOf(entry).connect(gain);
      entry.gain = gain;
    } else {
      gain.gain.setTargetAtTime(level, this.ctx.currentTime, GLIDE_S);
      if (boosted === (entry.boost !== null)) return;
      gain.disconnect();
      entry.boost?.disconnect();
      entry.boost = null;
    }
    if (boosted) {
      entry.boost = limiterOf(this.ctx);
      gain.connect(entry.boost).connect(this.mix);
    } else {
      gain.connect(this.mix);
    }
  }

  private meter(mid: string, entry: Entry): void {
    if (entry.slot !== null) return;
    let slot = this.slots.indexOf(null);
    if (slot < 0) {
      const node = new AudioWorkletNode(this.ctx, "pl-voice-speakers", {
        numberOfInputs: SPEAKER_SLOTS,
        numberOfOutputs: 0,
        channelCount: 1,
        channelCountMode: "explicit",
        channelInterpretation: "speakers",
      });
      const base = this.speakers.length * SPEAKER_SLOTS;
      node.port.onmessage = (event: MessageEvent<SpeakersPost>) =>
        this.onSpeaker(base + event.data.slot, event.data.on, event.data.generation);
      this.speakers.push(node);
      slot = this.slots.length;
      for (let i = 0; i < SPEAKER_SLOTS; i++) {
        this.slots.push(null);
        this.slotGenerations.push(0);
      }
    }
    const node = this.speakers[Math.floor(slot / SPEAKER_SLOTS)];
    if (!node) return;
    this.generations += 1;
    this.slots[slot] = mid;
    this.slotGenerations[slot] = this.generations;
    entry.slot = slot;
    node.port.postMessage({
      t: "reset",
      slot: slot % SPEAKER_SLOTS,
      generation: this.generations,
    } satisfies SpeakersMessage);
    this.sourceOf(entry).connect(node, 0, slot % SPEAKER_SLOTS);
  }

  private unmeter(mid: string, entry: Entry): void {
    const slot = entry.slot;
    if (slot === null) return;
    entry.slot = null;
    this.slots[slot] = null;
    const node = this.speakers[Math.floor(slot / SPEAKER_SLOTS)];
    if (node && entry.source) {
      try {
        entry.source.disconnect(node, 0, slot % SPEAKER_SLOTS);
      } catch {
        // Not connected there.
      }
    }
    if (this.lit.delete(slot)) this.events.speaking(mid, false);
  }

  private onSpeaker(slot: number, on: boolean, generation: number): void {
    const mid = this.slots[slot];
    // A post for the slot's previous person, still on its way when the slot changed hands.
    if (this.closed || !mid || generation !== this.slotGenerations[slot]) return;
    if (on) this.lit.add(slot);
    else this.lit.delete(slot);
    this.events.speaking(mid, on);
  }

  // -----------------------------------------------------------------------------------------
  // Sounds

  sound(name: EngineSound): void {
    if (this.closed) return;
    if (name === "stop") {
      this.waiting?.stop();
      this.waiting = null;
      return;
    }
    if (name === "waiting" && this.waiting) return;
    let buffer = this.buffers.get(name);
    if (!buffer) {
      const samples = renderSound(name, this.ctx.sampleRate);
      buffer = new AudioBuffer({
        length: samples.length,
        numberOfChannels: 1,
        sampleRate: this.ctx.sampleRate,
      });
      buffer.copyToChannel(samples, 0);
      this.buffers.set(name, buffer);
    }
    const node = new AudioBufferSourceNode(this.ctx, { buffer, loop: loops(name) });
    node.connect(this.limiter);
    node.addEventListener("ended", () => node.disconnect());
    node.start();
    if (name === "waiting") this.waiting = node;
  }

  close(): void {
    if (this.closed) return;
    this.closed = true;
    navigator.mediaDevices.removeEventListener("devicechange", this.onDeviceChange);
    this.post({ t: "close" });
    for (const node of this.speakers) {
      node.port.postMessage({ t: "close" } satisfies SpeakersMessage);
    }
    for (const mid of [...this.entries.keys()]) this.detach(mid);
    this.waiting?.stop();
    this.waiting = null;
    if (this.mic) {
      this.mic.source.disconnect();
      for (const track of this.mic.stream.getTracks()) track.stop();
      this.mic = null;
    }
    this.sendTrack.stop();
    void this.ctx.close().catch(() => undefined);
  }
}
