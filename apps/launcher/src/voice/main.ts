// The hidden voice chat window's page (src-tauri voice/): never shown, no React, and nothing runs
// until the launcher asks. It hands the launcher a channel for its commands (`EngineCommand`) and
// reports back with `voice_page_report` (`PageReport`); these two are the only commands this
// window may call (pl_voice::guard). The engine behind them is VOICE-L2's (`engine.ts`).
//
// Debug builds started with PROJECTS_VOICE_PROBE run VOICE-0's probe instead: only then may this
// window use events (the probe grants them at run time), so a refused listener means the product.
import { Channel, invoke } from "@tauri-apps/api/core";
import { emit, listen } from "@tauri-apps/api/event";
import type { EngineCommand, PageReport } from "../bindings";
import { createEngine } from "./engine";
import { type ProbeGo, runProbe } from "./probe";

/** VOICE-0's probe, when a debug launcher started one; false in the product. */
async function probe(): Promise<boolean> {
  // Release builds never run the probe (debug builds: the dev server, or TAURI_ENV_DEBUG).
  if (!import.meta.env.DEV && import.meta.env.TAURI_ENV_DEBUG !== "true") return false;
  try {
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
    return true;
  } catch {
    return false;
  }
}

function report(value: PageReport): void {
  void invoke("voice_page_report", { report: value }).catch(() => {
    // The launcher closed the window meanwhile.
  });
}

async function main(): Promise<void> {
  if (await probe()) return;
  const engine = createEngine(report);
  const channel = new Channel<EngineCommand>();
  channel.onmessage = (command) => engine.handle(command);
  await invoke("voice_page_attach", { onCommand: channel });
  report({ t: "ready" });
}

void main();
