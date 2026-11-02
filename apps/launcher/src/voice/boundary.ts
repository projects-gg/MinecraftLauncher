// The launcher ↔ voice page boundary (pl_voice::engine: `EngineCommand` in through the channel the
// page hands over with `voice_page_attach`, `PageReport` out through `voice_page_report`), as the
// page reads and writes it. A mirror of the Rust types until VOICE-L1's bindings carry them; then
// this file only re-exports them from `../bindings`.

/** The microphone as the engine saw it last. */
export type VoiceMic = "unknown" | "ok" | "denied" | "missing";

/** An audio device; `id` is what the settings keep (none there: the system's default). */
export interface VoiceDevice {
  id: string;
  label: string;
}

export interface VoiceDevices {
  inputs: VoiceDevice[];
  outputs: VoiceDevice[];
}

/** How the page processes and sends the microphone, and plays people. */
export interface EngineSettings {
  /** null: the system's default. */
  inputDevice: string | null;
  outputDevice: string | null;
  noiseSuppression: boolean;
  echoCancellation: boolean;
  autoGain: boolean;
  /** null: automatic; else the threshold, 0 (everything goes) to 100. */
  sensitivity: number | null;
  /** Percent (only while autoGain is off). */
  micVolume: number;
  /** The gate follows `talk` alone; voice activity is not used. */
  pushToTalk: boolean;
  /** Opus bit rate (kbps). */
  kbps: number;
  sounds: boolean;
}

/** One person as the page plays them. */
export interface EnginePerson {
  id: string;
  /** Percent (held to the playout's top here). */
  volume: number;
  /** Not played: muted for the player, or muted for everyone. */
  silent: boolean;
}

/** TURN credentials (only after ICE failed; never logged). */
export interface EngineIce {
  urls: string[];
  username: string;
  credential: string;
}

export type EngineSound = "join" | "leave" | "tick" | "waiting" | "stop";

/** Launcher → page. */
export type EngineCommand =
  | { t: "configure"; settings: EngineSettings }
  | { t: "join" }
  | { t: "answer"; sdp: string }
  | { t: "offer"; sdp: string; mids: { [key in string]: string } }
  | { t: "talk"; open: boolean }
  | { t: "deaf"; on: boolean }
  | { t: "people"; people: EnginePerson[] }
  | { t: "ice"; ice: EngineIce }
  | { t: "sound"; sound: EngineSound }
  | { t: "stats" }
  | { t: "setup"; on: boolean }
  | { t: "selfTest"; on: boolean }
  | { t: "leave" };

/** Page → launcher. */
export type PageReport =
  | { t: "ready" }
  | { t: "offer"; sdp: string | null; mid: string | null }
  | { t: "answer"; sdp: string }
  | { t: "speaking"; ids: string[]; me: boolean }
  | { t: "quality"; q: number }
  | { t: "needTurn" }
  | { t: "lost" }
  | { t: "connected" }
  | { t: "mic"; mic: VoiceMic; changed: boolean }
  | { t: "devices"; devices: VoiceDevices }
  | { t: "level"; level: number; open: boolean }
  | { t: "stats"; rttMs: number | null; loss: number | null; jitterMs: number | null }
  | { t: "failed"; code: string };
