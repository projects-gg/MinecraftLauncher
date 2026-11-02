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
  VoiceDevice,
  VoiceDevices,
  VoiceMic,
} from "../bindings";

/**
 * How people play (pl_voice::gain::Playout, same names): WebAudio gains reach 200 %, `<audio>`
 * elements (an engine whose echo canceller cannot hear WebAudio) stop at 100 %.
 */
export type Playout = "webAudio" | "element";

/**
 * The playout path, sent when the sound opens and whenever the path changes, so the volume slider
 * can stop at the path's top. Not in `PageReport` yet (VOICE-INT adds `PageReport::Playout`): until
 * then the launcher refuses it and the page goes on.
 */
export type PlayoutReport = { t: "playout"; playout: Playout };

/** Everything the page reports. */
export type EngineReport = PageReport | PlayoutReport;
