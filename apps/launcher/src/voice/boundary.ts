// The launcher ↔ voice page boundary (pl_voice::engine: `EngineCommand` in through the channel the
// page hands over with `voice_page_attach`, `PageReport` out through `voice_page_report`), as the
// page reads and writes it. Generated from the Rust types (VOICE-L1); the engine imports them here.
import type { PageReport } from "../bindings";

export type {
  EngineCommand,
  EngineIce,
  EnginePerson,
  EngineSettings,
  EngineSound,
  PageReport,
  /**
   * How people play (pl_voice::gain::Playout): WebAudio gains reach 200 %, `<audio>` elements (an
   * engine whose echo canceller cannot hear WebAudio) stop at 100 %. Sent as `{t: "playout"}` when
   * the sound opens and whenever the path changes: the volume slider stops at the path's top.
   */
  Playout,
  VoiceDevice,
  VoiceDevices,
  VoiceMic,
} from "../bindings";

/** Everything the page reports. */
export type EngineReport = PageReport;
