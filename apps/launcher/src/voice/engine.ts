// The voice engine behind the launcher's commands (VOICE-L2 builds it here; VOICE-0's probe.ts,
// dsp.ts, sdp.ts and worklets/ are its starting point). Until then it holds the boundary only: a
// join goes in listening (no microphone, no peer connection), so the room's flow can be followed
// without audio.
//
// What the launcher expects (pl_voice::engine):
// - `configure` any time; `join` → make a peer connection with one send-only audio track and
//   report `offer` (sdp null: no microphone, listen only); `answer` → set the room's answer;
//   `offer` → set it, report `answer`; `ice` → restart ICE with the TURN credentials;
// - `talk` opens or closes the gate (keep 200 ms after push-to-talk closes it); `deaf` stops all
//   playback; `people` sets each person's gain (`volume`, held to the playout's top) or silence;
// - `sound` plays join/leave/tick and the caller's waiting tone (`stop` ends it; `leave` too);
// - `setup` / `selfTest` (the settings page) and `stats` (once, on demand);
// - reports: `speaking` (≤ 10/s, on change), `quality`, `needTurn` (no ICE after 5 s), `lost`
//   (ICE gone for 3 s), `connected`, `mic`, `devices`, `level` (setup only), `failed`.
import type { EngineCommand, PageReport } from "../bindings";

export interface Engine {
  handle(command: EngineCommand): void;
}

export function createEngine(report: (value: PageReport) => void): Engine {
  return {
    handle(command) {
      switch (command.t) {
        case "join":
          report({ t: "offer", sdp: null, mid: null });
          break;
        default:
          break;
      }
    },
  };
}
