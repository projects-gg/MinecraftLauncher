// The hidden voice chat window's page (src-tauri voice.rs): never shown, no React, and nothing
// runs until the launcher asks. For now it only runs VOICE-0's probe, which a debug launcher
// starts with PROJECTS_VOICE_PROBE once the microphone's answer is in place.
import { emit, listen } from "@tauri-apps/api/event";
import { type ProbeGo, runProbe } from "./probe";

async function main(): Promise<void> {
  // How long an event takes to reach this page and come back.
  await listen<number>("voice-probe-ping", (event) => {
    void emit("voice-probe-pong", event.payload);
  });
  const stop = await listen<ProbeGo>("voice-probe-go", (event) => {
    stop();
    void runProbe(event.payload, (report) => {
      void emit("voice-probe-report", report);
    }).finally(() => {
      void emit("voice-probe-done");
    });
  });
  await emit("voice-probe-ready");
}

void main();
