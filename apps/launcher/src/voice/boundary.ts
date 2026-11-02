// The launcher ↔ voice page boundary (pl_voice::engine: `EngineCommand` in through the channel the
// page hands over with `voice_page_attach`, `PageReport` out through `voice_page_report`), as the
// page reads and writes it. Generated from the Rust types (VOICE-L1); the engine imports them here.
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
